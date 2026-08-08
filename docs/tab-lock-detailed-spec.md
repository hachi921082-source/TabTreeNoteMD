# Tab Lock — Permanent Unlock: Detailed Specification

This document elaborates `docs/spec.md` §4.1 pt.9 (and the revised pt.7) to the same level of detail
`docs/detailed-spec.md` normally provides. It was split out of `docs/detailed-spec.md` §5.9 because it
covers only the permanent-unlock-specific portion of Tab Lock's detailed spec — the pre-existing
lock/unlock/session/crash-mid-lock material stayed in `detailed-spec.md`.

**Read `docs/detailed-spec.md` §5.9 first.** It covers the Tab Lock context this document builds on and
does not repeat: "Locking a tab," "Recovery on tab-folder access (crash mid-lock)," "Unlocking (starting
a session)," "Deleting a locked tab," and "Cross-tab DND involving a locked tab." It also holds "Tab bar
during a pending recovery (either direction)," which documents both directions' (lock and permanent-
unlock) tab-bar behavior together — see the cross-reference near the end of this document.

---

**Permanently unlocking a tab** (elaborates `[spec §4.1 pt.9]`; irreversibly ends the tab's locked state
entirely — distinct from, and not required for, ending a session as described in `detailed-spec.md`
§5.9's "Unlocking (starting a session)"):

**Precondition**: if this tab has no `lock` field in `.tabtreenote.json` (i.e., it is not currently
locked), this operation errors out immediately — mirrors `tab_unlock`'s own equivalent check. Everything
below assumes the tab is confirmed locked at the time of invocation.

1. **Confirmation gate**: the frontend shows an explicit confirmation dialog before invoking the backend
   at all — a **simple OK/Cancel dialog with warning copy** (no typed tab-name confirmation, no
   checkbox, consistent with the app's existing lightweight confirm-dialog weight, same class as trash
   failure → confirm → direct delete, `detailed-spec.md` §5.8). The copy must state the irreversible/
   security-weakening consequence (`[spec §4.1 pt.9]`) **and** recommend copying the tab's folder first
   as a backup, pointing at the existing "folder copy = backup, works even while tab locked" property
   (`[spec §6]`) — this is required copy content, not left to UI discretion. Esc/Cancel aborts with no
   backend call and no change of any kind.
2. **Resolve the encryption key** (no on-disk changes yet):
   - If invoked from within an already-active unlock session for this tab: use the session's already-
     verified key directly — no password re-entry.
   - If invoked directly from the locked (no-session) state: prompt for the password, then derive and
     verify it against the lock blob exactly like `tab_unlock` steps 1-3 (`detailed-spec.md` §5.9,
     "Unlocking (starting a session)") — `DECRYPT_FAILED` on wrong password, no content of any kind
     touched or returned.
   - Either way, once the key is resolved, decrypt the (still fully intact) lock blob to obtain
     `realTabName`/`tree`/`fileMap` and hold them in memory for the rest of this operation. Decrypting
     the lock blob is read-only and does not by itself require the marker from step 4.
3. **Defensive check — reject if this tab has an open, unsaved memo**: mirrors `tab_lock`'s own
   defensive re-check (`tab_lock.rs`'s `active_memo.is_active_tab` step) for the same reason — an
   in-flight editor session for a memo in this tab must not race against that memo's file being
   decrypted/renamed underneath it. As with locking, the frontend is expected to have already
   force-saved-and-closed any open editor for this tab before invoking this operation (`[spec §8]`); this
   step is the backend's defensive confirmation that it actually happened, not a substitute for it.
4. **Write a minimal in-progress marker** (crash-safety measure, mirrors `[spec §4.1 pt.3]`'s marker for
   the lock direction, but does not need to duplicate `fileMap`/`kdf` the way that marker does): before
   any file is touched, atomically write a sibling marker (e.g. `.tabtreenote.unlock-inprogress`; exact
   filename/format is `developer-lite`'s call) whose only required content is a format-version tag.
   This is sufficient because the still-present `lock` field in `.tabtreenote.json` (left untouched
   until step 6) remains the authoritative planned mapping for the entire duration of this operation —
   the same role the complete `fileMap` written up front plays for locking. There is no need to also
   copy the real memo names into a second plaintext file: unlike the lock-direction marker's `fileMap`
   (not secret, since those real filenames are already visible on disk at the moment that marker is
   written), doing so here would briefly re-expose already-hidden real memo names in plaintext for no
   benefit — the still-present lock blob already holds that information exactly as securely as it did a
   moment before this operation started. The marker's only job is to flag "a permanent-unlock is in
   progress" for whichever access path (session or password) discovers it later. As a defensive check
   (mirroring `tab_lock`'s own step-3 defensive re-check), this operation must also refuse to start if
   the *lock-direction* in-progress marker (`[spec §4.1 pt.3]`) is present on this tab — the two
   operations are mutually exclusive by construction (a tab cannot be mid-lock and mid-permanent-unlock
   at once), and vice versa for `tab_lock`.
5. **For each entry in the (still-present) lock blob's `fileMap`**, in any order: register both paths in
   the self-write registry (`detailed-spec.md` §6.2), then:
   - **Collision check first**: if `<memoName>.md` already exists on disk **and** the token file also
     still exists (both present — external interference during the original lock, not expected in
     normal operation), treat this entry as **anomalous**, the same framing already established for
     crash-recovery's per-entry classification (`EntryState::Anomalous`) — do not touch either of its
     files (no overwrite, no auto-rename), and **abort the entire permanent-unlock operation
     immediately**: stop the loop without processing any further entries, surface the anomaly to the
     user as the failure reason, and leave the in-progress marker exactly as it is (whatever mix of
     already-restored, not-yet-restored, and this one anomalous entry currently exists on disk). Do
     **not** proceed to step 6 and do not attempt any partial/skip-and-continue result — this keeps a
     single binary outcome for the loop (full success, or leave the marker for recovery), matching every
     other path in this feature. The next time this tab's folder is accessed, the existing crash-mid-
     permanent-unlock recovery flow below (which already classifies per-entry state, including this same
     anomalous case) picks up the incomplete attempt — there is no separate "partial failure" concept to
     design; the ordinary path and the recovery path converge on the one mechanism.
   - Otherwise, decrypt the token file's content with the resolved key, atomically write the plaintext
     to `<memoName>.md`, then delete the token file **and its `.bak` companion if one exists**
     (best-effort — a missing `.bak` is not an error; leaving an orphaned encrypted `.bak` behind would
     be the same class of residue problem already fixed for session mutations, GitHub issue #18). This
     reuses the same per-entry restore step `tab_lock_recovery_restore` already performs, plus the
     `.bak` cleanup.
6. **Rewrite the tab's metadata**: only once every entry from step 5 has been restored with no
   anomalous entry encountered (an anomalous entry anywhere aborts the whole operation before reaching
   this step, per step 5 above), atomically write `.tabtreenote.json` with a plain `tree` field (the
   decrypted, real-named tree) in place of the `lock` field.
7. **Delete the in-progress marker** (self-write-registered) only after step 6 succeeds — the sole
   success signal recovery relies on, exactly as in the lock direction.
8. **Update `AppSettings.tabs[i]`**: clear `locked` and restore the real tab name (the reverse of
   `tab_lock` step 7, `[spec §4.1 pt.9]`).
9. **End any active lock session** for this tab, if one was active — its in-memory key/tree are now
   stale, since the tab is an ordinary unlocked tab from this point on.
10. **(Re-)index for full-text search**: index every restored memo's plaintext body via the same patch
    mechanism ordinary content changes already use (`upsert`, `search-index/PLAN.md` §2) — immediately,
    as part of this operation, not deferred to the next app restart. This is a deliberate contrast with
    the pre-existing, accepted gap where a newly-registered tab's initial `.md` files aren't indexed
    until restart (`search-index/PLAN.md`'s own status note) — permanent unlock's contract is that the
    tab behaves exactly like any other unlocked tab from the moment the operation completes, so its
    content must be searchable immediately, mirroring the purge-on-lock rule (`[spec §4.1 pt.6]`) in
    reverse.

**Serialization against other operations on this tab**: for the entire duration of steps 1-10 above, the
frontend must disable/block all other operations on this tab — tree edits, memo open/save, drag-and-drop
into/out of it, another lock/unlock action — the same requirement `tab_lock` itself already relies on
(that operation's own multi-step file loop has the identical exposure, just never stated as an explicit
requirement anywhere before now). This matters concretely here because `AppSettings.tabs[i].locked` is
not cleared until step 8, and (when entry point (a) was used) the session is not ended until step 9 —
so `resolve_tab_access` would otherwise still route other in-flight commands for this tab through the
`Session` branch while files are mid-restore underneath them, a race with no other guard against it.

**Order rationale (crash safety)**: steps 5-6 mirror `tab_lock`'s rename-loop-then-blob-write steps in
reverse — the metadata rewrite (step 6) only happens after every file is confirmed restored (step 5), so
the `lock`/`tree` field in `.tabtreenote.json` always accurately reflects whether the per-file restores
are known-complete. While the marker from step 4 is present, that invariant is allowed to be temporarily
false (some files may already be restored while `.tabtreenote.json` still says `lock`) — the marker's
presence is exactly what disambiguates that transitional state from an ordinary, fully-locked tab. Steps
7-9 mirror `tab_lock`'s marker-delete-then-settings-update steps: the in-memory session is only
discarded, and `AppSettings` only updated, after the on-disk state (files + metadata) is already fully
consistent — a crash before step 7 always leaves the marker in place for recovery, and a crash after
step 7 leaves an already-consistent, already-unlocked tab (the settings-lag window here is the same
already-accepted residual risk as `tab_lock`'s own settings-update step, not a new one).

**Recovery on tab-folder access (crash mid-permanent-unlock)**: mirrors `docs/detailed-spec.md` §5.9's
"Recovery on tab-folder access (crash mid-lock)" subsection, in the reverse direction. Whenever a tab's
folder is accessed and its permanent-unlock in-progress marker is found present, the app must resolve it
before treating the tab as normally lockable/unlockable again:

- **Per-entry state check**: for each entry in the still-present lock blob's `fileMap` (decryptable once
  a password/key is available), exactly one of these is expected: the token file exists and
  `<memoName>.md` does not (`not_restored` — this entry's restore, step 5, never ran or was rolled back
  already), or `<memoName>.md` exists and the token file does not (`restored` — this entry's restore
  completed). Neither/both is an anomalous state, surfaced to the user rather than auto-resolved, same
  as the lock direction and the same as the ordinary (non-crash) restore loop's own collision check
  (step 5 above) — the two checks share one framing, just triggered from different code paths (an
  interrupted-and-resumed run here, vs. an in-progress run there).
- **Zero entries `restored`** (crash happened at or immediately after the marker write, before any
  restore began): automatic rollback, no password needed — nothing was ever touched (the lock blob is
  still fully intact and valid), so recovery simply deletes the marker and the tab remains exactly as it
  was before the permanent-unlock attempt (still normally locked), with no dialog shown at all.
- **At least one entry `restored`** (partial or fully complete restore loop): a password (or, if a
  session for this tab happens to still be active, its cached key) is required either way — finishing
  needs the key to decrypt the remaining `not_restored` entries; rolling back needs the key to
  re-encrypt the already-`restored` entries back into token form. Present a dedicated recovery choice —
  **"Finish Unlocking"** vs. **"Restore Lock"** — analogous to the lock direction's "Finish
  Locking"/"Restore" choice, with the same tie-break default (default to "Finish Unlocking" when
  `restored` entries are a strict majority of the total; default to "Restore Lock" on a minority or
  exact tie).
  - **Finish**: with the verified key, restore every remaining `not_restored` entry (step 5 continued),
    then resume steps 6-10 above (metadata rewrite, delete marker, update settings, end session,
    reindex).
  - **Restore Lock**: with the verified key, for every `restored` entry: re-encrypt its plaintext
    content (fresh IV) into a newly-written token file (the same token name already recorded in the
    lock blob's `fileMap` — no new token needs to be generated), then delete the `<memoName>.md`
    plaintext file. Once every entry is back to token form, delete the marker. The `lock` field in
    `.tabtreenote.json` was never touched during this whole operation, so no metadata rewrite is needed
    for this path either; `AppSettings.tabs[i].locked` was never changed, so no settings rollback is
    needed.
- Both paths recompute their per-entry classification fresh each time they run, so re-interruption is
  safe to retry any number of times (idempotent, same requirement as the lock-direction recovery).
- **Trigger point**: left to `developer-lite`'s `PLAN.md`-level judgment, same as the lock-direction
  marker — the existing lazy, per-tab, on-`tree_open` trigger point is a natural place to also check for
  this marker, since both markers live as siblings of the same `.tabtreenote.json`.

**Tab bar during a pending recovery**: covered together with the lock-direction marker in
`docs/detailed-spec.md` §5.9's "Tab bar during a pending recovery (either direction)" subsection —
neither marker (lock-direction or permanent-unlock-direction) introduces any tab-bar badge of its own;
see that section for the full explanation covering both directions.
