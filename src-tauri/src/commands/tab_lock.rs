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
use crate::commands::image::plain_tab_image_dir;
use crate::commands::tree::{self, MemoMoveResult, MemoMutationResult, MemoMutationWarning, MemoProperties, TreeSnapshot};
use crate::commands::emit_settings_recovery_event;
use crate::crypto::kdf::{self, KdfParams};
use crate::crypto::lock_blob::{self, FileMapEntry, ImageFolderEntry, LockField, LockKey, LockPayload};
use crate::fs::atomic_write::{atomic_write, bak_path};
use crate::fs::image_folder_transfer::{transfer_all, undo_transfers, FolderTransfer};
use crate::fs::path_safety::{existing_direct_child_file, existing_memo_image_folder, memo_image_folder_path};
use crate::fs::trash;
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
    /// GitHub issue #272: ロック時に `<メモ名>/` 画像フォルダを改名する予定の対応表(メモ名 -> ランダム名)。
    /// 何かを改名する前にマーカーへ書くので、中断時の復旧はこれを見て前進/巻き戻しできる。
    /// 追加フィールドのみ(`format_version` は据え置き)。#272 以前のマーカーは空として読む。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    image_folders: Vec<ImageFolderEntry>,
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
    /// GitHub issue #273: 完全解除で `<メモ名>/` へ戻す予定の画像フォルダのランダム名(計画)。
    /// 何かを改名する前に書く。ランダム名だけなのでメモ名は漏れない。#273 以前のマーカーは空として読む。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    image_folders: Vec<String>,
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
        // GitHub issue #272: 本文はまだ平文・未書き換えなので、ランダム名へ改名済みの画像フォルダを
        // `<メモ名>/` へ戻せば元どおり(パスワード不要)。戻せないフォルダが残ればマーカーを残し、
        // 次回 `tree_open` で再試行する(平文タブとしては開ける)。
        if !rollback_image_folders(folder, &marker.image_folders, registry) {
            tracing::error!("check_marker_on_tree_open: some image folders could not be renamed back; marker kept for retry");
            return MarkerCheck::NoMarker;
        }
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
    let Some(marker) = load_unlock_marker(folder) else {
        return UnlockMarkerCheck::NoMarker;
    };

    // Case "zero restored" (§3.2): token files have no `.md` extension by construction, so the
    // presence of any `.md` file directly under the tab folder is a cheap, password-free proxy
    // for "at least one entry may already be restored." A false "not zero" classification merely
    // costs the user a password prompt that turns out not to have been strictly necessary — this
    // branch only ever deletes the marker, touching no other file.
    // #273: 計画した画像フォルダが 1 つでもランダム名から消えていれば(`<メモ名>/` へ改名済みの
    // 可能性)、自動削除せずパスワード付きの復旧へ回す(暗号化本文のリンクはランダム名を指すため)。
    let no_folder_renamed = marker.image_folders.iter().all(|random| existing_memo_image_folder(folder, random).is_some());
    if tree::scan_real_memo_names(folder).is_empty() && no_folder_renamed {
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

    // Steps 6-9 (marker, image-folder conversion, rename+encrypt, lock blob, marker delete).
    if let Err(payload) = lock_folder_files(&folder, loaded.tree, loaded.last_viewed_memo, tab_name, &key, kdf_params, hint, &registry) {
        return AppResult::err(payload);
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

/// `tab_lock` steps 6-9 — everything that touches the tab folder, split out of the command so it
/// can be unit-tested without an `AppHandle`.
///
/// GitHub issue #272 ordering (also `docs/plans/tab-lock/CRASH-RECOVERY.md` "Image Folders"):
/// 1. Step 6: plan `fileMap` AND the image-folder map (`<memo>/` -> fresh random name, only for
///    memos that have a real `<memo>/` folder, #263 guard), write both into the marker.
/// 2. Step 6b: rename every planned folder `<memo>/` -> `<random>/`. Plaintext bodies are NOT
///    touched in this phase. If any rename fails, the already-renamed folders are renamed back,
///    the marker is deleted and the lock is aborted (`ImageFolderRenameFailed`; user decision on
///    #272 — never lock with a folder still named after its memo).
/// 3. Step 7: per memo, rewrite `<memo>/…` image links to `<random>/…` in memory and encrypt the
///    rewritten body into the token file (the on-disk plaintext is never rewritten, so an
///    interrupted lock can always be undone exactly by renaming folders back).
/// 4. Step 8: lock blob, with the image-folder map. 5. Step 9: delete the marker.
#[allow(clippy::too_many_arguments)]
fn lock_folder_files(
    folder: &Path,
    tree_nodes: Vec<MetaTreeNode>,
    last_viewed_memo: Option<String>,
    tab_name: String,
    key: &LockKey,
    kdf_params: KdfParams,
    hint: Option<String>,
    registry: &SelfWriteRegistry,
) -> Result<(), AppErrorPayload> {
    // Step 6: full planned `fileMap` + image-folder map, computed up front; write the marker.
    let mut all_names = Vec::new();
    for node in &tree_nodes {
        tree::collect_names(node, &mut all_names);
    }
    let file_map: Vec<FileMapEntry> = all_names
        .iter()
        .map(|name| FileMapEntry {
            memo_name: name.clone(),
            token: obfuscated_naming::generate_unique_token(folder),
        })
        .collect();
    let image_folders = plan_lock_image_folders(folder, &all_names)?;
    let marker = LockInProgressMarker {
        format_version: MARKER_FORMAT_VERSION,
        kdf: kdf_params.clone(),
        file_map: file_map.clone(),
        image_folders: image_folders.clone(),
    };
    if let Err(err) = write_marker(folder, &marker, registry) {
        tracing::error!(error = %err, "tab_lock: failed to write in-progress marker");
        return Err(AppErrorPayload::new(ErrorCode::Unknown, "failed to start locking this tab"));
    }

    // Step 6b (#272): rename `<memo>/` image folders to their random names, or abort + roll back.
    if let Err(rollback_complete) = convert_image_folders_for_lock(folder, &image_folders, registry) {
        if rollback_complete {
            if let Err(err) = delete_marker(folder, registry) {
                // 残ったマーカーは進捗ゼロ扱いで次回 `tree_open` が黙って消す
                tracing::error!(error = %err, "tab_lock: failed to delete the marker after rolling back image folders");
            }
        } else {
            tracing::error!("tab_lock: some image folders could not be renamed back; marker left for tree_open to retry");
        }
        return Err(AppErrorPayload::new(ErrorCode::ImageFolderRenameFailed, "failed to rename a memo image folder; lock aborted"));
    }

    // Step 7: rename+encrypt every entry. On failure, the marker is left in place for
    // `tree_open`'s crash-recovery flow to resolve on next visit rather than attempting an
    // immediate ad hoc rollback here (reuses one recovery path instead of two).
    for entry_fm in &file_map {
        let image_folder = image_folder_for(&image_folders, &entry_fm.memo_name);
        if let Err(payload) = rename_and_encrypt_entry(folder, entry_fm, image_folder, key, registry) {
            tracing::error!(?payload, "tab_lock: failed mid-way through encrypting memo files; marker left for crash recovery");
            return Err(payload);
        }
    }

    // Step 8: build + write the lock blob. `last_viewed_memo` carries forward the pre-lock plain
    // tab's own value (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §1's "data-model symmetry" —
    // even a just-locked tab remembers what was last viewed, though it's never read back
    // automatically for a locked tab, `PLAN.md` §6).
    let payload = LockPayload {
        real_tab_name: tab_name,
        tree: tree_nodes,
        file_map,
        last_viewed_memo,
        image_folders,
    };
    let field = lock_blob::encrypt_lock_field(key, kdf_params, hint, &payload)
        .map_err(|_| AppErrorPayload::new(ErrorCode::Unknown, "failed to build the encrypted tab metadata"))?;
    if let Err(err) = write_lock_field(folder, &field, registry) {
        tracing::error!(error = %err, "tab_lock: failed to write the encrypted lock blob; marker left for crash recovery");
        return Err(AppErrorPayload::new(ErrorCode::Unknown, "failed to save the locked tab metadata"));
    }

    // Step 8b (GitHub issue #276): 暗号化状態が確定したので平文の `.bak`/一時ファイルを消す。
    // マーカー削除より前に行うので、ここで中断してもマーカーが残り、復旧「ロックを完了」が再実行する。
    remove_plaintext_residue(folder, &payload.file_map, true, registry);

    // Step 9: delete the marker — the sole success signal the recovery check relies on.
    if let Err(err) = delete_marker(folder, registry) {
        tracing::error!(error = %err, "tab_lock: failed to delete the in-progress marker after a successful lock");
        return Err(AppErrorPayload::new(ErrorCode::Unknown, "tab was locked, but a temporary recovery file could not be removed"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// GitHub issue #272: `<memo>/` image folders -> random names on lock
// ---------------------------------------------------------------------------

/// Step 6 of `lock_folder_files`: for each memo with a real `<memo>/` image folder (#263 guard),
/// assign a fresh random folder name. Memos without a folder (and pre-#139 flat images, which
/// live in no per-memo folder) are left out — nothing to convert.
fn plan_lock_image_folders(folder: &Path, memo_names: &[String]) -> Result<Vec<ImageFolderEntry>, AppErrorPayload> {
    let mut planned: Vec<ImageFolderEntry> = Vec::new();
    for name in memo_names {
        if existing_memo_image_folder(folder, name).is_none() {
            continue;
        }
        let (_, random) = generate_unique_image_folder(folder, &planned)
            .ok_or_else(|| AppErrorPayload::new(ErrorCode::Unknown, "failed to generate a unique image folder name"))?;
        planned.push(ImageFolderEntry {
            memo_name: name.clone(),
            folder: random,
        });
    }
    Ok(planned)
}

fn image_folder_for<'a>(image_folders: &'a [ImageFolderEntry], memo_name: &str) -> Option<&'a str> {
    image_folders.iter().find(|e| e.memo_name == memo_name).map(|e| e.folder.as_str())
}

/// On-disk state of one planned `<memo>/` <-> `<random>/` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageFolderState {
    /// Real folder under the memo name, nothing under the random name — not converted (yet).
    AtMemoName,
    /// Real folder under the random name, nothing under the memo name — converted.
    AtRandomName,
    /// Both / neither / a link / an unsafe name — external interference; never auto-touched.
    Anomalous,
}

fn image_folder_state(folder: &Path, entry: &ImageFolderEntry) -> ImageFolderState {
    let (Some(memo_path), Some(random_path)) = (memo_image_folder_path(folder, &entry.memo_name), memo_image_folder_path(folder, &entry.folder)) else {
        return ImageFolderState::Anomalous;
    };
    let memo_dir = existing_memo_image_folder(folder, &entry.memo_name).is_some();
    let random_dir = existing_memo_image_folder(folder, &entry.folder).is_some();
    let memo_any = std::fs::symlink_metadata(&memo_path).is_ok();
    let random_any = std::fs::symlink_metadata(&random_path).is_ok();
    match (memo_dir, memo_any, random_dir, random_any) {
        (true, _, false, false) => ImageFolderState::AtMemoName,
        (false, false, true, _) => ImageFolderState::AtRandomName,
        _ => ImageFolderState::Anomalous,
    }
}

/// Renames image folder `<from>/` -> `<to>/` (both direct children of the tab folder, #263
/// guard). Refuses to overwrite/merge into anything already at `<to>`.
fn rename_image_folder(folder: &Path, from: &str, to: &str, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    let src = existing_memo_image_folder(folder, from).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "image folder not found"))?;
    let dst = memo_image_folder_path(folder, to).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "unsafe image folder name"))?;
    if std::fs::symlink_metadata(&dst).is_ok() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "image folder destination already exists"));
    }
    registry.register(&src);
    registry.register(&dst);
    with_retry(|| std::fs::rename(&src, &dst))
}

/// Step 6b: renames every planned folder to its random name, in order. On the first failure,
/// renames the already-renamed ones back (reverse order) and returns `Err(rollback_complete)`.
fn convert_image_folders_for_lock(folder: &Path, image_folders: &[ImageFolderEntry], registry: &SelfWriteRegistry) -> Result<(), bool> {
    for (i, entry) in image_folders.iter().enumerate() {
        if let Err(err) = rename_image_folder(folder, &entry.memo_name, &entry.folder, registry) {
            tracing::error!(error = %err, "tab_lock: failed to rename a memo image folder; rolling back");
            let mut complete = true;
            for done in image_folders[..i].iter().rev() {
                if let Err(err) = rename_image_folder(folder, &done.folder, &done.memo_name, registry) {
                    tracing::error!(error = %err, "tab_lock: failed to roll back a renamed image folder");
                    complete = false;
                }
            }
            return Err(complete);
        }
    }
    Ok(())
}

/// Recovery "Finish": makes sure every planned folder is under its random name (idempotent —
/// already-converted and anomalous entries are skipped).
fn ensure_image_folders_converted(folder: &Path, image_folders: &[ImageFolderEntry], registry: &SelfWriteRegistry) -> Result<(), AppErrorPayload> {
    for entry in image_folders {
        match image_folder_state(folder, entry) {
            ImageFolderState::AtMemoName => {
                rename_image_folder(folder, &entry.memo_name, &entry.folder, registry).map_err(|err| {
                    tracing::error!(error = %err, "tab_lock_recovery_finish: failed to rename a memo image folder");
                    AppErrorPayload::new(ErrorCode::ImageFolderRenameFailed, "failed to rename a memo image folder")
                })?;
            }
            ImageFolderState::AtRandomName => {}
            ImageFolderState::Anomalous => tracing::warn!("tab_lock_recovery_finish: image folder pair in an unexpected state; left untouched"),
        }
    }
    Ok(())
}

/// Recovery rollback (zero-progress auto-rollback, "Restore"): renames every folder still under
/// its random name back to `<memo>/`. Anomalous pairs are left untouched (logged). Returns
/// `false` if any rename failed, so the caller keeps the marker for a later retry.
fn rollback_image_folders(folder: &Path, image_folders: &[ImageFolderEntry], registry: &SelfWriteRegistry) -> bool {
    let mut ok = true;
    for entry in image_folders {
        match image_folder_state(folder, entry) {
            ImageFolderState::AtRandomName => {
                if let Err(err) = rename_image_folder(folder, &entry.folder, &entry.memo_name, registry) {
                    tracing::error!(error = %err, "lock recovery: failed to rename an image folder back to its memo name");
                    ok = false;
                }
            }
            ImageFolderState::AtMemoName => {}
            ImageFolderState::Anomalous => tracing::warn!("lock recovery: image folder pair in an unexpected state; left untouched"),
        }
    }
    ok
}

/// Rewrites `![..](<from>/..)` image links to `<to>/..` in a memo body's raw bytes
/// (`util::image_link_rewrite`, same rules as #261's rename rewrite). UTF-8 only (BOM kept);
/// a non-UTF-8 body is returned unchanged (`None`), like #261.
fn rewrite_image_links_in_bytes(bytes: &[u8], from: &str, to: &str) -> Option<Vec<u8>> {
    const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
    let (bom, rest) = if bytes.starts_with(BOM) { bytes.split_at(BOM.len()) } else { (&[][..], bytes) };
    let Ok(body) = std::str::from_utf8(rest) else {
        tracing::warn!("tab lock: memo body is not UTF-8; image links not rewritten");
        return None;
    };
    let rewritten = crate::util::image_link_rewrite::rewrite_image_links_for_rename(body, from, to)?;
    let mut out = Vec::with_capacity(bom.len() + rewritten.len());
    out.extend_from_slice(bom);
    out.extend_from_slice(rewritten.as_bytes());
    Some(out)
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
///
/// GitHub issue #272: `image_folder` = this memo's planned random image-folder name, if any —
/// `<memo>/…` image links are rewritten to `<random>/…` in memory, so only the encrypted copy
/// carries the new links (the plaintext file is never rewritten on disk).
fn rename_and_encrypt_entry(folder: &Path, entry: &FileMapEntry, image_folder: Option<&str>, key: &LockKey, registry: &SelfWriteRegistry) -> Result<(), AppErrorPayload> {
    let src = folder.join(format!("{}.md", entry.memo_name));
    let dst = folder.join(&entry.token);

    let mut plaintext = std::fs::read(&src).map_err(|err| {
        tracing::error!(error = %err, path = %src.display(), "failed to read memo file to encrypt");
        AppErrorPayload::new(ErrorCode::Unknown, "failed to read a memo file while locking this tab")
    })?;
    if let Some(random) = image_folder {
        if let Some(rewritten) = rewrite_image_links_in_bytes(&plaintext, &entry.memo_name, random) {
            plaintext = rewritten;
        }
    }
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
// GitHub issue #276: plaintext residue cleanup (`CRASH-RECOVERY.md` "Plaintext Residue")
// ---------------------------------------------------------------------------

/// `.tabtreenote.json.bak` が暗号化済み(ロック済み)メタの形(`lock` あり・平文 `tree` なし)なら `true`。
/// セッション中の保存(`save_lock_blob` が前のロック済みメタを回す)で出来るもので平文を含まないため残す。
fn meta_bak_is_encrypted_shape(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else { return false };
    match serde_json::from_slice::<TabMetaFile>(&bytes) {
        Ok(meta) => meta.lock.is_some() && meta.tree.is_none(),
        Err(_) => false,
    }
}

/// GitHub issue #276: ロック後もタブフォルダに残る平文の残骸を完全削除する(ごみ箱へは送らない —
/// ごみ箱に平文が残るため)。対象:
/// - `fileMap` の各メモの `<メモ名>.md.bak`(`atomic_write` の 1 世代 `.bak` に回った旧版本文。名前自体がメモ名を明かす)
/// - `.tabtreenote.json.bak`(暗号化済みメタの形なら残す。ロック前の平文メタ = 全メモ名入りが
///   `write_lock_field` 自身の `atomic_write` で `.bak` に回ったもの)
/// - `.tabtreenote.lock-inprogress.bak`(マーカーの `.bak` は全メモ名を含む)
/// - `include_temp_files` のとき、`atomic_write` の残骸 `.ttn-tmp-*`(過去に中断した保存の平文本文の可能性)
///
/// 復号済み `fileMap` から作った名前と固定のメタ名だけを、`existing_direct_child_file`(直下・通常
/// ファイル・リンク不可)で確認してから消す。暗号化済みの `<token>` / `<token>.bak` は一致しない。
/// ベストエフォート: 失敗はログのみで、次回アンロック時に再試行される。
fn remove_plaintext_residue(folder: &Path, file_map: &[FileMapEntry], include_temp_files: bool, registry: &SelfWriteRegistry) {
    let mut names: Vec<String> = file_map.iter().map(|e| format!("{}.md.bak", e.memo_name)).collect();
    names.push(format!("{LOCK_INPROGRESS_MARKER_NAME}.bak"));
    let meta_bak = bak_path(&folder.join(TAB_META_FILE_NAME));
    if meta_bak.exists() && !meta_bak_is_encrypted_shape(&meta_bak) {
        names.push(format!("{TAB_META_FILE_NAME}.bak"));
    }
    if include_temp_files {
        if let Ok(entries) = std::fs::read_dir(folder) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if crate::fs::temp_naming::is_temp_file_name(name) {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }
    for name in names {
        let Some(path) = existing_direct_child_file(folder, &name) else { continue };
        registry.register(&path);
        if let Err(err) = with_retry(|| std::fs::remove_file(&path)) {
            tracing::warn!(error = %err, "tab lock: failed to remove a plaintext backup file (retried on next unlock)");
        }
    }
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

    // GitHub issue #276: 旧ビルドでロックしたタブ(またはロック時の削除に失敗したタブ)には平文の
    // `<メモ名>.md.bak` / 平文メタの `.bak` が残りうる。復号した `fileMap` でこのタブのメモ名が
    // 分かったので、ここで消す。一時ファイルはメモ名と照合できないのでここでは消さない。
    remove_plaintext_residue(&folder, &payload.file_map, false, &registry);

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
            image_folders: payload.image_folders,
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
                image_folders: s.image_folders.clone(),
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
///
/// `image_folder`: GitHub issue #273 - `Some(<random>)` when this memo's image folder has already
/// been renamed back to `<memo>/`; the decrypted body's `<random>/...` links are then rewritten to
/// `<memo>/...` in memory before the single plaintext write (the `.md` never points at a missing folder).
fn restore_one_permanent_unlock_entry(folder: &Path, entry: &FileMapEntry, image_folder: Option<&str>, key: &LockKey, registry: &SelfWriteRegistry) -> Result<(), AppErrorPayload> {
    let token_path = folder.join(&entry.token);
    let bytes = std::fs::read(&token_path).map_err(|err| {
        tracing::error!(error = %err, path = %token_path.display(), "failed to read an obfuscated memo file while permanently unlocking this tab");
        AppErrorPayload::new(ErrorCode::Unknown, "failed to read a memo file while unlocking this tab")
    })?;
    let mut plaintext = lock_blob::decrypt_memo_body(key, &bytes).map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "incorrect password"))?;
    if let Some(random) = image_folder {
        if let Some(rewritten) = rewrite_image_links_in_bytes(&plaintext, random, &entry.memo_name) {
            plaintext = rewritten;
        }
    }
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

// ---------------------------------------------------------------------------
// GitHub issue #273: random-name image folders -> `<memo>/` on permanent unlock
// (`docs/plans/tab-lock/CRASH-RECOVERY.md` "Image Folders on Permanent Unlock")
// ---------------------------------------------------------------------------

/// Result of `tab_permanent_unlock` / `tab_permanent_unlock_recovery_finish`: the usual `TabInfo`
/// plus how many image folders were left under their random name (an existing `<memo>/` folder,
/// folder in use, name rejected by the #263 guard) - a non-fatal warning.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermanentUnlockResult {
    #[serde(flatten)]
    pub tab: TabInfo,
    pub image_folders_left_random: usize,
}

fn unlock_file_map_entry<'a>(payload: &'a LockPayload, memo_name: &str) -> Option<&'a FileMapEntry> {
    payload.file_map.iter().find(|e| e.memo_name == memo_name)
}

/// 完全解除の計画: 対応表のうち、本文がまだ暗号化のままで、フォルダがランダム名で実在し、
/// `<メモ名>/` 側が空いているもののランダム名。`<メモ名>/` が既にある(衝突)・名前がガードを
/// 通らない・フォルダが無いものは計画に入れず、ランダム名のまま残す。
fn plan_unlock_image_folders(folder: &Path, payload: &LockPayload) -> Vec<String> {
    payload
        .image_folders
        .iter()
        .filter(|e| unlock_file_map_entry(payload, &e.memo_name).map(|fm| classify_unlock_entry(folder, fm) == UnlockEntryState::NotRestored).unwrap_or(false))
        .filter(|e| image_folder_state(folder, e) == ImageFolderState::AtRandomName)
        .map(|e| e.folder.clone())
        .collect()
}

/// 計画済みで、かつ今 `<メモ名>/` にある(=改名済み)フォルダの対応表エントリ。本文のリンクを
/// 書き換えるのはこの時だけ(改名できなかったメモのリンクは触らない)。
fn planned_folder_at_memo_name<'a>(folder: &Path, payload: &'a LockPayload, planned: &[String], memo_name: &str) -> Option<&'a ImageFolderEntry> {
    payload
        .image_folders
        .iter()
        .find(|e| e.memo_name == memo_name && planned.contains(&e.folder))
        .filter(|e| image_folder_state(folder, e) == ImageFolderState::AtMemoName)
}

/// 完全解除の後でランダム名のまま残った画像フォルダの数(警告用)。
fn count_image_folders_left_random(folder: &Path, payload: &LockPayload) -> usize {
    payload
        .image_folders
        .iter()
        .filter(|e| unlock_file_map_entry(payload, &e.memo_name).is_some() && existing_memo_image_folder(folder, &e.folder).is_some())
        .count()
}

/// `tab_permanent_unlock` steps 6b-8 (also the crash recovery's "Finish"; idempotent):
/// 6b. rename each planned folder `<random>/` -> `<memo>/` while its body is still encrypted
///     (failure = keep the random name and continue - user decision on #273);
/// 7.  restore every remaining body, rewriting `<random>/` -> `<memo>/` links in memory only when
///     that folder is now at `<memo>/`;
/// 8.  write the plain metadata (the image-folder map goes away with the lock blob).
fn permanent_unlock_restore_all(folder: &Path, payload: &LockPayload, planned: &[String], key: &LockKey, registry: &SelfWriteRegistry) -> Result<(), AppErrorPayload> {
    for e in &payload.image_folders {
        if !planned.contains(&e.folder) {
            continue;
        }
        // 本文を復元済みのメモは、そのリンクに合わせてフォルダ名を据え置く(改名しない)
        let Some(fm) = unlock_file_map_entry(payload, &e.memo_name) else { continue };
        if classify_unlock_entry(folder, fm) != UnlockEntryState::NotRestored {
            continue;
        }
        if image_folder_state(folder, e) == ImageFolderState::AtRandomName {
            if let Err(err) = rename_image_folder(folder, &e.folder, &e.memo_name, registry) {
                tracing::warn!(error = %err, "permanent unlock: an image folder could not be renamed back to its memo name; it keeps its random name");
            }
        }
    }

    for entry_fm in &payload.file_map {
        match classify_unlock_entry(folder, entry_fm) {
            UnlockEntryState::Restored => continue,
            // Abort without rolling back; the marker stays for crash recovery (`CRASH-RECOVERY.md`).
            UnlockEntryState::Anomalous => {
                tracing::error!(memo_name = %entry_fm.memo_name, "permanent unlock: encountered an anomalous entry; aborting, marker left for crash recovery");
                return Err(AppErrorPayload::new(ErrorCode::AnomalousFileState, "an unexpected file state was found while permanently unlocking this tab"));
            }
            UnlockEntryState::NotRestored => {
                let image_folder = planned_folder_at_memo_name(folder, payload, planned, &entry_fm.memo_name).map(|e| e.folder.as_str());
                restore_one_permanent_unlock_entry(folder, entry_fm, image_folder, key, registry).map_err(|err| {
                    tracing::error!(?err, "permanent unlock: failed mid-way through restoring memo files; marker left for crash recovery");
                    err
                })?;
            }
        }
    }

    tree::save_tab_meta(folder, TAB_META_SCHEMA_VERSION, env!("CARGO_PKG_VERSION").to_string(), &payload.tree, payload.last_viewed_memo.clone(), registry).map_err(|err| {
        tracing::error!(error = %err, "permanent unlock: failed to save plain tab metadata");
        AppErrorPayload::new(ErrorCode::Unknown, "failed to save tab metadata")
    })
}

/// Crash recovery "Restore lock" file work (`CRASH-RECOVERY.md`): re-encrypt every restored body
/// (rewriting `<memo>/` -> `<random>/` links in memory when its planned folder is at `<memo>/`),
/// then rename those planned folders back to their random names. Bodies first, so no plaintext
/// `.md` ever points at a folder that was already renamed away. A folder that cannot be renamed
/// back -> `ImageFolderRenameFailed`, marker kept (`tree_open` keeps the recovery pending).
fn permanent_unlock_relock_files(folder: &Path, payload: &LockPayload, planned: &[String], key: &LockKey, registry: &SelfWriteRegistry) -> Result<(), AppErrorPayload> {
    for entry_fm in &payload.file_map {
        match classify_unlock_entry(folder, entry_fm) {
            // Already back in token form.
            UnlockEntryState::NotRestored => continue,
            UnlockEntryState::Anomalous => {
                tracing::error!(memo_name = %entry_fm.memo_name, "tab_permanent_unlock_recovery_restore: encountered an anomalous entry; aborting");
                return Err(AppErrorPayload::new(ErrorCode::AnomalousFileState, "an unexpected file state was found while restoring this tab's lock"));
            }
            UnlockEntryState::Restored => {
                let real_path = folder.join(format!("{}.md", entry_fm.memo_name));
                let mut plaintext = std::fs::read(&real_path).map_err(|err| {
                    tracing::error!(error = %err, "tab_permanent_unlock_recovery_restore: failed to read a plaintext memo file");
                    AppErrorPayload::new(ErrorCode::Unknown, "failed to read a memo file while restoring this tab's lock")
                })?;
                if let Some(e) = planned_folder_at_memo_name(folder, payload, planned, &entry_fm.memo_name) {
                    if let Some(rewritten) = rewrite_image_links_in_bytes(&plaintext, &e.memo_name, &e.folder) {
                        plaintext = rewritten;
                    }
                }
                let encrypted = lock_blob::encrypt_memo_body(key, &plaintext);
                let token_path = folder.join(&entry_fm.token);
                atomic_write(&token_path, &encrypted, registry).map_err(|err| {
                    tracing::error!(error = %err, "tab_permanent_unlock_recovery_restore: failed to write an obfuscated memo file");
                    AppErrorPayload::new(ErrorCode::Unknown, "failed to restore an obfuscated memo file")
                })?;
                registry.register(&real_path);
                if let Err(err) = with_retry(|| std::fs::remove_file(&real_path)) {
                    tracing::error!(error = %err, "tab_permanent_unlock_recovery_restore: failed to remove a plaintext memo file after re-encrypting it");
                }
            }
        }
    }

    let mut folders_ok = true;
    for e in &payload.image_folders {
        if planned.contains(&e.folder) && image_folder_state(folder, e) == ImageFolderState::AtMemoName {
            if let Err(err) = rename_image_folder(folder, &e.memo_name, &e.folder, registry) {
                tracing::error!(error = %err, "tab_permanent_unlock_recovery_restore: failed to rename an image folder back to its random name");
                folders_ok = false;
            }
        }
    }
    if !folders_ok {
        return Err(AppErrorPayload::new(ErrorCode::ImageFolderRenameFailed, "some image folders could not be renamed back to their random names"));
    }
    Ok(())
}

/// `docs/plans/tab-lock-permanent-unlock/PLAN.md` §5. `password: None` for entry point (a) (an
/// active session supplies the key); `Some(pw)` for entry point (b).
#[tauri::command]
pub async fn tab_permanent_unlock(app: AppHandle, tab_id: String, password: Option<String>) -> AppResult<PermanentUnlockResult> {
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

    // Step 6: plan the image-folder renames (#273) and write the marker before touching any file.
    let planned = plan_unlock_image_folders(&folder, &payload);
    let marker = UnlockInProgressMarker {
        format_version: UNLOCK_MARKER_FORMAT_VERSION,
        image_folders: planned.clone(),
    };
    if let Err(err) = write_unlock_marker(&folder, &marker, &registry) {
        tracing::error!(error = %err, "tab_permanent_unlock: failed to write in-progress marker");
        return AppResult::err_code(ErrorCode::Unknown, "failed to start permanently unlocking this tab");
    }

    // Steps 6b-8: image folders back to `<memo>/`, restore loop, plain metadata.
    if let Err(err) = permanent_unlock_restore_all(&folder, &payload, &planned, &key, &registry) {
        return AppResult::err(err);
    }
    let image_folders_left_random = count_image_folders_left_random(&folder, &payload);

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

    AppResult::ok(PermanentUnlockResult {
        tab: TabInfo {
            id: tab_id,
            name: payload.real_tab_name,
            locked: false,
            lock_label,
        },
        image_folders_left_random,
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

    if let Err(payload) = recovery_finish_files(&folder, &marker, &key, tab_name, &registry) {
        return AppResult::err(payload);
    }

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

/// File-touching part of `tab_lock_recovery_finish` (everything before the marker delete).
fn recovery_finish_files(folder: &Path, marker: &LockInProgressMarker, key: &LockKey, tab_name: String, registry: &SelfWriteRegistry) -> Result<(), AppErrorPayload> {
    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(folder);
    let already_has_lock_field = parsed.as_ref().map(|m| m.lock.is_some()).unwrap_or(false);
    if already_has_lock_field {
        // Step 8 already succeeded before the interruption (the blob already carries the
        // image-folder map); nothing more to encrypt/write (`CRASH-RECOVERY.md`'s "Idempotency").
        // #276: step 8b(平文残骸の削除)より前に中断した可能性がある
        remove_plaintext_residue(folder, &marker.file_map, true, registry);
        return Ok(());
    }

    // GitHub issue #272: finish step 6b first (idempotent), so every remaining entry below is
    // encrypted with links that point at an existing `<random>/` folder.
    ensure_image_folders_converted(folder, &marker.image_folders, registry)?;

    // Step 8 never ran: finish encrypting the remaining entries, then build + write the lock
    // blob from the still-intact plain `tree` field.
    for entry_fm in &marker.file_map {
        if classify_entry(folder, entry_fm) == EntryState::NotRenamed {
            let image_folder = image_folder_for(&marker.image_folders, &entry_fm.memo_name);
            rename_and_encrypt_entry(folder, entry_fm, image_folder, key, registry)?;
        }
    }
    let last_viewed_memo = parsed.as_ref().and_then(|m| m.last_viewed_memo.clone());
    let tree_field = parsed.and_then(|m| m.tree).unwrap_or_default();
    let payload = LockPayload {
        real_tab_name: tab_name,
        tree: tree_field,
        file_map: marker.file_map.clone(),
        last_viewed_memo,
        // #272: 中断前に計画した対応表をそのまま引き継ぐ(#271 時点では空で上書きしていた)
        image_folders: marker.image_folders.clone(),
    };
    let field = lock_blob::encrypt_lock_field(key, marker.kdf.clone(), None, &payload)
        .map_err(|_| AppErrorPayload::new(ErrorCode::Unknown, "failed to build the encrypted tab metadata"))?;
    write_lock_field(folder, &field, registry).map_err(|err| {
        tracing::error!(error = %err, "tab_lock_recovery_finish: failed to write the encrypted lock blob");
        AppErrorPayload::new(ErrorCode::Unknown, "failed to save the locked tab metadata")
    })?;
    // #276: `lock_folder_files` と同じ step 8b
    remove_plaintext_residue(folder, &marker.file_map, true, registry);
    Ok(())
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

    match recovery_restore_files(&folder, &marker, &key, &registry) {
        Ok(true) => {}
        Ok(false) => {
            // メモ本文・メタは平文に戻ったが、画像フォルダの一部が `<ランダム名>/` のまま。マーカーは
            // 残し(以後は進捗ゼロ扱い)、次回 `tree_open` の自動巻き戻しで再試行する。
            return AppResult::err_code(ErrorCode::ImageFolderRenameFailed, "restored, but some image folders could not be renamed back");
        }
        Err(payload) => return AppResult::err(payload),
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

/// File-touching part of `tab_lock_recovery_restore` (everything before the marker delete).
/// Returns `Ok(false)` when memos/metadata were restored but some image folder could not be
/// renamed back from its random name (GitHub issue #272) — the caller keeps the marker.
fn recovery_restore_files(folder: &Path, marker: &LockInProgressMarker, key: &LockKey, registry: &SelfWriteRegistry) -> Result<bool, AppErrorPayload> {
    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(folder);
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
        let field: LockField = serde_json::from_value(lock_value).map_err(|_| AppErrorPayload::new(ErrorCode::Unknown, "this tab's lock metadata is corrupted"))?;
        let payload = lock_blob::decrypt_lock_field(key, &field).map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "incorrect password"))?;
        last_viewed_memo = payload.last_viewed_memo;
        payload.tree
    } else {
        parsed.and_then(|m| m.tree).unwrap_or_default()
    };

    for entry_fm in &marker.file_map {
        if classify_entry(folder, entry_fm) != EntryState::Renamed {
            continue;
        }
        let token_path = folder.join(&entry_fm.token);
        let bytes = std::fs::read(&token_path).map_err(|err| {
            tracing::error!(error = %err, "tab_lock_recovery_restore: failed to read an encrypted memo file");
            AppErrorPayload::new(ErrorCode::Unknown, "failed to read a memo file while restoring this tab")
        })?;
        let mut plaintext = lock_blob::decrypt_memo_body(key, &bytes).map_err(|_| AppErrorPayload::new(ErrorCode::DecryptFailed, "incorrect password"))?;
        // #272: 暗号化時に `<メモ名>/` -> `<ランダム名>/` へ書き換えたリンクを戻す(フォルダは下で戻す)
        if let Some(random) = image_folder_for(&marker.image_folders, &entry_fm.memo_name) {
            if let Some(rewritten) = rewrite_image_links_in_bytes(&plaintext, random, &entry_fm.memo_name) {
                plaintext = rewritten;
            }
        }
        let real_path = folder.join(format!("{}.md", entry_fm.memo_name));
        atomic_write(&real_path, &plaintext, registry).map_err(|err| {
            tracing::error!(error = %err, "tab_lock_recovery_restore: failed to restore a plaintext memo file");
            AppErrorPayload::new(ErrorCode::Unknown, "failed to restore a memo file")
        })?;
        registry.register(&token_path);
        if let Err(err) = with_retry(|| std::fs::remove_file(&token_path)) {
            tracing::error!(error = %err, "tab_lock_recovery_restore: failed to remove an obfuscated memo file after restoring it");
        }
    }

    // #272: every body now links `<memo>/…` again; rename the folders back to match.
    let folders_ok = rollback_image_folders(folder, &marker.image_folders, registry);

    tree::save_tab_meta(folder, TAB_META_SCHEMA_VERSION, env!("CARGO_PKG_VERSION").to_string(), &restored_tree, last_viewed_memo, registry).map_err(|err| {
        tracing::error!(error = %err, "tab_lock_recovery_restore: failed to restore plain tab metadata");
        AppErrorPayload::new(ErrorCode::Unknown, "failed to restore tab metadata")
    })?;
    Ok(folders_ok)
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
pub async fn tab_permanent_unlock_recovery_finish(app: AppHandle, tab_id: String, password: Option<String>) -> AppResult<PermanentUnlockResult> {
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

    let Some(marker) = load_unlock_marker(&folder) else {
        return AppResult::err_code(ErrorCode::NotFound, "no interrupted permanent-unlock attempt found for this tab");
    };

    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(&folder);
    let already_has_tree_field = parsed.as_ref().map(|m| m.tree.is_some()).unwrap_or(false);

    let (real_tab_name, image_folders_left_random): (Option<String>, usize) = if already_has_tree_field {
        // Case B reached here defensively: metadata already rewritten, nothing left to decrypt
        // (every image folder was already handled before step 8; no payload to count them).
        (sessions.with_session(&tab_id, |s| s.real_tab_name.clone()), 0)
    } else {
        let (key, payload) = match resolve_key_and_lock_payload(&folder, &sessions, &tab_id, password).await {
            Ok(v) => v,
            Err(err) => return AppResult::err(err),
        };
        // #273: マーカーの計画(ランダム名)に沿って、フォルダ改名 -> 本文復元 -> メタ を冪等に再開
        if let Err(err) = permanent_unlock_restore_all(&folder, &payload, &marker.image_folders, &key, &registry) {
            return AppResult::err(err);
        }
        let left = count_image_folders_left_random(&folder, &payload);
        (Some(payload.real_tab_name), left)
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

    AppResult::ok(PermanentUnlockResult {
        tab: TabInfo {
            id: tab_id,
            name: real_tab_name.unwrap_or(current_name),
            locked: false,
            lock_label,
        },
        image_folders_left_random,
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

    let Some(marker) = load_unlock_marker(&folder) else {
        return AppResult::err_code(ErrorCode::NotFound, "no interrupted permanent-unlock attempt found for this tab");
    };

    let (parsed, _recovery) = tree::load_tab_meta_with_recovery(&folder);
    if parsed.as_ref().map(|m| m.tree.is_some()).unwrap_or(false) {
        return AppResult::err_code(ErrorCode::Unknown, "this permanent-unlock attempt already finished; nothing to restore");
    }

    let (key, payload) = match resolve_key_and_lock_payload(&folder, &sessions, &tab_id, password).await {
        Ok(v) => v,
        Err(err) => return AppResult::err(err),
    };

    // #273: 本文を再暗号化してから、`<メモ名>/` へ戻した画像フォルダをランダム名へ戻す。
    // 戻せないフォルダがあればマーカーを残し、次回の復旧で再試行する。
    if let Err(err) = permanent_unlock_relock_files(&folder, &payload, &marker.image_folders, &key, &registry) {
        return AppResult::err(err);
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
fn save_lock_blob(
    folder: &Path,
    session: &LockSession,
    tree: &[MetaTreeNode],
    file_map: &[FileMapEntry],
    last_viewed_memo: Option<String>,
    image_folders: &[ImageFolderEntry],
    registry: &SelfWriteRegistry,
) -> std::io::Result<()> {
    let key = LockKey::new(&session.key);
    let payload = LockPayload {
        real_tab_name: session.real_tab_name.clone(),
        tree: tree.to_vec(),
        file_map: file_map.to_vec(),
        last_viewed_memo,
        image_folders: image_folders.to_vec(),
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
    commit_session_mutation_with_images(sessions, registry, folder, tab_id, |session, tree, file_map, _image_folders| mutate(session, tree, file_map))
}

/// `commit_session_mutation` に画像フォルダ対応表(GitHub issue #271)の下書きも渡す版。対応表も
/// tree/file_map と同じく、ロック blob の保存成功後にだけ `session` へ反映する(GH #17 の方針)。
fn commit_session_mutation_with_images(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    folder: &Path,
    tab_id: &str,
    mutate: impl FnOnce(&LockSession, &mut Vec<MetaTreeNode>, &mut Vec<FileMapEntry>, &mut Vec<ImageFolderEntry>) -> Result<(), AppErrorPayload>,
) -> Option<Result<(), AppErrorPayload>> {
    sessions.with_session_mut(tab_id, |session| -> Result<(), AppErrorPayload> {
        let mut tree = session.tree.clone();
        let mut file_map = session.file_map.clone();
        let mut image_folders = session.image_folders.clone();
        mutate(session, &mut tree, &mut file_map, &mut image_folders)?;
        save_lock_blob(folder, session, &tree, &file_map, session.last_viewed_memo.clone(), &image_folders, registry).map_err(|err| {
            tracing::error!(error = %err, tab_id, "session mutation: failed to save lock blob; in-memory session left unchanged");
            AppErrorPayload::new(ErrorCode::Unknown, "failed to save tab metadata")
        })?;
        session.tree = tree;
        session.file_map = file_map;
        session.image_folders = image_folders;
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
        save_lock_blob(folder, session, &session.tree.clone(), &session.file_map.clone(), memo_name.clone(), &session.image_folders.clone(), registry).map_err(|err| {
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
    let result = commit_session_mutation_with_images(sessions, registry, folder, tab_id, |_session, tree, file_map, image_folders| {
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
        // GitHub issue #271: 画像フォルダはランダム名なので改名しない。対応表のキーだけ更新し、
        // 本文中のリンク(`<ランダム名>/NNN.ext`)も書き換え不要。
        if let Some(entry) = image_folders.iter_mut().find(|e| e.memo_name == path) {
            entry.memo_name = new_name.to_string();
        }
        Ok(())
    });
    finish_session_mutation(sessions, tab_id, result)
}

/// `force_direct` (GitHub issue #271): セッション中メモの画像フォルダ(`<ランダム名>/`)にだけ
/// 適用する、平文タブ #263 と同じごみ箱/直接削除の切り替え。難読化トークンファイル自体は従来通り
/// 常に完全削除。
///
/// 順序: 先に全メモの画像フォルダを処理し、その後でトークンファイルを削除する。トークンは
/// ごみ箱に行かず復元不能なので、ごみ箱移動失敗(`TrashFailed` -> 同意ダイアログ -> 直接削除で
/// 再試行、`[spec §5.8]`)で利用者がキャンセルしても、本文だけ消えてツリーに残る壊れたメモを
/// 作らないため。直接削除経路でのフォルダ削除失敗は #263 と同じく非致命の警告として返す。
pub(crate) fn session_delete_memo(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    folder: &Path,
    tab_id: &str,
    paths: &[NodePath],
    force_direct: bool,
) -> AppResult<MemoMutationResult> {
    let mut warning: Option<MemoMutationWarning> = None;
    let result = commit_session_mutation_with_images(sessions, registry, folder, tab_id, |_session, tree, file_map, image_folders| {
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

        // GitHub issue #271: 各メモの画像フォルダ。対応表にあり、かつタブ直下の実ディレクトリに
        // 解決できるもの(#263 のパスガード)だけを対象にする。
        for name in &all_names {
            let Some(entry) = image_folders.iter().find(|e| &e.memo_name == name) else {
                continue;
            };
            let Some(image_folder) = existing_memo_image_folder(folder, &entry.folder) else {
                continue;
            };
            if force_direct {
                if let Err(err) = trash::delete_dir_directly(&image_folder, registry) {
                    tracing::warn!(error = %err, "session_delete_memo: failed to delete memo's image folder");
                    warning = Some(MemoMutationWarning::ImageFolderDeleteFailed);
                }
            } else if let Err(code) = trash::trash_file(&image_folder, registry) {
                tracing::error!(?code, "session_delete_memo: failed to trash memo's image folder");
                return Err(AppErrorPayload::new(code, "failed to move the memo's image folder to the trash"));
            }
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
            image_folders.retain(|e| &e.memo_name != name);
        }

        Ok(())
    });
    match finish_session_mutation(sessions, tab_id, result) {
        AppResult::Ok(snapshot) => AppResult::ok(MemoMutationResult { snapshot, warning }),
        AppResult::Err(e) => AppResult::err(e),
    }
}

/// GitHub issue #271: ロック中タブのセッションで `memo_name` の画像保存先フォルダを返す
/// (`(<tab_folder>/<ランダム名>, ランダム名)`)。対応表に既にあればそれを使い、無ければ
/// 推測不能なランダム名を新規に割り当て、対応表をロック blob へ再暗号化保存してから返す
/// (フォルダ自体の作成は呼び出し側 `image_save_pasted`)。実メモ名はフォルダ名に一切現れない。
pub(crate) fn session_memo_image_dir(sessions: &LockSessionStore, registry: &SelfWriteRegistry, folder: &Path, tab_id: &str, memo_name: &str) -> Result<(PathBuf, String), AppErrorPayload> {
    // 既存エントリがあればそれを使う(保存不要なので読み取りのみ)
    let existing = sessions.with_session(tab_id, |session| {
        if !session.file_map.iter().any(|e| e.memo_name == memo_name) {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "memo not found"));
        }
        Ok(session.image_folders.iter().find(|e| e.memo_name == memo_name).map(|e| e.folder.clone()))
    });
    match existing {
        None => return Err(AppErrorPayload::new(ErrorCode::TabLocked, "no active session for this tab")),
        Some(Err(e)) => return Err(e),
        Some(Ok(Some(name))) => {
            let dir = plain_tab_image_dir(folder, &name).map_err(|code| AppErrorPayload::new(code, "invalid memo image folder"))?;
            return Ok((dir, name));
        }
        Some(Ok(None)) => {}
    }

    let mut assigned: Option<(PathBuf, String)> = None;
    let result = commit_session_mutation_with_images(sessions, registry, folder, tab_id, |_session, _tree, _file_map, image_folders| {
        // with_session と commit の間に別呼び出しが割り当て済みならそれを使う
        if let Some(entry) = image_folders.iter().find(|e| e.memo_name == memo_name) {
            let dir = plain_tab_image_dir(folder, &entry.folder).map_err(|code| AppErrorPayload::new(code, "invalid memo image folder"))?;
            assigned = Some((dir, entry.folder.clone()));
            return Ok(());
        }
        let (dir, name) = generate_unique_image_folder(folder, image_folders).ok_or_else(|| AppErrorPayload::new(ErrorCode::Unknown, "failed to generate a unique image folder name"))?;
        image_folders.push(ImageFolderEntry {
            memo_name: memo_name.to_string(),
            folder: name.clone(),
        });
        assigned = Some((dir, name));
        Ok(())
    });
    match result {
        Some(Ok(())) => assigned.ok_or_else(|| AppErrorPayload::new(ErrorCode::Unknown, "failed to assign an image folder")),
        Some(Err(e)) => Err(e),
        None => Err(AppErrorPayload::new(ErrorCode::TabLocked, "no active session for this tab")),
    }
}

/// GitHub issue #271: 衝突しないランダム画像フォルダ名を生成する。条件: #263 のパスガード
/// (`memo_image_folder_path`)を通ること、タブ直下に同名のエントリ(ファイル/フォルダ/リンク)が
/// 無いこと、対応表の既存フォルダ名と(大文字小文字を無視して)重ならないこと。
fn generate_unique_image_folder(folder: &Path, image_folders: &[ImageFolderEntry]) -> Option<(PathBuf, String)> {
    for _ in 0..crate::commands::image::MAX_FILENAME_COLLISION_ATTEMPTS {
        let name = obfuscated_naming::generate_image_folder_name();
        let Some(dir) = memo_image_folder_path(folder, &name) else {
            continue;
        };
        if std::fs::symlink_metadata(&dir).is_ok() {
            continue;
        }
        if image_folders.iter().any(|e| e.folder.eq_ignore_ascii_case(&name)) {
            continue;
        }
        return Some((dir, name));
    }
    None
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

type GatheredMove = (Vec<MetaTreeNode>, HashMap<String, Vec<u8>>, HashMap<String, String>, HashMap<String, PathBuf>);

/// Reads (decrypts) the plaintext bodies of `paths`' full subtrees from `source_tab_id`'s active
/// session, without mutating anything. Returns the moved-subtree root nodes (for reinsertion),
/// each name's decrypted plaintext, each name's original token (for cleanup on a non-copy
/// move), and (GitHub issue #262) each name's existing random-name image folder from the
/// `imageFolders` map (only real folders passing the #263 guard).
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
        let mut image_sources = HashMap::new();
        for name in &all_names {
            if let Some(src) = image_folder_for(&session.image_folders, name).and_then(|f| existing_memo_image_folder(source_folder, f)) {
                image_sources.insert(name.clone(), src);
            }
        }
        Ok((nodes_to_move, plaintexts, tokens, image_sources))
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
    let result = commit_session_mutation_with_images(sessions, registry, source_folder, source_tab_id, |_session, tree, file_map, image_folders| {
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
            // GitHub issue #271: 移動元の対応表からも外す(残すと同名の新メモが旧フォルダを再利用し、
            // その削除で移動済みメモの画像までごみ箱へ送られる)。フォルダ自体は #262 で移動先へ運び済み。
            image_folders.retain(|e| &e.memo_name != name);
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
///
/// GitHub issue #262 (`CRASH-RECOVERY.md` §10, locked destination): each memo in
/// `image_sources` (memoName -> its image folder in the source tab) gets a fresh random folder
/// name and `imageFolders` entry in the same lock-blob save, with its `<source>/…` image links
/// rewritten to `<random>/…` before encryption. Only after that save are the folders moved/copied
/// (`copy`); if a transfer fails, the transferred folders are put back, the target commit is
/// undone, and `IMAGE_FOLDER_RENAME_FAILED` is returned.
#[allow(clippy::too_many_arguments)]
fn apply_session_move_target(
    sessions: &LockSessionStore,
    registry: &SelfWriteRegistry,
    target_folder: &Path,
    target_tab_id: &str,
    nodes_to_move: &[MetaTreeNode],
    all_names: &[String],
    plaintexts: &HashMap<String, Vec<u8>>,
    image_sources: &HashMap<String, PathBuf>,
    copy: bool,
    new_parent: Option<&str>,
    index: usize,
) -> Result<(), AppErrorPayload> {
    let mut new_tokens: HashMap<String, String> = HashMap::new();
    let mut transfers: Vec<FolderTransfer> = Vec::new();
    let result = commit_session_mutation_with_images(sessions, registry, target_folder, target_tab_id, |session, tree, file_map, image_folders| {
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
            // #262: 画像フォルダを持つメモは移動先で新しいランダム名を割り当て、リンクを暗号化前に書き換える
            let mut rewritten: Option<Vec<u8>> = None;
            if let Some(src) = image_sources.get(name) {
                let src_name = src.file_name().and_then(|n| n.to_str()).ok_or_else(|| AppErrorPayload::new(ErrorCode::Unknown, "invalid source image folder name"))?;
                let (dst, random) = generate_unique_image_folder(target_folder, image_folders).ok_or_else(|| AppErrorPayload::new(ErrorCode::Unknown, "failed to generate a unique image folder name"))?;
                rewritten = rewrite_image_links_in_bytes(plaintext, src_name, &random);
                // 同名メモの古いエントリが残っていれば置き換える(先頭一致で古いフォルダを引かないため)
                image_folders.retain(|e| &e.memo_name != name);
                image_folders.push(ImageFolderEntry {
                    memo_name: name.clone(),
                    folder: random,
                });
                transfers.push(FolderTransfer { src: src.clone(), dst });
            }
            let plaintext = rewritten.as_deref().unwrap_or(plaintext);
            let token = obfuscated_naming::generate_unique_token(target_folder);
            let encrypted = lock_blob::encrypt_memo_body(&key, plaintext);
            if let Err(err) = atomic_write(&target_folder.join(&token), &encrypted, registry) {
                tracing::error!(error = %err, "cross-tab move: failed to write obfuscated memo file");
                return Err(AppErrorPayload::new(ErrorCode::Unknown, "failed to move memo file into locked tab"));
            }
            new_tokens.insert(name.clone(), token.clone());
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
        Some(Ok(())) => {}
        Some(Err(e)) => return Err(e),
        None => return Err(AppErrorPayload::new(ErrorCode::TabLocked, "target tab session ended")),
    }
    // 移動先のロック blob が新フォルダ名を参照した後でフォルダを運ぶ(§10: ロック中タブ内で
    // 対応表に無いランダム名フォルダが孤立する窓を作らない)
    if transfer_all(&transfers, copy, registry).is_err() {
        if let Err(err) = cleanup_session_move_source(sessions, registry, target_folder, target_tab_id, all_names, &new_tokens) {
            tracing::error!(error = %err.message, "cross-tab move: failed to undo the target commit after an image folder transfer failure");
        }
        return Err(AppErrorPayload::new(ErrorCode::ImageFolderRenameFailed, "failed to move a memo image folder between tabs"));
    }
    Ok(())
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

    // #262: 移動元の `<メモ名>/` 画像フォルダ
    let image_sources: HashMap<String, PathBuf> = all_names.iter().filter_map(|name| existing_memo_image_folder(source_folder, name).map(|src| (name.clone(), src))).collect();

    if let Err(payload) = apply_session_move_target(sessions, registry, target_folder, target_tab_id, &nodes_to_move, &all_names, &plaintexts, &image_sources, copy, new_parent, index) {
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
    let (nodes_to_move, plaintexts, tokens, image_sources) = match gathered {
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
    // #262: ランダム名画像フォルダを移動先の `<メモ名>/` へ。衝突は何も触る前に拒否し、フォルダを
    // 先に運ぶ(§10 平文の移動先)。失敗時は運んだ分を戻して中止。
    let image_sources_vec = all_names.iter().filter_map(|name| image_sources.get(name).map(|src| (name.clone(), src.clone()))).collect();
    let image_transfers = match tree::plan_image_folders_into_plain_tab(target_folder, image_sources_vec) {
        Ok(v) => v,
        Err(err) => return AppResult::err(err),
    };
    let transfers: Vec<FolderTransfer> = image_transfers.iter().map(|(_, t)| t.clone()).collect();
    if transfer_all(&transfers, copy, registry).is_err() {
        return AppResult::err_code(ErrorCode::ImageFolderRenameFailed, "failed to move a memo image folder between tabs");
    }
    for (i, name) in all_names.iter().enumerate() {
        if let Some(plaintext) = plaintexts.get(name) {
            // リンク `<ランダム名>/…` → `<メモ名>/…`(書き込み前にメモリ上で)
            let src_name = image_sources.get(name).and_then(|src| src.file_name()).and_then(|n| n.to_str());
            let rewritten = src_name.and_then(|from| rewrite_image_links_in_bytes(plaintext, from, name));
            let plaintext = rewritten.as_deref().unwrap_or(plaintext);
            let dst = target_folder.join(format!("{name}.md"));
            if let Err(err) = atomic_write(&dst, plaintext, registry) {
                tracing::error!(error = %err, "move_cross_tab_session_to_plain: failed to write plaintext memo file");
                // 本文をまだ書けていないメモ(このメモ以降)の画像フォルダは元へ戻す
                undo_transfers(&tree::image_transfers_for(&image_transfers, &all_names[i..]), copy, registry);
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
    let (nodes_to_move, plaintexts, tokens, image_sources) = match gathered {
        Ok(v) => v,
        Err(payload) => return AppResult::err(payload),
    };
    let mut all_names = Vec::new();
    for node in &nodes_to_move {
        tree::collect_names(node, &mut all_names);
    }

    if let Err(payload) = apply_session_move_target(sessions, registry, target_folder, target_tab_id, &nodes_to_move, &all_names, &plaintexts, &image_sources, copy, new_parent, index) {
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
            image_folders: vec![],
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
            image_folders: vec![],
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
            image_folders: vec![],
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

        rename_and_encrypt_entry(&dir, &entry, None, &key, &registry).unwrap();

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
                image_folders: Vec::new(),
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
                image_folders: Vec::new(),
            },
        );
        std::fs::write(dir.join(TAB_META_FILE_NAME), b"placeholder").unwrap();
        std::fs::create_dir(dir.join(format!("{TAB_META_FILE_NAME}.bak"))).unwrap();

        let before_tree = sessions.with_session("tab-1", |s| s.tree.clone()).unwrap();
        let before_file_map = sessions.with_session("tab-1", |s| s.file_map.clone()).unwrap();

        match session_delete_memo(&sessions, &registry, &dir, "tab-1", &["Diary".to_string()], false) {
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
                image_folders: Vec::new(),
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
                image_folders: Vec::new(),
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
                image_folders: Vec::new(),
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
            image_folders: vec![],
        };
        write_marker(&dir, &marker, &registry).unwrap();
        for entry in &file_map {
            rename_and_encrypt_entry(&dir, entry, None, &key, &registry).unwrap();
        }
        assert!(!dir.join("Diary.md").exists());
        assert!(!dir.join("Shopping list.md").exists());

        let lock_payload = LockPayload {
            real_tab_name: "Private".to_string(),
            tree: loaded_tree,
            file_map: file_map.clone(),
            last_viewed_memo: None,
            image_folders: Vec::new(),
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
                image_folders: Vec::new(),
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
        let snapshot = session_delete_memo(&sessions, &registry, &dir, "tab-1", &["Diary".to_string()], false);
        let AppResult::Ok(snapshot) = snapshot else { panic!("expected Ok") };
        assert!(tree::find_node(&snapshot.snapshot.nodes, "Diary").is_none());
        assert!(tree::find_node(&snapshot.snapshot.nodes, "Ideas").is_none(), "deleting a node deletes its whole subtree");

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
                image_folders: Vec::new(),
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
            image_folders: Vec::new(),
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
            image_folders: Vec::new(),
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

        restore_one_permanent_unlock_entry(&dir, &entry, None, &key, &registry).unwrap();

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
            image_folders: Vec::new(),
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
                    restore_one_permanent_unlock_entry(&dir, entry_fm, None, &key, &registry).unwrap();
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
            image_folders: vec![],
        };
        write_marker(&dir, &marker, &registry).unwrap();
        for entry in &file_map {
            rename_and_encrypt_entry(&dir, entry, None, &key, &registry).unwrap();
        }
        let tree = vec![MetaTreeNode::default_for_new_file("Diary".to_string())];
        let lock_payload = LockPayload {
            real_tab_name: "Private".to_string(),
            tree: tree.clone(),
            file_map: file_map.clone(),
            last_viewed_memo: None,
            image_folders: Vec::new(),
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
                image_folders: Vec::new(),
            },
        );

        // ---- `tab_permanent_unlock` steps 5-9, driven manually, session branch (password: None)
        let (resolved_key, resolved_payload) = resolve_key_and_lock_payload(&dir, &sessions, "tab-1", None).await.unwrap();
        let unlock_marker = UnlockInProgressMarker {
            format_version: UNLOCK_MARKER_FORMAT_VERSION,
            image_folders: Vec::new(),
        };
        write_unlock_marker(&dir, &unlock_marker, &registry).unwrap();
        for entry_fm in &resolved_payload.file_map {
            match classify_unlock_entry(&dir, entry_fm) {
                UnlockEntryState::Restored => continue,
                UnlockEntryState::Anomalous => panic!("no anomalous entry expected"),
                UnlockEntryState::NotRestored => {
                    restore_one_permanent_unlock_entry(&dir, entry_fm, None, &resolved_key, &registry).unwrap();
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
            rename_and_encrypt_entry(&dir2, entry, None, &key2, &registry).unwrap();
        }
        let tree2 = vec![MetaTreeNode::default_for_new_file("Notes".to_string())];
        let lock_payload2 = LockPayload {
            real_tab_name: "Second Private".to_string(),
            tree: tree2,
            file_map: file_map2.clone(),
            last_viewed_memo: None,
            image_folders: Vec::new(),
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
            image_folders: Vec::new(),
        };
        write_unlock_marker(&dir2, &unlock_marker2, &registry).unwrap();
        for entry_fm in &resolved_payload2.file_map {
            if classify_unlock_entry(&dir2, entry_fm) == UnlockEntryState::NotRestored {
                restore_one_permanent_unlock_entry(&dir2, entry_fm, None, &resolved_key2, &registry).unwrap();
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

    // ---- GitHub issue #271: ロック中タブのメモ単位ランダム名画像フォルダ ----------------------

    const IMG_KEY: [u8; 32] = [9u8; 32];

    /// 日本語メモ「日記」(子「日記の続き」)を持つセッションを開始する。
    fn start_image_session(dir: &Path, sessions: &LockSessionStore, image_folders: Vec<ImageFolderEntry>) {
        let mut parent = MetaTreeNode::default_for_new_file("日記".to_string());
        parent.children.push(MetaTreeNode::default_for_new_file("日記の続き".to_string()));
        let file_map = vec![
            FileMapEntry { memo_name: "日記".to_string(), token: "tok-nikki".to_string() },
            FileMapEntry { memo_name: "日記の続き".to_string(), token: "tok-tsuzuki".to_string() },
        ];
        for e in &file_map {
            std::fs::write(dir.join(&e.token), b"encrypted-placeholder").unwrap();
        }
        sessions.start(
            "tab-1".to_string(),
            LockSession {
                real_tab_name: "非公開".to_string(),
                tree: vec![parent],
                file_map,
                kdf: KdfParams::generate_new(),
                hint: None,
                key: IMG_KEY,
                last_viewed_memo: None,
                image_folders,
            },
        );
    }

    /// ディスク上のロック blob を復号して対応表を返す。
    fn persisted_image_folders(dir: &Path) -> Vec<ImageFolderEntry> {
        let (parsed, _) = tree::load_tab_meta_with_recovery(dir);
        let field: LockField = serde_json::from_value(parsed.unwrap().lock.unwrap()).unwrap();
        lock_blob::decrypt_lock_field(&LockKey::new(&IMG_KEY), &field).unwrap().image_folders
    }

    #[test]
    fn session_memo_image_dir_assigns_a_random_folder_persists_the_map_and_reuses_it() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        start_image_session(&dir, &sessions, vec![]);

        let (path, name) = session_memo_image_dir(&sessions, &registry, &dir, "tab-1", "日記").unwrap();
        assert_eq!(path, dir.join(&name));
        assert_eq!(name.len(), 16);
        assert!(name.chars().all(|c| c.is_ascii_hexdigit()));
        // #263 のパスガードを通る名前
        assert_eq!(memo_image_folder_path(&dir, &name), Some(path.clone()));
        assert_eq!(persisted_image_folders(&dir), vec![ImageFolderEntry { memo_name: "日記".to_string(), folder: name.clone() }]);

        // 2 回目は同じフォルダを再利用(新しい割り当てなし)
        std::fs::create_dir_all(&path).unwrap();
        let (_, again) = session_memo_image_dir(&sessions, &registry, &dir, "tab-1", "日記").unwrap();
        assert_eq!(again, name);
        // 別メモには別フォルダ
        let (_, other) = session_memo_image_dir(&sessions, &registry, &dir, "tab-1", "日記の続き").unwrap();
        assert_ne!(other, name);
        assert_eq!(persisted_image_folders(&dir).len(), 2);

        // タブフォルダ直下の名前に実メモ名が現れない
        for entry in std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
            assert!(!entry.file_name().to_string_lossy().contains("日記"));
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn session_memo_image_dir_rejects_unknown_memo_and_unsafe_stored_folder_names() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        std::fs::write(dir.join("0123456789abcdef"), b"x").unwrap();
        start_image_session(
            &dir,
            &sessions,
            vec![
                ImageFolderEntry { memo_name: "日記".to_string(), folder: "..".to_string() },
                ImageFolderEntry { memo_name: "日記の続き".to_string(), folder: "0123456789abcdef".to_string() },
            ],
        );
        // 存在しないメモ
        assert_eq!(session_memo_image_dir(&sessions, &registry, &dir, "tab-1", "無いメモ").unwrap_err().code, ErrorCode::NotFound);
        // `..`(タブの親)やファイルが居座る名前は拒否し、タブ外/ファイルへは書かない
        assert_eq!(session_memo_image_dir(&sessions, &registry, &dir, "tab-1", "日記").unwrap_err().code, ErrorCode::NameInvalid);
        assert_eq!(session_memo_image_dir(&sessions, &registry, &dir, "tab-1", "日記の続き").unwrap_err().code, ErrorCode::NameInvalid);
        // セッションなし
        assert_eq!(session_memo_image_dir(&sessions, &registry, &dir, "tab-x", "日記").unwrap_err().code, ErrorCode::TabLocked);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn generate_unique_image_folder_avoids_existing_entries_and_map_names() {
        let dir = tempdir();
        let taken = vec![ImageFolderEntry { memo_name: "日記".to_string(), folder: "ABCDEF0123456789".to_string() }];
        for _ in 0..200 {
            let (path, name) = generate_unique_image_folder(&dir, &taken).unwrap();
            assert_eq!(memo_image_folder_path(&dir, &name), Some(path.clone()));
            assert!(std::fs::symlink_metadata(&path).is_err());
            assert!(!name.eq_ignore_ascii_case("abcdef0123456789"));
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn session_rename_memo_updates_the_image_folder_map_key_only() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        start_image_session(&dir, &sessions, vec![]);
        let (path, name) = session_memo_image_dir(&sessions, &registry, &dir, "tab-1", "日記").unwrap();
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("001.png"), b"img").unwrap();

        let AppResult::Ok(_) = session_rename_memo(&sessions, &registry, &dir, "tab-1", "日記", "旅行記") else { panic!("expected Ok") };

        // フォルダは改名しない
        assert!(path.join("001.png").exists());
        let expected = vec![ImageFolderEntry { memo_name: "旅行記".to_string(), folder: name.clone() }];
        assert_eq!(sessions.with_session("tab-1", |s| s.image_folders.clone()).unwrap(), expected);
        assert_eq!(persisted_image_folders(&dir), expected);
        // 新しい名前で引くと同じフォルダ
        let (_, again) = session_memo_image_dir(&sessions, &registry, &dir, "tab-1", "旅行記").unwrap();
        assert_eq!(again, name);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn session_delete_memo_force_direct_removes_subtree_image_folders_and_drops_entries() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        start_image_session(&dir, &sessions, vec![]);
        let mut paths = Vec::new();
        for memo in ["日記", "日記の続き"] {
            let (path, _) = session_memo_image_dir(&sessions, &registry, &dir, "tab-1", memo).unwrap();
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("001.png"), b"img").unwrap();
            paths.push(path);
        }
        // 対応表に無い既存のフラット画像(#271 以前)はそのまま残る
        std::fs::write(dir.join("img_0011223344556677.png"), b"old").unwrap();

        let AppResult::Ok(result) = session_delete_memo(&sessions, &registry, &dir, "tab-1", &["日記".to_string()], true) else { panic!("expected Ok") };
        assert!(result.warning.is_none());
        for path in &paths {
            assert!(!path.exists(), "subtree memo image folders must be deleted");
        }
        assert!(!dir.join("tok-nikki").exists() && !dir.join("tok-tsuzuki").exists());
        assert!(dir.join("img_0011223344556677.png").exists());
        assert!(sessions.with_session("tab-1", |s| s.image_folders.clone()).unwrap().is_empty());
        assert!(persisted_image_folders(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn session_delete_memo_without_image_folder_is_not_an_error() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        // 対応表にはあるがフォルダ未作成(作成前に失敗した等)
        start_image_session(&dir, &sessions, vec![ImageFolderEntry { memo_name: "日記".to_string(), folder: "fedcba9876543210".to_string() }]);
        let AppResult::Ok(result) = session_delete_memo(&sessions, &registry, &dir, "tab-1", &["日記".to_string()], false) else { panic!("expected Ok") };
        assert!(result.warning.is_none());
        assert!(persisted_image_folders(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cleanup_session_move_source_drops_moved_memos_from_the_image_folder_map() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        start_image_session(&dir, &sessions, vec![]);
        let (path, _) = session_memo_image_dir(&sessions, &registry, &dir, "tab-1", "日記の続き").unwrap();
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("001.png"), b"img").unwrap();

        // 「日記」とその子「日記の続き」を移動した後の後始末
        let names = vec!["日記".to_string(), "日記の続き".to_string()];
        let tokens: HashMap<String, String> = [("日記", "tok-nikki"), ("日記の続き", "tok-tsuzuki")].iter().map(|(n, t)| (n.to_string(), t.to_string())).collect();
        cleanup_session_move_source(&sessions, &registry, &dir, "tab-1", &names, &tokens).unwrap();

        assert!(sessions.with_session("tab-1", |s| s.image_folders.clone()).unwrap().is_empty());
        assert!(persisted_image_folders(&dir).is_empty());
        // フォルダ自体は触らない(#262 の範囲)
        assert!(path.join("001.png").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    // ---- GitHub issue #272: ロック時の `<メモ名>/` 画像フォルダのランダム名変換 -----------------

    const T272_KEY: [u8; 32] = [7u8; 32];
    /// 「日記」の本文: 生・山括弧・パーセントエンコードの各形、インラインコード、子メモのフォルダ参照。
    const T272_NIKKI: &str = "# 日記\n![](日記/001.png)\n![図](<日記/002.png>)\n![](%E6%97%A5%E8%A8%98/003.png)\n`![](日記/001.png)`\n![](日記の続き/001.png)\n";
    /// 「日記の続き」の本文(BOM + CRLF)。
    const T272_TSUZUKI: &str = "\u{FEFF}![](日記の続き/001.png)\r\n";
    /// 空白入りメモ名: 山括弧形は書き換え対象、山括弧なし(#266 で元々壊れている形)は触らない。
    const T272_SPACE: &str = "![](<新 メモ/001.png>)\n![](新 メモ/002.png)\n";
    const T272_OLD: &str = "![](img_old.png)\n";

    /// 日本語メモ「日記」(子「日記の続き」)・「新 メモ」に画像フォルダ、#139 以前の「古いメモ」は
    /// フォルダ無し(タブ直下のフラット画像)の平文タブを作り、ツリーを返す。
    fn t272_setup(dir: &Path, registry: &SelfWriteRegistry) -> Vec<MetaTreeNode> {
        let img = |folder: &str| {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
            std::fs::write(dir.join(folder).join("001.png"), b"png-bytes").unwrap();
        };
        std::fs::write(dir.join("日記.md"), T272_NIKKI).unwrap();
        img("日記");
        std::fs::write(dir.join("日記の続き.md"), T272_TSUZUKI).unwrap();
        img("日記の続き");
        std::fs::write(dir.join("新 メモ.md"), T272_SPACE).unwrap();
        img("新 メモ");
        std::fs::write(dir.join("古いメモ.md"), T272_OLD).unwrap();
        std::fs::write(dir.join("img_old.png"), b"png-bytes").unwrap();
        let mut parent = MetaTreeNode::default_for_new_file("日記".to_string());
        parent.children.push(MetaTreeNode::default_for_new_file("日記の続き".to_string()));
        let nodes = vec![parent, MetaTreeNode::default_for_new_file("新 メモ".to_string()), MetaTreeNode::default_for_new_file("古いメモ".to_string())];
        tree::save_tab_meta(dir, TAB_META_SCHEMA_VERSION, "test".to_string(), &nodes, None, registry).unwrap();
        nodes
    }

    /// タブフォルダ配下の全ファイル(相対パス -> 内容)。`.tabtreenote*` は除く。
    fn t272_snapshot(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        fn walk(base: &Path, cur: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
            for e in std::fs::read_dir(cur).unwrap().filter_map(|e| e.ok()) {
                let path = e.path();
                let rel = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
                if rel.starts_with(".tabtreenote") {
                    continue;
                }
                if path.is_dir() {
                    walk(base, &path, out);
                } else {
                    out.insert(rel, std::fs::read(&path).unwrap());
                }
            }
        }
        let mut out = std::collections::BTreeMap::new();
        walk(dir, dir, &mut out);
        out
    }

    /// `lock_folder_files` の step 6(計画 + マーカー書き込み)だけを行う。
    fn t272_begin(dir: &Path, nodes: &[MetaTreeNode], registry: &SelfWriteRegistry) -> LockInProgressMarker {
        let mut names = Vec::new();
        for n in nodes {
            tree::collect_names(n, &mut names);
        }
        let file_map: Vec<FileMapEntry> = names
            .iter()
            .map(|name| FileMapEntry {
                memo_name: name.clone(),
                token: obfuscated_naming::generate_unique_token(dir),
            })
            .collect();
        let marker = LockInProgressMarker {
            format_version: MARKER_FORMAT_VERSION,
            kdf: KdfParams::generate_new(),
            file_map,
            image_folders: plan_lock_image_folders(dir, &names).unwrap(),
        };
        write_marker(dir, &marker, registry).unwrap();
        marker
    }

    fn t272_encrypt(dir: &Path, marker: &LockInProgressMarker, memo: &str, registry: &SelfWriteRegistry) {
        let entry = marker.file_map.iter().find(|e| e.memo_name == memo).unwrap();
        rename_and_encrypt_entry(dir, entry, image_folder_for(&marker.image_folders, memo), &LockKey::new(&T272_KEY), registry).unwrap();
    }

    fn t272_write_blob(dir: &Path, marker: &LockInProgressMarker, nodes: &[MetaTreeNode], registry: &SelfWriteRegistry) {
        let payload = LockPayload {
            real_tab_name: "非公開".to_string(),
            tree: nodes.to_vec(),
            file_map: marker.file_map.clone(),
            last_viewed_memo: None,
            image_folders: marker.image_folders.clone(),
        };
        let field = lock_blob::encrypt_lock_field(&LockKey::new(&T272_KEY), marker.kdf.clone(), None, &payload).unwrap();
        write_lock_field(dir, &field, registry).unwrap();
    }

    fn t272_payload(dir: &Path) -> LockPayload {
        let (parsed, _) = tree::load_tab_meta_with_recovery(dir);
        let field: LockField = serde_json::from_value(parsed.unwrap().lock.unwrap()).unwrap();
        lock_blob::decrypt_lock_field(&LockKey::new(&T272_KEY), &field).unwrap()
    }

    /// 完全にロックされた状態を検証する: 対応表・フォルダ名・暗号化本文のリンク・対象外ファイル。
    fn t272_assert_locked(dir: &Path) {
        let payload = t272_payload(dir);
        let mut mapped: Vec<&str> = payload.image_folders.iter().map(|e| e.memo_name.as_str()).collect();
        mapped.sort();
        assert_eq!(mapped, vec!["新 メモ", "日記", "日記の続き"]);
        let rand = |memo: &str| image_folder_for(&payload.image_folders, memo).unwrap().to_string();
        for e in &payload.image_folders {
            assert!(dir.join(&e.folder).join("001.png").exists());
            assert!(std::fs::symlink_metadata(dir.join(&e.memo_name)).is_err(), "{} folder must be gone", e.memo_name);
        }
        let key = LockKey::new(&T272_KEY);
        let body = |memo: &str| {
            let tok = &payload.file_map.iter().find(|e| e.memo_name == memo).unwrap().token;
            String::from_utf8(lock_blob::decrypt_memo_body(&key, &std::fs::read(dir.join(tok)).unwrap()).unwrap()).unwrap()
        };
        let r = rand("日記");
        // 子メモのフォルダ参照は本人の本文ではないので対象外(#261 と同じ範囲)
        let want = format!("# 日記\n![]({r}/001.png)\n![図](<{r}/002.png>)\n![]({r}/003.png)\n`![](日記/001.png)`\n![](日記の続き/001.png)\n");
        assert_eq!(body("日記"), want);
        assert_eq!(body("日記の続き"), format!("\u{FEFF}![]({}/001.png)\r\n", rand("日記の続き")));
        assert_eq!(body("新 メモ"), format!("![](<{}/001.png>)\n![](新 メモ/002.png)\n", rand("新 メモ")));
        assert_eq!(body("古いメモ"), T272_OLD);
        assert!(dir.join("img_old.png").exists());
        assert!(tree::scan_real_memo_names(dir).is_empty());
        assert!(!marker_path(dir).exists());
        // タブ直下に実メモ名が現れない
        for entry in std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.contains("日記") && !name.contains("メモ"), "leaked name: {name}");
        }
    }

    #[test]
    fn lock_converts_image_folders_links_and_map() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let nodes = t272_setup(&dir, &registry);
        lock_folder_files(&dir, nodes, None, "非公開".to_string(), &LockKey::new(&T272_KEY), KdfParams::generate_new(), None, &registry).unwrap();
        t272_assert_locked(&dir);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lock_without_any_image_folder_leaves_an_empty_map() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        std::fs::write(dir.join("古いメモ.md"), T272_OLD).unwrap();
        std::fs::write(dir.join("img_old.png"), b"png-bytes").unwrap();
        let nodes = vec![MetaTreeNode::default_for_new_file("古いメモ".to_string())];
        lock_folder_files(&dir, nodes, None, "非公開".to_string(), &LockKey::new(&T272_KEY), KdfParams::generate_new(), None, &registry).unwrap();
        let payload = t272_payload(&dir);
        assert!(payload.image_folders.is_empty());
        let tok = &payload.file_map[0].token;
        assert_eq!(lock_blob::decrypt_memo_body(&LockKey::new(&T272_KEY), &std::fs::read(dir.join(tok)).unwrap()).unwrap(), T272_OLD.as_bytes());
        assert!(dir.join("img_old.png").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn convert_image_folders_rolls_back_when_a_destination_is_squatted() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let nodes = t272_setup(&dir, &registry);
        let before = t272_snapshot(&dir);
        let marker = t272_begin(&dir, &nodes, &registry);
        assert_eq!(marker.image_folders.len(), 3);
        // 3 番目の改名先にファイルを置いて改名を失敗させる
        std::fs::write(dir.join(&marker.image_folders[2].folder), b"squatter").unwrap();
        assert_eq!(convert_image_folders_for_lock(&dir, &marker.image_folders, &registry), Err(true));
        std::fs::remove_file(dir.join(&marker.image_folders[2].folder)).unwrap();
        assert_eq!(t272_snapshot(&dir), before);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 実際の失敗要因(フォルダ内のファイルを他プロセスが共有なしで開いている)で `tab_lock` 全体が
    /// 中止・巻き戻しされ、元の状態(本文・フォルダ・メタ・マーカー無し)に戻ることを確認する。
    #[cfg(windows)]
    #[test]
    fn lock_aborts_and_restores_original_state_when_a_folder_is_in_use() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let nodes = t272_setup(&dir, &registry);
        let before = t272_snapshot(&dir);
        let meta_before = std::fs::read(dir.join(TAB_META_FILE_NAME)).unwrap();
        // ツリー順で最後の画像フォルダ(「新 メモ」)を使用中にする -> 先の 2 つは改名後に戻される
        let handle = std::fs::OpenOptions::new().read(true).share_mode(0).open(dir.join("新 メモ").join("001.png")).unwrap();
        let err = lock_folder_files(&dir, nodes, None, "非公開".to_string(), &LockKey::new(&T272_KEY), KdfParams::generate_new(), None, &registry).unwrap_err();
        drop(handle);
        assert_eq!(err.code, ErrorCode::ImageFolderRenameFailed);
        assert_eq!(t272_snapshot(&dir), before);
        assert_eq!(std::fs::read(dir.join(TAB_META_FILE_NAME)).unwrap(), meta_before);
        assert!(!marker_path(&dir).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 中断点 A/B/C(マーカー書き込み後、本文暗号化前): 進捗ゼロとして `tree_open` が
    /// パスワード無しで画像フォルダを戻し、マーカーを消す。
    #[test]
    fn interrupted_before_any_encryption_auto_rolls_back_image_folders() {
        for renamed_folders in [0usize, 1, 3] {
            let dir = tempdir();
            let registry = SelfWriteRegistry::new();
            let nodes = t272_setup(&dir, &registry);
            let before = t272_snapshot(&dir);
            let marker = t272_begin(&dir, &nodes, &registry);
            convert_image_folders_for_lock(&dir, &marker.image_folders[..renamed_folders], &registry).unwrap();
            assert!(matches!(check_marker_on_tree_open(&dir, &registry), MarkerCheck::NoMarker));
            assert!(!marker_path(&dir).exists(), "case {renamed_folders}");
            assert_eq!(t272_snapshot(&dir), before, "case {renamed_folders}");
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// 戻せないフォルダがあればマーカーを残す(次回 `tree_open` で再試行)。
    #[cfg(windows)]
    #[test]
    fn auto_rollback_keeps_the_marker_when_a_folder_cannot_be_renamed_back() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let nodes = t272_setup(&dir, &registry);
        let before = t272_snapshot(&dir);
        let marker = t272_begin(&dir, &nodes, &registry);
        convert_image_folders_for_lock(&dir, &marker.image_folders, &registry).unwrap();
        let busy = dir.join(&marker.image_folders[0].folder).join("001.png");
        let handle = std::fs::OpenOptions::new().read(true).share_mode(0).open(&busy).unwrap();
        assert!(matches!(check_marker_on_tree_open(&dir, &registry), MarkerCheck::NoMarker));
        assert!(marker_path(&dir).exists());
        drop(handle);
        // 再試行で完了
        assert!(matches!(check_marker_on_tree_open(&dir, &registry), MarkerCheck::NoMarker));
        assert!(!marker_path(&dir).exists());
        assert_eq!(t272_snapshot(&dir), before);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 中断点 D(一部メモ暗号化済み)と E(ロック blob 書き込み済み・マーカー削除前)。
    /// 「ロックを完了」はリンク書き換え済み・対応表付きでロックを完了する(#271 の空の対応表で
    /// 上書きしない)。「元に戻す」は本文のリンクとフォルダ名を元に戻す。
    #[test]
    fn interrupted_mid_encryption_or_before_marker_delete_finishes_or_restores() {
        for after_blob in [false, true] {
            for finish in [true, false] {
                let dir = tempdir();
                let registry = SelfWriteRegistry::new();
                let nodes = t272_setup(&dir, &registry);
                let before = t272_snapshot(&dir);
                let marker = t272_begin(&dir, &nodes, &registry);
                convert_image_folders_for_lock(&dir, &marker.image_folders, &registry).unwrap();
                t272_encrypt(&dir, &marker, "日記", &registry);
                if after_blob {
                    for memo in ["日記の続き", "新 メモ", "古いメモ"] {
                        t272_encrypt(&dir, &marker, memo, &registry);
                    }
                    t272_write_blob(&dir, &marker, &nodes, &registry);
                }
                assert!(matches!(check_marker_on_tree_open(&dir, &registry), MarkerCheck::RecoveryPending));
                let key = LockKey::new(&T272_KEY);
                if finish {
                    recovery_finish_files(&dir, &marker, &key, "非公開".to_string(), &registry).unwrap();
                    delete_marker(&dir, &registry).unwrap();
                    t272_assert_locked(&dir);
                } else {
                    assert!(recovery_restore_files(&dir, &marker, &key, &registry).unwrap());
                    delete_marker(&dir, &registry).unwrap();
                    // 暗号化済みだった「日記」だけは、パーセントエンコード形のリンクが生の形で戻る
                    // (ランダム名は16進でエンコード形と生の形が同一になり、元の形を区別できないため。
                    // 指す先は同じ)。それ以外はバイト単位で元どおり。
                    let mut want = before.clone();
                    want.insert("日記.md".to_string(), T272_NIKKI.replace("%E6%97%A5%E8%A8%98/", "日記/").into_bytes());
                    assert_eq!(t272_snapshot(&dir), want, "after_blob={after_blob}");
                    let (parsed, _) = tree::load_tab_meta_with_recovery(&dir);
                    let parsed = parsed.unwrap();
                    assert!(parsed.lock.is_none());
                    assert_eq!(parsed.tree.unwrap(), nodes);
                }
                std::fs::remove_dir_all(&dir).ok();
            }
        }
    }

    /// 一部フォルダがまだ `<メモ名>/` のままでも(通常は起きないが冪等性の確認)、
    /// 「ロックを完了」が残りを改名してから暗号化する。
    #[test]
    fn recovery_finish_converts_folders_left_at_memo_names() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let nodes = t272_setup(&dir, &registry);
        let marker = t272_begin(&dir, &nodes, &registry);
        convert_image_folders_for_lock(&dir, &marker.image_folders[..1], &registry).unwrap();
        t272_encrypt(&dir, &marker, "古いメモ", &registry);
        recovery_finish_files(&dir, &marker, &LockKey::new(&T272_KEY), "非公開".to_string(), &registry).unwrap();
        delete_marker(&dir, &registry).unwrap();
        t272_assert_locked(&dir);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// #272 以前に書かれたマーカー(`imageFolders` 無し)は空の対応表として読める。
    #[test]
    fn pre_272_marker_without_image_folders_still_parses() {
        let marker = LockInProgressMarker {
            format_version: MARKER_FORMAT_VERSION,
            kdf: KdfParams::generate_new(),
            file_map: vec![],
            image_folders: vec![ImageFolderEntry {
                memo_name: "日記".to_string(),
                folder: "0123456789abcdef".to_string(),
            }],
        };
        let mut value = serde_json::to_value(&marker).unwrap();
        assert!(value.as_object_mut().unwrap().remove("imageFolders").is_some());
        let parsed: LockInProgressMarker = serde_json::from_value(value).unwrap();
        assert!(parsed.image_folders.is_empty());
    }

    // -----------------------------------------------------------------------
    // GitHub issue #276: ロック後に平文の .bak / 一時ファイルが残らない
    // -----------------------------------------------------------------------

    const T276_NAMES: [&str; 4] = ["日記", "日記の続き", "新 メモ", "古いメモ"];
    const T276_STRAY: &str = ".ttn-tmp-00000000deadbeef";

    fn t276_old_body(memo: &str) -> String {
        format!("旧版の本文: {memo} の下書き\n")
    }

    /// t272_setup の平文タブに、通常の保存で出来る残骸を作る: 各メモの `<メモ名>.md.bak`(旧版本文)、
    /// 平文メタの `.tabtreenote.json.bak`、中断した保存の一時ファイル。
    fn t276_setup(dir: &Path, registry: &SelfWriteRegistry) -> Vec<MetaTreeNode> {
        let nodes = t272_setup(dir, registry);
        for memo in T276_NAMES {
            let path = dir.join(format!("{memo}.md"));
            let current = std::fs::read(&path).unwrap();
            atomic_write(&path, t276_old_body(memo).as_bytes(), registry).unwrap();
            atomic_write(&path, &current, registry).unwrap();
            assert!(bak_path(&path).exists());
        }
        tree::save_tab_meta(dir, TAB_META_SCHEMA_VERSION, "test".to_string(), &nodes, None, registry).unwrap();
        assert!(dir.join(format!("{TAB_META_FILE_NAME}.bak")).exists());
        std::fs::write(dir.join(T276_STRAY), t276_old_body("日記")).unwrap();
        nodes
    }

    /// タブフォルダ配下の全ファイル(`.tabtreenote*` を含む)を走査し、ファイル名にもメモ名が無く、
    /// 内容にもメモ名・本文(旧版を含む)が平文で現れないことを確認する。
    fn t276_assert_no_plaintext(dir: &Path) {
        fn walk(cur: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(cur).unwrap().filter_map(|e| e.ok()) {
                if e.path().is_dir() {
                    walk(&e.path(), out);
                } else {
                    out.push(e.path());
                }
            }
        }
        let mut files = Vec::new();
        walk(dir, &mut files);
        let mut needles: Vec<Vec<u8>> = T276_NAMES.iter().map(|n| n.as_bytes().to_vec()).collect();
        needles.extend(T276_NAMES.iter().map(|n| t276_old_body(n).into_bytes()));
        needles.push(T272_OLD.as_bytes().to_vec());
        for f in files {
            let rel = f.strip_prefix(dir).unwrap().to_string_lossy().to_string();
            for n in T276_NAMES {
                assert!(!rel.contains(n), "memo name {n} in file name {rel}");
            }
            assert!(!crate::fs::temp_naming::is_temp_file_name(&rel), "stray temp file {rel}");
            let bytes = std::fs::read(&f).unwrap();
            for needle in &needles {
                assert!(!bytes.windows(needle.len()).any(|w| w == needle.as_slice()), "plaintext {:?} in {rel}", String::from_utf8_lossy(needle));
            }
        }
    }

    #[test]
    fn lock_leaves_no_plaintext_backup_or_temp_file() {
        let dir = tempdir().join("タブ フォルダ");
        std::fs::create_dir_all(&dir).unwrap();
        let registry = SelfWriteRegistry::new();
        let nodes = t276_setup(&dir, &registry);
        lock_folder_files(&dir, nodes, None, "非公開".to_string(), &LockKey::new(&T272_KEY), KdfParams::generate_new(), None, &registry).unwrap();
        t272_assert_locked(&dir);
        t276_assert_no_plaintext(&dir);
        std::fs::remove_dir_all(dir.parent().unwrap()).ok();
    }

    /// 中断点 D(一部暗号化済み)/ E(ロック blob 書き込み済み・step 8b 前)からの「ロックを完了」でも
    /// 平文残骸が消える。マーカーの `.bak` も消す。
    #[test]
    fn recovery_finish_removes_plaintext_backups() {
        for after_blob in [false, true] {
            let dir = tempdir();
            let registry = SelfWriteRegistry::new();
            let nodes = t276_setup(&dir, &registry);
            let marker = t272_begin(&dir, &nodes, &registry);
            std::fs::write(dir.join(format!("{LOCK_INPROGRESS_MARKER_NAME}.bak")), std::fs::read(marker_path(&dir)).unwrap()).unwrap();
            convert_image_folders_for_lock(&dir, &marker.image_folders, &registry).unwrap();
            t272_encrypt(&dir, &marker, "日記", &registry);
            if after_blob {
                for memo in ["日記の続き", "新 メモ", "古いメモ"] {
                    t272_encrypt(&dir, &marker, memo, &registry);
                }
                t272_write_blob(&dir, &marker, &nodes, &registry);
            }
            recovery_finish_files(&dir, &marker, &LockKey::new(&T272_KEY), "非公開".to_string(), &registry).unwrap();
            delete_marker(&dir, &registry).unwrap();
            t272_assert_locked(&dir);
            t276_assert_no_plaintext(&dir);
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// step 8b の後・マーカー削除前に中断しても、「元に戻す」は削除済みの `.bak` に頼らず
    /// ロック blob からツリーを戻せる。
    #[test]
    fn recovery_restore_does_not_need_the_deleted_backups() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let nodes = t276_setup(&dir, &registry);
        let marker = t272_begin(&dir, &nodes, &registry);
        convert_image_folders_for_lock(&dir, &marker.image_folders, &registry).unwrap();
        for memo in T276_NAMES {
            t272_encrypt(&dir, &marker, memo, &registry);
        }
        t272_write_blob(&dir, &marker, &nodes, &registry);
        remove_plaintext_residue(&dir, &marker.file_map, true, &registry);
        assert!(!dir.join(format!("{TAB_META_FILE_NAME}.bak")).exists());
        assert!(recovery_restore_files(&dir, &marker, &LockKey::new(&T272_KEY), &registry).unwrap());
        delete_marker(&dir, &registry).unwrap();
        let (parsed, _) = tree::load_tab_meta_with_recovery(&dir);
        assert_eq!(parsed.unwrap().tree.unwrap(), nodes);
        let mut restored = tree::scan_real_memo_names(&dir);
        restored.sort();
        let mut want: Vec<String> = T276_NAMES.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(restored, want);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 旧ビルドでロック済みのタブ: アンロック時の削除(`tab_unlock` が呼ぶのと同じ引数)で
    /// `fileMap` のメモ名に一致する平文 `.bak` と平文メタの `.bak` だけが消え、暗号化済みの
    /// `<token>.bak`、暗号化形のメタ `.bak`、無関係なファイル、一時ファイルは残る。
    #[test]
    fn unlock_cleanup_removes_only_this_tabs_plaintext_backups() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let nodes = t272_setup(&dir, &registry);
        lock_folder_files(&dir, nodes.clone(), None, "非公開".to_string(), &LockKey::new(&T272_KEY), KdfParams::generate_new(), None, &registry).unwrap();
        let payload = t272_payload(&dir);
        let token = payload.file_map[0].token.clone();
        let token_bak = dir.join(format!("{token}.bak"));
        std::fs::write(&token_bak, std::fs::read(dir.join(&token)).unwrap()).unwrap();
        let token_bak_bytes = std::fs::read(&token_bak).unwrap();
        // 旧ビルドの残骸
        for memo in T276_NAMES {
            std::fs::write(dir.join(format!("{memo}.md.bak")), t276_old_body(memo)).unwrap();
        }
        let plain_meta = TabMetaFile {
            schema_version: TAB_META_SCHEMA_VERSION,
            app_version: "old".to_string(),
            tree: Some(nodes.clone()),
            lock: None,
            last_viewed_memo: None,
        };
        let meta_bak = dir.join(format!("{TAB_META_FILE_NAME}.bak"));
        std::fs::write(&meta_bak, serde_json::to_vec_pretty(&plain_meta).unwrap()).unwrap();
        std::fs::write(dir.join("無関係.md.bak"), b"keep").unwrap();
        std::fs::write(dir.join(T276_STRAY), b"keep").unwrap();

        remove_plaintext_residue(&dir, &payload.file_map, false, &registry);

        for memo in T276_NAMES {
            assert!(!dir.join(format!("{memo}.md.bak")).exists(), "{memo}.md.bak");
        }
        assert!(!meta_bak.exists());
        assert_eq!(std::fs::read(&token_bak).unwrap(), token_bak_bytes);
        assert!(dir.join("無関係.md.bak").exists());
        assert!(dir.join(T276_STRAY).exists());

        // 暗号化形のメタ `.bak`(セッション中の保存で出来るもの)は残す
        std::fs::copy(dir.join(TAB_META_FILE_NAME), &meta_bak).unwrap();
        remove_plaintext_residue(&dir, &payload.file_map, false, &registry);
        assert!(meta_bak.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    // -----------------------------------------------------------------------
    // GitHub issue #273: 完全解除でランダム名画像フォルダを `<メモ名>/` へ戻しリンクを書き換える
    // -----------------------------------------------------------------------

    /// t272_setup の平文タブを #272 の手順でロックし、ツリーとロック前のスナップショットを返す。
    fn t273_locked(dir: &Path, registry: &SelfWriteRegistry) -> (Vec<MetaTreeNode>, std::collections::BTreeMap<String, Vec<u8>>) {
        let nodes = t272_setup(dir, registry);
        let before = t272_snapshot(dir);
        lock_folder_files(dir, nodes.clone(), None, "非公開".to_string(), &LockKey::new(&T272_KEY), KdfParams::generate_new(), None, registry).unwrap();
        (nodes, before)
    }

    /// `tab_permanent_unlock` の step 5-6(復号した payload で計画し、マーカーを書く)。
    fn t273_begin(dir: &Path, registry: &SelfWriteRegistry) -> (LockPayload, Vec<String>) {
        let payload = t272_payload(dir);
        let planned = plan_unlock_image_folders(dir, &payload);
        let marker = UnlockInProgressMarker {
            format_version: UNLOCK_MARKER_FORMAT_VERSION,
            image_folders: planned.clone(),
        };
        write_unlock_marker(dir, &marker, registry).unwrap();
        (payload, planned)
    }

    /// `tab_permanent_unlock` の step 5-9(ファイル操作部分)を通しで行い、ランダム名のまま残った数を返す。
    fn t273_unlock(dir: &Path, registry: &SelfWriteRegistry) -> usize {
        let (payload, planned) = t273_begin(dir, registry);
        permanent_unlock_restore_all(dir, &payload, &planned, &LockKey::new(&T272_KEY), registry).unwrap();
        let left = count_image_folders_left_random(dir, &payload);
        delete_unlock_marker(dir, registry).unwrap();
        left
    }

    /// `tree_open` の step 0(完全解除マーカー)。設定ファイルに触れる「完了済み」分岐には入らない状態でだけ呼ぶ。
    fn t273_tree_open_pending(dir: &Path, registry: &SelfWriteRegistry) -> bool {
        let store = AppSettingsStore::new();
        let sessions = LockSessionStore::new();
        let search_index = SearchIndex::new();
        matches!(check_unlock_marker_on_tree_open(dir, registry, &store, &sessions, &search_index, "tab-273"), UnlockMarkerCheck::RecoveryPending)
    }

    /// 完全解除後: フォルダは `<メモ名>/`、本文リンクは `<メモ名>/…`(パーセントエンコード形は生の形)、
    /// メタは平文ツリー(対応表は消える)、トークン・マーカー無し。
    fn t273_assert_unlocked(dir: &Path, nodes: &[MetaTreeNode], before: &std::collections::BTreeMap<String, Vec<u8>>) {
        let mut want = before.clone();
        want.insert("日記.md".to_string(), T272_NIKKI.replace("%E6%97%A5%E8%A8%98/", "日記/").into_bytes());
        assert_eq!(t272_snapshot(dir), want);
        let (parsed, _) = tree::load_tab_meta_with_recovery(dir);
        let parsed = parsed.unwrap();
        assert!(parsed.lock.is_none());
        assert_eq!(parsed.tree.unwrap(), nodes);
        assert!(!unlock_marker_path(dir).exists());
    }

    /// ロック中の「日記」の復号本文(ランダム名リンク)。
    fn t273_locked_body(dir: &Path, payload: &LockPayload, memo: &str) -> Vec<u8> {
        let tok = &payload.file_map.iter().find(|e| e.memo_name == memo).unwrap().token;
        lock_blob::decrypt_memo_body(&LockKey::new(&T272_KEY), &std::fs::read(dir.join(tok)).unwrap()).unwrap()
    }

    #[test]
    fn permanent_unlock_renames_image_folders_back_rewrites_links_and_drops_map() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let (nodes, before) = t273_locked(&dir, &registry);
        assert_eq!(t273_unlock(&dir, &registry), 0);
        // 親子(日記 / 日記の続き)・空白入り名・フラット画像(img_old.png)を含めて元どおり
        t273_assert_unlocked(&dir, &nodes, &before);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn permanent_unlock_keeps_random_name_when_memo_folder_already_exists() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let (_nodes, _before) = t273_locked(&dir, &registry);
        let payload = t272_payload(&dir);
        let random = image_folder_for(&payload.image_folders, "日記").unwrap().to_string();
        let locked_body = t273_locked_body(&dir, &payload, "日記");
        // 同名の `<メモ名>/` が既にある(マージ・上書きしない)
        std::fs::create_dir_all(dir.join("日記")).unwrap();
        std::fs::write(dir.join("日記").join("other.png"), b"other").unwrap();

        assert_eq!(t273_unlock(&dir, &registry), 1);
        assert_eq!(std::fs::read(dir.join("日記.md")).unwrap(), locked_body, "links keep pointing at the random folder");
        assert!(dir.join(&random).join("001.png").exists());
        let names: Vec<String> = std::fs::read_dir(dir.join("日記")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec!["other.png".to_string()]);
        // 他のメモは通常どおり戻る
        assert!(dir.join("日記の続き").join("001.png").exists());
        assert_eq!(std::fs::read(dir.join("日記の続き.md")).unwrap(), T272_TSUZUKI.as_bytes());
        assert_eq!(std::fs::read(dir.join("新 メモ.md")).unwrap(), T272_SPACE.as_bytes());
        assert!(tree::load_tab_meta_with_recovery(&dir).0.unwrap().lock.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 実際の失敗要因(フォルダ内ファイルを共有なしで開いている / フォルダをカレントにしたプロセス)
    /// でも完全解除は続行し、そのメモだけランダム名・リンクそのままで残る。
    #[cfg(windows)]
    #[test]
    fn permanent_unlock_continues_when_a_folder_is_in_use() {
        use std::os::windows::fs::OpenOptionsExt;
        for use_cwd in [false, true] {
            let dir = tempdir();
            let registry = SelfWriteRegistry::new();
            let (_nodes, _before) = t273_locked(&dir, &registry);
            let payload = t272_payload(&dir);
            let random = image_folder_for(&payload.image_folders, "新 メモ").unwrap().to_string();
            let locked_body = t273_locked_body(&dir, &payload, "新 メモ");

            let mut child = None;
            let mut handle = None;
            if use_cwd {
                child = Some(std::process::Command::new("cmd").args(["/c", "ping -n 60 127.0.0.1 >nul"]).current_dir(dir.join(&random)).spawn().unwrap());
            } else {
                handle = Some(std::fs::OpenOptions::new().read(true).share_mode(0).open(dir.join(&random).join("001.png")).unwrap());
            }
            let left = t273_unlock(&dir, &registry);
            drop(handle);
            if let Some(mut c) = child {
                c.kill().ok();
                c.wait().ok();
            }

            assert_eq!(left, 1, "use_cwd={use_cwd}");
            assert_eq!(std::fs::read(dir.join("新 メモ.md")).unwrap(), locked_body);
            assert!(dir.join(&random).join("001.png").exists());
            assert!(std::fs::symlink_metadata(dir.join("新 メモ")).is_err());
            assert!(dir.join("日記").join("001.png").exists());
            assert_eq!(std::fs::read(dir.join("日記の続き.md")).unwrap(), T272_TSUZUKI.as_bytes());
            assert!(tree::load_tab_meta_with_recovery(&dir).0.unwrap().lock.is_none());
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// 中断点ごとの復旧: (0) マーカー直後 (1) フォルダ 1 つ改名後 (2) 全フォルダ改名 + 本文 1 件復元後
    /// (3) メタ書き込み後(マーカー削除前)。どの時点でも平文 `.md` のリンク先フォルダは実在する。
    #[test]
    fn permanent_unlock_interrupted_at_each_point_recovers() {
        let key = LockKey::new(&T272_KEY);
        for point in 0..4 {
            for finish in [true, false] {
                let dir = tempdir();
                let registry = SelfWriteRegistry::new();
                let (nodes, before) = t273_locked(&dir, &registry);
                let (payload, planned) = t273_begin(&dir, &registry);
                assert_eq!(planned.len(), 3);
                if point >= 1 {
                    let e = payload.image_folders.iter().find(|e| e.folder == planned[0]).unwrap();
                    rename_image_folder(&dir, &e.folder, &e.memo_name, &registry).unwrap();
                }
                if point >= 2 {
                    for e in payload.image_folders.iter().filter(|e| e.folder != planned[0]) {
                        rename_image_folder(&dir, &e.folder, &e.memo_name, &registry).unwrap();
                    }
                    let fm = unlock_file_map_entry(&payload, "日記").unwrap();
                    let from = planned_folder_at_memo_name(&dir, &payload, &planned, "日記").map(|e| e.folder.as_str());
                    restore_one_permanent_unlock_entry(&dir, fm, from, &key, &registry).unwrap();
                    // 復元済みの本文は既に存在するフォルダ(日記/)を指す
                    assert_eq!(std::fs::read(dir.join("日記.md")).unwrap(), T272_NIKKI.replace("%E6%97%A5%E8%A8%98/", "日記/").into_bytes());
                }
                if point == 3 {
                    permanent_unlock_restore_all(&dir, &payload, &planned, &key, &registry).unwrap();
                    // 「完了済み」(メタが平文ツリー): tree_open はマーカーを消すだけ。ファイルは既に最終状態。
                    std::fs::remove_file(unlock_marker_path(&dir)).unwrap();
                    t273_assert_unlocked(&dir, &nodes, &before);
                    std::fs::remove_dir_all(&dir).ok();
                    continue;
                }

                if point == 0 {
                    // 何も改名していない: パスワード無しでマーカーを消し、ロック状態のまま
                    assert!(!t273_tree_open_pending(&dir, &registry));
                    assert!(!unlock_marker_path(&dir).exists());
                    t272_assert_locked(&dir);
                    std::fs::remove_dir_all(&dir).ok();
                    continue;
                }
                // `.md` が 0 件でもフォルダを改名済みならパスワード付きの復旧へ回す
                assert!(t273_tree_open_pending(&dir, &registry), "point={point}");
                let marker = load_unlock_marker(&dir).unwrap();
                assert_eq!(marker.image_folders, planned);
                let payload = t272_payload(&dir);
                if finish {
                    permanent_unlock_restore_all(&dir, &payload, &marker.image_folders, &key, &registry).unwrap();
                    assert_eq!(count_image_folders_left_random(&dir, &payload), 0);
                    delete_unlock_marker(&dir, &registry).unwrap();
                    t273_assert_unlocked(&dir, &nodes, &before);
                } else {
                    permanent_unlock_relock_files(&dir, &payload, &marker.image_folders, &key, &registry).unwrap();
                    delete_unlock_marker(&dir, &registry).unwrap();
                    t272_assert_locked(&dir);
                }
                std::fs::remove_dir_all(&dir).ok();
            }
        }
    }

    /// 「ロックに戻す」で本文の再暗号化後にフォルダを戻せない -> エラー・マーカー保持。`.md` は 0 件でも
    /// tree_open は自動削除せず復旧待ちのまま。再試行で完了する。
    #[cfg(windows)]
    #[test]
    fn permanent_unlock_relock_keeps_marker_when_a_folder_cannot_be_renamed_back() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let key = LockKey::new(&T272_KEY);
        let (_nodes, _before) = t273_locked(&dir, &registry);
        let (payload, planned) = t273_begin(&dir, &registry);
        for e in &payload.image_folders {
            rename_image_folder(&dir, &e.folder, &e.memo_name, &registry).unwrap();
        }
        let fm = unlock_file_map_entry(&payload, "日記").unwrap();
        let from = planned_folder_at_memo_name(&dir, &payload, &planned, "日記").map(|e| e.folder.as_str());
        restore_one_permanent_unlock_entry(&dir, fm, from, &key, &registry).unwrap();

        let handle = std::fs::OpenOptions::new().read(true).share_mode(0).open(dir.join("日記").join("001.png")).unwrap();
        let err = permanent_unlock_relock_files(&dir, &payload, &planned, &key, &registry).unwrap_err();
        drop(handle);
        assert_eq!(err.code, ErrorCode::ImageFolderRenameFailed);
        assert!(tree::scan_real_memo_names(&dir).is_empty());
        assert!(t273_tree_open_pending(&dir, &registry));
        assert!(unlock_marker_path(&dir).exists());

        let marker = load_unlock_marker(&dir).unwrap();
        permanent_unlock_relock_files(&dir, &payload, &marker.image_folders, &key, &registry).unwrap();
        delete_unlock_marker(&dir, &registry).unwrap();
        t272_assert_locked(&dir);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// #271 だけのビルドでロックしたタブ: `<メモ名>/` は変換されず残り、セッション中の貼り付けで
    /// ランダム名フォルダが対応表に載っている。ランダム名フォルダは戻し、同名の旧フォルダがあるメモは
    /// ランダム名のまま。フラット画像は触らない。
    #[test]
    fn permanent_unlock_handles_a_tab_locked_by_a_271_only_build() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let key = LockKey::new(&T272_KEY);
        let r_nikki = "0123456789abcdef";
        let r_space = "fedcba9876543210";
        let r_child = "00112233aabbccdd";
        for r in [r_nikki, r_space, r_child] {
            std::fs::create_dir_all(dir.join(r)).unwrap();
            std::fs::write(dir.join(r).join("010.png"), b"png").unwrap();
        }
        // 「日記の続き」は #271 以前からの `<メモ名>/` も持つ(衝突)
        std::fs::create_dir_all(dir.join("日記の続き")).unwrap();
        std::fs::write(dir.join("日記の続き").join("001.png"), b"old").unwrap();
        std::fs::write(dir.join("img_old.png"), b"flat").unwrap();
        let bodies = [
            ("日記", format!("![]({r_nikki}/010.png)\n![](img_old.png)\n")),
            ("日記の続き", format!("![](日記の続き/001.png)\n![]({r_child}/010.png)\n")),
            ("新 メモ", format!("![](<{r_space}/010.png>)\n")),
        ];
        let mut file_map = Vec::new();
        for (memo, body) in &bodies {
            std::fs::write(dir.join(format!("{memo}.md")), body).unwrap();
            let entry = FileMapEntry { memo_name: memo.to_string(), token: obfuscated_naming::generate_unique_token(&dir) };
            rename_and_encrypt_entry(&dir, &entry, None, &key, &registry).unwrap();
            file_map.push(entry);
        }
        let mut parent = MetaTreeNode::default_for_new_file("日記".to_string());
        parent.children.push(MetaTreeNode::default_for_new_file("日記の続き".to_string()));
        let nodes = vec![parent, MetaTreeNode::default_for_new_file("新 メモ".to_string())];
        let payload = LockPayload {
            real_tab_name: "非公開".to_string(),
            tree: nodes.clone(),
            file_map,
            last_viewed_memo: None,
            image_folders: vec![
                ImageFolderEntry { memo_name: "日記".to_string(), folder: r_nikki.to_string() },
                ImageFolderEntry { memo_name: "新 メモ".to_string(), folder: r_space.to_string() },
                ImageFolderEntry { memo_name: "日記の続き".to_string(), folder: r_child.to_string() },
            ],
        };
        let field = lock_blob::encrypt_lock_field(&key, KdfParams::generate_new(), None, &payload).unwrap();
        write_lock_field(&dir, &field, &registry).unwrap();

        assert_eq!(t273_unlock(&dir, &registry), 1);
        assert_eq!(std::fs::read_to_string(dir.join("日記.md")).unwrap(), "![](日記/010.png)\n![](img_old.png)\n");
        assert_eq!(std::fs::read_to_string(dir.join("新 メモ.md")).unwrap(), "![](<新 メモ/010.png>)\n");
        assert_eq!(std::fs::read_to_string(dir.join("日記の続き.md")).unwrap(), bodies[1].1);
        assert!(dir.join("日記").join("010.png").exists());
        assert!(dir.join("新 メモ").join("010.png").exists());
        assert!(dir.join(r_child).join("010.png").exists());
        assert_eq!(std::fs::read(dir.join("日記の続き").join("001.png")).unwrap(), b"old");
        assert_eq!(std::fs::read(dir.join("img_old.png")).unwrap(), b"flat");
        let parsed = tree::load_tab_meta_with_recovery(&dir).0.unwrap();
        assert!(parsed.lock.is_none());
        assert_eq!(parsed.tree.unwrap(), nodes);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// #273 以前の完全解除マーカー(`imageFolders` 無し)は空の計画として読め、`.md` 0 件なら従来どおり自動削除。
    #[test]
    fn pre_273_unlock_marker_without_image_folders_still_parses() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        std::fs::write(unlock_marker_path(&dir), br#"{"formatVersion":1}"#).unwrap();
        assert!(load_unlock_marker(&dir).unwrap().image_folders.is_empty());
        std::fs::write(dir.join("tok123"), b"encrypted").unwrap();
        assert!(!t273_tree_open_pending(&dir, &registry));
        assert!(!unlock_marker_path(&dir).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn permanent_unlock_result_serializes_flat_camel_case() {
        let value = serde_json::to_value(PermanentUnlockResult {
            tab: TabInfo { id: "t".to_string(), name: "非公開".to_string(), locked: false, lock_label: None },
            image_folders_left_random: 2,
        })
        .unwrap();
        assert_eq!(value["name"], "非公開");
        assert_eq!(value["imageFoldersLeftRandom"], 2);
        assert_eq!(value["lockLabel"], serde_json::Value::Null);
    }

    // ---- GitHub issue #262: タブ間移動/コピーで画像フォルダも運ぶ(ロック中タブが絡む組み合わせ) ----

    const T262_KEY: [u8; 32] = [5u8; 32];
    const T262_PARENT: &str = "日記　その1";
    const T262_CHILD: &str = "子メモ";
    /// ロック中タブ側の既存ランダム名フォルダ(親・子)。
    const T262_FOLDERS: [&str; 2] = ["0123456789abcdef", "fedcba9876543210"];

    /// 画像フォルダ `folder` を参照する本文(山括弧形と生の形)。
    fn t262_body(folder: &str) -> String {
        format!("# メモ\n![](<{folder}/001.png>)\n![図]({folder}/002.png)\n")
    }

    fn t262_images(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("001.png"), b"img1").unwrap();
        std::fs::write(path.join("002.png"), b"img2").unwrap();
    }

    fn t262_assert_images(path: &Path) {
        assert_eq!(std::fs::read(path.join("001.png")).unwrap(), b"img1", "{}", path.display());
        assert_eq!(std::fs::read(path.join("002.png")).unwrap(), b"img2", "{}", path.display());
    }

    fn t262_tree() -> Vec<MetaTreeNode> {
        let mut parent = MetaTreeNode::default_for_new_file(T262_PARENT.to_string());
        parent.children.push(MetaTreeNode::default_for_new_file(T262_CHILD.to_string()));
        vec![parent]
    }

    /// 平文タブ: 親「日記　その1」(子「子メモ」)、両方に `<メモ名>/` 画像フォルダと本文リンク。
    fn t262_plain(dir: &Path, registry: &SelfWriteRegistry) {
        for name in [T262_PARENT, T262_CHILD] {
            std::fs::write(dir.join(format!("{name}.md")), t262_body(name)).unwrap();
            t262_images(&dir.join(name));
        }
        tree::save_tab_meta(dir, TAB_META_SCHEMA_VERSION, "test".to_string(), &t262_tree(), None, registry).unwrap();
    }

    /// ロック中タブのセッションを開始する。`with_memos` なら同じ構成をランダム名フォルダで持つ。
    fn t262_session(dir: &Path, sessions: &LockSessionStore, tab_id: &str, with_memos: bool, registry: &SelfWriteRegistry) {
        let key = LockKey::new(&T262_KEY);
        let (tree, mut file_map, mut image_folders) = (if with_memos { t262_tree() } else { vec![] }, vec![], vec![]);
        if with_memos {
            for (i, name) in [T262_PARENT, T262_CHILD].iter().enumerate() {
                let token = format!("tok{i}");
                std::fs::write(dir.join(&token), lock_blob::encrypt_memo_body(&key, t262_body(T262_FOLDERS[i]).as_bytes())).unwrap();
                t262_images(&dir.join(T262_FOLDERS[i]));
                file_map.push(FileMapEntry { memo_name: name.to_string(), token });
                image_folders.push(ImageFolderEntry { memo_name: name.to_string(), folder: T262_FOLDERS[i].to_string() });
            }
        }
        sessions.start(
            tab_id.to_string(),
            LockSession {
                real_tab_name: "非公開".to_string(),
                tree,
                file_map,
                kdf: KdfParams::generate_new(),
                hint: None,
                key: T262_KEY,
                last_viewed_memo: None,
                image_folders,
            },
        );
        sessions.with_session(tab_id, |s| save_lock_blob(dir, s, &s.tree, &s.file_map, None, &s.image_folders, registry)).unwrap().unwrap();
    }

    fn t262_session_folder(sessions: &LockSessionStore, tab_id: &str, memo: &str) -> Option<String> {
        sessions.with_session(tab_id, |s| image_folder_for(&s.image_folders, memo).map(str::to_string)).unwrap()
    }

    fn t262_session_body(dir: &Path, sessions: &LockSessionStore, tab_id: &str, memo: &str) -> String {
        let token = sessions.with_session(tab_id, |s| s.file_map.iter().find(|e| e.memo_name == memo).unwrap().token.clone()).unwrap();
        String::from_utf8(lock_blob::decrypt_memo_body(&LockKey::new(&T262_KEY), &std::fs::read(dir.join(token)).unwrap()).unwrap()).unwrap()
    }

    fn t262_persisted_map(dir: &Path) -> Vec<ImageFolderEntry> {
        let (parsed, _) = tree::load_tab_meta_with_recovery(dir);
        let field: LockField = serde_json::from_value(parsed.unwrap().lock.unwrap()).unwrap();
        lock_blob::decrypt_lock_field(&LockKey::new(&T262_KEY), &field).unwrap().image_folders
    }

    /// 移動先ロック中タブで、各メモが新しいランダム名フォルダ・対応表(メモリ/ディスク)・書き換え済み本文を持つ。
    fn t262_assert_carried_into_session(dir: &Path, sessions: &LockSessionStore, tab_id: &str) {
        let persisted = t262_persisted_map(dir);
        for name in [T262_PARENT, T262_CHILD] {
            let folder = t262_session_folder(sessions, tab_id, name).expect("map entry");
            assert_eq!(folder.len(), 16);
            assert!(folder.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(!T262_FOLDERS.contains(&folder.as_str()), "a fresh random name must be assigned");
            t262_assert_images(&dir.join(&folder));
            assert_eq!(t262_session_body(dir, sessions, tab_id, name), t262_body(&folder));
            assert!(persisted.iter().any(|e| e.memo_name == name && e.folder == folder));
        }
    }

    #[test]
    fn plain_to_locked_move_and_copy_carry_image_folders() {
        for copy in [false, true] {
            let (src, dst) = (tempdir(), tempdir());
            let registry = SelfWriteRegistry::new();
            let sessions = LockSessionStore::new();
            t262_plain(&src, &registry);
            t262_session(&dst, &sessions, "tab-2", false, &registry);
            let result = move_cross_tab_plain_to_session("tab-1", &src, "元", "tab-2", &dst, &[T262_PARENT.to_string()], None, 0, copy, &registry, &sessions, &SearchIndex::new());
            assert!(matches!(result, AppResult::Ok(_)), "copy={copy}");
            t262_assert_carried_into_session(&dst, &sessions, "tab-2");
            for name in [T262_PARENT, T262_CHILD] {
                assert!(!dst.join(name).exists(), "no memo-named folder in the locked tab");
                assert_eq!(src.join(name).exists(), copy, "copy={copy}");
                assert_eq!(src.join(format!("{name}.md")).exists(), copy);
                if copy {
                    t262_assert_images(&src.join(name));
                    assert_eq!(std::fs::read_to_string(src.join(format!("{name}.md"))).unwrap(), t262_body(name));
                }
            }
            std::fs::remove_dir_all(&src).ok();
            std::fs::remove_dir_all(&dst).ok();
        }
    }

    #[test]
    fn locked_to_plain_move_and_copy_carry_image_folders() {
        for copy in [false, true] {
            let (src, dst) = (tempdir(), tempdir());
            let registry = SelfWriteRegistry::new();
            let sessions = LockSessionStore::new();
            t262_session(&src, &sessions, "tab-1", true, &registry);
            let result = move_cross_tab_session_to_plain("tab-1", &src, "tab-2", &dst, "先", &[T262_PARENT.to_string()], None, 0, copy, &registry, &sessions, &SearchIndex::new());
            assert!(matches!(result, AppResult::Ok(_)), "copy={copy}");
            for (i, name) in [T262_PARENT, T262_CHILD].iter().enumerate() {
                t262_assert_images(&dst.join(name));
                assert_eq!(std::fs::read_to_string(dst.join(format!("{name}.md"))).unwrap(), t262_body(name));
                assert_eq!(src.join(T262_FOLDERS[i]).exists(), copy, "copy={copy}");
                assert_eq!(t262_session_folder(&sessions, "tab-1", name).is_some(), copy, "move drops the source map entry");
                assert_eq!(t262_persisted_map(&src).iter().any(|e| &e.memo_name == name), copy);
                if copy {
                    t262_assert_images(&src.join(T262_FOLDERS[i]));
                    assert_eq!(t262_session_body(&src, &sessions, "tab-1", name), t262_body(T262_FOLDERS[i]));
                }
            }
            std::fs::remove_dir_all(&src).ok();
            std::fs::remove_dir_all(&dst).ok();
        }
    }

    #[test]
    fn locked_to_locked_move_and_copy_carry_image_folders() {
        for copy in [false, true] {
            let (src, dst) = (tempdir(), tempdir());
            let registry = SelfWriteRegistry::new();
            let sessions = LockSessionStore::new();
            t262_session(&src, &sessions, "tab-1", true, &registry);
            t262_session(&dst, &sessions, "tab-2", false, &registry);
            let result = move_cross_tab_session_to_session("tab-1", &src, "tab-2", &dst, &[T262_PARENT.to_string()], None, 0, copy, &registry, &sessions);
            assert!(matches!(result, AppResult::Ok(_)), "copy={copy}");
            t262_assert_carried_into_session(&dst, &sessions, "tab-2");
            for (i, name) in [T262_PARENT, T262_CHILD].iter().enumerate() {
                assert_eq!(src.join(T262_FOLDERS[i]).exists(), copy, "copy={copy}");
                assert_eq!(t262_session_folder(&sessions, "tab-1", name).is_some(), copy);
            }
            std::fs::remove_dir_all(&src).ok();
            std::fs::remove_dir_all(&dst).ok();
        }
    }

    /// 平文の移動先に `<メモ名>/` が既にあれば、何も触らずに拒否する(子メモ側の衝突でも親のフォルダは動かない)。
    #[test]
    fn locked_to_plain_rejects_an_image_folder_collision_without_partial_changes() {
        for copy in [false, true] {
            let (src, dst) = (tempdir(), tempdir());
            let registry = SelfWriteRegistry::new();
            let sessions = LockSessionStore::new();
            t262_session(&src, &sessions, "tab-1", true, &registry);
            std::fs::create_dir_all(dst.join(T262_CHILD)).unwrap();
            let before = sessions.with_session("tab-1", |s| (s.tree.clone(), s.file_map.clone(), s.image_folders.clone())).unwrap();
            let result = move_cross_tab_session_to_plain("tab-1", &src, "tab-2", &dst, "先", &[T262_PARENT.to_string()], None, 0, copy, &registry, &sessions, &SearchIndex::new());
            let AppResult::Err(err) = result else { panic!("expected rejection") };
            assert_eq!(err.code, ErrorCode::NameDuplicate);
            assert_eq!(sessions.with_session("tab-1", |s| (s.tree.clone(), s.file_map.clone(), s.image_folders.clone())).unwrap(), before);
            for (i, name) in [T262_PARENT, T262_CHILD].iter().enumerate() {
                t262_assert_images(&src.join(T262_FOLDERS[i]));
                assert!(!dst.join(format!("{name}.md")).exists());
            }
            assert!(!dst.join(T262_PARENT).exists());
            assert_eq!(std::fs::read_dir(dst.join(T262_CHILD)).unwrap().count(), 0);
            std::fs::remove_dir_all(&src).ok();
            std::fs::remove_dir_all(&dst).ok();
        }
    }
}
