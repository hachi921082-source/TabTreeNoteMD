//! Tab Lock — `tab_lock`/`tab_unlock`/`tab_end_session`, crash-mid-lock recovery, and the
//! locked-active-session branch every mutating tree/editor command consults
//! (`docs/plans/tab-lock/PLAN.md`, `docs/plans/tab-lock/CRASH-RECOVERY.md`).
//!
//! This is the single "`TabAccess` enum/trait" branch point `tab-lock/PLAN.md` §3 calls for
//! ("centralized ... to avoid the two code paths silently diverging over time") — every
//! tree/editor command in `commands/tree.rs`/`commands/editor.rs` calls `resolve_tab_access` and
//! dispatches to either its own existing plain-tab logic or one of this module's session-path
//! functions, rather than re-deriving the locked/session check itself.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::commands::editor::{MemoContent, SaveResult};
use crate::commands::tab::TabInfo;
use crate::commands::tree::{self, MemoMoveResult, MemoProperties, TreeSnapshot};
use crate::commands::emit_settings_recovery_event;
use crate::crypto::kdf::{self, KdfParams};
use crate::crypto::lock_blob::{self, FileMapEntry, LockField, LockKey, LockPayload};
use crate::fs::atomic_write::{atomic_write, bak_path};
use crate::fs::retry::with_retry;
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::{AppSettings, EolPolicy};
use crate::model::result::{AppErrorPayload, AppResult, ErrorCode};
use crate::model::tab_meta::{TabMetaFile, TAB_META_FILE_NAME, TAB_META_SCHEMA_VERSION};
use crate::model::tree::{MetaTreeNode, NodePath};
use crate::search::index::SearchIndex;
use crate::settings::app_settings_store::AppSettingsStore;
use crate::store::lock_session_store::{LockSession, LockSessionStore};
use crate::store::memo_file_service::{decode_memo_body, detect_eol, encode_for_save, hash_bytes, normalize_to_lf, restore_eol, Eol};
use crate::watch::active_memo::{ActiveMemoKey, ActiveMemoTracker};

use crate::crypto::obfuscated_naming;

// ---------------------------------------------------------------------------
// Marker file (`CRASH-RECOVERY.md` "Marker File Format")
// ---------------------------------------------------------------------------

/// `<tab folder>/.tabtreenote.lock-inprogress` (`CRASH-RECOVERY.md`). `pub(crate)` so
/// `commands/tree.rs`'s `tree_open` step 0 can detect its presence without a string-literal
/// duplicate.
pub(crate) const LOCK_INPROGRESS_MARKER_NAME: &str = ".tabtreenote.lock-inprogress";
const MARKER_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockInProgressMarker {
    format_version: u32,
    kdf: KdfParams,
    file_map: Vec<FileMapEntry>,
}

fn marker_path(folder: &Path) -> PathBuf {
    folder.join(LOCK_INPROGRESS_MARKER_NAME)
}

fn load_marker(folder: &Path) -> Option<LockInProgressMarker> {
    let bytes = std::fs::read(marker_path(folder)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_marker(folder: &Path, marker: &LockInProgressMarker, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(marker)?;
    atomic_write(&marker_path(folder), &json, registry)
}

fn delete_marker(folder: &Path, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    let path = marker_path(folder);
    registry.register(&path);
    with_retry(|| std::fs::remove_file(&path))
}

// ---------------------------------------------------------------------------
// Permanent-unlock in-progress marker (`docs/plans/tab-lock-permanent-unlock/PLAN.md` §2)
// ---------------------------------------------------------------------------

/// `<tab folder>/.tabtreenote.unlock-inprogress`. `pub(crate)` so `commands/tree.rs`'s
/// `tree_open` step 0 can detect its presence without a string-literal duplicate, mirroring
/// `LOCK_INPROGRESS_MARKER_NAME` above.
pub(crate) const UNLOCK_INPROGRESS_MARKER_NAME: &str = ".tabtreenote.unlock-inprogress";
const UNLOCK_MARKER_FORMAT_VERSION: u32 = 1;

/// Deliberately minimal — no `fileMap`/`kdf` (contrast with `LockInProgressMarker` above): the
/// still-present `lock` field in `.tabtreenote.json` (untouched until step 8 of
/// `tab_permanent_unlock` below) already is the authoritative planned mapping for the entire
/// operation (`PLAN.md` §2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnlockInProgressMarker {
    format_version: u32,
}

fn unlock_marker_path(folder: &Path) -> PathBuf {
    folder.join(UNLOCK_INPROGRESS_MARKER_NAME)
}

fn load_unlock_marker(folder: &Path) -> Option<UnlockInProgressMarker> {
    let bytes = std::fs::read(unlock_marker_path(folder)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_unlock_marker(folder: &Path, marker: &UnlockInProgressMarker, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(marker)?;
    atomic_write(&unlock_marker_path(folder), &json, registry)
}

fn delete_unlock_marker(folder: &Path, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    let path = unlock_marker_path(folder);
    registry.register(&path);
    with_retry(|| std::fs::remove_file(&path))
}

// ---------------------------------------------------------------------------
// Per-entry state check (`CRASH-RECOVERY.md` "Per-Entry State Check ...")
// ---------------------------------------------------------------------------

/// Direction-agnostic on-disk evidence for one `{ memoName, token }` pair — `classify_entry`
/// (lock direction) and `classify_unlock_entry` (below, permanent-unlock direction,
/// `tab-lock-permanent-unlock/PLAN.md` §4.1) each interpret the same two booleans according to
/// their own direction's success criterion. Returns `(real_exists, token_exists)`.
fn pair_exists(folder: &Path, entry: &FileMapEntry) -> (bool, bool) {
    let real = folder.join(format!("{}.md", entry.memo_name));
    let token = folder.join(&entry.token);
    (real.exists(), token.exists())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryState {
    /// Token file exists, `<memoName>.md` does not — this entry's rename+encrypt (`tab_lock`
    /// step 7) completed.
    Renamed,
    /// `<memoName>.md` exists, token file does not — this entry's rename+encrypt never started.
    NotRenamed,
    /// Both or neither exist — not expected outside deliberate external interference during the
    /// crash window (`CRASH-RECOVERY.md`); surfaced to the user rather than auto-resolved.
    Anomalous,
}

fn classify_entry(folder: &Path, entry: &FileMapEntry) -> EntryState {
    match pair_exists(folder, entry) {
        (false, true) => EntryState::Renamed,
        (true, false) => EntryState::NotRenamed,
        _ => EntryState::Anomalous,
    }
}

/// Mirror of `EntryState`/`Classification`/`classify_marker` for the opposite (permanent-unlock)
/// direction (`tab-lock-permanent-unlock/PLAN.md` §4.2). Operates on a plain `&[FileMapEntry]`
/// slice rather than a marker struct, since `UnlockInProgressMarker` carries no `fileMap` (§2
/// above) — the still-present `lock` field's decrypted payload is the caller's source of entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnlockEntryState {
    /// `<memoName>.md` exists, token does not — this entry's restore (`tab_permanent_unlock`
    /// step 7) completed.
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

struct UnlockClassification {
    restored: usize,
    not_restored: usize,
    anomalous: usize,
    total: usize,
}

fn classify_unlock_entries(folder: &Path, file_map: &[FileMapEntry]) -> UnlockClassification {
    let mut c = UnlockClassification {
        restored: 0,
        not_restored: 0,
        anomalous: 0,
        total: file_map.len(),
    };
    for entry in file_map {
        match classify_unlock_entry(folder, entry) {
            UnlockEntryState::Restored => c.restored += 1,
            UnlockEntryState::NotRestored => c.not_restored += 1,
            UnlockEntryState::Anomalous => c.anomalous += 1,
        }
    }
    c
}

struct Classification {
    renamed: usize,
    not_renamed: usize,
    anomalous: usize,
    total: usize,
}

fn classify_marker(folder: &Path, marker: &LockInProgressMarker) -> Classification {
    let mut c = Classification {
        renamed: 0,
        not_renamed: 0,
        anomalous: 0,
        total: marker.file_map.len(),
    };
    for entry in &marker.file_map {
        match classify_entry(folder, entry) {
            EntryState::Renamed => c.renamed += 1,
            EntryState::NotRenamed => c.not_renamed += 1,
            EntryState::Anomalous => c.anomalous += 1,
        }
    }
    c
}

/// Result of `tree_open`'s step-0 marker check (`CRASH-RECOVERY.md` "Trigger Point").
pub(crate) enum MarkerCheck {
    /// No marker found, or a zero-`renamed`/no-anomaly marker was found and has already been
    /// silently deleted (automatic rollback, no dialog) — the caller should proceed with
    /// ordinary `tree_open` reconciliation.
    NoMarker,
    /// A marker with at least one `renamed` or `anomalous` entry was found — the caller should
    /// return `recovery_pending: true` instead of the normal tree.
    RecoveryPending,
}

/// `tree_open`'s step 0 (`docs/plans/memo-tree/PLAN.md` §2, `CRASH-RECOVERY.md`'s "Trigger
/// Point"). Performs the "zero entries renamed" automatic rollback inline (delete the marker, no
/// password, no dialog) since nothing was ever touched in that case.
pub(crate) fn check_marker_on_tree_open(folder: &Path, registry: &SelfWriteRegistry) -> MarkerCheck {
    let Some(marker) = load_marker(folder) else {
        return MarkerCheck::NoMarker;
    };
    let c = classify_marker(folder, &marker);
    if c.renamed == 0 && c.anomalous == 0 {
        if let Err(err) = delete_marker(folder, registry) {
            tracing::error!(error = %err, "check_marker_on_tree_open: failed to delete a zero-progress in-progress marker");
            return MarkerCheck::RecoveryPending;
        }
        return MarkerCheck::NoMarker;
    }
    MarkerCheck::RecoveryPending
}

/// Result of `tree_open`'s step-0 check for a pending permanent-unlock marker
/// (`tab-lock-permanent-unlock/CRASH-RECOVERY.md` §3.1).
pub(crate) enum UnlockMarkerCheck {
    /// No marker found, or found and fully auto-resolved in place (the "zero restored" or
    /// "already finished" cases below) — caller proceeds with ordinary `tree_open` handling,
    /// after re-reading `AppSettings` (see `CRASH-RECOVERY.md` §3's note: this call may have
    /// already mutated `AppSettings`/`LockSessionStore`/`SearchIndex` as a side effect).
    NoMarker,
    /// A marker was found with at least one restored/anomalous entry and the classification
    /// could not be auto-resolved without a password — caller returns `recoveryPending: true`,
    /// `recoveryKind: PermanentUnlock`.
    RecoveryPending,
}

/// `tree_open`'s step 0, second check (`tab-lock-permanent-unlock/CRASH-RECOVERY.md` §3) — a
/// second, independent marker check alongside `check_marker_on_tree_open` above. Folds in the two
/// fully-deterministic, password-free, no-dialog auto-resolve cases (§3.2 "zero restored", §3.3
/// "already finished" / Case B) inline, exactly mirroring how `check_marker_on_tree_open` folds
/// in its own "zero renamed" case.
pub(crate) fn check_unlock_marker_on_tree_open(
    folder: &Path,
    registry: &SelfWriteRegistry,
    store: &AppSettingsStore,
    sessions: &LockSessionStore,
    search_index: &SearchIndex,
    tab_id: &str,
) -> UnlockMarkerCheck {
    if load_unlock_marker(folder).is_none() {
        return UnlockMarkerCheck::NoMarker;
    }

    // Case "zero restored" (§3.2): token files have no `.md` extension by construction, so the
    // presence of any `.md` file directly under the tab folder is a cheap, password-free proxy
    // for "at least one entry may already be restored." A false "not zero" classification merely
    // costs the user a password prompt that turns out not to have been strictly necessary — this
    // branch only ever deletes the marker, touching no other file.
    if tree::scan_real_memo_names(folder).is_empty() {
        if let Err(err) = delete_unlock_marker(folder, registry) {
            tracing::error!(error = %err, "check_unlock_marker_on_tree_open: failed to delete a zero-progress in-progress marker");
            return UnlockMarkerCheck::RecoveryPending;
        }
        return UnlockMarkerCheck::NoMarker;
    }

    // Case "already finished" (Case B, §3.3): `.tabtreenote.json` already shows a plain `tree`
    // field, meaning `tab_permanent_unlock` step 8 already ran — every `fileMap` entry is
    // provably already restored, with no per-entry decision left to make and no key needed.
    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(folder);
    if parsed.as_ref().map(|m| m.tree.is_some()).unwrap_or(false) {
        // Accepted residual gap (`CRASH-RECOVERY.md` §3.3): if no session is active, `real_name`
        // is `None` — the tab's displayed name stays stuck at the 🔒 placeholder until the user
        // manually renames it, but `locked` is still cleared so the tab is fully functional.
        let real_name = sessions.with_session(tab_id, |s| s.real_tab_name.clone());
        let _ = clear_locked_placeholder(store, registry, tab_id, real_name.as_deref());
        sessions.end(tab_id);
        for name in tree::scan_real_memo_names(folder) {
            if let Ok(bytes) = std::fs::read(folder.join(format!("{name}.md"))) {
                let (decoded, _encoding) = decode_memo_body(&bytes);
                search_index.upsert(tab_id, &name, &decoded);
            }
        }
        if let Err(err) = delete_unlock_marker(folder, registry) {
            tracing::error!(error = %err, "check_unlock_marker_on_tree_open: failed to delete an already-finished marker");
            return UnlockMarkerCheck::RecoveryPending;
        }
        return UnlockMarkerCheck::NoMarker;
    }

    UnlockMarkerCheck::RecoveryPending
}

// ---------------------------------------------------------------------------
// TabAccess — the centralized locked/session branch point
// ---------------------------------------------------------------------------

pub(crate) enum TabAccess {
    Plain { folder: PathBuf, tab_name: String },
    Session { tab_id: String, folder: PathBuf },
}

/// `docs/plans/tab-lock/PLAN.md` §3 "While a Session Is Active": every tree/editor command
/// resolves the tab through this function instead of independently checking `locked`/session
/// state, so the two code paths (plain-file vs. session-encrypted) can never silently diverge.
pub(crate) fn resolve_tab_access(settings: &AppSettings, sessions: &LockSessionStore, tab_id: &str) -> Result<TabAccess, AppErrorPayload> {
    let entry = settings
        .tabs
        .iter()
        .find(|t| t.id == tab_id)
        .ok_or_else(|| AppErrorPayload::new(ErrorCode::NotFound, "tab not found"))?;
    let folder = PathBuf::from(&entry.folder_path);
    if entry.locked.unwrap_or(false) {
        if sessions.has_session(tab_id) {
            Ok(TabAccess::Session {
                tab_id: tab_id.to_string(),
                folder,
            })
        } else {
            Err(AppErrorPayload::new(ErrorCode::TabLocked, "cannot modify a locked tab"))
        }
    } else {
        Ok(TabAccess::Plain {
            folder,
            tab_name: entry.name.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// tab_lock
// ---------------------------------------------------------------------------

/// `docs/plans/tab-lock/PLAN.md` §3 `tab_lock`.
#[tauri::command]
pub async fn tab_lock(app: AppHandle, tab_id: String, password: String, hint: Option<String>) -> AppResult<TabInfo> {
    let store = app.state::<AppSettingsStore>();
    let registry = app.state::<Arc<SelfWriteRegistry>>();
    let active_memo = app.state::<ActiveMemoTracker>();

    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    if entry.locked.unwrap_or(false) {
        return AppResult::err_code(ErrorCode::TabLocked, "tab is already locked");
    }
    let tab_name = entry.name.clone();
    let lock_label = entry.lock_label.clone();
    let folder = PathBuf::from(&entry.folder_path);
    drop(settings);

    // Step 2 (defensive, `PLAN.md` §3 step 2): the frontend must have already force-saved-and-
    // closed any open editor for this tab before calling this command.
    if active_memo.is_active_tab(&tab_id) {
        return AppResult::err_code(ErrorCode::Unknown, "cannot lock a tab with an open, unsaved memo");
    }

    // Step 3 (defensive; the primary trigger is always `tree_open`'s own marker check).
    if load_marker(&folder).is_some() {
        return AppResult::err_code(ErrorCode::Unknown, "a previous lock attempt on this tab was never recovered; open the tab to run recovery first");
    }
    // Mutual exclusion, the other direction (`tab-lock-permanent-unlock/PLAN.md` §5.1): a
    // never-recovered permanent-unlock attempt must be resolved before this tab can be locked
    // again.
    if load_unlock_marker(&folder).is_some() {
        return AppResult::err_code(ErrorCode::Unknown, "a previous permanent-unlock attempt on this tab was never recovered; open the tab to run recovery first");
    }

    // Step 4: current unlocked tree.
    let loaded = tree::load_and_reconcile(&folder);

    // Step 5: fresh salt + derived key, off the command-dispatch thread.
    let kdf_params = KdfParams::generate_new();
    let key_bytes = match kdf::derive_key(password, kdf_params.clone()).await {
        Ok(k) => k,
        Err(_) => return AppResult::err_code(ErrorCode::Unknown, "failed to derive encryption key"),
    };
    let key = LockKey::new(&key_bytes);

    // Step 6: full planned `fileMap`, computed up front; write the in-progress marker.
    let mut all_names = Vec::new();
    for node in &loaded.tree {
        tree::collect_names(node, &mut all_names);
    }
    let file_map: Vec<FileMapEntry> = all_names
        .iter()
        .map(|name| FileMapEntry {
            memo_name: name.clone(),
            token: obfuscated_naming::generate_unique_token(&folder),
        })
        .collect();
    let marker = LockInProgressMarker {
        format_version: MARKER_FORMAT_VERSION,
        kdf: kdf_params.clone(),
        file_map: file_map.clone(),
    };
    if let Err(err) = write_marker(&folder, &marker, &registry) {
        tracing::error!(error = %err, "tab_lock: failed to write in-progress marker");
        return AppResult::err_code(ErrorCode::Unknown, "failed to start locking this tab");
    }

    // Step 7: rename+encrypt every entry. On failure, the marker is left in place for
    // `tree_open`'s crash-recovery flow to resolve on next visit rather than attempting an
    // immediate ad hoc rollback here (reuses one recovery path instead of two).
    for entry_fm in &file_map {
        if let Err(payload) = rename_and_encrypt_entry(&folder, entry_fm, &key, &registry) {
            tracing::error!(?payload, "tab_lock: failed mid-way through encrypting memo files; marker left for crash recovery");
            return AppResult::err(payload);
        }
    }

    // Step 8: build + write the lock blob. `last_viewed_memo` carries forward the pre-lock plain
    // tab's own value (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §1's "data-model symmetry" —
    // even a just-locked tab remembers what was last viewed, though it's never read back
    // automatically for a locked tab, `PLAN.md` §6).
    let payload = LockPayload {
        real_tab_name: tab_name,
        tree: loaded.tree,
        file_map,
        last_viewed_memo: loaded.last_viewed_memo,
    };
    let field = match lock_blob::encrypt_lock_field(&key, kdf_params, hint, &payload) {
        Ok(f) => f,
        Err(_) => return AppResult::err_code(ErrorCode::Unknown, "failed to build the encrypted tab metadata"),
    };
    if let Err(err) = write_lock_field(&folder, &field, &registry) {
        tracing::error!(error = %err, "tab_lock: failed to write the encrypted lock blob; marker left for crash recovery");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save the locked tab metadata");
    }

    // Step 9: delete the marker — the sole success signal the recovery check relies on.
    if let Err(err) = delete_marker(&folder, &registry) {
        tracing::error!(error = %err, "tab_lock: failed to delete the in-progress marker after a successful lock");
        return AppResult::err_code(ErrorCode::Unknown, "tab was locked, but a temporary recovery file could not be removed");
    }

    // Step 10: settings.
    match set_locked_placeholder(&store, &registry, &tab_id) {
        Ok(outcome) => emit_settings_recovery_event(&app, outcome),
        Err(()) => {
            tracing::error!("tab_lock: failed to persist locked tab settings");
            return AppResult::err_code(ErrorCode::Unknown, "tab was locked, but settings could not be updated");
        }
    }

    // Step 11: search index purge (`search-index/PLAN.md` §1's lock-purge rule — locking a tab
    // bulk-purges its entries; confirmed applicable here by `designer-lite` during that feature's
    // implementation, since this is one in-memory index side-effect among `tab_lock`'s 12 steps,
    // not a persisted invariant).
    app.state::<SearchIndex>().purge_tab(&tab_id);

    // Step 12: no watcher to stop — this tab was never watched while locked (see `tab_unlock`'s
    // Step 6 comment; GitHub issue #210).

    AppResult::ok(TabInfo {
        id: tab_id,
        name: LOCKED_PLACEHOLDER_NAME.to_string(),
        locked: true,
        lock_label,
    })
}

/// 🔒 glyph alone (`DECISIONS.md` "Tab-Lock Placeholder Display Name"). `pub(crate)` so
/// `commands/tab.rs`'s `register_tab_at_path` (`tab-lock/PLAN.md` §7 "Portability") and
/// `commands/tree.rs`'s `tree_open` self-correction path can apply the exact same placeholder a
/// locked-in-app tab already gets, rather than ever displaying a locked tab's real name.
pub(crate) const LOCKED_PLACEHOLDER_NAME: &str = "\u{1F512}";

fn set_locked_placeholder(store: &AppSettingsStore, registry: &SelfWriteRegistry, tab_id: &str) -> Result<crate::settings::app_settings_store::RecoveryOutcome, ()> {
    match store.modify(registry, |s| -> Result<(), ()> {
        if let Some(t) = s.tabs.iter_mut().find(|t| t.id == tab_id) {
            t.locked = Some(true);
            t.name = LOCKED_PLACEHOLDER_NAME.to_string();
        }
        Ok(())
    }) {
        Ok(Ok((_settings, outcome))) => Ok(outcome),
        _ => Err(()),
    }
}

/// Reverse of `set_locked_placeholder` above (`tab-lock-permanent-unlock/PLAN.md` §4.5).
/// `real_name: None` clears `locked` without touching `name` — used by the Case-B auto-finish
/// path (`CRASH-RECOVERY.md` §3.3) when no active session survives to supply the real name.
fn clear_locked_placeholder(
    store: &AppSettingsStore,
    registry: &SelfWriteRegistry,
    tab_id: &str,
    real_name: Option<&str>,
) -> Result<crate::settings::app_settings_store::RecoveryOutcome, ()> {
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

fn write_lock_field(folder: &Path, field: &LockField, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    let value = serde_json::to_value(field).map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    // `last_viewed_memo: None` — a locked tab's own value lives inside the encrypted `lock`
    // payload instead (`LockPayload::last_viewed_memo`), never as a plaintext sibling here
    // (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §1).
    let meta = TabMetaFile {
        schema_version: TAB_META_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        tree: None,
        lock: Some(value),
        last_viewed_memo: None,
    };
    let json = serde_json::to_vec_pretty(&meta)?;
    atomic_write(&folder.join(TAB_META_FILE_NAME), &json, registry)
}

/// One `fileMap` entry's rename+encrypt (`PLAN.md` §3 step 7, reused verbatim by
/// `tab_lock_recovery_finish` for each remaining `not_renamed` entry): encrypt the plaintext
/// file's content, write it under the token filename (a brand-new file, so momentarily both the
/// real and token file exist — the narrow, explicitly-acknowledged "both" anomalous window,
/// `CRASH-RECOVERY.md`), then remove the original plaintext file. This ordering guarantees a
/// crash can never leave a `Renamed`-classified entry with plaintext content, which the recovery
/// flow's "Restore" branch depends on.
fn rename_and_encrypt_entry(folder: &Path, entry: &FileMapEntry, key: &LockKey, registry: &SelfWriteRegistry) -> Result<(), AppErrorPayload> {
    let src = folder.join(format!("{}.md", entry.memo_name));
    let dst = folder.join(&entry.token);

    let plaintext = std::fs::read(&src).map_err(|err| {
        tracing::error!(error = %err, path = %src.display(), "failed to read memo file to encrypt");
        AppErrorPayload::new(ErrorCode::Unknown, "failed to read a memo file while locking this tab")
    })?;
    let encrypted = lock_blob::encrypt_memo_body(key, &plaintext);
    atomic_write(&dst, &encrypted, registry).map_err(|err| {
        tracing::error!(error = %err, path = %dst.display(), "failed to write obfuscated memo file");
        AppErrorPayload::new(ErrorCode::Unknown, "failed to write an obfuscated memo file while locking this tab")
    })?;

    registry.register(&src);
    with_retry(|| std::fs::remove_file(&src)).map_err(|err| {
        tracing::error!(error = %err, path = %src.display(), "failed to remove the original plaintext memo file after encrypting it");
        AppErrorPayload::new(ErrorCode::IoLocked, "failed to remove a plaintext memo file while locking this tab")
    })
}

// ---------------------------------------------------------------------------
// tab_unlock
// ---------------------------------------------------------------------------

/// `docs/plans/tab-lock/PLAN.md` §3 `tab_unlock`.
#[tauri::command]
pub async fn tab_unlock(app: AppHandle, tab_id: String, password: String) -> AppResult<TabInfo> {
    let store = app.state::<AppSettingsStore>();
    let registry = app.state::<Arc<SelfWriteRegistry>>();
    let sessions = app.state::<LockSessionStore>();

    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    let lock_label = entry.lock_label.clone();
    let folder = PathBuf::from(&entry.folder_path);
    drop(settings);

    // Step 1.
    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(&folder);
    let Some(meta) = parsed else {
        return AppResult::err_code(ErrorCode::Unknown, "tab metadata could not be read");
    };
    let Some(lock_value) = meta.lock else {
        return AppResult::err_code(ErrorCode::Unknown, "this tab is not locked");
    };
    let field: LockField = match serde_json::from_value(lock_value) {
        Ok(f) => f,
        Err(_) => return AppResult::err_code(ErrorCode::Unknown, "this tab's lock metadata is corrupted"),
    };

    // Step 2.
    let key_bytes = match kdf::derive_key(password, field.kdf.clone()).await {
        Ok(k) => k,
        Err(_) => return AppResult::err_code(ErrorCode::Unknown, "failed to derive decryption key"),
    };
    let key = LockKey::new(&key_bytes);

    // Step 3: auth-tag verification failure -> `DecryptFailed`, no content of any kind returned.
    let payload = match lock_blob::decrypt_lock_field(&key, &field) {
        Ok(p) => p,
        Err(_) => return AppResult::err_code(ErrorCode::DecryptFailed, "incorrect password"),
    };

    // Step 4: hold the decrypted session in memory.
    sessions.start(
        tab_id.clone(),
        LockSession {
            real_tab_name: payload.real_tab_name.clone(),
            tree: payload.tree,
            file_map: payload.file_map,
            kdf: field.kdf,
            hint: field.hint,
            key: key_bytes,
            last_viewed_memo: payload.last_viewed_memo,
        },
    );

    // Step 5: `AppSettings.tabs[i].name` intentionally NOT rewritten to the real name while a
    // session is active (`PLAN.md` §3 step 5's "safer default").

    // Step 6: deliberately does NOT start a file watcher for this tab's session — GitHub issue
    // #210 investigated this and closed it as won't-fix: a locked tab's files during a session are
    // still obfuscated (not `.md`), the watcher pipeline's `is_md_file` filter (`watch/watcher_registry.rs`)
    // would silently ignore every event for them regardless, and the app is the only writer to
    // them anyway (`persistence-fs/PLAN.md` §5's own stated rationale, confirmed still correct).

    AppResult::ok(TabInfo {
        id: tab_id,
        name: payload.real_tab_name,
        locked: true,
        lock_label,
    })
}

// ---------------------------------------------------------------------------
// tab_end_session
// ---------------------------------------------------------------------------

/// `docs/plans/tab-lock/PLAN.md` §3 `tab_end_session`. Every session mutation already
/// synchronously persisted its own re-encrypted state, so this only needs to discard the
/// in-memory plaintext.
#[tauri::command]
pub fn tab_end_session(app: AppHandle, tab_id: String) -> AppResult<()> {
    let sessions = app.state::<LockSessionStore>();
    sessions.end(&tab_id);
    AppResult::ok(())
}

// ---------------------------------------------------------------------------
// tab_permanent_unlock (`docs/plans/tab-lock-permanent-unlock/PLAN.md` §4-§5)
// ---------------------------------------------------------------------------

/// Resolves the encryption key + decrypted lock payload for a currently-locked tab (`PLAN.md`
/// §4.3). If `tab_id` has an active unlock session, uses its already-verified key/tree/fileMap
/// directly (entry point (a); no disk re-read, no re-verification, `password` is ignored).
/// Otherwise requires `password` and performs the same decrypt-and-verify `tab_unlock` steps 1-3
/// already do (entry point (b)). Shared by `tab_permanent_unlock` and all three recovery
/// commands' Case-A branch (`CRASH-RECOVERY.md` §4).
async fn resolve_key_and_lock_payload(
    folder: &Path,
    sessions: &LockSessionStore,
    tab_id: &str,
    password: Option<String>,
) -> Result<(LockKey, LockPayload), AppErrorPayload> {
    if let Some((key_bytes, payload)) = sessions.with_session(tab_id, |s| {
        (
            s.key,
            LockPayload {
                real_tab_name: s.real_tab_name.clone(),
                tree: s.tree.clone(),
                file_map: s.file_map.clone(),
                last_viewed_memo: s.last_viewed_memo.clone(),
            },
        )
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
    let field: LockField = serde_json::from_value(lock_value).map_err(|_| AppErrorPayload::new(ErrorCode::Unknown, "this tab's lock metadata is corrupted"))?;
    let key_bytes = kdf::derive_key(password, field.kdf.clone())
        .await
        .map_err(|_| AppErrorPayload::new(ErrorCode::Unknown, "failed to derive decryption key"))?;
    let key = LockKey::new(&key_bytes);
    let payload = lock_blob::decrypt_lock_field(&key, &field).map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "incorrect password"))?;
    Ok((key, payload))
}

/// One `fileMap` entry's restore, plus a `.bak`-companion cleanup (`PLAN.md` §4.4). Reuses
/// `tab_lock_recovery_restore`'s existing per-entry body (decrypt token -> `atomic_write`
/// plaintext -> remove token), plus a new best-effort `.bak`-companion cleanup (`DECISIONS.md`'s
/// follow-up review: "same residue class as the already-fixed GitHub issue #18"). Scoped to this
/// new code path only — `tab_lock_recovery_restore`'s own existing loop is left untouched.
fn restore_one_permanent_unlock_entry(folder: &Path, entry: &FileMapEntry, key: &LockKey, registry: &SelfWriteRegistry) -> Result<(), AppErrorPayload> {
    let token_path = folder.join(&entry.token);
    let bytes = std::fs::read(&token_path).map_err(|err| {
        tracing::error!(error = %err, path = %token_path.display(), "failed to read an obfuscated memo file while permanently unlocking this tab");
        AppErrorPayload::new(ErrorCode::Unknown, "failed to read a memo file while unlocking this tab")
    })?;
    let plaintext = lock_blob::decrypt_memo_body(key, &bytes).map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "incorrect password"))?;
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

/// `docs/plans/tab-lock-permanent-unlock/PLAN.md` §5. `password: None` for entry point (a) (an
/// active session supplies the key); `Some(pw)` for entry point (b).
#[tauri::command]
pub async fn tab_permanent_unlock(app: AppHandle, tab_id: String, password: Option<String>) -> AppResult<TabInfo> {
    let store = app.state::<AppSettingsStore>();
    let registry = app.state::<Arc<SelfWriteRegistry>>();
    let sessions = app.state::<LockSessionStore>();
    let active_memo = app.state::<ActiveMemoTracker>();

    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    let lock_label = entry.lock_label.clone();
    let folder = PathBuf::from(&entry.folder_path);
    drop(settings);

    // Step 2: precondition (mirrors `tab_unlock`'s own check).
    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(&folder);
    let Some(meta) = parsed else {
        return AppResult::err_code(ErrorCode::Unknown, "tab metadata could not be read");
    };
    if meta.lock.is_none() {
        return AppResult::err_code(ErrorCode::Unknown, "this tab is not locked");
    }

    // Step 3 (defensive; mirrors `tab_lock` step 2).
    if active_memo.is_active_tab(&tab_id) {
        return AppResult::err_code(ErrorCode::Unknown, "cannot permanently unlock a tab with an open, unsaved memo");
    }

    // Step 4: mutual exclusion, both directions.
    if load_marker(&folder).is_some() {
        return AppResult::err_code(ErrorCode::Unknown, "a previous lock attempt on this tab was never recovered; open the tab to run recovery first");
    }
    if load_unlock_marker(&folder).is_some() {
        return AppResult::err_code(ErrorCode::Unknown, "a previous permanent-unlock attempt on this tab was never recovered; open the tab to run recovery first");
    }

    // Step 5: resolve key + payload.
    let (key, payload) = match resolve_key_and_lock_payload(&folder, &sessions, &tab_id, password).await {
        Ok(v) => v,
        Err(err) => return AppResult::err(err),
    };

    // Step 6: write the marker before touching any file.
    let marker = UnlockInProgressMarker {
        format_version: UNLOCK_MARKER_FORMAT_VERSION,
    };
    if let Err(err) = write_unlock_marker(&folder, &marker, &registry) {
        tracing::error!(error = %err, "tab_permanent_unlock: failed to write in-progress marker");
        return AppResult::err_code(ErrorCode::Unknown, "failed to start permanently unlocking this tab");
    }

    // Step 7: restore loop, in any order.
    for entry_fm in &payload.file_map {
        match classify_unlock_entry(&folder, entry_fm) {
            // Idempotent; not reachable on a fresh call, relevant only if this exact call is
            // somehow retried.
            UnlockEntryState::Restored => continue,
            // Abort the entire operation immediately: do not process further entries, do not
            // roll back already-restored entries, leave the marker in place. The next
            // tab-folder access runs the crash-recovery flow, which classifies this exact same
            // state (`CRASH-RECOVERY.md`).
            UnlockEntryState::Anomalous => {
                tracing::error!(memo_name = %entry_fm.memo_name, "tab_permanent_unlock: encountered an anomalous entry; aborting the whole operation, marker left for crash recovery");
                return AppResult::err_code(ErrorCode::AnomalousFileState, "an unexpected file state was found while permanently unlocking this tab");
            }
            UnlockEntryState::NotRestored => {
                if let Err(err) = restore_one_permanent_unlock_entry(&folder, entry_fm, &key, &registry) {
                    tracing::error!(?err, "tab_permanent_unlock: failed mid-way through restoring memo files; marker left for crash recovery");
                    return AppResult::err(err);
                }
            }
        }
    }

    // Step 8: rewrite metadata — plain `tree` field, real names. Only reached once step 7
    // completed with zero anomalous entries.
    if let Err(err) = tree::save_tab_meta(&folder, TAB_META_SCHEMA_VERSION, env!("CARGO_PKG_VERSION").to_string(), &payload.tree, payload.last_viewed_memo.clone(), &registry) {
        tracing::error!(error = %err, "tab_permanent_unlock: failed to save plain tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
    }

    // Step 9: delete the marker — sole success signal recovery relies on.
    if let Err(err) = delete_unlock_marker(&folder, &registry) {
        tracing::error!(error = %err, "tab_permanent_unlock: failed to delete the in-progress marker after a successful unlock");
        return AppResult::err_code(ErrorCode::Unknown, "tab was permanently unlocked, but a temporary recovery file could not be removed");
    }

    // Step 10: settings.
    match clear_locked_placeholder(&store, &registry, &tab_id, Some(&payload.real_tab_name)) {
        Ok(outcome) => emit_settings_recovery_event(&app, outcome),
        Err(()) => {
            tracing::error!("tab_permanent_unlock: failed to persist unlocked tab settings");
            return AppResult::err_code(ErrorCode::Unknown, "tab was permanently unlocked, but settings could not be updated");
        }
    }

    // Step 11: end session (no-op if none was active).
    sessions.end(&tab_id);

    // Step 12: reindex.
    let search_index = app.state::<SearchIndex>();
    for entry_fm in &payload.file_map {
        if let Ok(bytes) = std::fs::read(folder.join(format!("{}.md", entry_fm.memo_name))) {
            let (decoded, _encoding) = decode_memo_body(&bytes);
            search_index.upsert(&tab_id, &entry_fm.memo_name, &decoded);
        }
    }

    AppResult::ok(TabInfo {
        id: tab_id,
        name: payload.real_tab_name,
        locked: false,
        lock_label,
    })
}

// ---------------------------------------------------------------------------
// Crash-mid-lock recovery commands (`CRASH-RECOVERY.md`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryDefaultAction {
    Finish,
    Restore,
}

/// Return type of `tab_lock_recovery_status`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryStatus {
    pub renamed_count: usize,
    pub not_renamed_count: usize,
    pub anomalous_count: usize,
    pub total: usize,
    /// UI default-button tie-break (`CRASH-RECOVERY.md`): "Finish Locking" when `renamed`
    /// entries are a strict majority of the total, "Restore" on a minority or exact tie.
    pub default_action: RecoveryDefaultAction,
    /// `true` if any entry is in the anomalous (both-or-neither) state — the frontend should
    /// show a distinct error message instead of the ordinary Finish/Restore choice
    /// (`CRASH-RECOVERY.md`: "surfaced as a distinct error state ... rather than auto-resolved").
    pub anomalous: bool,
}

#[tauri::command]
pub fn tab_lock_recovery_status(app: AppHandle, tab_id: String) -> AppResult<RecoveryStatus> {
    let store = app.state::<AppSettingsStore>();
    let registry = app.state::<Arc<SelfWriteRegistry>>();
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    let folder = PathBuf::from(&entry.folder_path);
    drop(settings);

    let Some(marker) = load_marker(&folder) else {
        return AppResult::err_code(ErrorCode::NotFound, "no interrupted lock attempt found for this tab");
    };
    let c = classify_marker(&folder, &marker);
    let default_action = if c.total > 0 && c.renamed * 2 > c.total {
        RecoveryDefaultAction::Finish
    } else {
        RecoveryDefaultAction::Restore
    };

    AppResult::ok(RecoveryStatus {
        renamed_count: c.renamed,
        not_renamed_count: c.not_renamed,
        anomalous_count: c.anomalous,
        total: c.total,
        default_action,
        anomalous: c.anomalous > 0,
    })
}

/// Derives the key from the marker's own `kdf`, then verifies it by attempting to decrypt one
/// already-`Renamed` entry's content (`CRASH-RECOVERY.md`). Shared by Finish and Restore.
async fn derive_and_verify_recovery_key(folder: &Path, marker: &LockInProgressMarker, password: String) -> Result<LockKey, AppErrorPayload> {
    let key_bytes = kdf::derive_key(password, marker.kdf.clone())
        .await
        .map_err(|_| AppErrorPayload::new(ErrorCode::Unknown, "failed to derive decryption key"))?;
    let key = LockKey::new(&key_bytes);

    let Some(verify_entry) = marker.file_map.iter().find(|e| classify_entry(folder, e) == EntryState::Renamed) else {
        return Err(AppErrorPayload::new(ErrorCode::Unknown, "nothing to recover for this tab"));
    };
    let verify_bytes = std::fs::read(folder.join(&verify_entry.token))
        .map_err(|_| AppErrorPayload::new(ErrorCode::Unknown, "failed to read a recovered memo file"))?;
    lock_blob::decrypt_memo_body(&key, &verify_bytes).map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "incorrect password"))?;

    Ok(key)
}

/// `CRASH-RECOVERY.md` "Finish": with the verified key, rename+encrypt every remaining
/// `not_renamed` entry, then resume `PLAN.md` §3 steps 8-12.
#[tauri::command]
pub async fn tab_lock_recovery_finish(app: AppHandle, tab_id: String, password: String) -> AppResult<TabInfo> {
    let store = app.state::<AppSettingsStore>();
    let registry = app.state::<Arc<SelfWriteRegistry>>();

    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    let tab_name = entry.name.clone();
    let lock_label = entry.lock_label.clone();
    let folder = PathBuf::from(&entry.folder_path);
    drop(settings);

    let Some(marker) = load_marker(&folder) else {
        return AppResult::err_code(ErrorCode::NotFound, "no interrupted lock attempt found for this tab");
    };

    let key = match derive_and_verify_recovery_key(&folder, &marker, password).await {
        Ok(k) => k,
        Err(payload) => return AppResult::err(payload),
    };

    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(&folder);
    let already_has_lock_field = parsed.as_ref().map(|m| m.lock.is_some()).unwrap_or(false);

    if !already_has_lock_field {
        // Step 8 never ran: finish encrypting the remaining entries, then build + write the lock
        // blob from the still-intact plain `tree` field.
        for entry_fm in &marker.file_map {
            if classify_entry(&folder, entry_fm) == EntryState::NotRenamed {
                if let Err(payload) = rename_and_encrypt_entry(&folder, entry_fm, &key, &registry) {
                    return AppResult::err(payload);
                }
            }
        }
        let last_viewed_memo = parsed.as_ref().and_then(|m| m.last_viewed_memo.clone());
        let tree_field = parsed.and_then(|m| m.tree).unwrap_or_default();
        let payload = LockPayload {
            real_tab_name: tab_name,
            tree: tree_field,
            file_map: marker.file_map.clone(),
            last_viewed_memo,
        };
        let field = match lock_blob::encrypt_lock_field(&key, marker.kdf.clone(), None, &payload) {
            Ok(f) => f,
            Err(_) => return AppResult::err_code(ErrorCode::Unknown, "failed to build the encrypted tab metadata"),
        };
        if let Err(err) = write_lock_field(&folder, &field, &registry) {
            tracing::error!(error = %err, "tab_lock_recovery_finish: failed to write the encrypted lock blob");
            return AppResult::err_code(ErrorCode::Unknown, "failed to save the locked tab metadata");
        }
    }
    // else: step 8 already succeeded before the interruption; nothing more to encrypt/write
    // (`CRASH-RECOVERY.md`'s "Idempotency" note).

    if let Err(err) = delete_marker(&folder, &registry) {
        tracing::error!(error = %err, "tab_lock_recovery_finish: failed to delete the in-progress marker");
        return AppResult::err_code(ErrorCode::Unknown, "recovery finished, but a temporary recovery file could not be removed");
    }

    match set_locked_placeholder(&store, &registry, &tab_id) {
        Ok(outcome) => emit_settings_recovery_event(&app, outcome),
        Err(()) => {
            tracing::error!("tab_lock_recovery_finish: failed to persist locked tab settings");
            return AppResult::err_code(ErrorCode::Unknown, "recovery finished, but settings could not be updated");
        }
    }

    app.state::<SearchIndex>().purge_tab(&tab_id);

    AppResult::ok(TabInfo {
        id: tab_id,
        name: LOCKED_PLACEHOLDER_NAME.to_string(),
        locked: true,
        lock_label,
    })
}

/// `CRASH-RECOVERY.md` "Restore": with the verified key, for every `Renamed` entry decrypt +
/// rename back to `<memoName>.md`. No `lock` blob is ever (left) written; `AppSettings.tabs[i].locked`
/// was never changed, so no settings rollback is needed either.
#[tauri::command]
pub async fn tab_lock_recovery_restore(app: AppHandle, tab_id: String, password: String) -> AppResult<TabInfo> {
    let store = app.state::<AppSettingsStore>();
    let registry = app.state::<Arc<SelfWriteRegistry>>();

    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    let tab_name = entry.name.clone();
    let lock_label = entry.lock_label.clone();
    let folder = PathBuf::from(&entry.folder_path);
    drop(settings);

    let Some(marker) = load_marker(&folder) else {
        return AppResult::err_code(ErrorCode::NotFound, "no interrupted lock attempt found for this tab");
    };

    let key = match derive_and_verify_recovery_key(&folder, &marker, password).await {
        Ok(k) => k,
        Err(payload) => return AppResult::err(payload),
    };

    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(&folder);
    let already_has_lock_field = parsed.as_ref().and_then(|m| m.lock.clone()).is_some();
    // Plain-shaped-file case (the `else` branch below) has its own top-level `last_viewed_memo`;
    // the already-locked case overwrites this with the decrypted payload's own value instead
    // (captured in that branch, below) — a locked tab's top-level field is always `None`.
    let mut last_viewed_memo = parsed.as_ref().and_then(|m| m.last_viewed_memo.clone());

    let restored_tree: Vec<MetaTreeNode> = if already_has_lock_field {
        // Step 8 already succeeded: decrypt the already-written lock blob (with the now-verified
        // key) to recover its exact tree before undoing anything, so structure/labels aren't
        // lost by falling back to a flat rebuild.
        let lock_value = parsed.and_then(|m| m.lock).expect("checked above");
        let field: LockField = match serde_json::from_value(lock_value) {
            Ok(f) => f,
            Err(_) => return AppResult::err_code(ErrorCode::Unknown, "this tab's lock metadata is corrupted"),
        };
        match lock_blob::decrypt_lock_field(&key, &field) {
            Ok(payload) => {
                last_viewed_memo = payload.last_viewed_memo;
                payload.tree
            }
            Err(_) => return AppResult::err_code(ErrorCode::DecryptFailed, "incorrect password"),
        }
    } else {
        parsed.and_then(|m| m.tree).unwrap_or_default()
    };

    for entry_fm in &marker.file_map {
        if classify_entry(&folder, entry_fm) != EntryState::Renamed {
            continue;
        }
        let token_path = folder.join(&entry_fm.token);
        let bytes = match std::fs::read(&token_path) {
            Ok(b) => b,
            Err(err) => {
                tracing::error!(error = %err, "tab_lock_recovery_restore: failed to read an encrypted memo file");
                return AppResult::err_code(ErrorCode::Unknown, "failed to read a memo file while restoring this tab");
            }
        };
        let plaintext = match lock_blob::decrypt_memo_body(&key, &bytes) {
            Ok(p) => p,
            Err(_) => return AppResult::err_code(ErrorCode::DecryptFailed, "incorrect password"),
        };
        let real_path = folder.join(format!("{}.md", entry_fm.memo_name));
        if let Err(err) = atomic_write(&real_path, &plaintext, &registry) {
            tracing::error!(error = %err, "tab_lock_recovery_restore: failed to restore a plaintext memo file");
            return AppResult::err_code(ErrorCode::Unknown, "failed to restore a memo file");
        }
        registry.register(&token_path);
        if let Err(err) = with_retry(|| std::fs::remove_file(&token_path)) {
            tracing::error!(error = %err, "tab_lock_recovery_restore: failed to remove an obfuscated memo file after restoring it");
        }
    }

    if let Err(err) = tree::save_tab_meta(&folder, TAB_META_SCHEMA_VERSION, env!("CARGO_PKG_VERSION").to_string(), &restored_tree, last_viewed_memo, &registry) {
        tracing::error!(error = %err, "tab_lock_recovery_restore: failed to restore plain tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to restore tab metadata");
    }

    if let Err(err) = delete_marker(&folder, &registry) {
        tracing::error!(error = %err, "tab_lock_recovery_restore: failed to delete the in-progress marker");
        return AppResult::err_code(ErrorCode::Unknown, "restore finished, but a temporary recovery file could not be removed");
    }

    AppResult::ok(TabInfo {
        id: tab_id,
        name: tab_name,
        locked: false,
        lock_label,
    })
}

// ---------------------------------------------------------------------------
// Crash-mid-permanent-unlock recovery commands
// (`docs/plans/tab-lock-permanent-unlock/CRASH-RECOVERY.md` §4)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PermanentUnlockRecoveryDefaultAction {
    FinishUnlocking,
    RestoreLock,
}

/// Return type of `tab_permanent_unlock_recovery_status`. Unlike the lock direction's own
/// `RecoveryStatus`, classification here may require a password (the marker carries no
/// `fileMap`/`kdf` of its own, `PLAN.md` §2) — the extra `AlreadyFinished`/`PasswordRequired`
/// variants stage that asymmetry for the frontend (`CRASH-RECOVERY.md` §4).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PermanentUnlockRecoveryStatus {
    /// Case B (§3.3) resolved between the status call and now, or was somehow reached here
    /// anyway (defensive) — no password needed, only "Finish Unlocking" makes sense.
    AlreadyFinished,
    /// No cached session key and no `password` supplied — frontend should show a password field.
    PasswordRequired,
    /// A key was available (session or verified password) and classification succeeded.
    ///
    /// `#[serde(rename_all = "camelCase")]` on the enclosing `enum` only renames the variant
    /// *tag* (`"kind"`), not a struct-like variant's own field names — a per-variant
    /// `rename_all` is required here too, or these fields would serialize as snake_case
    /// (`restored_count`, ...) while the sibling lock-direction `RecoveryStatus` (a plain
    /// `struct` with `rename_all` directly on it) serializes as camelCase, silently breaking the
    /// frontend's `PermanentUnlockRecoveryStatus` parsing (a real regression caught in review).
    #[serde(rename_all = "camelCase")]
    Classified {
        restored_count: usize,
        not_restored_count: usize,
        anomalous_count: usize,
        total: usize,
        default_action: PermanentUnlockRecoveryDefaultAction,
        anomalous: bool,
    },
}

#[tauri::command]
pub async fn tab_permanent_unlock_recovery_status(app: AppHandle, tab_id: String, password: Option<String>) -> AppResult<PermanentUnlockRecoveryStatus> {
    let store = app.state::<AppSettingsStore>();
    let registry = app.state::<Arc<SelfWriteRegistry>>();
    let sessions = app.state::<LockSessionStore>();

    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    let folder = PathBuf::from(&entry.folder_path);
    drop(settings);

    if load_unlock_marker(&folder).is_none() {
        return AppResult::err_code(ErrorCode::NotFound, "no interrupted permanent-unlock attempt found for this tab");
    }

    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(&folder);
    if parsed.as_ref().map(|m| m.tree.is_some()).unwrap_or(false) {
        return AppResult::ok(PermanentUnlockRecoveryStatus::AlreadyFinished);
    }

    if password.is_none() && !sessions.has_session(&tab_id) {
        return AppResult::ok(PermanentUnlockRecoveryStatus::PasswordRequired);
    }

    let payload = match resolve_key_and_lock_payload(&folder, &sessions, &tab_id, password).await {
        Ok((_key, payload)) => payload,
        Err(err) => return AppResult::err(err),
    };
    let c = classify_unlock_entries(&folder, &payload.file_map);
    let default_action = if c.total > 0 && c.restored * 2 > c.total {
        PermanentUnlockRecoveryDefaultAction::FinishUnlocking
    } else {
        PermanentUnlockRecoveryDefaultAction::RestoreLock
    };

    AppResult::ok(PermanentUnlockRecoveryStatus::Classified {
        restored_count: c.restored,
        not_restored_count: c.not_restored,
        anomalous_count: c.anomalous,
        total: c.total,
        default_action,
        anomalous: c.anomalous > 0,
    })
}

/// `CRASH-RECOVERY.md` §4.2 "Finish": with the resolved key, restore every remaining
/// `NotRestored` entry, then resume `PLAN.md` §5 steps 8-12.
#[tauri::command]
pub async fn tab_permanent_unlock_recovery_finish(app: AppHandle, tab_id: String, password: Option<String>) -> AppResult<TabInfo> {
    let store = app.state::<AppSettingsStore>();
    let registry = app.state::<Arc<SelfWriteRegistry>>();
    let sessions = app.state::<LockSessionStore>();

    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    let current_name = entry.name.clone();
    let lock_label = entry.lock_label.clone();
    let folder = PathBuf::from(&entry.folder_path);
    drop(settings);

    if load_unlock_marker(&folder).is_none() {
        return AppResult::err_code(ErrorCode::NotFound, "no interrupted permanent-unlock attempt found for this tab");
    }

    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(&folder);
    let already_has_tree_field = parsed.as_ref().map(|m| m.tree.is_some()).unwrap_or(false);

    let real_tab_name: Option<String> = if already_has_tree_field {
        // Case B reached here defensively: metadata already rewritten, nothing left to decrypt.
        sessions.with_session(&tab_id, |s| s.real_tab_name.clone())
    } else {
        let payload = match resolve_key_and_lock_payload(&folder, &sessions, &tab_id, password).await {
            Ok((key, payload)) => {
                for entry_fm in &payload.file_map {
                    match classify_unlock_entry(&folder, entry_fm) {
                        UnlockEntryState::Restored => continue,
                        UnlockEntryState::Anomalous => {
                            tracing::error!(memo_name = %entry_fm.memo_name, "tab_permanent_unlock_recovery_finish: encountered an anomalous entry; aborting");
                            return AppResult::err_code(ErrorCode::AnomalousFileState, "an unexpected file state was found while finishing this tab's unlock");
                        }
                        UnlockEntryState::NotRestored => {
                            if let Err(err) = restore_one_permanent_unlock_entry(&folder, entry_fm, &key, &registry) {
                                return AppResult::err(err);
                            }
                        }
                    }
                }
                payload
            }
            Err(err) => return AppResult::err(err),
        };
        if let Err(err) = tree::save_tab_meta(&folder, TAB_META_SCHEMA_VERSION, env!("CARGO_PKG_VERSION").to_string(), &payload.tree, payload.last_viewed_memo.clone(), &registry) {
            tracing::error!(error = %err, "tab_permanent_unlock_recovery_finish: failed to save plain tab metadata");
            return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
        }
        Some(payload.real_tab_name)
    };

    if let Err(err) = delete_unlock_marker(&folder, &registry) {
        tracing::error!(error = %err, "tab_permanent_unlock_recovery_finish: failed to delete the in-progress marker");
        return AppResult::err_code(ErrorCode::Unknown, "recovery finished, but a temporary recovery file could not be removed");
    }

    match clear_locked_placeholder(&store, &registry, &tab_id, real_tab_name.as_deref()) {
        Ok(outcome) => emit_settings_recovery_event(&app, outcome),
        Err(()) => {
            tracing::error!("tab_permanent_unlock_recovery_finish: failed to persist unlocked tab settings");
            return AppResult::err_code(ErrorCode::Unknown, "recovery finished, but settings could not be updated");
        }
    }

    sessions.end(&tab_id);

    // Reindex from the actual on-disk file list (not `payload.file_map`), so the Case-B
    // short-circuit — which has no `payload` — reindexes correctly too.
    let search_index = app.state::<SearchIndex>();
    for name in tree::scan_real_memo_names(&folder) {
        if let Ok(bytes) = std::fs::read(folder.join(format!("{name}.md"))) {
            let (decoded, _encoding) = decode_memo_body(&bytes);
            search_index.upsert(&tab_id, &name, &decoded);
        }
    }

    AppResult::ok(TabInfo {
        id: tab_id,
        name: real_tab_name.unwrap_or(current_name),
        locked: false,
        lock_label,
    })
}

/// `CRASH-RECOVERY.md` §4.3 "Restore": with the resolved key, for every `Restored` entry
/// re-encrypt (fresh IV) back into its already-recorded token name, then remove the plaintext.
/// No `lock` blob is ever touched by this whole operation, and `AppSettings.tabs[i].locked` was
/// never changed, so no settings rollback is needed either — the tab ends up exactly as if
/// `tab_permanent_unlock` had never been called.
#[tauri::command]
pub async fn tab_permanent_unlock_recovery_restore(app: AppHandle, tab_id: String, password: Option<String>) -> AppResult<TabInfo> {
    let store = app.state::<AppSettingsStore>();
    let registry = app.state::<Arc<SelfWriteRegistry>>();
    let sessions = app.state::<LockSessionStore>();

    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    let current_name = entry.name.clone();
    let lock_label = entry.lock_label.clone();
    let folder = PathBuf::from(&entry.folder_path);
    drop(settings);

    if load_unlock_marker(&folder).is_none() {
        return AppResult::err_code(ErrorCode::NotFound, "no interrupted permanent-unlock attempt found for this tab");
    }

    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(&folder);
    if parsed.as_ref().map(|m| m.tree.is_some()).unwrap_or(false) {
        return AppResult::err_code(ErrorCode::Unknown, "this permanent-unlock attempt already finished; nothing to restore");
    }

    let (key, payload) = match resolve_key_and_lock_payload(&folder, &sessions, &tab_id, password).await {
        Ok(v) => v,
        Err(err) => return AppResult::err(err),
    };

    for entry_fm in &payload.file_map {
        match classify_unlock_entry(&folder, entry_fm) {
            // Already back in token form.
            UnlockEntryState::NotRestored => continue,
            UnlockEntryState::Anomalous => {
                tracing::error!(memo_name = %entry_fm.memo_name, "tab_permanent_unlock_recovery_restore: encountered an anomalous entry; aborting");
                return AppResult::err_code(ErrorCode::AnomalousFileState, "an unexpected file state was found while restoring this tab's lock");
            }
            UnlockEntryState::Restored => {
                let real_path = folder.join(format!("{}.md", entry_fm.memo_name));
                let plaintext = match std::fs::read(&real_path) {
                    Ok(b) => b,
                    Err(err) => {
                        tracing::error!(error = %err, "tab_permanent_unlock_recovery_restore: failed to read a plaintext memo file");
                        return AppResult::err_code(ErrorCode::Unknown, "failed to read a memo file while restoring this tab's lock");
                    }
                };
                let encrypted = lock_blob::encrypt_memo_body(&key, &plaintext);
                let token_path = folder.join(&entry_fm.token);
                if let Err(err) = atomic_write(&token_path, &encrypted, &registry) {
                    tracing::error!(error = %err, "tab_permanent_unlock_recovery_restore: failed to write an obfuscated memo file");
                    return AppResult::err_code(ErrorCode::Unknown, "failed to restore an obfuscated memo file");
                }
                registry.register(&real_path);
                if let Err(err) = with_retry(|| std::fs::remove_file(&real_path)) {
                    tracing::error!(error = %err, "tab_permanent_unlock_recovery_restore: failed to remove a plaintext memo file after re-encrypting it");
                }
            }
        }
    }

    if let Err(err) = delete_unlock_marker(&folder, &registry) {
        tracing::error!(error = %err, "tab_permanent_unlock_recovery_restore: failed to delete the in-progress marker");
        return AppResult::err_code(ErrorCode::Unknown, "restore finished, but a temporary recovery file could not be removed");
    }

    AppResult::ok(TabInfo {
        id: tab_id,
        name: current_name,
        locked: true,
        lock_label,
    })
}

// ---------------------------------------------------------------------------
// Session-path helpers: tree structure mutations while a session is active
// (`docs/plans/tab-lock/PLAN.md` §3 "While a Session Is Active")
// ---------------------------------------------------------------------------

pub(crate) fn session_tree_snapshot(session: &LockSession) -> TreeSnapshot {
    TreeSnapshot {
        nodes: session.tree.clone(),
        tab_name: session.real_tab_name.clone(),
        locked: true,
        recovery_pending: false,
        recovery_kind: None,
        last_viewed_memo: session.last_viewed_memo.clone(),
    }
}

fn empty_locked_snapshot() -> TreeSnapshot {
    TreeSnapshot {
        nodes: vec![],
        tab_name: String::new(),
        locked: true,
        recovery_pending: false,
        recovery_kind: None,
        last_viewed_memo: None,
    }
}

/// Re-encrypts and atomically re-saves the lock blob for `session`'s static fields
/// (key/kdf/hint/realTabName) combined with an explicit `tree`/`file_map`/`last_viewed_memo` —
/// called after every structure-changing session mutation (`PLAN.md` §3), and by
/// `session_set_last_viewed` below (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §4). Every
/// parameter besides `session` is taken explicitly (rather than read from `session` itself) so
/// callers can save a not-yet-committed draft and only write it into `session` once this succeeds
/// (`commit_session_mutation`, GH #17).
fn save_lock_blob(folder: &Path, session: &LockSession, tree: &[MetaTreeNode], file_map: &[FileMapEntry], last_viewed_memo: Option<String>, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    let key = LockKey::new(&session.key);
    let payload = LockPayload {
        real_tab_name: session.real_tab_name.clone(),
        tree: tree.to_vec(),
        file_map: file_map.to_vec(),
        last_viewed_memo,
    };
    let field = lock_blob::encrypt_lock_field(&key, session.kdf.clone(), session.hint.clone(), &payload).map_err(|_| std::io::Error::other("failed to encrypt lock blob"))?;
    write_lock_field(folder, &field, registry)
}

/// Shared "build locally, then commit" pattern for every session structure mutation
/// (`session_create_memo`, `session_rename_memo`, `session_delete_memo`, `session_move_within`,
/// `session_mutate_node_field`, `apply_session_move_target`, `cleanup_session_move_source`) —
/// fixes GH #17: previously each of these mutated `session.tree`/`session.file_map` directly and
/// only called `save_lock_blob` as a last step, so a save failure (e.g. a transient Windows
/// AV/indexer lock hitting `atomic_write`'s rename) left the in-memory session mutated even though
/// the command reported failure.
///
/// `mutate` receives read-only access to `session` (for its key/kdf/hint/realTabName) plus mutable
/// access to *clones* of `session.tree`/`session.file_map`; it may also perform physical
/// obfuscated-file I/O as a side effect (writing/deleting token files). That physical I/O is
/// intentionally **not** rolled back if a later step fails — an accepted pre-existing limitation
/// this issue explicitly scopes out (only the in-memory `tree`/`file_map` and the final lock-blob
/// save need to roll back). Only after `mutate` succeeds AND the resulting lock blob save succeeds
/// are the computed `tree`/`file_map` written back into `session`; on any error, `session.tree`/
/// `session.file_map` are left exactly as they were before this call.
fn commit_session_mutation(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    folder: &Path,
    tab_id: &str,
    mutate: impl FnOnce(&LockSession, &mut Vec<MetaTreeNode>, &mut Vec<FileMapEntry>) -> Result<(), AppErrorPayload>,
) -> Option<Result<(), AppErrorPayload>> {
    sessions.with_session_mut(tab_id, |session| -> Result<(), AppErrorPayload> {
        let mut tree = session.tree.clone();
        let mut file_map = session.file_map.clone();
        mutate(session, &mut tree, &mut file_map)?;
        save_lock_blob(folder, session, &tree, &file_map, session.last_viewed_memo.clone(), registry).map_err(|err| {
            tracing::error!(error = %err, tab_id, "session mutation: failed to save lock blob; in-memory session left unchanged");
            AppErrorPayload::new(ErrorCode::Unknown, "failed to save tab metadata")
        })?;
        session.tree = tree;
        session.file_map = file_map;
        Ok(())
    })
}

/// `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §4, session branch of `tree_set_last_viewed`: sets
/// `session.last_viewed_memo` and re-encrypts+re-saves the lock blob (fresh IV) — same "structure
/// change -> re-encrypt and re-save the blob" rule as every other session mutation above, even
/// though this field isn't part of `tree`/`file_map`. Follows the same "commit only after the save
/// succeeds" discipline as `commit_session_mutation` (GH #17): `session.last_viewed_memo` is left
/// unchanged if the lock-blob save fails.
pub(crate) fn session_set_last_viewed(sessions: &LockSessionStore, registry: &SelfWriteRegistry, folder: &Path, tab_id: &str, memo_name: Option<String>) -> AppResult<()> {
    let result = sessions.with_session_mut(tab_id, |session| -> Result<(), AppErrorPayload> {
        save_lock_blob(folder, session, &session.tree.clone(), &session.file_map.clone(), memo_name.clone(), registry).map_err(|err| {
            tracing::error!(error = %err, tab_id, "session_set_last_viewed: failed to save lock blob; in-memory session left unchanged");
            AppErrorPayload::new(ErrorCode::Unknown, "failed to save tab metadata")
        })?;
        session.last_viewed_memo = memo_name;
        Ok(())
    });
    match result {
        Some(Ok(())) => AppResult::ok(()),
        Some(Err(payload)) => AppResult::err(payload),
        None => AppResult::err_code(ErrorCode::TabLocked, "no active session for this tab"),
    }
}

pub(crate) fn session_create_memo(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    folder: &Path,
    tab_id: &str,
    name: &str,
    parent: Option<&str>,
) -> AppResult<TreeSnapshot> {
    let result = commit_session_mutation(sessions, registry, folder, tab_id, |session, tree, file_map| {
        if tree::contains_name(tree, name) {
            return Err(AppErrorPayload::new(ErrorCode::NameDuplicate, "a memo with this name already exists in this tab"));
        }
        if let Some(p) = parent {
            if tree::find_node(tree, p).is_none() {
                return Err(AppErrorPayload::new(ErrorCode::NotFound, "parent memo not found"));
            }
        }
        let token = obfuscated_naming::generate_unique_token(folder);
        let key = LockKey::new(&session.key);
        let encrypted = lock_blob::encrypt_memo_body(&key, b"");
        if let Err(err) = atomic_write(&folder.join(&token), &encrypted, registry) {
            tracing::error!(error = %err, "session_create_memo: failed to write obfuscated memo file");
            return Err(AppErrorPayload::new(ErrorCode::Unknown, "failed to create memo file"));
        }
        let node = MetaTreeNode::default_for_new_file(name.to_string());
        tree::insert_node_at_end(tree, parent, node).map_err(|code| AppErrorPayload::new(code, "failed to insert memo into tree"))?;
        file_map.push(FileMapEntry {
            memo_name: name.to_string(),
            token,
        });
        Ok(())
    });
    finish_session_mutation(sessions, tab_id, result)
}

pub(crate) fn session_rename_memo(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    folder: &Path,
    tab_id: &str,
    path: &str,
    new_name: &str,
) -> AppResult<TreeSnapshot> {
    let result = commit_session_mutation(sessions, registry, folder, tab_id, |_session, tree, file_map| {
        if tree::find_node(tree, path).is_none() {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "memo not found"));
        }
        if !path.eq_ignore_ascii_case(new_name) && tree::contains_name(tree, new_name) {
            return Err(AppErrorPayload::new(ErrorCode::NameDuplicate, "a memo with this name already exists in this tab"));
        }
        // No file rename is needed — the obfuscated token filename never reflects `memoName`.
        if let Some(node) = tree::find_node_mut(tree, path) {
            node.memo_name = new_name.to_string();
        }
        if let Some(entry) = file_map.iter_mut().find(|e| e.memo_name == path) {
            entry.memo_name = new_name.to_string();
        }
        Ok(())
    });
    finish_session_mutation(sessions, tab_id, result)
}

pub(crate) fn session_delete_memo(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    folder: &Path,
    tab_id: &str,
    paths: &[NodePath],
) -> AppResult<TreeSnapshot> {
    let result = commit_session_mutation(sessions, registry, folder, tab_id, |_session, tree, file_map| {
        let mut all_names: Vec<String> = Vec::new();
        for p in paths {
            if let Some(node) = tree::find_node(tree, p) {
                let mut names = Vec::new();
                tree::collect_names(node, &mut names);
                for n in names {
                    if !all_names.contains(&n) {
                        all_names.push(n);
                    }
                }
            }
        }
        if all_names.is_empty() {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "no matching memo to delete"));
        }

        // Deletion of a locked-session memo's obfuscated file is a permanent delete, not a
        // trash-move: an obfuscated filename in the recycle bin is meaningless to the user and
        // there is no plaintext name to show them in an OS trash UI either way, so trash offers
        // no benefit here unlike the plain-tab case.
        for name in &all_names {
            let Some(entry) = file_map.iter().find(|e| &e.memo_name == name) else {
                continue;
            };
            let token_path = folder.join(&entry.token);
            if !token_path.exists() {
                continue;
            }
            registry.register(&token_path);
            if let Err(err) = with_retry(|| std::fs::remove_file(&token_path)) {
                tracing::error!(error = %err, "session_delete_memo: failed to delete obfuscated memo file");
                return Err(AppErrorPayload::new(ErrorCode::Unknown, "failed to delete one or more memo files"));
            }
        }

        for name in &all_names {
            tree::remove_node(tree, name);
            file_map.retain(|e| &e.memo_name != name);
        }

        Ok(())
    });
    finish_session_mutation(sessions, tab_id, result)
}

pub(crate) fn session_move_within(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    folder: &Path,
    tab_id: &str,
    paths: &[NodePath],
    new_parent: Option<&str>,
    index: usize,
) -> AppResult<TreeSnapshot> {
    let result = commit_session_mutation(sessions, registry, folder, tab_id, |_session, tree, _file_map| {
        let mut removed = Vec::new();
        for p in paths {
            if let Some(node) = tree::remove_node(tree, p) {
                removed.push(node);
            }
        }
        if removed.is_empty() {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "no matching memo to move"));
        }
        if let Some(p) = new_parent {
            if tree::find_node(tree, p).is_none() {
                return Err(AppErrorPayload::new(ErrorCode::NotFound, "target parent memo not found"));
            }
        }
        for (i, node) in removed.into_iter().enumerate() {
            tree::insert_node_at(tree, new_parent, index + i, node).map_err(|code| AppErrorPayload::new(code, "failed to move memo"))?;
        }
        Ok(())
    });
    finish_session_mutation(sessions, tab_id, result)
}

pub(crate) fn session_mutate_node_field(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    folder: &Path,
    tab_id: &str,
    path: &str,
    f: impl FnOnce(&mut MetaTreeNode) -> Result<(), ErrorCode>,
) -> AppResult<TreeSnapshot> {
    let result = commit_session_mutation(sessions, registry, folder, tab_id, |_session, tree, _file_map| {
        let Some(node) = tree::find_node_mut(tree, path) else {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "memo not found"));
        };
        f(node).map_err(|code| AppErrorPayload::new(code, "failed to update memo"))
    });
    finish_session_mutation(sessions, tab_id, result)
}

/// `docs/plans/v1.1.0/folder-import/PLAN.md` §5 — the `TabAccess::Session` branch of
/// `import_file` (`commands/import.rs`). `content_bytes` is already the fully processed bytes
/// (source read -> decoded -> EOL-restored -> re-encoded, `import_file`'s shared steps 4-6, which
/// run once regardless of branch) ready to either write plain or encrypt; this function's own job
/// is the structural counterpart of the plain path's steps 8-10 (insert-into-tree-and-file_map, or
/// overwrite-in-place), but through the session's in-memory tree/file-map + re-encrypted lock blob
/// instead of a plain `.md` file (`[spec v1.1.0 §1.5]`: "proceeds exactly like dropping onto a
/// normal unlocked tab"). Never touches `SearchIndex` — same as every other session-path helper
/// above, locked tabs stay unconditionally excluded from the index even with an active session
/// (`search-index/PLAN.md` §1).
#[allow(clippy::too_many_arguments)]
pub(crate) fn session_import_file(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    folder: &Path,
    tab_id: &str,
    target_name: &str,
    content_bytes: &[u8],
    overwrite: bool,
    parent: Option<&str>,
) -> AppResult<TreeSnapshot> {
    let result = commit_session_mutation(sessions, registry, folder, tab_id, |session, tree, file_map| {
        let key = LockKey::new(&session.key);
        let encrypted = lock_blob::encrypt_memo_body(&key, content_bytes);

        if !overwrite {
            if tree::contains_name(tree, target_name) {
                return Err(AppErrorPayload::new(ErrorCode::NameDuplicate, "a memo with this name already exists in this tab"));
            }
            if let Some(p) = parent {
                if tree::find_node(tree, p).is_none() {
                    return Err(AppErrorPayload::new(ErrorCode::NotFound, "parent memo not found"));
                }
            }
            let token = obfuscated_naming::generate_unique_token(folder);
            if let Err(err) = atomic_write(&folder.join(&token), &encrypted, registry) {
                tracing::error!(error = %err, "session_import_file: failed to write obfuscated memo file");
                return Err(AppErrorPayload::new(ErrorCode::Unknown, "failed to import memo file"));
            }
            let node = MetaTreeNode::default_for_new_file(target_name.to_string());
            tree::insert_node_at_end(tree, parent, node).map_err(|code| AppErrorPayload::new(code, "failed to insert memo into tree"))?;
            file_map.push(FileMapEntry {
                memo_name: target_name.to_string(),
                token,
            });
        } else {
            // Case-insensitive lookup, same reasoning as `commands/import.rs::find_node_ci` for
            // the plain path — the just-dropped source's sanitized name need not match the
            // existing entry's own stored casing.
            let Some(entry) = file_map.iter().find(|e| e.memo_name.eq_ignore_ascii_case(target_name)) else {
                return Err(AppErrorPayload::new(ErrorCode::NotFound, "memo to overwrite not found"));
            };
            let real_name = entry.memo_name.clone();
            let token_path = folder.join(&entry.token);
            if tree::find_node(tree, &real_name).and_then(|n| n.locked) == Some(true) {
                // Defensive fallback, same policy as the plain path (`commands/import.rs`) — the
                // frontend already disables the Overwrite control for an edit-locked collision
                // target.
                return Err(AppErrorPayload::new(ErrorCode::Unknown, "cannot overwrite an edit-locked memo"));
            }
            if let Err(err) = atomic_write(&token_path, &encrypted, registry) {
                tracing::error!(error = %err, "session_import_file: failed to write obfuscated memo file");
                return Err(AppErrorPayload::new(ErrorCode::Unknown, "failed to import memo file"));
            }
        }
        Ok(())
    });
    finish_session_mutation(sessions, tab_id, result)
}

fn finish_session_mutation(sessions: &LockSessionStore, tab_id: &str, result: Option<Result<(), AppErrorPayload>>) -> AppResult<TreeSnapshot> {
    match result {
        Some(Ok(())) => match sessions.with_session(tab_id, session_tree_snapshot) {
            Some(snapshot) => AppResult::ok(snapshot),
            None => AppResult::err_code(ErrorCode::TabLocked, "session ended"),
        },
        Some(Err(payload)) => AppResult::err(payload),
        None => AppResult::err_code(ErrorCode::TabLocked, "no active session for this tab"),
    }
}

pub(crate) fn session_get_properties(sessions: &LockSessionStore, folder: &Path, tab_id: &str, path: &str) -> AppResult<MemoProperties> {
    let result = sessions.with_session(tab_id, |session| -> Result<MemoProperties, AppErrorPayload> {
        let Some(entry) = session.file_map.iter().find(|e| e.memo_name == path) else {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "memo file not found"));
        };
        let token_path = folder.join(&entry.token);
        let metadata = std::fs::metadata(&token_path).map_err(|_| AppErrorPayload::new(ErrorCode::NotFound, "memo file not found"))?;
        let bytes = std::fs::read(&token_path).map_err(|_| AppErrorPayload::new(ErrorCode::NotFound, "memo file not found"))?;
        let key = LockKey::new(&session.key);
        let plaintext = lock_blob::decrypt_memo_body(&key, &bytes).map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "failed to decrypt memo body"))?;
        let (decoded, _encoding) = decode_memo_body(&plaintext);
        let normalized = normalize_to_lf(&decoded);
        let char_count = normalized.chars().count();
        let char_count_excluding_whitespace = normalized.chars().filter(|c| !c.is_whitespace()).count();
        let line_count = normalized.split('\n').count();
        // Show the logical memo path (mirrors the plain, non-locked path's display), not the
        // obfuscated on-disk token filename — the token name is a meaningless hash to the user.
        let display_path = folder.join(format!("{path}.md"));
        Ok(MemoProperties {
            file_name: format!("{}.md", path.rsplit('/').next().unwrap_or(path)),
            file_path: display_path.display().to_string(),
            file_size_bytes: plaintext.len() as u64,
            created_ms: metadata.created().ok().map(tree::system_time_to_epoch_ms).unwrap_or_default(),
            modified_ms: metadata.modified().ok().map(tree::system_time_to_epoch_ms).unwrap_or_default(),
            char_count,
            char_count_excluding_whitespace,
            line_count,
        })
    });
    match result {
        Some(Ok(props)) => AppResult::ok(props),
        Some(Err(payload)) => AppResult::err(payload),
        None => AppResult::err_code(ErrorCode::TabLocked, "no active session for this tab"),
    }
}

// ---------------------------------------------------------------------------
// Session-path helpers: editor load/save while a session is active
// ---------------------------------------------------------------------------

pub(crate) fn session_editor_load(
    sessions: &LockSessionStore,
    active_memo: &ActiveMemoTracker,
    folder: &Path,
    tab_id: &str,
    path: &str,
) -> AppResult<MemoContent> {
    let result = sessions.with_session(tab_id, |session| -> Result<MemoContent, AppErrorPayload> {
        let Some(node) = tree::find_node(&session.tree, path) else {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "memo not found"));
        };
        let edit_locked = node.locked.unwrap_or(false);
        let scroll_position = node.editor.as_ref().and_then(|e| e.scroll_position);
        let cursor_anchor = node.editor.as_ref().and_then(|e| e.cursor_anchor);
        let cursor_head = node.editor.as_ref().and_then(|e| e.cursor_head);

        let Some(entry) = session.file_map.iter().find(|e| e.memo_name == path) else {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "memo not found"));
        };
        let bytes = std::fs::read(folder.join(&entry.token)).map_err(|err| {
            tracing::error!(error = %err, "session_editor_load: failed to read obfuscated memo file");
            AppErrorPayload::new(ErrorCode::NotFound, "failed to read memo file")
        })?;
        let key = LockKey::new(&session.key);
        let plaintext = lock_blob::decrypt_memo_body(&key, &bytes).map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "failed to decrypt memo body"))?;

        let (decoded, encoding) = decode_memo_body(&plaintext);
        let eol = detect_eol(&decoded);
        let text = normalize_to_lf(&decoded);
        // Hashed over the decrypted plaintext (not the on-disk ciphertext, which changes every
        // save due to its fresh IV even for byte-identical content) so unchanged content
        // compares equal across saves, matching the plain-tab hash's meaning.
        let hash = hash_bytes(&plaintext);

        Ok(MemoContent {
            text,
            encoding,
            eol,
            hash,
            edit_locked,
            scroll_position,
            cursor_anchor,
            cursor_head,
        })
    });

    match result {
        Some(Ok(content)) => {
            active_memo.set(ActiveMemoKey {
                tab_id: tab_id.to_string(),
                memo_name: path.to_string(),
            });
            AppResult::ok(content)
        }
        Some(Err(payload)) => AppResult::err(payload),
        None => AppResult::err_code(ErrorCode::TabLocked, "no active session for this tab"),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn session_editor_save(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    folder: &Path,
    tab_id: &str,
    path: &str,
    text: &str,
    expected_hash: &str,
    eol_policy: EolPolicy,
    default_save_encoding: &str,
) -> AppResult<SaveResult> {
    let result = sessions.with_session(tab_id, |session| -> Result<SaveResult, AppErrorPayload> {
        let Some(node) = tree::find_node(&session.tree, path) else {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "memo not found"));
        };
        if node.locked.unwrap_or(false) {
            return Err(AppErrorPayload::new(ErrorCode::Unknown, "cannot save: this memo is edit-locked"));
        }
        let Some(entry) = session.file_map.iter().find(|e| e.memo_name == path) else {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "memo not found"));
        };
        let token_path = folder.join(&entry.token);

        let on_disk = std::fs::read(&token_path).map_err(|err| {
            tracing::error!(error = %err, "session_editor_save: failed to read obfuscated memo file for conflict check");
            AppErrorPayload::new(ErrorCode::NotFound, "memo file no longer exists")
        })?;
        let key = LockKey::new(&session.key);
        let current_plaintext = lock_blob::decrypt_memo_body(&key, &on_disk).map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "failed to decrypt memo body"))?;

        let current_hash = hash_bytes(&current_plaintext);
        if current_hash != expected_hash {
            return Err(AppErrorPayload::new(ErrorCode::Conflict, "the memo was changed on disk since it was loaded"));
        }

        let (decoded_on_disk, _encoding) = decode_memo_body(&current_plaintext);
        let detected_eol = detect_eol(&decoded_on_disk);
        let output_eol = match eol_policy {
            EolPolicy::Preserve => detected_eol,
            EolPolicy::Crlf => Eol::Crlf,
            EolPolicy::Lf => Eol::Lf,
            EolPolicy::Cr => Eol::Cr,
        };
        let restored = restore_eol(text, output_eol);
        let plaintext_bytes = encode_for_save(&restored, default_save_encoding);

        let encrypted = lock_blob::encrypt_memo_body(&key, &plaintext_bytes);
        atomic_write(&token_path, &encrypted, registry).map_err(|err| {
            tracing::error!(error = %err, "session_editor_save: failed to write obfuscated memo file");
            AppErrorPayload::new(ErrorCode::Unknown, "failed to save memo file")
        })?;

        // No search-index patch call: locked tabs are unconditionally excluded from the index,
        // with no exception for the active-session case (`[spec §2.4, §4.1 pt.6]`).

        Ok(SaveResult { hash: hash_bytes(&plaintext_bytes) })
    });

    match result {
        Some(Ok(save_result)) => AppResult::ok(save_result),
        Some(Err(payload)) => AppResult::err(payload),
        None => AppResult::err_code(ErrorCode::TabLocked, "no active session for this tab"),
    }
}

// ---------------------------------------------------------------------------
// Cross-tab move combos involving a locked (session) tab (`PLAN.md` §5)
// ---------------------------------------------------------------------------

type GatheredMove = (Vec<MetaTreeNode>, HashMap<String, Vec<u8>>, HashMap<String, String>);

/// Reads (decrypts) the plaintext bodies of `paths`' full subtrees from `source_tab_id`'s active
/// session, without mutating anything. Returns the moved-subtree root nodes (for reinsertion),
/// each name's decrypted plaintext, and each name's original token (for cleanup on a non-copy
/// move).
fn gather_session_move(sessions: &LockSessionStore, source_tab_id: &str, source_folder: &Path, paths: &[NodePath]) -> Option<Result<GatheredMove, AppErrorPayload>> {
    sessions.with_session(source_tab_id, |session| -> Result<GatheredMove, AppErrorPayload> {
        let mut nodes_to_move = Vec::new();
        for p in paths {
            if let Some(node) = tree::find_node(&session.tree, p) {
                nodes_to_move.push(node.clone());
            }
        }
        if nodes_to_move.is_empty() {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "no matching memo to move"));
        }
        let mut all_names = Vec::new();
        for node in &nodes_to_move {
            tree::collect_names(node, &mut all_names);
        }

        let key = LockKey::new(&session.key);
        let mut plaintexts = HashMap::new();
        let mut tokens = HashMap::new();
        for name in &all_names {
            let Some(entry) = session.file_map.iter().find(|e| &e.memo_name == name) else {
                return Err(AppErrorPayload::new(ErrorCode::Unknown, "internal inconsistency: memo missing from file map"));
            };
            let bytes = std::fs::read(source_folder.join(&entry.token)).map_err(|err| {
                tracing::error!(error = %err, "cross-tab move: failed to read source memo file");
                AppErrorPayload::new(ErrorCode::Unknown, "failed to read memo file to move")
            })?;
            let plaintext = lock_blob::decrypt_memo_body(&key, &bytes).map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "failed to decrypt memo body"))?;
            plaintexts.insert(name.clone(), plaintext);
            tokens.insert(name.clone(), entry.token.clone());
        }
        Ok((nodes_to_move, plaintexts, tokens))
    })
}

/// Removes `all_names` from `source_tab_id`'s active session (tree + fileMap), deletes their
/// obfuscated files, then re-saves the source lock blob — the "not copy" cleanup shared by
/// session-to-plain and session-to-session cross-tab moves.
fn cleanup_session_move_source(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    source_folder: &Path,
    source_tab_id: &str,
    all_names: &[String],
    tokens: &HashMap<String, String>,
) -> Result<(), AppErrorPayload> {
    let result = commit_session_mutation(sessions, registry, source_folder, source_tab_id, |_session, tree, file_map| {
        for name in all_names {
            if let Some(token) = tokens.get(name) {
                let token_path = source_folder.join(token);
                registry.register(&token_path);
                if let Err(err) = with_retry(|| std::fs::remove_file(&token_path)) {
                    tracing::error!(error = %err, "cross-tab move: failed to remove obfuscated source file after move");
                    return Err(AppErrorPayload::new(ErrorCode::Unknown, "failed to remove one or more source memo files"));
                }
            }
            tree::remove_node(tree, name);
            file_map.retain(|e| &e.memo_name != name);
        }
        Ok(())
    });
    match result {
        Some(r) => r,
        None => Err(AppErrorPayload::new(ErrorCode::TabLocked, "source tab session ended")),
    }
}

/// Encrypts `plaintexts` (keyed by memoName) into fresh obfuscated files under `target_folder`
/// using `target_tab_id`'s active session key, inserts `nodes_to_move` into its tree, appends
/// `fileMap` entries, and re-saves the target lock blob — the "apply" step shared by
/// plain-to-session and session-to-session cross-tab moves.
#[allow(clippy::too_many_arguments)]
fn apply_session_move_target(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    target_folder: &Path,
    target_tab_id: &str,
    nodes_to_move: &[MetaTreeNode],
    all_names: &[String],
    plaintexts: &HashMap<String, Vec<u8>>,
    new_parent: Option<&str>,
    index: usize,
) -> Result<(), AppErrorPayload> {
    let result = commit_session_mutation(sessions, registry, target_folder, target_tab_id, |session, tree, file_map| {
        if let Some(p) = new_parent {
            if tree::find_node(tree, p).is_none() {
                return Err(AppErrorPayload::new(ErrorCode::NotFound, "target parent memo not found"));
            }
        }
        for name in all_names {
            if tree::contains_name(tree, name) {
                return Err(AppErrorPayload::new(ErrorCode::NameDuplicate, "a memo with this name already exists in the target tab"));
            }
        }
        let key = LockKey::new(&session.key);
        for name in all_names {
            let Some(plaintext) = plaintexts.get(name) else { continue };
            let token = obfuscated_naming::generate_unique_token(target_folder);
            let encrypted = lock_blob::encrypt_memo_body(&key, plaintext);
            if let Err(err) = atomic_write(&target_folder.join(&token), &encrypted, registry) {
                tracing::error!(error = %err, "cross-tab move: failed to write obfuscated memo file");
                return Err(AppErrorPayload::new(ErrorCode::Unknown, "failed to move memo file into locked tab"));
            }
            file_map.push(FileMapEntry {
                memo_name: name.clone(),
                token,
            });
        }
        for (i, node) in nodes_to_move.iter().enumerate() {
            let _ = tree::insert_node_at(tree, new_parent, index + i, node.clone());
        }
        Ok(())
    });
    match result {
        Some(r) => r,
        None => Err(AppErrorPayload::new(ErrorCode::TabLocked, "target tab session ended")),
    }
}

/// Dropping onto a locked tab with an active session, from a normal unlocked tab
/// (`PLAN.md` §5: "proceeds exactly like dropping onto a normal unlocked tab (encrypt on the way
/// in) — no password prompt needed, session is already live").
#[allow(clippy::too_many_arguments)]
pub(crate) fn move_cross_tab_plain_to_session(
    source_tab_id: &str,
    source_folder: &Path,
    source_tab_name: &str,
    target_tab_id: &str,
    target_folder: &Path,
    paths: &[NodePath],
    new_parent: Option<&str>,
    index: usize,
    copy: bool,
    registry: &SelfWriteRegistry,
    sessions: &LockSessionStore,
    search_index: &SearchIndex,
) -> AppResult<MemoMoveResult> {
    let mut source_loaded = tree::load_and_reconcile(source_folder);
    let mut nodes_to_move = Vec::new();
    for p in paths {
        if let Some(node) = tree::find_node(&source_loaded.tree, p) {
            nodes_to_move.push(node.clone());
        }
    }
    if nodes_to_move.is_empty() {
        return AppResult::err_code(ErrorCode::NotFound, "no matching memo to move");
    }
    let mut all_names = Vec::new();
    for node in &nodes_to_move {
        tree::collect_names(node, &mut all_names);
    }

    let mut plaintexts = HashMap::new();
    for name in &all_names {
        let path = source_folder.join(format!("{name}.md"));
        match std::fs::read(&path) {
            Ok(bytes) => {
                plaintexts.insert(name.clone(), bytes);
            }
            Err(err) => {
                tracing::error!(error = %err, "move_cross_tab_plain_to_session: failed to read source memo file");
                return AppResult::err_code(ErrorCode::Unknown, "failed to read memo file to move");
            }
        }
    }

    if let Err(payload) = apply_session_move_target(sessions, registry, target_folder, target_tab_id, &nodes_to_move, &all_names, &plaintexts, new_parent, index) {
        return AppResult::err(payload);
    }

    if !copy {
        for name in &all_names {
            let path = source_folder.join(format!("{name}.md"));
            registry.register(&path);
            if let Err(err) = with_retry(|| std::fs::remove_file(&path)) {
                tracing::error!(error = %err, "move_cross_tab_plain_to_session: failed to remove source memo file after move");
            }
        }
        for p in paths {
            tree::remove_node(&mut source_loaded.tree, p);
        }
        if let Err(err) = tree::save_tab_meta(source_folder, source_loaded.schema_version, source_loaded.app_version, &source_loaded.tree, source_loaded.last_viewed_memo.clone(), registry) {
            tracing::error!(error = %err, "move_cross_tab_plain_to_session: failed to save source tab metadata");
            return AppResult::err_code(ErrorCode::Unknown, "failed to save source tab metadata");
        }
        // The moved memo(s) left the plain source tab's index scope for good (not a copy) and
        // never enter the locked target's (`search-index/PLAN.md` §1: unconditional exclusion, no
        // session exception).
        for name in &all_names {
            search_index.remove(source_tab_id, name);
        }
    }

    let target_snapshot = sessions.with_session(target_tab_id, session_tree_snapshot).unwrap_or_else(empty_locked_snapshot);
    AppResult::ok(MemoMoveResult {
        source: TreeSnapshot {
            nodes: source_loaded.tree,
            tab_name: source_tab_name.to_string(),
            locked: false,
            recovery_pending: false,
            recovery_kind: None,
            last_viewed_memo: source_loaded.last_viewed_memo,
        },
        target: Some(target_snapshot),
    })
}

/// Dragging from a tab with an active unlock session, dropping onto a normal unlocked tab
/// (`PLAN.md` §5: "the memo is decrypted at the moment of drop and lands as plaintext `.md` in
/// the target ... not a one-way vault").
#[allow(clippy::too_many_arguments)]
pub(crate) fn move_cross_tab_session_to_plain(
    source_tab_id: &str,
    source_folder: &Path,
    target_tab_id: &str,
    target_folder: &Path,
    target_tab_name: &str,
    paths: &[NodePath],
    new_parent: Option<&str>,
    index: usize,
    copy: bool,
    registry: &SelfWriteRegistry,
    sessions: &LockSessionStore,
    search_index: &SearchIndex,
) -> AppResult<MemoMoveResult> {
    let Some(gathered) = gather_session_move(sessions, source_tab_id, source_folder, paths) else {
        return AppResult::err_code(ErrorCode::TabLocked, "source tab session ended");
    };
    let (nodes_to_move, plaintexts, tokens) = match gathered {
        Ok(v) => v,
        Err(payload) => return AppResult::err(payload),
    };
    let mut all_names = Vec::new();
    for node in &nodes_to_move {
        tree::collect_names(node, &mut all_names);
    }

    let mut target_loaded = tree::load_and_reconcile(target_folder);
    if let Some(p) = new_parent {
        if tree::find_node(&target_loaded.tree, p).is_none() {
            return AppResult::err_code(ErrorCode::NotFound, "target parent memo not found");
        }
    }
    for name in &all_names {
        if tree::contains_name(&target_loaded.tree, name) {
            return AppResult::err_code(ErrorCode::NameDuplicate, "a memo with this name already exists in the target tab");
        }
    }
    for name in &all_names {
        if let Some(plaintext) = plaintexts.get(name) {
            let dst = target_folder.join(format!("{name}.md"));
            if let Err(err) = atomic_write(&dst, plaintext, registry) {
                tracing::error!(error = %err, "move_cross_tab_session_to_plain: failed to write plaintext memo file");
                return AppResult::err_code(ErrorCode::Unknown, "failed to move memo file into target tab");
            }
            // The memo becomes indexable now that it lives in a plain (unlocked) tab, regardless
            // of `copy` — the still-locked source stayed excluded either way, so there is nothing
            // to remove there (`search-index/PLAN.md` §1).
            let (decoded, _encoding) = decode_memo_body(plaintext);
            search_index.upsert(target_tab_id, name, &decoded);
        }
    }
    for (i, node) in nodes_to_move.iter().enumerate() {
        let _ = tree::insert_node_at(&mut target_loaded.tree, new_parent, index + i, node.clone());
    }
    if let Err(err) = tree::save_tab_meta(target_folder, target_loaded.schema_version, target_loaded.app_version, &target_loaded.tree, target_loaded.last_viewed_memo.clone(), registry) {
        tracing::error!(error = %err, "move_cross_tab_session_to_plain: failed to save target tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save target tab metadata");
    }

    if !copy {
        if let Err(payload) = cleanup_session_move_source(sessions, registry, source_folder, source_tab_id, &all_names, &tokens) {
            return AppResult::err(payload);
        }
    }

    let source_snapshot = sessions.with_session(source_tab_id, session_tree_snapshot).unwrap_or_else(empty_locked_snapshot);
    AppResult::ok(MemoMoveResult {
        source: source_snapshot,
        target: Some(TreeSnapshot {
            nodes: target_loaded.tree,
            tab_name: target_tab_name.to_string(),
            locked: false,
            recovery_pending: false,
            recovery_kind: None,
            last_viewed_memo: target_loaded.last_viewed_memo,
        }),
    })
}

/// Both endpoints are locked tabs with an active session (possibly different tabs).
#[allow(clippy::too_many_arguments)]
pub(crate) fn move_cross_tab_session_to_session(
    source_tab_id: &str,
    source_folder: &Path,
    target_tab_id: &str,
    target_folder: &Path,
    paths: &[NodePath],
    new_parent: Option<&str>,
    index: usize,
    copy: bool,
    registry: &SelfWriteRegistry,
    sessions: &LockSessionStore,
) -> AppResult<MemoMoveResult> {
    let Some(gathered) = gather_session_move(sessions, source_tab_id, source_folder, paths) else {
        return AppResult::err_code(ErrorCode::TabLocked, "source tab session ended");
    };
    let (nodes_to_move, plaintexts, tokens) = match gathered {
        Ok(v) => v,
        Err(payload) => return AppResult::err(payload),
    };
    let mut all_names = Vec::new();
    for node in &nodes_to_move {
        tree::collect_names(node, &mut all_names);
    }

    if let Err(payload) = apply_session_move_target(sessions, registry, target_folder, target_tab_id, &nodes_to_move, &all_names, &plaintexts, new_parent, index) {
        return AppResult::err(payload);
    }

    if !copy {
        if let Err(payload) = cleanup_session_move_source(sessions, registry, source_folder, source_tab_id, &all_names, &tokens) {
            return AppResult::err(payload);
        }
    }

    let source_snapshot = sessions.with_session(source_tab_id, session_tree_snapshot).unwrap_or_else(empty_locked_snapshot);
    let target_snapshot = sessions.with_session(target_tab_id, session_tree_snapshot).unwrap_or_else(empty_locked_snapshot);
    AppResult::ok(MemoMoveResult {
        source: source_snapshot,
        target: Some(target_snapshot),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-tab-lock-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn classify_entry_states() {
        let dir = tempdir();
        let entry = FileMapEntry {
            memo_name: "Diary".to_string(),
            token: "abc123".to_string(),
        };

        // Neither exists yet.
        assert_eq!(classify_entry(&dir, &entry), EntryState::Anomalous);

        std::fs::write(dir.join("Diary.md"), b"hello").unwrap();
        assert_eq!(classify_entry(&dir, &entry), EntryState::NotRenamed);

        std::fs::remove_file(dir.join("Diary.md")).unwrap();
        std::fs::write(dir.join("abc123"), b"encrypted").unwrap();
        assert_eq!(classify_entry(&dir, &entry), EntryState::Renamed);

        std::fs::write(dir.join("Diary.md"), b"hello again").unwrap();
        assert_eq!(classify_entry(&dir, &entry), EntryState::Anomalous);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn check_marker_on_tree_open_auto_rolls_back_a_zero_progress_marker() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        std::fs::write(dir.join("Diary.md"), b"hello").unwrap();

        let marker = LockInProgressMarker {
            format_version: MARKER_FORMAT_VERSION,
            kdf: KdfParams::generate_new(),
            file_map: vec![FileMapEntry {
                memo_name: "Diary".to_string(),
                token: "abc123".to_string(),
            }],
        };
        write_marker(&dir, &marker, &registry).unwrap();

        assert!(matches!(check_marker_on_tree_open(&dir, &registry), MarkerCheck::NoMarker));
        assert!(!marker_path(&dir).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn check_marker_on_tree_open_reports_recovery_pending_when_any_entry_is_renamed() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        std::fs::write(dir.join("abc123"), b"encrypted").unwrap();

        let marker = LockInProgressMarker {
            format_version: MARKER_FORMAT_VERSION,
            kdf: KdfParams::generate_new(),
            file_map: vec![FileMapEntry {
                memo_name: "Diary".to_string(),
                token: "abc123".to_string(),
            }],
        };
        write_marker(&dir, &marker, &registry).unwrap();

        assert!(matches!(check_marker_on_tree_open(&dir, &registry), MarkerCheck::RecoveryPending));
        assert!(marker_path(&dir).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recovery_default_action_tie_break() {
        let dir = tempdir();
        // 2 renamed, 1 not_renamed -> strict majority renamed -> Finish.
        let marker = LockInProgressMarker {
            format_version: MARKER_FORMAT_VERSION,
            kdf: KdfParams::generate_new(),
            file_map: vec![
                FileMapEntry { memo_name: "A".into(), token: "ta".into() },
                FileMapEntry { memo_name: "B".into(), token: "tb".into() },
                FileMapEntry { memo_name: "C".into(), token: "tc".into() },
            ],
        };
        std::fs::write(dir.join("ta"), b"x").unwrap();
        std::fs::write(dir.join("tb"), b"x").unwrap();
        std::fs::write(dir.join("C.md"), b"x").unwrap();
        let c = classify_marker(&dir, &marker);
        assert_eq!(c.renamed, 2);
        assert_eq!(c.not_renamed, 1);
        let default_action = if c.total > 0 && c.renamed * 2 > c.total { RecoveryDefaultAction::Finish } else { RecoveryDefaultAction::Restore };
        assert_eq!(default_action, RecoveryDefaultAction::Finish);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rename_and_encrypt_entry_leaves_only_the_token_file() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        std::fs::write(dir.join("Diary.md"), b"secret content").unwrap();
        let entry = FileMapEntry {
            memo_name: "Diary".to_string(),
            token: "tok123".to_string(),
        };
        let key = LockKey::new(&[9u8; 32]);

        rename_and_encrypt_entry(&dir, &entry, &key, &registry).unwrap();

        assert!(!dir.join("Diary.md").exists());
        assert!(dir.join("tok123").exists());
        let bytes = std::fs::read(dir.join("tok123")).unwrap();
        let plaintext = lock_blob::decrypt_memo_body(&key, &bytes).unwrap();
        assert_eq!(plaintext, b"secret content");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// GH #17: forces `save_lock_blob`'s `atomic_write` call to fail deterministically by
    /// pre-creating the lock-blob target file and turning its `.bak` sibling into a directory —
    /// `atomic_write`'s "rotate previous target into `.bak`" step then hits `fs::rename`'s
    /// documented "to is a directory" error (`fs/atomic_write.rs`), the same shape of transient
    /// failure the issue describes for a real AV/indexer lock. Verifies `session_create_memo`
    /// leaves `session.tree`/`session.file_map` completely unchanged when this happens, even
    /// though the new memo's obfuscated body file was already written to disk beforehand (an
    /// accepted pre-existing limitation this issue explicitly scopes out).
    #[test]
    fn session_create_memo_rolls_back_session_state_when_save_lock_blob_fails() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        sessions.start(
            "tab-1".to_string(),
            LockSession {
                real_tab_name: "Private".to_string(),
                tree: vec![MetaTreeNode::default_for_new_file("Diary".to_string())],
                file_map: vec![FileMapEntry {
                    memo_name: "Diary".to_string(),
                    token: "tok-diary".to_string(),
                }],
                kdf: KdfParams::generate_new(),
                hint: None,
                key: [7u8; 32],
                last_viewed_memo: None,
            },
        );
        std::fs::write(dir.join(TAB_META_FILE_NAME), b"placeholder").unwrap();
        std::fs::create_dir(dir.join(format!("{TAB_META_FILE_NAME}.bak"))).unwrap();

        let before_tree = sessions.with_session("tab-1", |s| s.tree.clone()).unwrap();
        let before_file_map = sessions.with_session("tab-1", |s| s.file_map.clone()).unwrap();

        match session_create_memo(&sessions, &registry, &dir, "tab-1", "Notes", None) {
            AppResult::Err(err) => assert_eq!(err.code, ErrorCode::Unknown),
            AppResult::Ok(_) => panic!("expected the forced save_lock_blob failure to surface as an error"),
        }

        let after_tree = sessions.with_session("tab-1", |s| s.tree.clone()).unwrap();
        let after_file_map = sessions.with_session("tab-1", |s| s.file_map.clone()).unwrap();
        assert_eq!(before_tree, after_tree, "session.tree must be rolled back when save_lock_blob fails");
        assert_eq!(before_file_map, after_file_map, "session.file_map must be rolled back when save_lock_blob fails");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// GH #17, same failure-injection technique as the create-memo test above, applied to
    /// `session_delete_memo`: even though the obfuscated body file for "Diary" is already
    /// permanently deleted from disk before the (forced-to-fail) lock blob save runs, `session.
    /// tree`/`session.file_map` must still show "Diary" as present afterward — the in-memory
    /// mutation must not be committed when the save fails.
    #[test]
    fn session_delete_memo_rolls_back_session_state_when_save_lock_blob_fails() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        let token = "tok-diary".to_string();
        std::fs::write(dir.join(&token), b"encrypted-placeholder").unwrap();
        sessions.start(
            "tab-1".to_string(),
            LockSession {
                real_tab_name: "Private".to_string(),
                tree: vec![MetaTreeNode::default_for_new_file("Diary".to_string())],
                file_map: vec![FileMapEntry {
                    memo_name: "Diary".to_string(),
                    token: token.clone(),
                }],
                kdf: KdfParams::generate_new(),
                hint: None,
                key: [7u8; 32],
                last_viewed_memo: None,
            },
        );
        std::fs::write(dir.join(TAB_META_FILE_NAME), b"placeholder").unwrap();
        std::fs::create_dir(dir.join(format!("{TAB_META_FILE_NAME}.bak"))).unwrap();

        let before_tree = sessions.with_session("tab-1", |s| s.tree.clone()).unwrap();
        let before_file_map = sessions.with_session("tab-1", |s| s.file_map.clone()).unwrap();

        match session_delete_memo(&sessions, &registry, &dir, "tab-1", &["Diary".to_string()]) {
            AppResult::Err(err) => assert_eq!(err.code, ErrorCode::Unknown),
            AppResult::Ok(_) => panic!("expected the forced save_lock_blob failure to surface as an error"),
        }

        let after_tree = sessions.with_session("tab-1", |s| s.tree.clone()).unwrap();
        let after_file_map = sessions.with_session("tab-1", |s| s.file_map.clone()).unwrap();
        assert_eq!(before_tree, after_tree, "session.tree must be rolled back when save_lock_blob fails");
        assert_eq!(before_file_map, after_file_map, "session.file_map must be rolled back when save_lock_blob fails");
        // The physical obfuscated-file delete already happened before the failed save — an
        // accepted pre-existing limitation this issue explicitly scopes out (only the in-memory
        // tree/file_map and the final lock-blob save need to roll back).
        assert!(!dir.join(&token).exists(), "the physical delete is not expected to be rolled back");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Folder-import (`docs/plans/v1.1.0/folder-import/PLAN.md` §5) new-file path (`overwrite:
    /// false`): the imported memo lands in both `session.tree` and `session.file_map`, and its
    /// encrypted body is written under a fresh obfuscated token filename — the session-locked-tab
    /// counterpart of the plain path's `import_file` (`commands/import.rs`).
    #[test]
    fn session_import_file_creates_new_file_when_not_overwriting() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        sessions.start(
            "tab-1".to_string(),
            LockSession {
                real_tab_name: "Private".to_string(),
                tree: Vec::new(),
                file_map: Vec::new(),
                kdf: KdfParams::generate_new(),
                hint: None,
                key: [7u8; 32],
                last_viewed_memo: None,
            },
        );

        match session_import_file(&sessions, &registry, &dir, "tab-1", "Notes", b"hello world", false, None) {
            AppResult::Ok(_) => {}
            AppResult::Err(err) => panic!("expected success, got {err:?}"),
        }

        let tree = sessions.with_session("tab-1", |s| s.tree.clone()).unwrap();
        let file_map = sessions.with_session("tab-1", |s| s.file_map.clone()).unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].memo_name, "Notes");
        assert_eq!(file_map.len(), 1);
        assert_eq!(file_map[0].memo_name, "Notes");

        let bytes = std::fs::read(dir.join(&file_map[0].token)).unwrap();
        let plaintext = lock_blob::decrypt_memo_body(&LockKey::new(&[7u8; 32]), &bytes).unwrap();
        assert_eq!(plaintext, b"hello world");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Folder-import Overwrite-on-edit-locked-target rejection (`PLAN.md` §5, mirrors
    /// `commands/import.rs::find_node_ci`'s same defensive check for the plain path) — the
    /// frontend already disables the Overwrite control once it detects the collision target
    /// carries `locked: true` (`state/useImport.ts`'s `overwriteDisabled`), but the backend must
    /// independently refuse it too. Uses a target name that only case-insensitively matches the
    /// existing entry ("note" vs. the stored "Note") to also exercise the case-insensitive lookup
    /// this rejection relies on.
    #[test]
    fn session_import_file_rejects_overwrite_of_edit_locked_target() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        let token = "tok-note".to_string();
        std::fs::write(dir.join(&token), b"pre-existing-placeholder").unwrap();
        sessions.start(
            "tab-1".to_string(),
            LockSession {
                real_tab_name: "Private".to_string(),
                tree: vec![MetaTreeNode {
                    memo_name: "Note".to_string(),
                    expanded: true,
                    label: None,
                    editor: None,
                    locked: Some(true),
                    favorite: None,
                    children: Vec::new(),
                }],
                file_map: vec![FileMapEntry {
                    memo_name: "Note".to_string(),
                    token: token.clone(),
                }],
                kdf: KdfParams::generate_new(),
                hint: None,
                key: [7u8; 32],
                last_viewed_memo: None,
            },
        );

        match session_import_file(&sessions, &registry, &dir, "tab-1", "note", b"new content", true, None) {
            AppResult::Err(err) => assert_eq!(err.code, ErrorCode::Unknown),
            AppResult::Ok(_) => panic!("expected the edit-locked overwrite target to be rejected"),
        }

        // The physical file must be left untouched — the lock check runs before any write.
        let bytes = std::fs::read(dir.join(&token)).unwrap();
        assert_eq!(bytes, b"pre-existing-placeholder");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// GH #18: `cleanup_session_move_source` (the "not copy" cross-tab-move cleanup) must, like
    /// `session_delete_memo` above, delete the physical obfuscated file *before* mutating
    /// `tree`/`file_map`, and abort (propagating `Err`) without mutating them if that delete
    /// fails — otherwise a failed delete leaves an orphaned encrypted file on disk that's no
    /// longer referenced by the tab's tree/fileMap while the move still reports success. The
    /// token path is made a directory instead of a file so `std::fs::remove_file` deterministically
    /// fails on it (also within `with_retry`'s retry loop).
    #[test]
    fn cleanup_session_move_source_aborts_without_mutating_session_state_when_file_delete_fails() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        let token = "tok-diary".to_string();
        std::fs::create_dir(dir.join(&token)).unwrap();
        sessions.start(
            "tab-1".to_string(),
            LockSession {
                real_tab_name: "Private".to_string(),
                tree: vec![MetaTreeNode::default_for_new_file("Diary".to_string())],
                file_map: vec![FileMapEntry {
                    memo_name: "Diary".to_string(),
                    token: token.clone(),
                }],
                kdf: KdfParams::generate_new(),
                hint: None,
                key: [7u8; 32],
                last_viewed_memo: None,
            },
        );

        let before_tree = sessions.with_session("tab-1", |s| s.tree.clone()).unwrap();
        let before_file_map = sessions.with_session("tab-1", |s| s.file_map.clone()).unwrap();

        let all_names = vec!["Diary".to_string()];
        let mut tokens = HashMap::new();
        tokens.insert("Diary".to_string(), token.clone());

        match cleanup_session_move_source(&sessions, &registry, &dir, "tab-1", &all_names, &tokens) {
            Err(err) => assert_eq!(err.code, ErrorCode::Unknown),
            Ok(_) => panic!("expected the forced remove_file failure to surface as an error"),
        }

        let after_tree = sessions.with_session("tab-1", |s| s.tree.clone()).unwrap();
        let after_file_map = sessions.with_session("tab-1", |s| s.file_map.clone()).unwrap();
        assert_eq!(before_tree, after_tree, "session.tree must be left unchanged when the physical delete fails");
        assert_eq!(before_file_map, after_file_map, "session.file_map must be left unchanged when the physical delete fails");
        assert!(dir.join(&token).is_dir(), "the un-deletable token path must still be present");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// End-to-end integration test covering the whole feature's core data flow without going
    /// through the `#[tauri::command]` wrappers themselves (which need a live `State<T>`
    /// extraction not easily constructed outside a running `tauri::App`, same constraint already
    /// documented by `commands/editor.rs`'s own tests) — this test instead drives every
    /// `pub(crate)` building block `tab_lock`/`tab_unlock`/the session-path helpers are built
    /// from, in the same sequence those commands use them: lock (marker write -> rename+encrypt
    /// -> lock blob write -> marker delete) -> unlock (decrypt lock blob into a session) ->
    /// create/rename/move/delete within the session, re-verifying the on-disk lock blob after
    /// each mutation -> editor load/save through the session -> a cross-tab move out to a plain
    /// tab and back into a (second) locked session, verifying plaintext survives round-trip.
    #[tokio::test]
    async fn full_lock_unlock_session_round_trip() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        std::fs::write(dir.join("Diary.md"), b"page one").unwrap();
        std::fs::write(dir.join("Shopping list.md"), b"milk\neggs").unwrap();

        // ---- "tab_lock" steps 4-9, driven manually -------------------------------------------
        let loaded_tree = vec![
            MetaTreeNode::default_for_new_file("Diary".to_string()),
            MetaTreeNode::default_for_new_file("Shopping list".to_string()),
        ];
        let kdf_params = KdfParams::generate_new();
        let key_bytes = kdf::derive_key("hunter2".to_string(), kdf_params.clone()).await.unwrap();
        let key = LockKey::new(&key_bytes);

        let file_map: Vec<FileMapEntry> = ["Diary", "Shopping list"]
            .iter()
            .map(|name| FileMapEntry {
                memo_name: name.to_string(),
                token: obfuscated_naming::generate_unique_token(&dir),
            })
            .collect();
        let marker = LockInProgressMarker {
            format_version: MARKER_FORMAT_VERSION,
            kdf: kdf_params.clone(),
            file_map: file_map.clone(),
        };
        write_marker(&dir, &marker, &registry).unwrap();
        for entry in &file_map {
            rename_and_encrypt_entry(&dir, entry, &key, &registry).unwrap();
        }
        assert!(!dir.join("Diary.md").exists());
        assert!(!dir.join("Shopping list.md").exists());

        let lock_payload = LockPayload {
            real_tab_name: "Private".to_string(),
            tree: loaded_tree,
            file_map: file_map.clone(),
            last_viewed_memo: None,
        };
        let field = lock_blob::encrypt_lock_field(&key, kdf_params, Some("my hint".to_string()), &lock_payload).unwrap();
        write_lock_field(&dir, &field, &registry).unwrap();
        delete_marker(&dir, &registry).unwrap();
        assert!(!marker_path(&dir).exists());

        // ---- "tab_unlock", driven manually ----------------------------------------------------
        let (parsed, _recovery) = tree::load_tab_meta_with_recovery(&dir);
        let meta = parsed.expect("lock blob should parse");
        assert!(meta.tree.is_none(), "a locked tab's metadata must not carry a plaintext tree field");
        let stored_field: LockField = serde_json::from_value(meta.lock.expect("lock field present")).unwrap();

        let unlock_key_bytes = kdf::derive_key("hunter2".to_string(), stored_field.kdf.clone()).await.unwrap();
        let unlock_key = LockKey::new(&unlock_key_bytes);
        let decrypted_payload = lock_blob::decrypt_lock_field(&unlock_key, &stored_field).unwrap();
        assert_eq!(decrypted_payload.real_tab_name, "Private");
        assert_eq!(decrypted_payload.tree.len(), 2);

        // Wrong password must fail cleanly (`DecryptFailed`), not panic or leak partial content.
        let wrong_key_bytes = kdf::derive_key("wrong password".to_string(), stored_field.kdf.clone()).await.unwrap();
        let wrong_key = LockKey::new(&wrong_key_bytes);
        assert!(lock_blob::decrypt_lock_field(&wrong_key, &stored_field).is_err());

        let sessions = LockSessionStore::new();
        sessions.start(
            "tab-1".to_string(),
            LockSession {
                real_tab_name: decrypted_payload.real_tab_name,
                tree: decrypted_payload.tree,
                file_map: decrypted_payload.file_map,
                kdf: stored_field.kdf,
                hint: stored_field.hint,
                key: unlock_key_bytes,
                last_viewed_memo: decrypted_payload.last_viewed_memo,
            },
        );
        assert!(sessions.has_session("tab-1"));

        // ---- session-path structural mutations, each re-verified against the on-disk blob -----
        let snapshot = session_create_memo(&sessions, &registry, &dir, "tab-1", "Notes", None);
        let AppResult::Ok(snapshot) = snapshot else { panic!("expected Ok") };
        assert_eq!(snapshot.nodes.len(), 3);
        assert_eq!(snapshot.tab_name, "Private");

        let snapshot = session_rename_memo(&sessions, &registry, &dir, "tab-1", "Notes", "Ideas");
        let AppResult::Ok(snapshot) = snapshot else { panic!("expected Ok") };
        assert!(tree::find_node(&snapshot.nodes, "Ideas").is_some());
        assert!(tree::find_node(&snapshot.nodes, "Notes").is_none());

        let snapshot = session_move_within(&sessions, &registry, &dir, "tab-1", &["Ideas".to_string()], Some("Diary"), 0);
        let AppResult::Ok(snapshot) = snapshot else { panic!("expected Ok") };
        let diary = tree::find_node(&snapshot.nodes, "Diary").unwrap();
        assert_eq!(diary.children.len(), 1);
        assert_eq!(diary.children[0].memo_name, "Ideas");

        // Re-verify the persisted lock blob agrees with the in-memory session after every save.
        {
            let (parsed, _r) = tree::load_tab_meta_with_recovery(&dir);
            let field: LockField = serde_json::from_value(parsed.unwrap().lock.unwrap()).unwrap();
            let payload = lock_blob::decrypt_lock_field(&LockKey::new(&unlock_key_bytes), &field).unwrap();
            assert_eq!(payload.file_map.len(), 3);
            let diary = tree::find_node(&payload.tree, "Diary").unwrap();
            assert_eq!(diary.children[0].memo_name, "Ideas");
        }

        // ---- editor load/save through the session ---------------------------------------------
        let active_memo = ActiveMemoTracker::new();
        let content = session_editor_load(&sessions, &active_memo, &dir, "tab-1", "Shopping list");
        let AppResult::Ok(content) = content else { panic!("expected Ok") };
        assert_eq!(content.text, "milk\neggs");
        assert!(active_memo.is_active("tab-1", "Shopping list"));

        let save_result = session_editor_save(&sessions, &registry, &dir, "tab-1", "Shopping list", "milk\neggs\nbread", &content.hash, EolPolicy::Lf, "utf-8");
        let AppResult::Ok(save_result) = save_result else { panic!("expected Ok") };
        assert_ne!(save_result.hash, content.hash);

        // A stale hash must be rejected as a conflict, exactly like the plain-tab path.
        let stale_save = session_editor_save(&sessions, &registry, &dir, "tab-1", "Shopping list", "different content", &content.hash, EolPolicy::Lf, "utf-8");
        match stale_save {
            AppResult::Err(err) => assert_eq!(err.code, ErrorCode::Conflict),
            _ => panic!("expected a Conflict error for a stale hash"),
        }

        let reloaded = session_editor_load(&sessions, &active_memo, &dir, "tab-1", "Shopping list");
        let AppResult::Ok(reloaded) = reloaded else { panic!("expected Ok") };
        assert_eq!(reloaded.text, "milk\neggs\nbread");

        // ---- delete within the session ---------------------------------------------------------
        let snapshot = session_delete_memo(&sessions, &registry, &dir, "tab-1", &["Diary".to_string()]);
        let AppResult::Ok(snapshot) = snapshot else { panic!("expected Ok") };
        assert!(tree::find_node(&snapshot.nodes, "Diary").is_none());
        assert!(tree::find_node(&snapshot.nodes, "Ideas").is_none(), "deleting a node deletes its whole subtree");

        // ---- cross-tab move: session -> plain, then plain -> a second locked session ----------
        // Also exercises `search-index/PLAN.md` §1's rule via each function's own
        // `search_index` parameter: content becomes indexable the moment it lands in a plain
        // tab, and is removed the moment it leaves one for a locked session.
        let search_index = SearchIndex::new();
        let plain_dir = tempdir();
        let move_result = move_cross_tab_session_to_plain("tab-1", &dir, "plain-tab", &plain_dir, "Plain Target", &["Shopping list".to_string()], None, 0, false, &registry, &sessions, &search_index);
        let AppResult::Ok(move_result) = move_result else { panic!("expected Ok") };
        assert!(tree::find_node(&move_result.source.nodes, "Shopping list").is_none());
        let target_snapshot = move_result.target.expect("target snapshot present");
        assert!(tree::find_node(&target_snapshot.nodes, "Shopping list").is_some());
        let plaintext_bytes = std::fs::read(plain_dir.join("Shopping list.md")).unwrap();
        assert_eq!(plaintext_bytes, b"milk\neggs\nbread");
        assert!(
            search_index.snapshot().iter().any(|e| e.tab_id == "plain-tab" && e.memo_name == "Shopping list" && e.plaintext.contains("milk")),
            "moving out of a locked session into a plain tab must index the memo"
        );

        // Second locked tab, its own independent session.
        let second_dir = tempdir();
        let second_key_bytes = [42u8; 32];
        sessions.start(
            "tab-2".to_string(),
            LockSession {
                real_tab_name: "Second Tab".to_string(),
                tree: vec![],
                file_map: vec![],
                kdf: KdfParams::generate_new(),
                hint: None,
                key: second_key_bytes,
                last_viewed_memo: None,
            },
        );
        let move_back = move_cross_tab_plain_to_session("plain-tab", &plain_dir, "Plain Target", "tab-2", &second_dir, &["Shopping list".to_string()], None, 0, false, &registry, &sessions, &search_index);
        let AppResult::Ok(move_back) = move_back else { panic!("expected Ok") };
        assert!(tree::find_node(&move_back.source.nodes, "Shopping list").is_none());
        assert!(!plain_dir.join("Shopping list.md").exists());
        let second_snapshot = move_back.target.expect("target snapshot present");
        assert!(tree::find_node(&second_snapshot.nodes, "Shopping list").is_some());
        assert!(
            !search_index.snapshot().iter().any(|e| e.tab_id == "plain-tab"),
            "moving out of a plain tab into a locked session must remove the memo from the index"
        );

        // The moved memo is now encrypted under `tab-2`'s own key/token — decrypt it directly to
        // confirm content survived two round trips (session -> plaintext -> a different session).
        let second_key = LockKey::new(&second_key_bytes);
        let token = sessions.with_session("tab-2", |s| s.file_map[0].token.clone()).unwrap();
        let encrypted_bytes = std::fs::read(second_dir.join(&token)).unwrap();
        let final_plaintext = lock_blob::decrypt_memo_body(&second_key, &encrypted_bytes).unwrap();
        assert_eq!(final_plaintext, b"milk\neggs\nbread");

        // ---- tab_end_session leaves the tab genuinely inaccessible again -----------------------
        sessions.end("tab-1");
        assert!(!sessions.has_session("tab-1"));
        match session_editor_load(&sessions, &active_memo, &dir, "tab-1", "Shopping list") {
            AppResult::Err(err) => assert_eq!(err.code, ErrorCode::TabLocked),
            AppResult::Ok(_) => panic!("expected an error once the session has ended"),
        }

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&plain_dir).ok();
        std::fs::remove_dir_all(&second_dir).ok();
    }

    // -----------------------------------------------------------------------
    // Permanent unlock (`docs/plans/tab-lock-permanent-unlock/PLAN.md` +
    // `CRASH-RECOVERY.md`)
    // -----------------------------------------------------------------------

    #[test]
    fn classify_unlock_entry_states() {
        let dir = tempdir();
        let entry = FileMapEntry {
            memo_name: "Diary".to_string(),
            token: "abc123".to_string(),
        };

        // Neither exists yet.
        assert_eq!(classify_unlock_entry(&dir, &entry), UnlockEntryState::Anomalous);

        std::fs::write(dir.join("abc123"), b"encrypted").unwrap();
        assert_eq!(classify_unlock_entry(&dir, &entry), UnlockEntryState::NotRestored);

        std::fs::remove_file(dir.join("abc123")).unwrap();
        std::fs::write(dir.join("Diary.md"), b"hello").unwrap();
        assert_eq!(classify_unlock_entry(&dir, &entry), UnlockEntryState::Restored);

        std::fs::write(dir.join("abc123"), b"encrypted again").unwrap();
        assert_eq!(classify_unlock_entry(&dir, &entry), UnlockEntryState::Anomalous);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Mirrors `check_marker_on_tree_open_auto_rolls_back_a_zero_progress_marker`'s "zero
    /// renamed" case, opposite direction (`CRASH-RECOVERY.md` §3.2): with no `.md` file present
    /// anywhere under the tab folder, the marker is silently deleted with no password/dialog.
    #[test]
    fn check_unlock_marker_on_tree_open_auto_rolls_back_a_zero_restored_marker() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let store = AppSettingsStore::new();
        let sessions = LockSessionStore::new();
        let search_index = SearchIndex::new();
        std::fs::write(dir.join("tok123"), b"encrypted").unwrap();

        let marker = UnlockInProgressMarker {
            format_version: UNLOCK_MARKER_FORMAT_VERSION,
        };
        write_unlock_marker(&dir, &marker, &registry).unwrap();

        assert!(matches!(
            check_unlock_marker_on_tree_open(&dir, &registry, &store, &sessions, &search_index, "tab-x"),
            UnlockMarkerCheck::NoMarker
        ));
        assert!(!unlock_marker_path(&dir).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `CRASH-RECOVERY.md` §3: with at least one `.md` file present and `.tabtreenote.json` not
    /// yet showing a plain `tree` field, classification cannot be auto-resolved without a
    /// password — the marker is left in place and `RecoveryPending` is reported.
    #[test]
    fn check_unlock_marker_on_tree_open_reports_recovery_pending_when_a_real_file_is_present() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let store = AppSettingsStore::new();
        let sessions = LockSessionStore::new();
        let search_index = SearchIndex::new();
        std::fs::write(dir.join("Diary.md"), b"hello").unwrap();

        let marker = UnlockInProgressMarker {
            format_version: UNLOCK_MARKER_FORMAT_VERSION,
        };
        write_unlock_marker(&dir, &marker, &registry).unwrap();

        assert!(matches!(
            check_unlock_marker_on_tree_open(&dir, &registry, &store, &sessions, &search_index, "tab-x"),
            UnlockMarkerCheck::RecoveryPending
        ));
        assert!(unlock_marker_path(&dir).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn permanent_unlock_recovery_default_action_tie_break() {
        let dir = tempdir();
        // 2 restored, 1 not_restored -> strict majority restored -> FinishUnlocking.
        let file_map = vec![
            FileMapEntry { memo_name: "A".into(), token: "ta".into() },
            FileMapEntry { memo_name: "B".into(), token: "tb".into() },
            FileMapEntry { memo_name: "C".into(), token: "tc".into() },
        ];
        std::fs::write(dir.join("A.md"), b"x").unwrap();
        std::fs::write(dir.join("B.md"), b"x").unwrap();
        std::fs::write(dir.join("tc"), b"x").unwrap();
        let c = classify_unlock_entries(&dir, &file_map);
        assert_eq!(c.restored, 2);
        assert_eq!(c.not_restored, 1);
        let default_action = if c.total > 0 && c.restored * 2 > c.total {
            PermanentUnlockRecoveryDefaultAction::FinishUnlocking
        } else {
            PermanentUnlockRecoveryDefaultAction::RestoreLock
        };
        assert_eq!(default_action, PermanentUnlockRecoveryDefaultAction::FinishUnlocking);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Regression test (reviewer-lite finding): `#[serde(rename_all = "camelCase")]` on the
    /// `PermanentUnlockRecoveryStatus` enum only renames the variant tag (`"kind"`), not a
    /// struct-like variant's own field names — without a per-variant `rename_all` on `Classified`
    /// too, its fields would silently serialize as snake_case (`restored_count`, ...) while
    /// `src/shared/models.ts`'s TS type expects camelCase (`restoredCount`, ...), so every one of
    /// these fields would deserialize as `undefined` on the frontend with no compiler/runtime
    /// error anywhere in the Rust side to catch it. Asserts on the actual serialized JSON shape
    /// (`serde_json::to_value`), not just the Rust struct, since that's exactly the layer the bug
    /// lived in.
    #[test]
    fn permanent_unlock_recovery_status_classified_serializes_fields_as_camel_case() {
        let status = PermanentUnlockRecoveryStatus::Classified {
            restored_count: 2,
            not_restored_count: 1,
            anomalous_count: 0,
            total: 3,
            default_action: PermanentUnlockRecoveryDefaultAction::FinishUnlocking,
            anomalous: false,
        };
        let json = serde_json::to_value(&status).unwrap();

        assert_eq!(json["kind"], "classified");
        assert_eq!(json["restoredCount"], 2);
        assert_eq!(json["notRestoredCount"], 1);
        assert_eq!(json["anomalousCount"], 0);
        assert_eq!(json["total"], 3);
        assert_eq!(json["defaultAction"], "finishUnlocking");
        assert_eq!(json["anomalous"], false);

        // The snake_case field names must NOT be present — this is the exact shape that fooled
        // the frontend's `Result<PermanentUnlockRecoveryStatus>` parsing into silent `undefined`s.
        assert!(json.get("restored_count").is_none());
        assert!(json.get("not_restored_count").is_none());
        assert!(json.get("anomalous_count").is_none());
        assert!(json.get("default_action").is_none());
    }

    /// `PLAN.md` §4.4's `.bak`-companion cleanup (`DECISIONS.md`'s follow-up review, the
    /// GH #18-class residue) — a leftover `<token>.bak` file (e.g. from an earlier
    /// `atomic_write` rotation) is removed as part of restoring the entry, alongside the primary
    /// token file.
    #[test]
    fn restore_one_permanent_unlock_entry_restores_plaintext_and_cleans_bak_companion() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let key = LockKey::new(&[11u8; 32]);
        let entry = FileMapEntry {
            memo_name: "Diary".to_string(),
            token: "tok456".to_string(),
        };
        let encrypted = lock_blob::encrypt_memo_body(&key, b"secret content");
        std::fs::write(dir.join(&entry.token), &encrypted).unwrap();
        // A leftover `.bak` companion, orphaned from an earlier write.
        std::fs::write(dir.join("tok456.bak"), b"stale encrypted leftover").unwrap();

        restore_one_permanent_unlock_entry(&dir, &entry, &key, &registry).unwrap();

        assert!(!dir.join("tok456").exists());
        assert!(!dir.join("tok456.bak").exists(), "the orphaned .bak companion must also be cleaned up");
        assert_eq!(std::fs::read(dir.join("Diary.md")).unwrap(), b"secret content");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `PLAN.md` §5 step 7's abort-on-anomalous-entry policy, driven directly against the same
    /// building blocks `tab_permanent_unlock` itself uses (the command function needs a live
    /// `State<T>` extraction not easily constructed outside a running `tauri::App`, same
    /// constraint already documented by `full_lock_unlock_session_round_trip` above): once an
    /// `Anomalous` entry is found, the whole operation stops immediately — entries already
    /// restored before it are left restored (no rollback), entries after it are left completely
    /// untouched, and the marker stays on disk for the crash-recovery flow to pick up.
    #[test]
    fn permanent_unlock_restore_loop_aborts_on_anomalous_entry_without_rollback() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let key = LockKey::new(&[13u8; 32]);

        let file_map = vec![
            FileMapEntry { memo_name: "First".into(), token: "tok-first".into() },
            FileMapEntry { memo_name: "Second".into(), token: "tok-second".into() },
            FileMapEntry { memo_name: "Third".into(), token: "tok-third".into() },
        ];
        // "First" is already restored (both `.bak`-cleanup-relevant primary files consistent).
        std::fs::write(dir.join("First.md"), b"first content").unwrap();
        // "Second" is anomalous: both the real file and the token happen to be present.
        std::fs::write(dir.join("Second.md"), b"second content").unwrap();
        std::fs::write(dir.join("tok-second"), b"second token bytes").unwrap();
        // "Third" is not yet restored (only the token exists).
        let encrypted_third = lock_blob::encrypt_memo_body(&key, b"third content");
        std::fs::write(dir.join("tok-third"), &encrypted_third).unwrap();

        let marker = UnlockInProgressMarker {
            format_version: UNLOCK_MARKER_FORMAT_VERSION,
        };
        write_unlock_marker(&dir, &marker, &registry).unwrap();

        // Same shape as `tab_permanent_unlock` step 7's loop.
        let mut aborted = false;
        for entry_fm in &file_map {
            match classify_unlock_entry(&dir, entry_fm) {
                UnlockEntryState::Restored => continue,
                UnlockEntryState::Anomalous => {
                    aborted = true;
                    break;
                }
                UnlockEntryState::NotRestored => {
                    restore_one_permanent_unlock_entry(&dir, entry_fm, &key, &registry).unwrap();
                }
            }
        }

        assert!(aborted, "the loop must stop at the anomalous entry");
        // "First" was already restored before the loop ran and is untouched either way.
        assert!(dir.join("First.md").exists());
        // "Second" (the anomalous entry) is left exactly as found — both files still present.
        assert!(dir.join("Second.md").exists());
        assert!(dir.join("tok-second").exists());
        // "Third" comes after the anomalous entry in iteration order and must never be reached.
        assert!(!dir.join("Third.md").exists(), "an entry after the anomalous one must not be processed");
        assert!(dir.join("tok-third").exists(), "an entry after the anomalous one must not be processed");
        // The marker is left in place for the crash-recovery flow.
        assert!(unlock_marker_path(&dir).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// End-to-end integration test for the whole feature's core data flow, in the same style as
    /// `full_lock_unlock_session_round_trip` above (driving the `pub(crate)` building blocks
    /// directly rather than the `#[tauri::command]` wrappers, which need a live `State<T>`
    /// extraction). Covers both entry points: (a) permanently unlocking from within an active
    /// session (no password), and (b) permanently unlocking a second, session-less locked tab
    /// directly with its password, via `resolve_key_and_lock_payload`.
    #[tokio::test]
    async fn full_lock_unlock_permanent_unlock_round_trip() {
        // ---- Entry point (a): lock -> unlock into a session -> permanently unlock via the session
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        std::fs::write(dir.join("Diary.md"), b"page one").unwrap();

        let kdf_params = KdfParams::generate_new();
        let key_bytes = kdf::derive_key("hunter2".to_string(), kdf_params.clone()).await.unwrap();
        let key = LockKey::new(&key_bytes);
        let file_map = vec![FileMapEntry {
            memo_name: "Diary".to_string(),
            token: obfuscated_naming::generate_unique_token(&dir),
        }];
        let marker = LockInProgressMarker {
            format_version: MARKER_FORMAT_VERSION,
            kdf: kdf_params.clone(),
            file_map: file_map.clone(),
        };
        write_marker(&dir, &marker, &registry).unwrap();
        for entry in &file_map {
            rename_and_encrypt_entry(&dir, entry, &key, &registry).unwrap();
        }
        let tree = vec![MetaTreeNode::default_for_new_file("Diary".to_string())];
        let lock_payload = LockPayload {
            real_tab_name: "Private".to_string(),
            tree: tree.clone(),
            file_map: file_map.clone(),
            last_viewed_memo: None,
        };
        let field = lock_blob::encrypt_lock_field(&key, kdf_params, None, &lock_payload).unwrap();
        write_lock_field(&dir, &field, &registry).unwrap();
        delete_marker(&dir, &registry).unwrap();

        let sessions = LockSessionStore::new();
        sessions.start(
            "tab-1".to_string(),
            LockSession {
                real_tab_name: lock_payload.real_tab_name.clone(),
                tree: lock_payload.tree.clone(),
                file_map: lock_payload.file_map.clone(),
                kdf: field.kdf.clone(),
                hint: field.hint.clone(),
                key: key_bytes,
                last_viewed_memo: lock_payload.last_viewed_memo.clone(),
            },
        );

        // ---- `tab_permanent_unlock` steps 5-9, driven manually, session branch (password: None)
        let (resolved_key, resolved_payload) = resolve_key_and_lock_payload(&dir, &sessions, "tab-1", None).await.unwrap();
        let unlock_marker = UnlockInProgressMarker {
            format_version: UNLOCK_MARKER_FORMAT_VERSION,
        };
        write_unlock_marker(&dir, &unlock_marker, &registry).unwrap();
        for entry_fm in &resolved_payload.file_map {
            match classify_unlock_entry(&dir, entry_fm) {
                UnlockEntryState::Restored => continue,
                UnlockEntryState::Anomalous => panic!("no anomalous entry expected"),
                UnlockEntryState::NotRestored => {
                    restore_one_permanent_unlock_entry(&dir, entry_fm, &resolved_key, &registry).unwrap();
                }
            }
        }
        tree::save_tab_meta(&dir, TAB_META_SCHEMA_VERSION, env!("CARGO_PKG_VERSION").to_string(), &resolved_payload.tree, resolved_payload.last_viewed_memo.clone(), &registry).unwrap();
        delete_unlock_marker(&dir, &registry).unwrap();
        sessions.end("tab-1");

        // Verify: plain memo file back, plain tree in metadata (no `lock` field), token/marker gone.
        assert_eq!(std::fs::read(dir.join("Diary.md")).unwrap(), b"page one");
        assert!(!dir.join(&file_map[0].token).exists());
        assert!(!unlock_marker_path(&dir).exists());
        let (parsed, _recovery) = tree::load_tab_meta_with_recovery(&dir);
        let meta = parsed.expect("metadata should parse");
        assert!(meta.lock.is_none(), "a permanently-unlocked tab must not carry a `lock` field");
        assert_eq!(meta.tree.unwrap().len(), 1);
        assert!(!sessions.has_session("tab-1"));

        // ---- Entry point (b): a second locked tab, no session, permanently unlocked by password
        let dir2 = tempdir();
        std::fs::write(dir2.join("Notes.md"), b"second tab content").unwrap();
        let kdf_params2 = KdfParams::generate_new();
        let key_bytes2 = kdf::derive_key("hunter3".to_string(), kdf_params2.clone()).await.unwrap();
        let key2 = LockKey::new(&key_bytes2);
        let file_map2 = vec![FileMapEntry {
            memo_name: "Notes".to_string(),
            token: obfuscated_naming::generate_unique_token(&dir2),
        }];
        for entry in &file_map2 {
            rename_and_encrypt_entry(&dir2, entry, &key2, &registry).unwrap();
        }
        let tree2 = vec![MetaTreeNode::default_for_new_file("Notes".to_string())];
        let lock_payload2 = LockPayload {
            real_tab_name: "Second Private".to_string(),
            tree: tree2,
            file_map: file_map2.clone(),
            last_viewed_memo: None,
        };
        let field2 = lock_blob::encrypt_lock_field(&key2, kdf_params2, None, &lock_payload2).unwrap();
        write_lock_field(&dir2, &field2, &registry).unwrap();

        // A wrong password must fail cleanly (`DecryptFailed`), leaving the locked tab untouched.
        let no_sessions = LockSessionStore::new();
        match resolve_key_and_lock_payload(&dir2, &no_sessions, "tab-2", Some("wrong password".to_string())).await {
            Err(err) => assert_eq!(err.code, ErrorCode::DecryptFailed),
            Ok(_) => panic!("expected a DecryptFailed error for a wrong password"),
        }

        // No active session for "tab-2" — `resolve_key_and_lock_payload` must fall back to
        // deriving the key from the given password.
        let (resolved_key2, resolved_payload2) = resolve_key_and_lock_payload(&dir2, &no_sessions, "tab-2", Some("hunter3".to_string())).await.unwrap();
        assert_eq!(resolved_payload2.real_tab_name, "Second Private");

        let unlock_marker2 = UnlockInProgressMarker {
            format_version: UNLOCK_MARKER_FORMAT_VERSION,
        };
        write_unlock_marker(&dir2, &unlock_marker2, &registry).unwrap();
        for entry_fm in &resolved_payload2.file_map {
            if classify_unlock_entry(&dir2, entry_fm) == UnlockEntryState::NotRestored {
                restore_one_permanent_unlock_entry(&dir2, entry_fm, &resolved_key2, &registry).unwrap();
            }
        }
        tree::save_tab_meta(&dir2, TAB_META_SCHEMA_VERSION, env!("CARGO_PKG_VERSION").to_string(), &resolved_payload2.tree, resolved_payload2.last_viewed_memo.clone(), &registry).unwrap();
        delete_unlock_marker(&dir2, &registry).unwrap();

        assert_eq!(std::fs::read(dir2.join("Notes.md")).unwrap(), b"second tab content");
        assert!(!dir2.join(&file_map2[0].token).exists());
        assert!(!unlock_marker_path(&dir2).exists());
        let (parsed2, _recovery2) = tree::load_tab_meta_with_recovery(&dir2);
        let meta2 = parsed2.expect("metadata should parse");
        assert!(meta2.lock.is_none());

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&dir2).ok();
    }
}
