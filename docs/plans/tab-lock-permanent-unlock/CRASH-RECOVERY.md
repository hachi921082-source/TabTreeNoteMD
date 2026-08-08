# PLAN: Tab Lock — Permanent Unlock — Crash-Mid-Permanent-Unlock Recovery

> Status: planning complete, not yet implemented. Companion to [`PLAN.md`](PLAN.md) (read that file
> first). Mirrors, in the reverse direction, `docs/plans/tab-lock/CRASH-RECOVERY.md` — read that file
> too; several design points below only make sense in contrast to it. Elaborates
> `docs/tab-lock-detailed-spec.md`'s "Recovery on tab-folder access (crash mid-permanent-unlock)"
> subsection (split out of `docs/detailed-spec.md` §5.9), that file's "Tab bar during a pending
> recovery" cross-reference to `docs/detailed-spec.md` §5.9's own "Tab bar during a pending recovery
> (either direction)" subsection, and `docs/DECISIONS.md`'s "Tab Lock: Permanent Unlock Operation
> Added" entries.

---

## 1. Context

If `tab_permanent_unlock` (`PLAN.md` §5) is interrupted anywhere from step 6 (marker write) through
step 9 (marker delete), the marker is left on disk. Unlike the lock direction, the still-present
`lock` field in `.tabtreenote.json` is the source of truth for the planned `fileMap` — this marker
itself carries no `fileMap`/`kdf` (`PLAN.md` §2), so classifying per-entry state generally requires
decrypting that still-present lock blob, which requires a key. This is the one structural asymmetry
from the lock-direction recovery flow that shapes everything below.

## 2. Marker File Format

Already specified in `PLAN.md` §2 — restated: `.tabtreenote.unlock-inprogress`, `{ "formatVersion": 1 }`.

## 3. Trigger Point

**Same lazy, per-tab, on-`tree_open` site** as the lock direction (`tab-lock/CRASH-RECOVERY.md`
"Trigger Point") — a second, independent check alongside the existing one, both living in
`tree_open`'s step 0, before its ordinary reconciliation logic:

```rust
// commands/tree.rs, tree_open, step 0:
if matches!(tab_lock::check_marker_on_tree_open(&folder, &registry), tab_lock::MarkerCheck::RecoveryPending) {
    return AppResult::ok(TreeSnapshot { nodes: vec![], tab_name, locked: false, recovery_pending: true, recovery_kind: Some(RecoveryKind::Lock) });
}

let search_index = app.state::<SearchIndex>();
if matches!(
    tab_lock::check_unlock_marker_on_tree_open(&folder, &registry, &store, &sessions, &search_index, &tab_id),
    tab_lock::UnlockMarkerCheck::RecoveryPending
) {
    return AppResult::ok(TreeSnapshot { nodes: vec![], tab_name, locked: cached_locked, recovery_pending: true, recovery_kind: Some(RecoveryKind::PermanentUnlock) });
}
```

**Important**: `check_unlock_marker_on_tree_open`'s `NoMarker` return may have *already mutated*
`AppSettings`/`LockSessionStore`/`SearchIndex` as a side effect (the "Case B" auto-finish branch,
§3.3 below) — unlike the lock direction's `check_marker_on_tree_open`, which only ever deletes a
file. `tree_open` must therefore **re-fetch `AppSettings`** (`store.get(&registry)` again) after this
call before computing `tab_name`/`cached_locked`/proceeding to its existing `really_locked`
reconciliation block, rather than reusing the `tab_name`/`cached_locked` locals captured at the very
top of the function (those may now be stale).

### 3.1 `UnlockMarkerCheck`

```rust
/// Result of `tree_open`'s step-0 check for a pending permanent-unlock marker.
pub(crate) enum UnlockMarkerCheck {
    /// No marker found, or found and fully auto-resolved in place (§3.2 or §3.3 below) — caller
    /// proceeds with ordinary `tree_open` handling, after re-reading `AppSettings` (see above).
    NoMarker,
    /// A marker was found with at least one restored/anomalous entry and the classification could
    /// not be auto-resolved without a password — caller returns `recoveryPending: true`,
    /// `recoveryKind: PermanentUnlock`.
    RecoveryPending,
}
```

### 3.2 Case "zero restored" — automatic rollback, no password, no dialog

Symmetric to the lock direction's own "zero renamed" case. Token files have no `.md` extension by
construction (`crypto/obfuscated_naming.rs`), so **the presence of any `.md` file directly under the
tab folder** is a cheap, password-free proxy for "at least one entry may already be restored (or,
rarer, an anomalous external file appeared)". Conservatively routing that ambiguous case to the
dialog (rather than trying to refine the heuristic) is safe: this branch only ever *deletes the
marker*, touching no other file, so a false "not zero" classification merely costs the user a
password prompt it turns out wasn't strictly necessary — never a correctness problem.

```rust
if tree::scan_real_memo_names(folder).is_empty() {
    if let Err(err) = delete_unlock_marker(folder, registry) {
        tracing::error!(error = %err, "check_unlock_marker_on_tree_open: failed to delete a zero-progress in-progress marker");
        return UnlockMarkerCheck::RecoveryPending;
    }
    return UnlockMarkerCheck::NoMarker;
}
```

### 3.3 Case "already finished" (Case B) — automatic finish, no password, no dialog

A crash landing between `PLAN.md` §5 step 8 (metadata rewrite) and step 9 (marker delete): by the
time step 8 runs, every `fileMap` entry is *provably* already restored (step 8 is only ever reached
after step 7's loop completed with zero anomalous/not-restored entries remaining) — there is no
per-entry decision left to make, no key needed to know the outcome. Detected via
`.tabtreenote.json` already showing a plain `tree` field (not `lock`):

```rust
let (parsed, _recovery) = tree::load_tab_meta_with_recovery(folder);
if parsed.as_ref().map(|m| m.tree.is_some()).unwrap_or(false) {
    let real_name = sessions.with_session(tab_id, |s| s.real_tab_name.clone());
    let _ = clear_locked_placeholder(store, registry, tab_id, real_name.as_deref());
    sessions.end(tab_id);
    for name in tree::scan_real_memo_names(folder) {
        if let Ok(bytes) = std::fs::read(folder.join(format!("{name}.md"))) {
            let (decoded, _e) = decode_memo_body(&bytes);
            search_index.upsert(tab_id, &name, &decoded);
        }
    }
    if let Err(err) = delete_unlock_marker(folder, registry) {
        tracing::error!(error = %err, "check_unlock_marker_on_tree_open: failed to delete an already-finished marker");
        return UnlockMarkerCheck::RecoveryPending; // retry on the next tab-folder access
    }
    return UnlockMarkerCheck::NoMarker;
}
```

**Accepted residual gap**: if no session happens to be active for this tab (i.e. the crash was a
full process restart, or entry point (b) was used), `real_name` is `None` — the display name was only
ever held in the (now-discarded) lock blob and in `AppSettings.tabs[i].name` (not yet updated, since
that's step 10, unreached). `clear_locked_placeholder(..., None)` still clears `locked`, so the tab
becomes a fully functional, fully unencrypted tab immediately — only its *displayed name* stays stuck
at the 🔒 placeholder until the user manually renames it. This is the same class of "settings-cache
lag" risk `docs/tab-lock-detailed-spec.md`'s "Order rationale" section already calls out as accepted for
the lock direction's own settings-update step, just landing on the *name* specifically here because that
one field's only other copy (the lock blob) is gone by this point. Memo content is never affected.

## 4. Recovery Commands (Case A: `lock` field still present)

Once §3.2/§3.3's fully-deterministic cases are ruled out, `tree_open` reports `recoveryPending: true`
/ `recoveryKind: "permanentUnlock"`. The frontend's staging dialog (`PLAN.md` §7.7) drives the three
commands below, all `Option<String>` on `password` (session key reused when available, exactly like
`resolve_key_and_lock_payload`, `PLAN.md` §4.3):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PermanentUnlockRecoveryDefaultAction { FinishUnlocking, RestoreLock }

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PermanentUnlockRecoveryStatus {
    /// Case B (§3.3) resolved between the status call and now, or was somehow reached here anyway
    /// (defensive) — no password needed, only "Finish Unlocking" makes sense.
    AlreadyFinished,
    /// No cached session key and no `password` supplied — frontend should show a password field.
    PasswordRequired,
    /// A key was available (session or verified password) and classification succeeded.
    Classified {
        restored_count: usize,
        not_restored_count: usize,
        anomalous_count: usize,
        total: usize,
        default_action: PermanentUnlockRecoveryDefaultAction,
        anomalous: bool,
    },
}
```

### 4.1 `tab_permanent_unlock_recovery_status`

```rust
#[tauri::command]
pub async fn tab_permanent_unlock_recovery_status(app: AppHandle, tab_id: String, password: Option<String>) -> AppResult<PermanentUnlockRecoveryStatus>
```

1. Resolve `folder`. `NotFound` if `load_unlock_marker(&folder)` is absent.
2. If `.tabtreenote.json` already has a `tree` field → `Ok(AlreadyFinished)` (defensive; §3.3 should
   normally have already resolved this at the trigger point).
3. If `password.is_none()` and `!sessions.has_session(&tab_id)` → `Ok(PasswordRequired)` (no attempt
   to decrypt anything — nothing to verify yet).
4. Otherwise, `resolve_key_and_lock_payload` (`PLAN.md` §4.3). `DecryptFailed` propagates as an error
   (wrong password). On success, `classify_unlock_entries(&folder, &payload.file_map)`
   (`PLAN.md` §4.2); tie-break identical in spirit to the lock direction's
   (`renamed`→`restored`, `Finish Locking`→`Finish Unlocking`, `Restore`→`Restore Lock`): default to
   `FinishUnlocking` when `restored` entries are a strict majority of `total`, else `RestoreLock`.

### 4.2 `tab_permanent_unlock_recovery_finish`

```rust
#[tauri::command]
pub async fn tab_permanent_unlock_recovery_finish(app: AppHandle, tab_id: String, password: Option<String>) -> AppResult<TabInfo>
```

1. Resolve `folder`/current cached `name`. `NotFound` if no marker.
2. If `.tabtreenote.json` already has a `tree` field (Case B reached here defensively): skip straight
   to step 5 below with `real_tab_name = sessions.with_session(&tab_id, |s| s.real_tab_name.clone())`
   (`None` if no session — same accepted gap as §3.3) and no file I/O.
3. Otherwise: `resolve_key_and_lock_payload`. For each `payload.file_map` entry:
   `NotRestored` → `restore_one_permanent_unlock_entry` (`PLAN.md` §4.4); `Restored` → skip;
   `Anomalous` → abort with `ErrorCode::AnomalousFileState` (same framing as the ordinary loop,
   `PLAN.md` §5 step 7 — one shared mechanism, two trigger paths).
4. `tree::save_tab_meta(folder, ..., &payload.tree, ...)` (skipped in the Case-B short-circuit — the
   tree field is already correct).
5. `delete_unlock_marker` — sole success signal.
6. `clear_locked_placeholder(&store, &registry, &tab_id, real_tab_name.as_deref())` — always run,
   `None` name only in the Case-B-without-session sub-case (§3.3's accepted gap).
7. `sessions.end(&tab_id)`.
8. Reindex: `tree::scan_real_memo_names(&folder)`, read + `search_index.upsert` each (uses the actual
   on-disk file list rather than `payload.file_map`, so the Case-B short-circuit — which has no
   `payload` — reindexes correctly too).
9. `Ok(TabInfo { id: tab_id, name: real_tab_name.unwrap_or(current cached name), locked: false })`.

### 4.3 `tab_permanent_unlock_recovery_restore`

```rust
#[tauri::command]
pub async fn tab_permanent_unlock_recovery_restore(app: AppHandle, tab_id: String, password: Option<String>) -> AppResult<TabInfo>
```

1. Resolve `folder`/current cached `name`. `NotFound` if no marker.
2. If `.tabtreenote.json` already has a `tree` field → `Unknown`/"this permanent-unlock attempt
   already finished; nothing to restore" (Restore Lock is not a valid choice once the lock blob is
   gone — mirrored absence of an analogous branch on the lock direction's own `Restore`).
3. Otherwise: `resolve_key_and_lock_payload`. For each `payload.file_map` entry:
   `NotRestored` → skip (already back in token form); `Anomalous` → abort with
   `ErrorCode::AnomalousFileState`; `Restored` → re-encrypt (fresh IV) the plaintext into the
   **same token name already recorded in `fileMap`** (no new token generated), `atomic_write` it,
   then remove the `<memoName>.md` plaintext file.
4. `delete_unlock_marker`. The `lock` field in `.tabtreenote.json` was never touched by this whole
   operation, so no metadata rewrite; `AppSettings.tabs[i].locked` was never changed, so no settings
   rollback either — the tab is exactly as if `tab_permanent_unlock` had never been called.
5. `Ok(TabInfo { id: tab_id, name: current cached (placeholder) name, locked: true })`.

### 4.4 Idempotency

Both `_finish` and `_restore` recompute `classify_unlock_entries` fresh each call (never trusting a
count carried over from an earlier `_status` call) — safe to retry any number of times if recovery
itself is interrupted again, identical requirement to the lock direction.

## 5. Frontend

Staging flow already specified in `PLAN.md` §7.7 (`PermanentUnlockRecoveryDialog.tsx`, delegating its
final "classified" stage to the generalized `RecoveryDialog.tsx`, `PLAN.md` §7.6). Esc-closability:
same policy as the lock direction's dialog (`tab-lock/CRASH-RECOVERY.md` "Frontend") — closing
without choosing an action leaves the marker in place, the tab stays `recoveryPending: true` until a
later visit; nothing is lost or further damaged by deferring the choice.

## 6. Implementation Notes

- §3.2/§3.3's auto-resolve branches are folded directly into `check_unlock_marker_on_tree_open`
  (called from `tree_open` step 0), exactly mirroring how the lock direction's own "zero renamed"
  case is folded into `check_marker_on_tree_open` — no dialog round-trip for either.
- `tab_permanent_unlock_recovery_finish`/`_restore` both check for an already-present `tree` field
  before doing any decrypt work, exactly mirroring `tab_lock_recovery_finish`/`_restore`'s own
  `already_has_lock_field` check (`tab-lock/PLAN.md`'s "Implementation Notes") — same reason: the
  per-entry counts alone can't distinguish "all done, metadata already rewritten" from "all done,
  metadata not yet rewritten," and only one of those needs any more file I/O.

## Open Items

None. `PLAN.md`'s "Open Items" already records the one accepted residual gap (Case B's display-name
edge case) that this file's §3.3 documents in full.
