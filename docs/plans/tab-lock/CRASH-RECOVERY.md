# PLAN: Tab Lock — Crash-Mid-Lock Recovery

> Status: implemented, together with [`PLAN.md`](PLAN.md) as a single unit of work (the marker
> write/delete calls this file specifies are inline in `PLAN.md` §3's `tab_lock` steps 6-9).
> Marker read/write/classify/auto-rollback: `src-tauri/src/commands/tab_lock.rs`'s
> `LockInProgressMarker`/`load_marker`/`write_marker`/`delete_marker`/`classify_entry`/
> `classify_marker`/`check_marker_on_tree_open`. Trigger point: wired into
> `src-tauri/src/commands/tree.rs`'s `tree_open` step 0. Finish/Restore: `tab_lock_recovery_finish`/
> `tab_lock_recovery_restore` commands in `commands/tab_lock.rs`, backed by the shared
> `derive_and_verify_recovery_key` password-verification helper. Frontend recovery dialog:
> `src/components/dialogs/RecoveryDialog.tsx`, shown by `src/components/Tree/TreeView.tsx` when
> `tree_open` reports `recoveryPending: true`. Companion to [`PLAN.md`](PLAN.md) (read that file
> first) — split into its own file because it is a large, self-contained sub-algorithm only relevant to
> the crash-recovery path, not to `tab_lock`'s/`tab_unlock`'s ordinary happy-path flow. Elaborates
> `[detailed-spec §5.9]`'s "Recovery on tab-folder access" subsection and `DECISIONS.md`'s Option-B
> resolution of former `QA.md` item Q1. Resolves the three items `detailed-spec.md` explicitly left to
> this plan: marker format, trigger point, and finish-vs-rollback tie-break rule.

Sources: `docs/detailed-spec.md` §5.9; `docs/DECISIONS.md` "Tab Lock: Crash-Mid-Lock Recovery via
Explicit Marker + Resume/Rollback".

## Index

1. Context
2. Marker File Format
3. Trigger Point
4. Per-Entry State Check and Finish-vs-Rollback Tie-Break Rule
5. Frontend
6. Implementation Notes
7. Image Folders (GitHub Issue #272)
8. Plaintext Residue (GitHub Issue #276)
9. Image Folders on Permanent Unlock (GitHub Issue #273)
10. Image Folders on Cross-Tab Move/Copy (GitHub Issue #262)
11. Open Items

---

## Context

If the process is interrupted anywhere in `PLAN.md` §3's `tab_lock` steps 6–9 (after the in-progress
marker is written, before it is deleted), the marker is left on disk. This file specifies how the app
resolves that the next time the tab's folder is accessed, before treating the tab as normally
lockable/unlockable again.

## Marker File Format

`<tab folder>/.tabtreenote.lock-inprogress` (hidden, sibling to `.tabtreenote.json`), plain JSON (not
itself encrypted — see rationale below):

```jsonc
{
  "formatVersion": 1,
  "kdf": { "algo": "scrypt", "N": 32768, "r": 8, "p": 1, "keyLen": 32, "saltB64": "..." },
  "fileMap": [
    { "memoName": "Diary 2026-07", "token": "9f2c7a1e4b3d0f8c6a5e2d1b0c9a8f7e" }
  ]
}
```

- `kdf`/`saltB64` are the **same** salt+params that will end up in the eventual lock blob (`PLAN.md`
  §2) — generated once at `tab_lock` step 5, reused for both the marker and (if the operation
  completes) the final `lock.kdf` field. This is what lets recovery re-derive the *same* key from a
  re-entered password without needing anything else persisted, since the session key itself is never
  persisted (`[spec §5.10]`, security invariant — the marker holds only public KDF parameters, never a
  key).
- `fileMap` is the complete, final planned mapping (every `.md` file that existed when locking began),
  not a partial/incremental log — this is a **correctness requirement**, not a style choice
  (`[detailed-spec §5.9]`): it's what lets recovery determine each entry's state deterministically from
  on-disk evidence alone ("Per-Entry State Check" below), with no separate "how far did the loop get"
  counter to keep in sync.
- **Why the marker is not itself encrypted**: at the point the marker is written (before any renaming
  has happened), every listed `memoName` is still plainly visible as an actual `.md` filename in the
  same folder — the marker's plaintext content reveals nothing an onlooker couldn't already see by
  listing the folder. There is no privacy regression versus the pre-lock state, and encrypting the
  marker would create a chicken-and-egg problem (recovery would need a password just to know which
  files the marker even refers to, before it could offer the user anything to decide between).
- Written/deleted via the same `atomic_write`/self-write-registry/`with_retry` mechanisms as everything
  else in this app (`persistence-fs/PLAN.md` §3, §4, §7) — no special-cased I/O path.

## Trigger Point

**Lazy, per-tab, on `tree_open`** (`memo-tree/PLAN.md` §2) — not a full-startup scan across all
registered tabs. `tree_open`'s very first step, *before* its existing §3.5 reconciliation logic, checks
whether `.tabtreenote.lock-inprogress` exists in the target tab's folder:

- **Absent** → proceed with `tree_open` exactly as already specified in `memo-tree/PLAN.md` §2 (no
  change to that flow).
- **Present** → do **not** run normal reconciliation. Instead return a distinct state to the frontend
  (e.g. `TreeSnapshot { nodes: [], tab_name, locked: false, recovery_pending: true }` — exact field
  addition is this plan's call, not a new command) so the frontend shows the recovery dialog
  ("Frontend" below) instead of the tab's normal tree/empty-state view. This tab still appears as an
  ordinary **unlocked** tab in the tab bar in the meantime (`AppSettings.tabs[i].locked` was never set
  to `true` — that only happens at the very end of a *successful* `tab_lock`, `PLAN.md` §3 step 10 — so
  there is nothing inconsistent about the tab bar continuing to show its real name until recovery
  resolves).
- This reuses the existing per-tab-open code path (cheap, proportional to "was this specific tab just
  selected," per `DECISIONS.md`'s cost-justification writeup) rather than adding a scan whose cost
  scales with the total number of registered tabs or memos.
- `tab_lock` (`PLAN.md` §3 step 3) also defensively re-checks this at its own entry point, but the
  primary and expected trigger is always this `tree_open` path, since a tab must be selected (and
  therefore `tree_open`ed) before the user could reach the "Lock tab" menu action on it again.

## Per-Entry State Check and Finish-vs-Rollback Tie-Break Rule

On detecting a marker, before presenting anything to the user, classify every `{ memoName, token }`
entry in its `fileMap` by on-disk evidence (`[detailed-spec §5.9]`'s per-entry state check):
`renamed` (token file exists, `<memoName>.md` does not) or `not_renamed` (`<memoName>.md` exists, token
file does not). (The neither/both anomalous cases from `[detailed-spec §5.9]` are surfaced as a
distinct error state to the user rather than auto-resolved — not expected outside deliberate external
interference during the crash window, per that section's own note.)

- **Zero entries `renamed`** (crash happened at or immediately after the marker write, before any
  rename began): **automatic rollback, no password needed** — nothing was ever touched, so recovery
  simply deletes the marker and the tab proceeds to normal `tree_open` reconciliation immediately, with
  no dialog shown at all.
- **At least one entry `renamed`** (partial or fully complete rename loop): a password is required
  either way — finishing needs the key to encrypt the remaining `not_renamed` entries and build the
  lock blob; rolling back needs the key to decrypt the already-`renamed` entries back to plaintext.
  Show a dedicated **"Resume interrupted lock"** dialog (shared custom dialog component, distinct copy
  from the ordinary unlock prompt, but reusing the same password-input UI), explaining that a previous
  lock attempt on this tab did not finish, with two actions: **"Finish Locking"** and **"Restore
  (undo lock attempt)"**. Both require the password field to be filled before they can be invoked.
  - On submit, derive the key from the marker's `kdf`/`saltB64` + the entered password, then **verify**
    it by attempting an AES-GCM decrypt of exactly one already-`renamed` entry's content (at least one
    is guaranteed to exist in this branch). Auth-tag failure → `DecryptFailed` ("password incorrect"),
    dialog stays open for retry, **no file is touched** by a failed verification attempt.
  - **Tie-break rule for which action is pre-selected as the default button** (UI default only, not a
    forced choice — user can still pick the other one): default to **"Finish Locking"** when `renamed`
    entries are a strict majority of `fileMap.len()`; default to **"Restore"** when they are a strict
    minority or an exact tie.
  - **Finish**: with the verified key, rename+encrypt every remaining `not_renamed` entry (`PLAN.md` §3
    step 7 continued), then resume `PLAN.md` §3 steps 8–12 (build+write lock blob, delete marker,
    update `AppSettings`, purge index, stop-then-restart the watcher in its locked configuration).
  - **Restore**: with the verified key, for every `renamed` entry: decrypt content, rename back to
    `<memoName>.md` (self-write-registered, `with_retry`). Once every entry is back to plaintext `.md`
    form, delete the marker. No `lock` blob is ever written; `AppSettings.tabs[i].locked` was never
    changed, so no settings rollback is needed either — the tab is exactly as if `tab_lock` had never
    been called.
- **Idempotency**: both Finish and Restore recompute the per-entry classification fresh each time they
  run, so if recovery itself is interrupted again mid-way, re-opening the tab re-triggers this same
  recovery flow against the now-smaller set of remaining entries — safe to retry any number of times
  (`[detailed-spec §5.9]`'s explicit idempotency requirement).

## Frontend

`components/dialogs/`'s shared dialog component, in a "resume interrupted lock" variant (previous
section). This dialog is **not** one of the Esc-disabled exceptions (`[detailed-spec §6.5]`) in the
sense that Esc may close it, but closing it without choosing an action leaves the marker in place — the
tab remains unusable as a normal tab (still shown as `recovery_pending: true`) until the user completes
recovery on a later visit to that tab; this is acceptable since nothing is lost or further damaged by
deferring the choice, unlike the external-change-conflict dialog's Esc-disabled rationale (which
protects against silently discarding a choice about live data).

## Implementation Notes

- The "zero entries `renamed`" automatic-rollback branch is folded directly into
  `check_marker_on_tree_open` (called from `tree_open` step 0) rather than being a separate
  command — it deletes the marker and returns a `NoMarker` sentinel so `tree_open` falls straight
  through to its normal reconciliation flow in the same call, with no dialog round-trip.
- `tab_lock_recovery_finish`/`tab_lock_recovery_restore` both first check whether
  `.tabtreenote.json` already has a `lock` field (meaning `PLAN.md` §3 step 8 already succeeded
  before the interruption) rather than trusting the per-entry `renamed`/`not_renamed` counts alone
  to infer that — see `PLAN.md`'s own "Implementation Notes" for why that distinction matters.

## Image Folders (GitHub Issue #272)

Locking a plain tab also converts each memo's `<memo_name>/` image folder (#139; only real
directories passing the #263 guard `existing_memo_image_folder`) to a fresh random name, so folder
names no longer leak memo names. Memos without such a folder, and pre-#139 flat images directly
under the tab folder, are untouched.

Marker: gains an optional `imageFolders: [{ memoName, folder }]` field (the planned map, written
before anything is renamed; omitted when empty; a pre-#272 marker reads as an empty map —
`formatVersion` unchanged). The same map is copied into the lock payload's `imageFolders` (#271).

Order inside `tab_lock` (`lock_folder_files`):

| Step | Action | Crash after this step → recovery |
|---|---|---|
| 6 | Plan `fileMap` + image-folder map, write marker | Zero progress: `tree_open` silently deletes the marker (nothing renamed). |
| 6b | Rename every planned `<memo>/` → `<random>/` (plaintext bodies untouched) | Zero `renamed` entries: `tree_open`'s automatic rollback (no password) renames every folder found at its random name back to `<memo>/`, then deletes the marker. The plaintext bodies were never rewritten, so the tab is byte-identical to before. If a folder cannot be renamed back, the marker is kept and the rollback retried on the next `tree_open` (the tab still opens as a plain tab). |
| 7 | Per memo: rewrite `<memo>/…` image links → `<random>/…` **in memory** and encrypt into the token file, then delete the plaintext | `renamed > 0` → the existing Finish/Restore dialog. **Finish**: idempotently completes step 6b, encrypts the remaining memos with the same in-memory rewrite, writes the lock blob with the marker's `imageFolders` (fixes #271's empty map). **Restore**: decrypts each encrypted memo, rewrites `<random>/…` back to `<memo>/…`, renames every folder back, restores the plain tree. |
| 8 | Write lock blob (with `imageFolders`) | Finish: no-op (the blob already has the map). Restore: as above, tree taken from the blob. |
| 9 | Delete marker | — |

Partial failure (user decision on #272): if any step-6b rename fails (e.g. a file inside the folder is
open in another program, or the destination is occupied), `tab_lock` renames the already-renamed
folders back, deletes the marker, and returns `IMAGE_FOLDER_RENAME_FAILED` (shown in the lock dialog).
It never locks with a folder still named after its memo. If even that rollback fails, the marker is
kept and `tree_open` retries the rollback as in the step-6b row.

Recovery Restore returns `IMAGE_FOLDER_RENAME_FAILED` (marker kept) if memos/metadata were restored
but some folder could not be renamed back; the next `tree_open` finishes the folder rollback.

Known limitations: link rewriting follows #261's rules (UTF-8 bodies only; only the memo's own
`![..](<memo>/..)` links — a link from another memo's body into this memo's folder is not
rewritten; bare destinations containing spaces are not parsed, see #266). On Restore of an
already-encrypted memo, a percent-encoded link comes back in raw form (`日記/003.png` instead of
`%E6%97%A5%E8%A8%98/003.png`; same target), because the random hex name is identical in both forms.
A pre-#272 build ignores the marker's `imageFolders` during its own recovery.

## Plaintext Residue (GitHub Issue #276)

`atomic_write` keeps a single-generation `.bak`, so a plain tab normally holds `<memo>.md.bak`
(previous body) and `.tabtreenote.json.bak` (plain meta, every memo name). Locking used to leave both,
plus `write_lock_field`'s own `atomic_write` rotated the plain meta into `.bak` at step 8. User
decision on #276: **permanently delete** them (never trash — the Recycle Bin would keep the
plaintext); do not encrypt-and-keep (session saves already produce encrypted `<token>.bak`).

`remove_plaintext_residue` (`commands/tab_lock.rs`) deletes, each guarded by
`fs/path_safety.rs`'s `existing_direct_child_file` (direct child, regular file, no link):

- `<memoName>.md.bak` for every `fileMap` entry;
- `.tabtreenote.json.bak` unless it parses as the encrypted meta shape (`lock` set, no `tree`);
- `.tabtreenote.lock-inprogress.bak` (a marker backup lists every memo name);
- at lock time only: stray `atomic_write` temp files (`.ttn-tmp-*`), which may hold a plaintext body
  from an earlier interrupted save.

Encrypted `<token>` / `<token>.bak` never match. Best-effort: failures are logged only.

| When | Point | Crash → |
|---|---|---|
| `tab_lock` | Step 8b: after the lock blob is written (step 8), before the marker delete (step 9) | Marker still present → recovery. **Finish** runs the same cleanup (both the "blob already written" early return and the normal path). **Restore** does not use any `.bak`: the tree comes from the lock blob (or the still-plain meta if step 8 never ran), bodies from the token files. |
| Recovery Finish | After its own lock-blob write / when the blob already exists | Re-run on the next Finish. |
| `tab_unlock` (legacy cleanup) | After the password is verified, before the session starts | Tabs locked by an older build, or whose cleanup failed, are cleaned on the next unlock using the decrypted `fileMap` names. Temp files are not touched here (names cannot be matched to this tab). |

Deleting before step 9 (not after) keeps the cleanup inside the marker-protected window, so an
interruption is always repaired by Finish rather than depending only on the next unlock. Recovery
Restore keeps the plain `.md.bak` files (the tab goes back to plain, so they are ordinary backups again).

Audit (no change needed): session writes only `atomic_write` encrypted `<token>` files and the locked
`.tabtreenote.json`, so their `.bak` rotations hold ciphertext only; the marker is written once
(no `.bak`) and removed with `remove_file`. Known limitation: a memo renamed inside a session after an
older build left `<old name>.md.bak` is not matched by the current `fileMap`, so that file stays.

## Image Folders on Permanent Unlock (GitHub Issue #273)

Permanent unlock (`tab_permanent_unlock`, `tab-lock-permanent-unlock/PLAN.md` §5) renames each
lock payload `imageFolders` entry (#271 map; random folders from #272 conversion or from #271
session pastes) back from `<random>/` to `<memo_name>/` and rewrites that memo's own image links
`<random>/…` → `<memo_name>/…` (`util/image_link_rewrite.rs`, same rules and limits as #272).
The map disappears with the lock blob (plain meta has none). Legacy flat images in the tab root
are untouched. Helpers in `commands/tab_lock.rs`: `plan_unlock_image_folders`,
`permanent_unlock_restore_all` (forward + recovery Finish), `permanent_unlock_relock_files`
(recovery Restore lock), `count_image_folders_left_random`; renames reuse #272's
`rename_image_folder` (#263 guard, never overwrites/merges).

Plan (step 6): a map entry is planned only if its memo is in `fileMap` with the body still
encrypted, its `<random>/` is a real folder, and `<memo_name>/` is free and valid for the #263
guard. The planned **random names only** go into the unlock marker as `imageFolders: [..]`
(no memo names, so the plaintext marker leaks nothing; absent = pre-#273 marker = empty plan).

Left at the random name, with links unchanged (non-fatal; user decision on #273):
- `<memo_name>/` already exists (e.g. a plain-era folder of a tab locked by a #271-only build),
  or the name fails the guard — never planned;
- the rename fails (a file inside is open without delete sharing, a process has its cwd inside
  the folder) — unlock continues.

Links are rewritten for a memo only when its planned folder is at `<memo_name>/` at the moment its
body is restored. The command returns `imageFoldersLeftRandom` (planned-or-not map entries whose
random folder still exists); the frontend (`state/useTabLock.ts`) shows a warning toast
(`toast.permanentUnlockImageFoldersLeftRandomTemplate`) when it is > 0, also after recovery Finish.

Order and crash points (no plaintext `.md` ever points at a missing folder: every folder rename
happens while that memo's body is still an encrypted token, and the body is written once, with
links matching the folder's on-disk name):

| Step | Action | Crash after this step → recovery |
|---|---|---|
| 6 | Plan, write unlock marker with `imageFolders` | No `.md`, every planned random folder still present → `tree_open` deletes the marker silently (tab stays locked, unchanged). |
| 6b | Rename planned `<random>/` → `<memo_name>/` (failures skipped) | No `.md` but a planned random folder is missing → **not** auto-deleted; recovery dialog (password). **Finish**: re-runs 6b for still-encrypted memos, then 7-8. **Restore lock**: renames planned folders found at `<memo_name>/` back to random. |
| 7 | Per memo: decrypt, rewrite links in memory iff its planned folder is at `<memo_name>/`, write `<memo_name>.md`, remove token | Recovery dialog. **Finish**: 6b only for memos still encrypted (a restored memo's folder is never renamed afterwards, so its links stay valid), then the rest of 7, 8. **Restore lock**: re-encrypt each restored body (links `<memo_name>/` → `<random>/` in memory iff its planned folder is at `<memo_name>/`), remove the `.md`, **then** rename those folders back to random. |
| 8 | Write plain meta (map dropped) | Existing "already finished" auto-resolve (all folder work was done before step 8). |
| 9 | Delete marker | — |

Restore lock: if a folder cannot be renamed back, it returns `IMAGE_FOLDER_RENAME_FAILED` and keeps
the marker; since that folder is not at its random name, the next `tree_open` keeps recovery
pending (even with zero `.md`), and a retry completes it.

#276 still holds: no new plaintext file is written (link rewrites are in memory before the single
`.md`/token write; restored `.md` files are new, so no `.md.bak`). Until the unlock completes,
renamed folders expose memo names — same exposure class as the already-restored `.md` files in
that window; Restore lock renames them back. Known limitations: those of #272 (UTF-8 bodies, only
the memo's own links, #266 bare-space links); a percent-encoded link comes back in raw form.

## Image Folders on Cross-Tab Move/Copy (GitHub Issue #262)

Cross-tab move/copy (`memo_move` `OtherTab`) carries each moved/copied memo's image folder along,
for all four combinations (plain→plain is included here so all four live in one place). No marker:
the operation is short, and the order below keeps the interruption windows narrow and recoverable by
hand. Pre-#139 flat images in the tab root are **not** carried (it would need a body-reference scan
of every other memo in the source tab, decrypting a locked one — too much added scope; #262 Q2).

Source folder `S`: plain source → `<memo_name>/` if a real folder (#263 guard); locked source → the
memo's `imageFolders` entry if that folder is real. Destination folder `D`: plain destination →
`<memo_name>/`; locked destination → a fresh random name (`generate_unique_image_folder`) added to
the destination payload's `imageFolders` (a stale same-name entry is replaced). Links `S/…` → `D/…`
are rewritten in memory (`rewrite_image_links_in_bytes`, #261/#272 rules) before the body is written
or encrypted; plain→plain needs no rewrite. Move = rename (`with_retry`, cross-volume fallback
copy + delete; a failed delete of the source after a complete copy is only logged); copy = recursive
copy (links are not followed). Locked source, move: `cleanup_session_move_source` drops the entry.

Pre-flight (before anything is touched; the operation is rejected, nothing changes): plain
destination where `<memo_name>/` already exists (file/folder/link) → `NAME_DUPLICATE`; where
`<memo_name>` fails the #263 guard → `NAME_INVALID`. Locked destination: random names never collide.

Order — plain destination (plain→plain, locked→plain): **folders first**.

| Step | Action | Crash after this step |
|---|---|---|
| 1 | Pre-flight (memo names + image folders) | Nothing changed. |
| 2 | Transfer every `S` → `<memo_name>/` in the target tab. Failure → already-transferred folders transferred back, `IMAGE_FOLDER_RENAME_FAILED`, nothing else changed. | Move: the source memo's images show broken; the folder sits in the target tab under the memo's own name (findable by hand). Copy: harmless extra folder. |
| 3 | Write/move bodies (links rewritten), save target meta, then (move) source cleanup. A body failure transfers the folders of the not-yet-moved memos back (existing partial-failure class otherwise). | Existing cross-tab crash class (duplicate or reconciled memo); images match the body's tab. |

Order — locked destination (plain→locked, locked→locked): **target commit first** (a random folder
orphaned in a locked tab is not findable by hand, so the folder only moves once the destination
payload already references it).

| Step | Action | Crash after this step |
|---|---|---|
| 1 | Encrypt bodies (links → `D`) into new tokens, add `imageFolders` entries, save target lock blob (rejects on memo-name collision before writing) | Target memo shows broken images; source memo complete (duplicate). |
| 2 | Transfer every `S` → `D`. Failure → already-transferred folders transferred back, the target commit undone (`cleanup_session_move_source` on the target: tokens, tree, entries), `IMAGE_FOLDER_RENAME_FAILED`. | Move: target complete; source memo is a duplicate with broken images. Copy: done. |
| 3 | (Move) source cleanup (`.md` delete + meta, or session cleanup) | — |

In every window at least one complete copy of each memo (body + reachable images) exists for a locked
destination; for a plain destination the images are always under a folder named after the memo.
Known limitations: those of #272 (UTF-8 bodies, only the memo's own links, #266 bare-space links); a
link from another memo into a moved memo's folder is not rewritten.

## Open Items

None. Fully specified and implemented alongside `PLAN.md`.
