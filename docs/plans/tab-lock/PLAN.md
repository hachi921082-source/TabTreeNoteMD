# PLAN: Tab Lock

> Status: implemented. Backend: `src-tauri/src/crypto/{kdf,lock_blob,obfuscated_naming}.rs`
> (scrypt/AES-256-GCM primitives), `src-tauri/src/store/lock_session_store.rs` (in-memory active
> sessions), `src-tauri/src/commands/tab_lock.rs` (`tab_lock`/`tab_unlock`/`tab_end_session`, the
> centralized `TabAccess`/`resolve_tab_access` branch point, every session-path tree/editor
> helper, and the four cross-tab-move combos), plus the resulting session dispatch wired into
> `src-tauri/src/commands/tree.rs` and `src-tauri/src/commands/editor.rs`, and the deletion
> restriction wired into `src-tauri/src/commands/tab.rs`'s `tab_remove`. Frontend:
> `src/state/useTabLock.ts`, `src/ipc/tabLock.ts`, `src/components/dialogs/PasswordDialog.tsx`,
> `src/components/TabBar/TabBar.tsx` (Lock/Unlock context-menu entries + name overlay),
> `src/components/Tree/TreeView.tsx` (unlock button, recovery dialog, drop-onto-locked-tab
> password flow), `src/App.tsx` (wiring + the pre-lock force-save-and-close via
> `EditorPane`'s new imperative `flushSaveNow` handle). Depends on `project-architecture/PLAN.md`
> and `persistence-fs/PLAN.md` (atomic write, self-write registry, retry policy — this feature is
> a heavy consumer of all three, confirmed already implemented and usable despite those two
> plans' own stale status headers at the time of this feature's implementation — see this repo's
> `docs/ISSUES.md`/git history for that correction). Interacts with `tab-management/PLAN.md`
> (lock/unlock are tab-domain commands), `memo-tree/PLAN.md` (cross-tab DND onto a locked tab, and
> the lazy recovery-check trigger point inside `tree_open`), and `search-index/PLAN.md` (index
> purge on lock — implemented as a local no-op stub, `touch_search_index_purge_tab` in
> `commands/tab_lock.rs`, matching the same seam convention `commands/tree.rs`/`commands/editor.rs`
> already use for that not-yet-implemented dependency; confirmed applicable by `designer-lite`
> during this feature's implementation). This was the highest-risk feature in the app
> (data-loss/security consequences of a mistake are severe) — every implementation choice below
> follows this file/`CRASH-RECOVERY.md` literally; no deviations were made without
> `designer-lite` re-confirmation (see "Implementation Notes" below for the one such point:
> §3 step 11's search-index seam).
>
> Crash-mid-lock recovery (marker-based, resumable/rollback): see the companion
> [`CRASH-RECOVERY.md`](CRASH-RECOVERY.md), also implemented.

Sources: `docs/spec.md` §2.9, §4.1, §6; `docs/detailed-spec.md` §3.4, §5.9; `docs/DECISIONS.md` ("Tab
Lock: Crash-Mid-Lock Recovery via Explicit Marker + Resume/Rollback" for the recovery flow now detailed
in `CRASH-RECOVERY.md`; "Implementation Policy: No Legacy Code Reference — Build from Scratch" — the
AES-256-GCM/scrypt scheme is fully specified in §2 below and does not require reading the old
repository's `src/main/crypto/*`).

---

## 1. Scope Recap

Lock target = **whole tab**, not per-memo, not per-subtree (legacy node lock and per-memo `.txtenc`
encryption are both discontinued, `[spec §4.1]` table). Distinct from **Edit Lock**
(`memo-tree/PLAN.md`'s `memo_set_edit_lock`, no password, body-edit-only block) — the two must never be
confused in naming or UI copy.

## 2. On-Disk Formats

Already fully specified in `[detailed-spec §3.4]`; restated here for this plan's self-containedness:

```jsonc
// <tab>/.tabtreenote.json, when locked
{
  "schemaVersion": 1, "appVersion": "0.1.0",
  "lock": {
    "formatVersion": 1,
    "kdf": { "algo": "scrypt", "N": 32768, "r": 8, "p": 1, "keyLen": 32, "saltB64": "..." },
    "cipher": { "algo": "aes-256-gcm", "ivB64": "...", "tagB64": "..." },
    "hint": "optional plaintext",
    "ciphertextB64": "..."
  }
}
```

Decrypted "lock payload" (never written to disk in this shape — only its encrypted form is persisted):

```jsonc
{
  "realTabName": "Private",
  "tree": [ /* MetaTreeNode[], real memoName values, same shape as memo-tree/PLAN.md's tree */ ],
  "fileMap": [ { "memoName": "Diary 2026-07", "token": "9f2c7a1e4b3d0f8c6a5e2d1b0c9a8f7e" } ]
}
```

- Tokens: 128-bit random hex (32 hex chars), **no file extension** on the obfuscated on-disk file
  (`crypto/obfuscated_naming.rs`, regenerate on filesystem collision).
- Each obfuscated memo file's content: independently AES-256-GCM encrypted, fresh random 12-byte IV
  per save, same password-derived key as the lock blob. Exact on-disk byte layout (e.g.
  `[IV][ciphertext][GCM tag]`, with or without a small version header) — `developer-lite`'s call,
  document the chosen layout inline in `crypto/lock_blob.rs`'s doc comment once decided; requirements
  are: IV never reused, key never re-derived per save (derive once per session, reuse for all saves in
  that session), plaintext never touches disk.
- KDF params read from the file at decrypt time (not hardcoded) so future param increases don't break
  old locked tabs. New locks: `N=32768, r=8, p=1, keyLen=32` (~32MB memory cost).
- Salt: stable for the life of a password, regenerated only on password change. IV: always fresh per
  write, for both the lock blob and every memo body write.
- `crypto/kdf.rs`'s scrypt call **must be async** (e.g. `tokio::task::spawn_blocking` wrapping the
  synchronous `scrypt` crate call, or an async-native scrypt implementation) — never block the Tauri
  command-dispatch thread during unlock (`[detailed-spec §3.4]`).

## 3. Commands

### `tab_lock(tab_id: String, password: String, hint: Option<String>) -> AppResult<TabInfo>`
Elaborates the "Locking a tab" flow (`[detailed-spec §5.9]`, revised sequence incorporating the
in-progress marker described in [`CRASH-RECOVERY.md`](CRASH-RECOVERY.md)):
1. Password (+ optional hint) is provided by the frontend as part of this call, entered in a setup
   dialog before invoking — this command does not itself prompt.
2. If any memo in this tab (or descendants) is currently open with unsaved changes, the **frontend**
   must force a save-confirm (auto-save-and-close) before calling this command at all
   (`[spec §8]`) — the backend command assumes a clean state and does not itself orchestrate
   editor-close UI, but should defensively verify no conflicting in-flight save is racing (e.g. by
   checking `watch::active_memo` isn't set to a memo in this tab, returning a distinct error if it is,
   so the frontend's ordering bug — if any — fails loudly rather than corrupting data).
3. Defensively check for a leftover in-progress marker (see below) from a previous, never-recovered
   crash before starting a *new* lock attempt; if one is found, refuse with a dedicated error/state
   telling the frontend to run the recovery flow first instead (in normal operation this is
   unreachable, since `tree_open` — called whenever this tab was last selected — already runs that
   check before the user could reach the "Lock tab" menu item again; this is defense-in-depth, not the
   primary trigger path).
4. Read current unlocked tree (`MetaTreeNode[]`, real names) from `.tabtreenote.json`.
5. Generate a fresh KDF salt; derive the encryption key once via async scrypt.
6. **Write the in-progress marker** (see [`CRASH-RECOVERY.md`](CRASH-RECOVERY.md)): compute the **full**
   planned `fileMap` up front — a fresh 128-bit hex token for *every* `.md` file directly under the tab
   folder, none generated lazily during the loop below — and atomic-write it (reusing
   `persistence-fs/PLAN.md` §3's shared `atomic_write` function, which already tolerates "no prior
   target," so no separate write path is needed for this brand-new file) to
   `.tabtreenote.lock-inprogress`, self-write-registered like any other self-caused FS event. See below
   for the exact marker schema.
7. For each entry in the marker's `fileMap`, in any order: self-write-register source+destination,
   encrypt content (fresh IV), rename+replace under the token filename (no extension). Use
   `with_retry` (`persistence-fs/PLAN.md` §7) for the rename in case of a transient Windows lock.
8. Build the lock payload (`realTabName`, tree with real names, the **same** `fileMap` written to the
   marker in step 6), encrypt it, atomic-write into `.tabtreenote.json`'s `lock` field (replacing
   `tree`).
9. **Delete the in-progress marker** (self-write-registered, `with_retry`) — only after step 8 has
   succeeded. This is the sole success signal the recovery check below relies on.
10. `settings_set`: `AppSettings.tabs[i].locked = true`, `name` replaced with a placeholder. **Confirmed
    placeholder value: the 🔒 glyph alone, no accompanying text** (`[detailed-spec §3.2]`,
    `DECISIONS.md` "Tab-Lock Placeholder Display Name: 🔒 Glyph Alone" — no longer a UI-copy decision
    left open).
11. `search/index.rs::purge_tab(tab_id)`.
12. Stop/remove this tab's file watcher (`persistence-fs/PLAN.md` §5 — a locked tab's obfuscated files
    are not meaningfully watchable/relevant while no session is active).

If the process is interrupted anywhere in steps 6–9, the marker is left on disk. The full recovery flow
(marker file format, the `tree_open`-triggered detection point, per-entry state check, finish-vs-rollback
tie-break rule, and the recovery dialog) is specified separately in
[`CRASH-RECOVERY.md`](CRASH-RECOVERY.md) — read it alongside this file before implementing `tab_lock`.

### `tab_unlock(tab_id: String, password: String) -> AppResult<TabInfo>`
1. Read `.tabtreenote.json`'s `lock` field; if absent (tab isn't actually locked), `Unknown`/defensive
   error (shouldn't be reachable from a correct frontend).
2. Derive key via scrypt(password, stored salt, stored KDF params); attempt AES-GCM decrypt of
   `ciphertextB64` using stored IV/tag.
3. Auth-tag verification failure → `DecryptFailed` ("password incorrect"), **no content of any kind
   returned**, no partial payload leaked, no distinguishing-error-message side channel (a wrong
   password and a corrupted blob should ideally be indistinguishable to the caller, though `spec.md`
   doesn't mandate constant-time comparison beyond what AES-GCM's own tag check already provides).
4. On success: hold the decrypted lock payload (real tree, real tab name, file map) in
   `store/lock_session_store.rs` as this tab's active session. Individual memo bodies are **not**
   pre-decrypted — decrypted on demand when `editor_load` (`editor-save/PLAN.md`) opens one, using the
   session's key.
5. `settings_set`: display `name` reverts to `realTabName` in the frontend's view (whether this writes
   back to `AppSettings.tabs[i].name` or is purely an in-memory display substitution layered on top of
   the still-placeholder settings value is implementation discretion — either is consistent with
   `[spec §4.1 pt.3]`'s "real name exists only in blob"; **not** persisting the real name into
   `AppSettings.tabs[i].name` while a session is active is the safer default, since that file is not
   itself encrypted).
6. Start this tab's file watcher again (obfuscated filenames still churn as re-encryption-on-save
   happens, self-write registry handles this normally).
7. Return updated `TabInfo` with `locked: true` still (locked state doesn't change — a session is not
   "unlocked" in the settings-cache sense, it's a temporary access grant) but the frontend now has the
   real tree/name to render via a **separate** in-session data channel (e.g. `tree_open`'s response for
   this tab now includes real data because the session store supplies it, per `memo-tree/PLAN.md` §2
   step 1's branch).

### `tab_end_session(tab_id: String) -> AppResult<()>`
- Triggered by the frontend the moment the user switches away from this tab (selecting a different
  tab) — **no idle timeout, no manual "lock now" button** distinct from tab-switching
  (`[spec §4.1 pt.5]`, `DECISIONS.md` "Tab Lock Session Lifetime").
- Nothing further needs to happen on disk (everything was already re-encrypted incrementally during
  the session per step 4 of the "while session active" behavior below) — this command just discards
  the in-memory plaintext session state and stops treating this tab's memos as open-able without a
  fresh `tab_unlock` call.
- Also called (or its effect achieved inline) if the app exits while a session is active — ensure any
  final pending re-encrypted save has completed first (same "must complete before process termination"
  rule as `editor-save/PLAN.md` §6's exit-trigger row).

### While a Session Is Active
Every mutating operation within the tab (rename, reorder, create, delete, edit-and-save) that would
normally hit `memo-tree/PLAN.md`'s or `editor-save/PLAN.md`'s plain-file/plain-metadata paths instead:
(a) operates on the in-memory decrypted session state (tree, file map) for structure changes, and (b)
re-encrypts and atomically re-saves the affected memo body (fresh IV) and/or the lock blob (fresh IV)
as needed — same save-trigger cadence as an unlocked tab (`[detailed-spec §5.9]` step 4). This means
`memo_create`/`memo_rename`/`memo_delete`/`memo_move`/`editor_save` (as implemented in their respective
plans) must each branch on "is this tab's session active" and, if so, call into
`crypto/lock_blob.rs`/`store/lock_session_store.rs` functions instead of the plain-file functions —
this branch point should be centralized (e.g. a `TabAccess` enum/trait the tree/editor commands consult
rather than each one independently checking session state) to avoid the two code paths silently
diverging over time.

## 4. Deletion Restriction

`tab_remove` (`tab-management/PLAN.md`) rejects with `TabLocked` while the tab is locked with no active
session (`[spec §4.1 pt.7]`). With an active session, `tab_remove` behaves like removing any other tab
(just unregisters — the still-encrypted files on disk are untouched either way, consistent with
`DECISIONS.md`'s tab-deletion-is-unregister-only rule applying uniformly).

## 5. Cross-Tab DND Involving a Locked Tab

Elaborates `[spec §4.1 pt.8]`, `[detailed-spec §5.9]`'s DND section — the actual DND mechanics
(selection, drag start, drop target detection) live in `memo-tree/PLAN.md` §3's `memo_move`; this
section specifies the **locked-tab-specific branch** that command must call into:

- **Dragging from a locked (no-session) tab**: not applicable — nothing is visible to drag (the tab's
  tree isn't rendered while locked-and-no-session, per `memo-tree/PLAN.md` §2 step 1).
- **Dropping onto a locked (no-session) tab**: show the password dialog **at the moment of drop**
  (frontend, shared dialog component). On successful entry, this is functionally an inline
  `tab_unlock` call followed immediately by an intra-session memo addition (encrypt the incoming body,
  add a `fileMap` entry, insert into the tree, re-save the lock blob) — **all in one drop interaction**,
  no need to leave/reselect the drag-source tab (`[spec §4.1 pt.8]`, this is exactly why permanent
  filename-obfuscation + in-place-session design was chosen over "unlock first, then drag," per that
  same spec passage's explanation). On wrong password (`DecryptFailed`), the drop is aborted — the
  source memo is untouched, no partial state.
- **Dropping onto a locked tab with an active session**: proceeds exactly like dropping onto a normal
  unlocked tab (encrypt on the way in) — no password prompt needed, session is already live.
- **Dragging from a tab with an active unlock session, dropping onto a normal unlocked tab**: the memo
  is decrypted at the moment of drop and lands as plaintext `.md` in the target (`[spec §4.1 pt.8]`,
  explicitly **not** a one-way vault — tab-level lock means no risk of protected/unprotected content
  being adjacent in the same tree, unlike the legacy per-subtree node-lock model).

## 6. Full-Text Search Exclusion

Already covered as the authoritative source in `search-index/PLAN.md` §1 — restated here only as a
cross-reference: locking purges the tab's index entries; unlocking (session start) never re-adds them,
by design (`[spec §2.4, §4.1 pt.6]`).

## 7. Portability

Per `[spec §6]`, a full folder copy of a locked tab (obfuscated `.md` files + `.tabtreenote.json` with
its `lock` blob), moved to a different machine/path and re-registered as a tab there, must still unlock
correctly with the same password — since the salt/KDF-params/IV/ciphertext are all self-contained
within the copied files, this should work without any special-casing as long as `tab_register_existing`
(`tab-management/PLAN.md`) correctly recognizes an existing locked `.tabtreenote.json` (i.e. one with a
`lock` field instead of `tree`) and registers it with `locked: true` in the new machine's
`AppSettings.tabs`, rather than attempting to treat it as an unlocked tab's metadata. This is a
concrete acceptance check `reviewer-lite`/`developer-lite` should verify during implementation
(`[detailed-spec §8]`).

## Implementation Notes (resolutions of this plan's discretion points)

- **Obfuscated memo-body on-disk layout** (§2's "developer-lite's call"): `[12-byte IV][ciphertext][16-byte
  GCM tag]`, concatenated directly with no version header — documented inline in
  `crypto/lock_blob.rs`'s doc comment. No header is needed because the containing tab's
  `lock.formatVersion` already versions the whole locked tab's on-disk shape.
- **`tab_lock` step 7's "rename+replace" ordering, resolved as encrypt-into-a-new-token-file
  first, then delete the original** (not a raw `fs::rename` followed by in-place re-encryption):
  this guarantees a crash can never leave a `CRASH-RECOVERY.md`-`Renamed`-classified entry with
  still-plaintext content, since `Renamed` is only reachable once the encrypted token file already
  exists — the narrow crash window where both the real and token file transiently coexist maps
  exactly onto `CRASH-RECOVERY.md`'s own explicitly-acknowledged "both" anomalous case, rather than
  an unaccounted-for third state that would silently break the Restore flow's "decrypt every
  `Renamed` entry" assumption.
- **Crash-recovery Finish/Restore's tree-source discrimination**: since `.tabtreenote.json`'s two
  possible shapes during an interrupted lock (`tree` field still present vs. already-replaced by
  `lock`) map to "step 8 never ran" vs. "step 8 already succeeded" respectively, both recovery
  actions branch on which field is present rather than trusting `renamed_count` alone (which can't
  by itself distinguish "all renamed, lock blob not yet written" from "all renamed, lock blob
  already written" — the crash window between those is real, if narrow). Restore's "already
  succeeded" branch decrypts the already-written lock blob (with the now-verified key) to recover
  its exact tree rather than falling back to a flat rebuild, so structure/labels survive.
- **§3 step 11's search-index seam**: implemented as a local no-op stub
  (`touch_search_index_purge_tab` in `commands/tab_lock.rs`) matching the exact convention already
  established by `commands/tree.rs`'s/`commands/editor.rs`'s own `touch_search_index_*` stubs for
  the same not-yet-implemented `search-index/PLAN.md` dependency — confirmed applicable to this
  call site by `designer-lite` during this feature's implementation (a purely in-memory index
  side-effect among 12 steps, not a persisted invariant), per this file's own re-confirmation
  policy for deviations.
- **Cross-tab move combos** (§5): `commands/tree.rs`'s `memo_move` now dispatches on both
  endpoints' resolved `TabAccess` (Plain/Session × Plain/Session), covering the pre-existing
  Plain↔Plain path plus three new ones in `commands/tab_lock.rs`
  (`move_cross_tab_plain_to_session`/`_session_to_plain`/`_session_to_session`) — the last of
  these also covers two *different* locked tabs each with their own independent active session,
  which §5's prose doesn't spell out as its own bullet but which the same decrypt-then-re-encrypt
  mechanics handle uniformly.
- **`tab_end_session` multi-session model**: the backend (`LockSessionStore`) and frontend
  (`state/useTabLock.ts`) both support more than one tab holding an active session at once — e.g.
  a cross-tab drop-onto-a-locked-tab unlocks that tab in place without leaving the drag-source tab
  (§5), so its session can outlive the drag-source tab remaining active. The frontend ends a given
  tab's session the moment the user switches *away* from that specific tab (not a single
  global "the one active session"), matching §3's per-tab wording literally.
- **`tab_lock` step 2's defensive check, fixed post-review**: `reviewer-lite` found live that
  nothing ever called `ActiveMemoTracker::clear()`, so this defensive check stayed permanently
  tripped once any memo in a tab had ever been opened — breaking `tab_lock` on the tab the user
  just had open, the single most common case. Fixed by adding `editor_close`
  (`commands/editor.rs`), called on every real "memo closed" transition
  (`state/useEditor.ts`'s switch-away cleanup) and explicitly awaited by `App.tsx`'s
  `beforeLockTab` right before invoking `tab_lock`, so that call is guaranteed to see a cleared
  tracker rather than racing a React effect.

## Open Items

None. The previously-blocked crash-mid-lock recovery question (formerly `docs/QA.md` item Q1) has been
resolved by the user (Option B) and landed in `docs/DECISIONS.md`, `docs/detailed-spec.md` §5.9, and
this feature's companion [`CRASH-RECOVERY.md`](CRASH-RECOVERY.md) (marker file format, trigger point,
finish-vs-rollback tie-break rule). Every flow in this file and its companion — `tab_lock` (including
crash-mid-lock recovery), `tab_unlock`, session-active behavior, deletion restriction (§4), cross-tab
DND (§5), search exclusion (§6), and portability (§7) — is fully specified and now implemented.
