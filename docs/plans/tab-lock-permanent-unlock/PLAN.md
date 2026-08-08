# PLAN: Tab Lock — Permanent Unlock

> Status: planning complete, not yet implemented. Sibling feature to
> [`docs/plans/tab-lock/PLAN.md`](../tab-lock/PLAN.md) (read that file and its
> [`CRASH-RECOVERY.md`](../tab-lock/CRASH-RECOVERY.md) first — this feature reuses their marker
> mechanism, `EntryState`-style classification, and `TabAccess`/session-store plumbing rather than
> introducing new machinery). Crash-mid-permanent-unlock recovery (its own marker, trigger point,
> per-entry classification, Finish-Unlocking/Restore-Lock choice) is specified separately in the
> companion [`CRASH-RECOVERY.md`](CRASH-RECOVERY.md) in this same directory, mirroring how the
> original feature split its own crash-recovery design into a sibling file.
>
> Split into its own directory (rather than extending `docs/plans/tab-lock/`) because it is a
> distinct, separate, irreversible operation (`[spec §4.1 pt.9]`) with its own command family, its
> own marker file, and its own recovery flow — large enough to warrant the same
> PLAN.md/CRASH-RECOVERY.md split the original feature used, not a few extra paragraphs bolted onto
> an already-large, already-implemented pair of documents.

Sources: `docs/spec.md` §4.1 pt.7 (revised), pt.9 (new); `docs/tab-lock-detailed-spec.md` "Permanently
unlocking a tab" and its "Recovery on tab-folder access (crash mid-permanent-unlock)"/"Serialization
against other operations on this tab"/"Order rationale (crash safety)" subsections (split out of
`docs/detailed-spec.md` §5.9); `docs/detailed-spec.md` §5.9's "Tab bar during a pending recovery"
subsection; `docs/DECISIONS.md` "Tab Lock: Permanent Unlock Operation Added" (+ its "Follow-up review"
addendum).

---

## 1. Scope Recap

An explicit, separate, irreversible action (`[spec §4.1 pt.9]`) that fully decrypts a locked tab back
into an ordinary unencrypted tab — distinct from ending a session (`tab-lock/PLAN.md` §3
`tab_end_session`, which is temporary/automatic and requires no confirmation). Two entry points, both
converging on the same backend command:

- **(a) From within an active unlock session** — reuses the session's already-verified key, no
  password re-entry.
- **(b) Directly from the locked (no-session) state** — its own password prompt, structurally the
  same decrypt-and-verify `tab_unlock` already performs.

Scope is still whole-tab only; nested/partial locking is not reintroduced.

## 2. On-Disk Format: the Permanent-Unlock In-Progress Marker

`<tab folder>/.tabtreenote.unlock-inprogress` (hidden, sibling to `.tabtreenote.json` and to
`tab-lock/PLAN.md`'s own `.tabtreenote.lock-inprogress`), plain JSON:

```jsonc
{
  "formatVersion": 1
}
```

Deliberately minimal — no `fileMap`/`kdf` (contrast with the lock-direction marker,
`tab-lock/CRASH-RECOVERY.md` "Marker File Format"). Rationale (`DECISIONS.md`): the still-present
`lock` field in `.tabtreenote.json` (untouched until step 6 of §5 below) already is the authoritative
planned mapping for the entire operation; duplicating real memo names into a second plaintext file
would briefly re-expose already-hidden names for no benefit.

```rust
// commands/tab_lock.rs
pub(crate) const UNLOCK_INPROGRESS_MARKER_NAME: &str = ".tabtreenote.unlock-inprogress";
const UNLOCK_MARKER_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnlockInProgressMarker {
    format_version: u32,
}

fn unlock_marker_path(folder: &Path) -> PathBuf { folder.join(UNLOCK_INPROGRESS_MARKER_NAME) }
fn load_unlock_marker(folder: &Path) -> Option<UnlockInProgressMarker> { /* mirrors load_marker */ }
fn write_unlock_marker(folder: &Path, marker: &UnlockInProgressMarker, registry: &SelfWriteRegistry) -> std::io::Result<()> { /* mirrors write_marker */ }
fn delete_unlock_marker(folder: &Path, registry: &SelfWriteRegistry) -> std::io::Result<()> { /* mirrors delete_marker */ }
```

Kept as a small, separate set of functions rather than forcing a shared generic over both marker
types — the two markers' content shapes are genuinely different (one carries `kdf`+`fileMap`, this
one carries only a version tag), and the four I/O functions are ~3 lines each; a generic would buy
little. Written/deleted via the same `atomic_write`/`SelfWriteRegistry`/`with_retry` mechanisms as
everything else (no special-cased I/O path).

## 3. New `ErrorCode` Variant

`model/result.rs`'s `ErrorCode` enum gains one variant, for step 5's abort-on-anomalous-entry case
(§5 below) and its two recovery-command equivalents (`CRASH-RECOVERY.md` §4):

```rust
pub enum ErrorCode {
    // ...existing variants unchanged...
    AnomalousFileState,
}
```

Wire form (via the existing `SCREAMING_SNAKE_CASE` rename): `"ANOMALOUS_FILE_STATE"`. Mirrored into
`src/shared/result.ts`'s `ErrorCode` union, with a new entry in `src/shared/errorMessages.ts`:

```ts
ANOMALOUS_FILE_STATE: "想定外の状態のファイルが見つかりました。タブをもう一度開いて復旧処理を行ってください。",
```

Not reused for the lock direction's own recovery (`RecoveryStatus.anomalous` already has its own
dedicated dialog-level surfacing there, `tab-lock/CRASH-RECOVERY.md`) — this variant is specifically
for the *ordinary* (non-recovery) permanent-unlock loop hitting an anomalous entry, a case the lock
direction's own ordinary `tab_lock` loop cannot reach (its tokens are always freshly generated and
collision-checked, `crypto/obfuscated_naming.rs`).

## 4. Shared Helpers (new, in `commands/tab_lock.rs`)

### 4.1 `pair_exists` — minimal shared extraction from `classify_entry`

Instructed reuse point: `classify_entry`/`EntryState`/`Classification`/`classify_marker` are
direction-specific interpretations of the same one-line on-disk check. Extract that one line without
touching `classify_entry`'s existing behavior, name, or the tests that reference it:

```rust
/// Direction-agnostic on-disk evidence for one `{ memoName, token }` pair — `classify_entry`
/// (lock direction) and `classify_unlock_entry` (below, permanent-unlock direction) each interpret
/// the same two booleans according to their own direction's success criterion.
fn pair_exists(folder: &Path, entry: &FileMapEntry) -> (bool, bool) {
    let real = folder.join(format!("{}.md", entry.memo_name));
    let token = folder.join(&entry.token);
    (real.exists(), token.exists())
}

// classify_entry's body becomes:
fn classify_entry(folder: &Path, entry: &FileMapEntry) -> EntryState {
    match pair_exists(folder, entry) {
        (false, true) => EntryState::Renamed,
        (true, false) => EntryState::NotRenamed,
        _ => EntryState::Anomalous,
    }
}
```

### 4.2 `classify_unlock_entry` / `UnlockClassification` / `classify_unlock_entries`

Mirror of `EntryState`/`Classification`/`classify_marker`, opposite success direction, and operating
on a plain `&[FileMapEntry]` slice rather than a marker struct (this marker carries no `fileMap` —
see §2):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnlockEntryState {
    /// `<memoName>.md` exists, token does not — this entry's restore (§5 step 5) completed.
    Restored,
    /// Token exists, `<memoName>.md` does not — this entry's restore never started.
    NotRestored,
    /// Both or neither exist — anomalous (external interference), never auto-resolved.
    Anomalous,
}

fn classify_unlock_entry(folder: &Path, entry: &FileMapEntry) -> UnlockEntryState {
    match pair_exists(folder, entry) {
        (true, false) => UnlockEntryState::Restored,
        (false, true) => UnlockEntryState::NotRestored,
        _ => UnlockEntryState::Anomalous,
    }
}

struct UnlockClassification { restored: usize, not_restored: usize, anomalous: usize, total: usize }

fn classify_unlock_entries(folder: &Path, file_map: &[FileMapEntry]) -> UnlockClassification {
    let mut c = UnlockClassification { restored: 0, not_restored: 0, anomalous: 0, total: file_map.len() };
    for entry in file_map {
        match classify_unlock_entry(folder, entry) {
            UnlockEntryState::Restored => c.restored += 1,
            UnlockEntryState::NotRestored => c.not_restored += 1,
            UnlockEntryState::Anomalous => c.anomalous += 1,
        }
    }
    c
}
```

### 4.3 `resolve_key_and_lock_payload` — shared key/payload resolution (both entry points)

Used by `tab_permanent_unlock` (§5) and all three recovery commands' Case-A branch
(`CRASH-RECOVERY.md` §4):

```rust
/// Resolves the encryption key + decrypted lock payload for a currently-locked tab. If `tab_id`
/// has an active unlock session, uses its already-verified key/tree/fileMap directly (entry point
/// (a); no disk re-read, no re-verification, `password` is ignored). Otherwise requires `password`
/// and performs the same decrypt-and-verify `tab_unlock` steps 1-3 already do (entry point (b)).
async fn resolve_key_and_lock_payload(
    folder: &Path,
    sessions: &LockSessionStore,
    tab_id: &str,
    password: Option<String>,
) -> Result<(LockKey, LockPayload), AppErrorPayload> {
    if let Some((key_bytes, payload)) = sessions.with_session(tab_id, |s| {
        (s.key, LockPayload { real_tab_name: s.real_tab_name.clone(), tree: s.tree.clone(), file_map: s.file_map.clone() })
    }) {
        return Ok((LockKey::new(&key_bytes), payload));
    }
    let Some(password) = password else {
        return Err(AppErrorPayload::new(ErrorCode::Unknown, "password is required to permanently unlock this tab"));
    };
    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(folder);
    let Some(lock_value) = parsed.and_then(|m| m.lock) else {
        return Err(AppErrorPayload::new(ErrorCode::Unknown, "this tab is not locked"));
    };
    let field: LockField = serde_json::from_value(lock_value)
        .map_err(|_| AppErrorPayload::new(ErrorCode::Unknown, "this tab's lock metadata is corrupted"))?;
    let key_bytes = kdf::derive_key(password, field.kdf.clone()).await
        .map_err(|_| AppErrorPayload::new(ErrorCode::Unknown, "failed to derive decryption key"))?;
    let key = LockKey::new(&key_bytes);
    let payload = lock_blob::decrypt_lock_field(&key, &field)
        .map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "incorrect password"))?;
    Ok((key, payload))
}
```

### 4.4 `restore_one_permanent_unlock_entry` — per-entry restore + `.bak` cleanup

Reuses `tab_lock_recovery_restore`'s existing per-entry body (decrypt token → `atomic_write`
plaintext → remove token), plus a new best-effort `.bak`-companion cleanup (`DECISIONS.md`'s
follow-up review: "same residue class as the already-fixed GitHub issue #18"). Scoped to this new
code path only — `tab_lock_recovery_restore`'s own existing loop is left untouched (out of scope for
this feature):

```rust
fn restore_one_permanent_unlock_entry(folder: &Path, entry: &FileMapEntry, key: &LockKey, registry: &SelfWriteRegistry) -> Result<(), AppErrorPayload> {
    let token_path = folder.join(&entry.token);
    let bytes = std::fs::read(&token_path).map_err(|err| {
        tracing::error!(error = %err, path = %token_path.display(), "failed to read an obfuscated memo file while permanently unlocking this tab");
        AppErrorPayload::new(ErrorCode::Unknown, "failed to read a memo file while unlocking this tab")
    })?;
    let plaintext = lock_blob::decrypt_memo_body(key, &bytes)
        .map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "incorrect password"))?;
    let real_path = folder.join(format!("{}.md", entry.memo_name));
    atomic_write(&real_path, &plaintext, registry).map_err(|err| {
        tracing::error!(error = %err, path = %real_path.display(), "failed to restore a plaintext memo file while permanently unlocking this tab");
        AppErrorPayload::new(ErrorCode::Unknown, "failed to restore a memo file")
    })?;
    registry.register(&token_path);
    with_retry(|| std::fs::remove_file(&token_path)).map_err(|err| {
        tracing::error!(error = %err, path = %token_path.display(), "failed to remove an obfuscated memo file after restoring it");
        AppErrorPayload::new(ErrorCode::IoLocked, "failed to remove an obfuscated memo file while unlocking this tab")
    })?;
    // Best-effort `.bak` companion cleanup (GH #18-class residue) — a missing `.bak` is not an error.
    let bak = bak_path(&token_path);
    if bak.exists() {
        registry.register(&bak);
        if let Err(err) = with_retry(|| std::fs::remove_file(&bak)) {
            tracing::warn!(error = %err, path = %bak.display(), "failed to remove an orphaned obfuscated .bak file (best-effort, not fatal)");
        }
    }
    Ok(())
}
```

### 4.5 `clear_locked_placeholder` — reverse of `set_locked_placeholder`

```rust
/// Reverse of `set_locked_placeholder` (`tab_lock` step 10). `real_name: None` clears `locked`
/// without touching `name` — used by the Case-B auto-finish path (`CRASH-RECOVERY.md` §3) when no
/// active session survives to supply the real name.
fn clear_locked_placeholder(store: &AppSettingsStore, registry: &SelfWriteRegistry, tab_id: &str, real_name: Option<&str>) -> Result<RecoveryOutcome, ()> {
    match store.modify(registry, |s| -> Result<(), ()> {
        if let Some(t) = s.tabs.iter_mut().find(|t| t.id == tab_id) {
            t.locked = Some(false);
            if let Some(name) = real_name {
                t.name = name.to_string();
            }
        }
        Ok(())
    }) {
        Ok(Ok((_settings, outcome))) => Ok(outcome),
        _ => Err(()),
    }
}
```

### 4.6 `tree.rs` visibility bump

`scan_real_memo_names` (currently private `fn`) becomes `pub(crate)` — reused by the trigger-point
check's password-free "zero restored" test (`CRASH-RECOVERY.md` §3) and by the reindex step below.
No behavior change.

## 5. Command: `tab_permanent_unlock`

```rust
#[tauri::command]
pub async fn tab_permanent_unlock(app: AppHandle, tab_id: String, password: Option<String>) -> AppResult<TabInfo>
```

`password: None` for entry point (a) (active session supplies the key); `Some(pw)` for entry point
(b). Elaborates `[tab-lock-detailed-spec]` "Permanently unlocking a tab" steps 1-10 (step 1, the
confirmation dialog, is frontend-only — see §7):

1. Resolve `tab_id` → `folder` from `AppSettings` (`NotFound` if missing).
2. **Precondition**: read `.tabtreenote.json`; if it has no `lock` field, `Unknown`/"this tab is not
   locked" (mirrors `tab_unlock`'s own check).
3. **Defensive check**: `ActiveMemoTracker::is_active_tab(&tab_id)` → refuse with `Unknown`/"cannot
   permanently unlock a tab with an open, unsaved memo" (mirrors `tab_lock` step 2).
4. **Mutual exclusion**: refuse (`Unknown`) if `load_marker(&folder)` (the *lock*-direction marker)
   is present, and refuse (`Unknown`) if `load_unlock_marker(&folder)` (this operation's own marker,
   defense-in-depth — the primary trigger is always `tree_open`, `CRASH-RECOVERY.md` §3) is present.
5. **Resolve key + payload**: `resolve_key_and_lock_payload` (§4.3). `DecryptFailed` on wrong
   password, `Unknown`/"password is required..." if no session and no password given.
6. **Write the marker** (§2) before touching any file. I/O failure → `Unknown`, nothing written yet.
7. **Restore loop**, for each entry in `payload.file_map`, in any order:
   - `Restored` → skip (idempotent; not reachable on a fresh call, relevant only if this exact call
     is somehow retried).
   - `Anomalous` → **abort the entire operation immediately**: `AppResult::err_code(AnomalousFileState, ...)`.
     Do not process further entries, do not roll back already-restored entries, leave the marker in
     place. The next tab-folder access runs the crash-recovery flow (`CRASH-RECOVERY.md`), which
     classifies this exact same state.
   - `NotRestored` → `restore_one_permanent_unlock_entry` (§4.4). Any error propagates immediately,
     marker left in place for recovery (same "leave it for the recovery path" policy as `tab_lock`'s
     own loop).
8. **Rewrite metadata**: `tree::save_tab_meta(folder, ..., &payload.tree, ...)` — plain `tree` field,
   real names. Only reached once step 7 completed with zero anomalous entries.
9. **Delete the marker** — sole success signal recovery relies on.
10. **Settings**: `clear_locked_placeholder(&store, &registry, &tab_id, Some(&payload.real_tab_name))`.
11. **End session**: `sessions.end(&tab_id)` (no-op if none was active).
12. **Reindex**: for each `payload.file_map` entry, read `<memoName>.md`, decode, `search_index.upsert(&tab_id, &memo_name, &decoded)`.
13. Return `AppResult::ok(TabInfo { id: tab_id, name: payload.real_tab_name, locked: false })`.

Register in `main.rs`'s `invoke_handler!` alongside the existing `tab_lock::*` entries:
`commands::tab_lock::tab_permanent_unlock`.

### 5.1 Mutual exclusion, the other direction

`tab_lock`'s own existing step 3 (`commands/tab_lock.rs`, the `if load_marker(&folder).is_some()`
defensive check) gains one more line immediately after it:

```rust
if load_unlock_marker(&folder).is_some() {
    return AppResult::err_code(ErrorCode::Unknown, "a previous permanent-unlock attempt on this tab was never recovered; open the tab to run recovery first");
}
```

No other existing command needs a symmetric check: `tab_unlock` (starting an ordinary session) is
already defensively unreachable while either marker is pending, since `tree_open` reports
`recoveryPending: true` for that tab in both cases (§6 below) and the frontend shows the recovery
dialog instead of the normal locked/unlock UI.

## 6. Crash Safety

Full design (marker trigger point, the two fully-deterministic no-dialog auto-resolve cases, the
per-entry Finish-Unlocking/Restore-Lock recovery choice, and the three recovery commands) is in the
companion [`CRASH-RECOVERY.md`](CRASH-RECOVERY.md) — read it alongside this file before implementing.

## 7. Frontend Wiring

### 7.1 `src/ipc/tabLock.ts` (new wrappers)

```ts
export async function tabPermanentUnlock(tabId: string, password?: string): Promise<Result<TabInfo>> {
  return invoke<Result<TabInfo>>("tab_permanent_unlock", { tabId, password: password ?? null });
}
export async function tabPermanentUnlockRecoveryStatus(tabId: string, password?: string): Promise<Result<PermanentUnlockRecoveryStatus>> {
  return invoke("tab_permanent_unlock_recovery_status", { tabId, password: password ?? null });
}
export async function tabPermanentUnlockRecoveryFinish(tabId: string, password?: string): Promise<Result<TabInfo>> {
  return invoke("tab_permanent_unlock_recovery_finish", { tabId, password: password ?? null });
}
export async function tabPermanentUnlockRecoveryRestore(tabId: string, password?: string): Promise<Result<TabInfo>> {
  return invoke("tab_permanent_unlock_recovery_restore", { tabId, password: password ?? null });
}
```

### 7.2 `src/shared/models.ts` (new types + `TreeSnapshot` extension)

```ts
export type PermanentUnlockRecoveryDefaultAction = "finishUnlocking" | "restoreLock";

export type PermanentUnlockRecoveryStatus =
  | { kind: "alreadyFinished" }
  | { kind: "passwordRequired" }
  | {
      kind: "classified";
      restoredCount: number;
      notRestoredCount: number;
      anomalousCount: number;
      total: number;
      defaultAction: PermanentUnlockRecoveryDefaultAction;
      anomalous: boolean;
    };

export type RecoveryKind = "lock" | "permanentUnlock";

export interface TreeSnapshot {
  nodes: TreeNode[];
  tabName: string;
  locked: boolean;
  recoveryPending: boolean;
  /** Which recovery dialog to show when `recoveryPending` is `true`. Always present when
   * `recoveryPending` is `true`, absent otherwise. */
  recoveryKind?: RecoveryKind;
}
```

### 7.3 `src/shared/result.ts` / `src/shared/errorMessages.ts`

Add `"ANOMALOUS_FILE_STATE"` to the `ErrorCode` union (§3) and its Japanese message.

### 7.4 `src/state/useTabLock.ts` (four new methods)

Same `busy`/`lastErrorCode`/refresh-on-success pattern as the four existing methods:
`permanentUnlockTab(tabId, password?)`, `fetchPermanentUnlockRecoveryStatus(tabId, password?)`,
`finishPermanentUnlockRecovery(tabId, password?)`, `restorePermanentUnlockRecovery(tabId, password?)`.
The first and latter two call `onTabsChanged()`/`onTreeChanged()` on success (they mutate
locked-state and tree content, like `lockTab`/`finishRecovery`/`restoreRecovery` already do);
`fetchPermanentUnlockRecoveryStatus` does not (a read-only probe, like `fetchRecoveryStatus`).

### 7.5 `src/components/dialogs/ConfirmDialog.tsx`

Widen `message: string` → `message: React.ReactNode` (backward compatible — a `string` is a valid
`ReactNode`) so the permanent-unlock confirmation can pass a two-`<p>` fragment (irreversibility
warning + required backup-copy recommendation, `[spec §4.1 pt.9]`, `[tab-lock-detailed-spec]` step 1)
instead of one concatenated string.

### 7.6 `src/components/dialogs/RecoveryDialog.tsx` — generalize for reuse

Add a `variant: "lock" | "permanentUnlock"` prop (existing call site in `TreeView.tsx` passes
`variant="lock"`, unchanged behavior otherwise) and normalize its numeric props so both directions'
differently-named status fields map onto one shape:

```ts
export interface RecoveryDialogProps {
  variant: "lock" | "permanentUnlock";
  doneCount: number;       // RecoveryStatus.renamedCount, or a Classified permanent-unlock status's restoredCount
  total: number;
  anomalous: boolean;
  anomalousCount: number;
  defaultAction: "finish" | "restore"; // normalized from either direction's own enum
  errorMessage?: string | null;
  busy?: boolean;
  onFinish: (password: string) => void;
  onRestore: (password: string) => void;
  onCancel: () => void;
}
```

`variant` only changes copy/labels: title ("ロック処理の復旧" / "完全ロック解除の復旧"), the two
action button labels ("元に戻す（ロックを取り消す）"/"ロックを完了する" vs. "ロックを元に戻す"/
"ロック解除を完了する"), and the anomalous-state message text.

### 7.7 New: `src/components/dialogs/PermanentUnlockRecoveryDialog.tsx`

Thin wrapper handling the extra staging `PermanentUnlockRecoveryStatus` has that the lock direction's
`RecoveryStatus` does not (§CRASH-RECOVERY.md §4's "no password needed to classify" asymmetry):

- On mount, calls `fetchPermanentUnlockRecoveryStatus(tabId, undefined)` (probes for a cached
  session key or the deterministic `alreadyFinished` case, no password prompt shown yet).
- `kind: "alreadyFinished"` → renders a message-only panel + a single passwordless "ロック解除を完了
  する" button, calling `finishPermanentUnlockRecovery(tabId, undefined)`.
- `kind: "passwordRequired"` → renders a password field + "続行" button; on submit, re-calls
  `fetchPermanentUnlockRecoveryStatus(tabId, password)` and caches `password` in local state.
- `kind: "classified"` (reached directly, or after the above re-fetch) → delegates to
  `RecoveryDialog` with `variant="permanentUnlock"`, passing the cached password (if any) through to
  `onFinish`/`onRestore`.

### 7.8 Entry point (b): `src/components/TabBar/TabBar.tsx`

Locked-tab context menu gains one item, always enabled while `contextTab.locked` (regardless of
active-session state — both entry points route through it, just skipping the password stage when a
session already exists): **"完全にロック解除"**. New `PendingDialog` variants:
`{ kind: "permanentUnlockConfirm"; tab: TabInfo }` → `ConfirmDialog` (simple OK/Cancel, irreversible +
backup-copy warning copy, `[spec §4.1 pt.9]`) → on confirm: if `tabLockState.activeSessionRealNames.has(tab.id)`,
call `tabLockState.permanentUnlockTab(tab.id, undefined)` directly; otherwise transition to
`{ kind: "permanentUnlockPassword"; tab: TabInfo }` → `PasswordDialog` (mode `"unlock"`-shaped, single
field) → on submit, call `tabLockState.permanentUnlockTab(tab.id, password)`.

### 7.9 Entry point (a): `src/components/Tree/TreeView.tsx`

When `locked && hasActiveSession` (the branch just above the normal tree render, `TreeView.tsx`
around line 404), render a small banner above the tree (mirrors the existing "🔒 このタブはロックさ
れています" no-session banner's styling) reading "🔓 このタブは一時的にロック解除されています" with
a "完全にロック解除" button. Reuses the exact same `permanentUnlockConfirm` → (session exists, so no
password stage) → `tabLock.permanentUnlockTab(activeTabId, undefined)` flow as §7.8, owned locally by
`TreeView.tsx`'s own `pendingDialog` state (no new state needs lifting into `App.tsx`).

## 8. Discretion-Point Resolutions (recap)

1. Marker filename/format — §2: `.tabtreenote.unlock-inprogress`, `{ formatVersion: 1 }`.
2. Command names/signatures — §5, `CRASH-RECOVERY.md` §4: `tab_permanent_unlock(tab_id, password:
   Option<String>)`, `tab_permanent_unlock_recovery_status/_finish/_restore(tab_id, password:
   Option<String>)`.
3. Crash-recovery trigger point — `CRASH-RECOVERY.md` §3: same lazy `tree_open` step-0 site as the
   lock direction, a second check alongside the existing one.
4. Mutual-exclusion implementation — §5.1.
5. Anomalous-entry abort behavior — §5 step 7, `ErrorCode::AnomalousFileState` (§3); no rollback of
   already-restored entries.
6. Frontend wiring — §7.
7. Marker I/O reuses the exact same `atomic_write`/`SelfWriteRegistry`/`with_retry` helpers already
   imported by `commands/tab_lock.rs` — no new I/O primitives (§2).

## Open Items

None at the planning level. `CRASH-RECOVERY.md`'s "Case B" (crash between the metadata rewrite and
marker deletion) documents one narrow, accepted residual UX gap (the tab's *displayed name* only,
never memo content) when that exact crash window is hit via entry point (b) with no session to fall
back on — see that file §3 for the full reasoning.
