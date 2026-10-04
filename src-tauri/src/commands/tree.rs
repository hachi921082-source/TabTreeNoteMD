//! Tree / memo domain commands — `docs/plans/memo-tree/PLAN.md`.
//!
//! Every command below that mutates a tab's tree follows the same skeleton
//! (mirrors `[detailed-spec §6.3]`'s sync order): resolve the tab (rejecting
//! locked tabs, since `tab-lock/PLAN.md` is not yet implemented and there is
//! therefore never an active session), load + reconcile the tree fresh from
//! disk, validate, perform the real file operation with self-write
//! registration, update the in-memory tree, then atomically save the
//! metadata JSON. Because every mutating command re-reconciles from disk on
//! entry and unconditionally re-saves on exit, `tree_open`'s own "reconciled
//! but not force-written" correction (`memo-tree/PLAN.md` §2 step 5) is
//! naturally picked up by whichever mutating command runs next, with no
//! separate dirty-tracking store needed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::commands::tab_lock;
use crate::commands::{emit_meta_recovered, emit_settings_recovery_event};
use crate::fs::atomic_write::{atomic_write, bak_path};
use crate::fs::image_folder_transfer::{transfer_all, undo_transfers, FolderTransfer};
use crate::fs::path_safety::{direct_child_file_path, existing_direct_child_file, existing_memo_image_folder, memo_image_folder_path, validate_name, ValidationMode};
use crate::fs::retry::{is_lock_class_error, with_retry};
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::fs::trash;
use crate::model::app_settings::AppSettings;
use crate::model::result::{AppErrorPayload, AppResult, ErrorCode};
use crate::util::unique_name::resolve_unique_name;
use crate::model::tab_meta::{TabMetaFile, TAB_META_FILE_NAME, TAB_META_SCHEMA_VERSION};
use crate::model::tree::{Label, LabelColor, MemoEditorSettings, MetaTreeNode, NodePath};
use crate::search::index::SearchIndex;
use crate::settings::app_settings_store::{AppSettingsStore, RecoveryOutcome};
use crate::store::lock_session_store::LockSessionStore;
use crate::store::tab_store::TabViewStore;

// ---------------------------------------------------------------------------
// Command I/O types
// ---------------------------------------------------------------------------

/// Return type of `tree_open` (`docs/plans/memo-tree/PLAN.md` §2).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeSnapshot {
    pub nodes: Vec<MetaTreeNode>,
    pub tab_name: String,
    pub locked: bool,
    /// `true` when a crash-mid-lock or crash-mid-permanent-unlock marker was found
    /// (`tab-lock/CRASH-RECOVERY.md`, `tab-lock-permanent-unlock/CRASH-RECOVERY.md`) — the
    /// frontend should show the recovery dialog identified by `recovery_kind` instead of the
    /// normal tree view.
    pub recovery_pending: bool,
    /// Which recovery dialog to show when `recovery_pending` is `true`. Always present when
    /// `recovery_pending` is `true`, `None` otherwise (`tab-lock-permanent-unlock/CRASH-RECOVERY.md` §3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_kind: Option<RecoveryKind>,
    /// `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §2 — the tab's raw persisted (plain tabs) or
    /// active-session (locked-with-session tabs) `lastViewedMemo`, unreconciled: if it names a
    /// memo `[detailed-spec §3.5]` reconciliation dropped, this is returned unchanged (the raw
    /// value), and reconciling *that* down to `None` is the frontend `restoreTabView`'s job, not
    /// this command's. `None` for a recovery-pending or locked-no-session snapshot (nothing to
    /// restore from either).
    pub last_viewed_memo: Option<String>,
}

/// GitHub issue #263: non-fatal problem with a memo's per-memo image folder (#139) that did NOT
/// block the primary `.md` operation — surfaced to the user as a toast, never rolled back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoMutationWarning {
    /// `memo_rename`: the `.md` was renamed but its image folder could not follow (e.g. a folder
    /// with the new name already exists) — images remain under the old name.
    ImageFolderRenameFailed,
    /// `memo_delete` (force-direct path): the `.md` was deleted but its image folder could not be.
    ImageFolderDeleteFailed,
}

/// Return type of `memo_rename`/`memo_delete`: the usual `TreeSnapshot` (flattened — same wire
/// shape as before) plus an optional non-fatal `warning` (GitHub issue #263).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoMutationResult {
    #[serde(flatten)]
    pub snapshot: TreeSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<MemoMutationWarning>,
}

/// 警告なしの `MemoMutationResult` へ変換(ロック中タブのセッション経路用)。
fn without_warning(result: AppResult<TreeSnapshot>) -> AppResult<MemoMutationResult> {
    match result {
        AppResult::Ok(snapshot) => AppResult::ok(MemoMutationResult { snapshot, warning: None }),
        AppResult::Err(e) => AppResult::err(e),
    }
}

/// Which crash-recovery flow `recovery_pending: true` refers to
/// (`docs/plans/tab-lock-permanent-unlock/CRASH-RECOVERY.md` §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryKind {
    Lock,
    PermanentUnlock,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoProperties {
    pub file_name: String,
    pub file_path: String,
    pub file_size_bytes: u64,
    /// Raw epoch-ms timestamps (not pre-formatted) — the frontend formats these locally,
    /// including weekday + relative "n日前", matching the old app's `formatDateTime`
    /// (`src/shared/formatDateTime.ts`), which requires local-timezone formatting rather than
    /// the backend's previous UTC-string approach.
    pub created_ms: u64,
    pub modified_ms: u64,
    pub char_count: usize,
    pub char_count_excluding_whitespace: usize,
    pub line_count: usize,
}

/// `docs/plans/memo-tree/PLAN.md` §3 `memo_move`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MoveTarget {
    #[serde(rename_all = "camelCase")]
    WithinTab {
        new_parent: Option<NodePath>,
        index: usize,
    },
    #[serde(rename_all = "camelCase")]
    OtherTab {
        target_tab_id: String,
        new_parent: Option<NodePath>,
        index: usize,
        copy: bool,
    },
}

/// `memo_move`'s return shape: the acting tab's own updated snapshot, plus (for the
/// cross-tab case) the other tab's updated snapshot. `target` is `None` for `WithinTab`
/// moves, where only one tab is ever affected.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoMoveResult {
    pub source: TreeSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<TreeSnapshot>,
}

// ---------------------------------------------------------------------------
// tree_open
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn tree_open(
    app: AppHandle,
    tab_id: String,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
    search_index: State<SearchIndex>,
    view_store: State<TabViewStore>,
) -> AppResult<TreeSnapshot> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    let folder = PathBuf::from(&entry.folder_path);
    let mut tab_name = entry.name.clone();
    let mut cached_locked = entry.locked.unwrap_or(false);
    drop(settings);

    // Step 0a (`tab-lock/CRASH-RECOVERY.md`'s "Trigger Point"): crash-mid-lock marker check,
    // before anything else. A zero-progress marker is silently auto-rolled-back inline; anything
    // else reports `recovery_pending: true` so the frontend shows the recovery dialog instead of
    // this tab's normal tree/empty-state view.
    if matches!(tab_lock::check_marker_on_tree_open(&folder, &registry), tab_lock::MarkerCheck::RecoveryPending) {
        return AppResult::ok(TreeSnapshot {
            nodes: vec![],
            tab_name,
            locked: false,
            recovery_pending: true,
            recovery_kind: Some(RecoveryKind::Lock),
            last_viewed_memo: None,
        });
    }

    // Step 0b (`tab-lock-permanent-unlock/CRASH-RECOVERY.md` §3): a second, independent marker
    // check for a crash-mid-permanent-unlock. Important: this call's `NoMarker` return may have
    // already mutated `AppSettings`/`LockSessionStore`/`SearchIndex` as a side effect (the "Case
    // B" auto-finish branch, `CRASH-RECOVERY.md` §3.3) — unlike `check_marker_on_tree_open`
    // above, which only ever deletes a file. `AppSettings` is therefore re-fetched immediately
    // below rather than reusing the `tab_name`/`cached_locked` locals captured above, which may
    // now be stale.
    if matches!(
        tab_lock::check_unlock_marker_on_tree_open(&folder, &registry, &store, &sessions, &search_index, &tab_id),
        tab_lock::UnlockMarkerCheck::RecoveryPending
    ) {
        return AppResult::ok(TreeSnapshot {
            nodes: vec![],
            tab_name,
            locked: cached_locked,
            recovery_pending: true,
            recovery_kind: Some(RecoveryKind::PermanentUnlock),
            last_viewed_memo: None,
        });
    }

    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    tab_name = entry.name.clone();
    cached_locked = entry.locked.unwrap_or(false);
    drop(settings);

    let (parsed, recovery_outcome) = load_tab_meta_with_recovery(&folder);
    let really_locked = parsed.as_ref().map(|m| m.lock.is_some()).unwrap_or(false);

    // `tab-management/PLAN.md` §3: this command is where the
    // `AppSettings.tabs[i].locked` display cache gets corrected if it
    // disagrees with the tab's own metadata file (e.g. after an external
    // folder copy swapped in a differently-locked `.tabtreenote.json`).
    if really_locked != cached_locked {
        let _ = store.modify(&registry, |s| -> Result<(), ()> {
            if let Some(t) = s.tabs.iter_mut().find(|t| t.id == tab_id) {
                t.locked = Some(really_locked);
                if really_locked {
                    // Mirrors `tab_lock`'s own step 10 placeholder swap (`tab-lock/PLAN.md` §3):
                    // this branch is reached whenever a tab is discovered to already be locked
                    // despite the cached `AppSettings` entry saying otherwise (e.g. `tab-lock/
                    // PLAN.md` §7's portability case, when `tab_register_existing`'s own
                    // detection — `commands/tab.rs`'s `register_tab_at_path` — was bypassed or
                    // predates this fix) — the real name must not stay displayed here either.
                    t.name = tab_lock::LOCKED_PLACEHOLDER_NAME.to_string();
                }
            }
            Ok(())
        });
        if really_locked {
            tab_name = tab_lock::LOCKED_PLACEHOLDER_NAME.to_string();
        }
    }

    if really_locked {
        // Step 1, session branch (`tab-lock/PLAN.md` §3 `tab_unlock` step 7): an active unlock
        // session supplies the real tree/name via this same command.
        if let Some(snapshot) = sessions.with_session(&tab_id, tab_lock::session_tree_snapshot) {
            // `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §3: refresh the runtime cache on every
            // read, not just the write path — picks up a session started elsewhere too.
            view_store.set(&tab_id, snapshot.last_viewed_memo.clone());
            return AppResult::ok(snapshot);
        }
        // Step 1, no-session branch: nothing is rendered for a locked-and-not-yet-unlocked tab.
        return AppResult::ok(TreeSnapshot {
            nodes: vec![],
            tab_name,
            locked: true,
            recovery_pending: false,
            recovery_kind: None,
            last_viewed_memo: None,
        });
    }

    // Captured before `parsed` is partially moved-from by `.and_then(|m| m.tree)` below.
    let last_viewed_memo = parsed.as_ref().and_then(|m| m.last_viewed_memo.clone());
    let persisted_tree = parsed.and_then(|m| m.tree).unwrap_or_default();
    let (reconciled, _changed) = reconcile_against_disk(&folder, persisted_tree);

    // `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §3: populate the runtime cache on every read —
    // the raw persisted value, not reconciled against `reconciled` (that reconciliation-drop check
    // is `restoreTabView`'s job, per this snapshot's own `last_viewed_memo` doc comment above).
    // Done before `tab_id` is potentially moved into `emit_meta_recovered` below.
    view_store.set(&tab_id, last_viewed_memo.clone());

    if recovery_outcome != RecoveryOutcome::Clean {
        emit_meta_recovered(&app, "tab", Some(tab_id));
    }

    AppResult::ok(TreeSnapshot {
        nodes: reconciled,
        tab_name,
        locked: false,
        recovery_pending: false,
        recovery_kind: None,
        last_viewed_memo,
    })
}

// ---------------------------------------------------------------------------
// memo_create / memo_create_child
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn memo_create(
    app: AppHandle,
    tab_id: String,
    name: String,
    parent: Option<NodePath>,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
    search_index: State<SearchIndex>,
) -> AppResult<TreeSnapshot> {
    memo_create_impl(&app, &tab_id, &name, parent.as_deref(), &store, &registry, &sessions, &search_index)
}

/// `parent` is required (not `Option`) here, matching the right-click "Create as Child" entry
/// point which only appears when a node is selected (`memo-tree/PLAN.md` §3).
#[tauri::command]
pub fn memo_create_child(
    app: AppHandle,
    tab_id: String,
    name: String,
    parent: NodePath,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
    search_index: State<SearchIndex>,
) -> AppResult<TreeSnapshot> {
    memo_create_impl(&app, &tab_id, &name, Some(parent.as_str()), &store, &registry, &sessions, &search_index)
}

#[allow(clippy::too_many_arguments)]
fn memo_create_impl(
    app: &AppHandle,
    tab_id: &str,
    name: &str,
    parent: Option<&str>,
    store: &AppSettingsStore,
    registry: &SelfWriteRegistry,
    sessions: &LockSessionStore,
    search_index: &SearchIndex,
) -> AppResult<TreeSnapshot> {
    let (settings, outcome) = store.get(registry);
    emit_settings_recovery_event(app, outcome);

    let trimmed = name.trim();
    if let Err(code) = validate_name(trimmed, ValidationMode::Strict) {
        return AppResult::err_code(code, "invalid memo name");
    }

    let access = match tab_lock::resolve_tab_access(&settings, sessions, tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };
    let (folder, tab_name) = match access {
        tab_lock::TabAccess::Session { tab_id, folder } => {
            return tab_lock::session_create_memo(sessions, registry, &folder, &tab_id, trimmed, parent);
        }
        tab_lock::TabAccess::Plain { folder, tab_name } => (folder, tab_name),
    };

    let mut loaded = load_and_reconcile(&folder);
    if let Some(p) = parent {
        if find_node(&loaded.tree, p).is_none() {
            return AppResult::err_code(ErrorCode::NotFound, "parent memo not found");
        }
    }
    // GitHub issue #97: creation auto-numbers past a duplicate name (`trimmed`, `trimmed(1)`, ...)
    // instead of blocking — rename operations are unaffected (`memo_rename` below still blocks).
    let resolved = resolve_unique_name(trimmed, |candidate| contains_name(&loaded.tree, candidate));

    let file_path = folder.join(format!("{resolved}.md"));
    if let Err(err) = atomic_write(&file_path, b"", registry) {
        tracing::error!(error = %err, path = %file_path.display(), "memo_create: failed to create memo file");
        return AppResult::err_code(ErrorCode::Unknown, "failed to create memo file");
    }

    let node = MetaTreeNode::default_for_new_file(resolved.clone());
    if let Err(code) = insert_node_at_end(&mut loaded.tree, parent, node) {
        return AppResult::err_code(code, "failed to insert memo into tree");
    }

    if let Err(err) = save_tab_meta(&folder, loaded.schema_version, loaded.app_version, &loaded.tree, loaded.last_viewed_memo.clone(), registry) {
        tracing::error!(error = %err, "memo_create: failed to save tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
    }

    // A brand-new file has no content yet, so this legitimately upserts an empty body
    // (`search-index/PLAN.md` §1's in-app update path).
    search_index.upsert(tab_id, &resolved, "");

    AppResult::ok(TreeSnapshot {
        nodes: loaded.tree,
        tab_name,
        locked: false,
        recovery_pending: false,
        recovery_kind: None,
        last_viewed_memo: loaded.last_viewed_memo,
    })
}

// ---------------------------------------------------------------------------
// memo_rename
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn memo_rename(
    app: AppHandle,
    tab_id: String,
    path: NodePath,
    new_name: String,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
    search_index: State<SearchIndex>,
) -> AppResult<MemoMutationResult> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    let trimmed = new_name.trim();
    if let Err(code) = validate_name(trimmed, ValidationMode::Strict) {
        return AppResult::err_code(code, "invalid memo name");
    }

    let access = match tab_lock::resolve_tab_access(&settings, &sessions, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };
    let (folder, tab_name) = match access {
        tab_lock::TabAccess::Session { tab_id, folder } => {
            return without_warning(tab_lock::session_rename_memo(&sessions, &registry, &folder, &tab_id, &path, trimmed));
        }
        tab_lock::TabAccess::Plain { folder, tab_name } => (folder, tab_name),
    };

    let mut loaded = load_and_reconcile(&folder);
    let Some(existing) = find_node(&loaded.tree, &path) else {
        return AppResult::err_code(ErrorCode::NotFound, "memo not found");
    };
    // GitHub issue #156: an edit-locked memo must not be renamed either, not just have its body
    // protected from edits.
    if existing.locked == Some(true) {
        return AppResult::err_code(ErrorCode::MemoEditLocked, "memo is edit-locked");
    }
    if !path.eq_ignore_ascii_case(trimmed) && contains_name(&loaded.tree, trimmed) {
        return AppResult::err_code(ErrorCode::NameDuplicate, "a memo with this name already exists in this tab");
    }

    let old_path = folder.join(format!("{path}.md"));
    let new_path = folder.join(format!("{trimmed}.md"));
    registry.register(&old_path);
    registry.register(&new_path);
    if let Err(err) = with_retry(|| std::fs::rename(&old_path, &new_path)) {
        tracing::error!(error = %err, "memo_rename: failed to rename memo file");
        let code = if is_lock_class_error(&err) { ErrorCode::IoLocked } else { ErrorCode::Unknown };
        return AppResult::err_code(code, "failed to rename memo file");
    }

    // GitHub issue #282: `<旧名>.md.bak` も新名へ運ぶ(ベストエフォート)
    carry_memo_bak(&folder, &path, &folder, trimmed, &registry);

    let warning = follow_rename_image_folder(&folder, &path, trimmed, &new_path, &registry);

    if let Some(node) = find_node_mut(&mut loaded.tree, &path) {
        node.memo_name = trimmed.to_string();
    }

    if let Err(err) = save_tab_meta(&folder, loaded.schema_version, loaded.app_version, &loaded.tree, loaded.last_viewed_memo.clone(), &registry) {
        tracing::error!(error = %err, "memo_rename: failed to save tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
    }

    // `[spec §3.1]`: any `memo:` link pointing at the old name now breaks —
    // known/accepted consequence, no automatic rewrite (out of scope).
    search_index.remove(&tab_id, &path);
    search_index.upsert(&tab_id, trimmed, &read_body_for_index(&folder, trimmed));

    AppResult::ok(MemoMutationResult {
        snapshot: TreeSnapshot {
            nodes: loaded.tree,
            tab_name,
            locked: false,
            recovery_pending: false,
            recovery_kind: None,
            last_viewed_memo: loaded.last_viewed_memo,
        },
        warning,
    })
}

/// GitHub issue #139: follow-renames the memo's per-memo image subfolder, if it has one — a memo
/// with no attachments has no such folder, so this is skipped rather than erroring. Best-effort:
/// the `.md` rename already succeeded, so a failure here (e.g. an orphaned folder already
/// occupying the new name) does not roll it back; it is logged and returned as a non-fatal
/// warning so the frontend can tell the user (GitHub issue #263).
fn follow_rename_image_folder(folder: &Path, old_name: &str, new_name: &str, new_memo_path: &Path, registry: &SelfWriteRegistry) -> Option<MemoMutationWarning> {
    // 旧名側は実在する実ディレクトリ(リンク以外)かつタブ直下の安全な名前のみ対象(`fs::path_safety`)。
    let Some(old_image_folder) = existing_memo_image_folder(folder, old_name) else {
        return None;
    };
    let Some(new_image_folder) = memo_image_folder_path(folder, new_name) else {
        tracing::warn!("memo_rename: new name is not usable as an image folder name; images remain under the old name");
        return Some(MemoMutationWarning::ImageFolderRenameFailed);
    };
    // 移動先が既に存在する場合は明示的に失敗扱い(Windowsでは空フォルダへのrenameが成功しうるため、
    // 既存フォルダへの上書き・統合は行わない)。リンク切れのリンクも存在扱い。大文字小文字のみの変更は
    // 同一フォルダなので対象外。
    if std::fs::symlink_metadata(&new_image_folder).is_ok() && !old_name.eq_ignore_ascii_case(new_name) {
        tracing::warn!(path = %new_image_folder.display(), "memo_rename: image folder destination already exists; images remain under the old name");
        return Some(MemoMutationWarning::ImageFolderRenameFailed);
    }
    registry.register(&old_image_folder);
    registry.register(&new_image_folder);
    match with_retry(|| std::fs::rename(&old_image_folder, &new_image_folder)) {
        Err(err) => {
            tracing::warn!(error = %err, "memo_rename: failed to rename memo's image folder; images may now be orphaned under the old name");
            Some(MemoMutationWarning::ImageFolderRenameFailed)
        }
        // GitHub issue #261: フォルダ名変更が成功した場合のみ、本文の画像リンクも追従させる
        // (後続の検索インデックス更新は書き換え後の本文を読み直す)。
        Ok(()) => {
            rewrite_renamed_memo_image_links(new_memo_path, old_name, new_name, registry);
            None
        }
    }
}

/// GitHub issue #261: rewrites `![..](<old>/..)` image links in the just-renamed memo's own body
/// to `<new>/..` (`util::image_link_rewrite`). Best-effort like the image-folder rename it
/// follows: the rename itself already succeeded, so any failure is logged, not returned. Only
/// UTF-8 bodies are rewritten (BOM and EOL bytes preserved verbatim); a non-UTF-8 (e.g.
/// Shift_JIS) body is skipped rather than silently re-encoded.
fn rewrite_renamed_memo_image_links(memo_path: &Path, old_name: &str, new_name: &str, registry: &SelfWriteRegistry) {
    let bytes = match std::fs::read(memo_path) {
        Ok(b) => b,
        Err(err) => {
            tracing::warn!(error = %err, "memo_rename: failed to read memo for image-link rewrite");
            return;
        }
    };
    const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
    let (bom, rest) = if bytes.starts_with(BOM) { bytes.split_at(BOM.len()) } else { (&[][..], &bytes[..]) };
    let Ok(body) = std::str::from_utf8(rest) else {
        tracing::warn!("memo_rename: memo body is not UTF-8; image links not rewritten");
        return;
    };
    let Some(rewritten) = crate::util::image_link_rewrite::rewrite_image_links_for_rename(body, old_name, new_name) else {
        return;
    };
    let mut out = Vec::with_capacity(bom.len() + rewritten.len());
    out.extend_from_slice(bom);
    out.extend_from_slice(rewritten.as_bytes());
    registry.register(memo_path);
    if let Err(err) = atomic_write(memo_path, &out, registry) {
        tracing::warn!(error = %err, "memo_rename: failed to write rewritten image links");
    }
}

/// GitHub issue #282: `atomic_write` の 1 世代バックアップ `<src_name>.md.bak`(`src_folder` 直下)を、
/// 改名/タブ間移動したメモに合わせて `<dst_folder>/<dst_name>.md.bak` へ運ぶ。ベストエフォート:
/// `.md` の操作は成功済みなので、ここでの失敗はログのみで戻さない。
/// - 移動元の `.bak` が無い(または直下の通常ファイルでない)なら何もしない
/// - 移動先の名前がパスガードを通らない、または移動先に既に何かあるなら上書きせず、移動元の
///   `.bak` を削除する(ただのバックアップなので、旧名で残すより消す)
/// - 同一フォルダで大文字小文字だけの改名は同じファイルなので、存在チェックをせずそのまま改名
/// - 改名に失敗したら(別ボリューム等)コピー + 削除で運ぶ
pub(crate) fn carry_memo_bak(src_folder: &Path, src_name: &str, dst_folder: &Path, dst_name: &str, registry: &SelfWriteRegistry) {
    let Some(src) = existing_direct_child_file(src_folder, &format!("{src_name}.md.bak")) else {
        return;
    };
    let Some(dst) = direct_child_file_path(dst_folder, &format!("{dst_name}.md.bak")) else {
        tracing::warn!("memo .bak: destination name is not usable; removing the source backup instead");
        remove_bak_file(&src, registry);
        return;
    };
    let same_file = src_folder == dst_folder && src_name.eq_ignore_ascii_case(dst_name);
    if !same_file && std::fs::symlink_metadata(&dst).is_ok() {
        tracing::warn!(path = %dst.display(), "memo .bak: destination backup already exists; removing the source backup instead");
        remove_bak_file(&src, registry);
        return;
    }
    registry.register(&src);
    registry.register(&dst);
    let result = with_retry(|| std::fs::rename(&src, &dst)).or_else(|_| {
        std::fs::copy(&src, &dst)?;
        std::fs::remove_file(&src)
    });
    if let Err(err) = result {
        tracing::warn!(error = %err, "memo .bak: failed to carry the backup file along (best-effort, not fatal)");
    }
}

/// GitHub issue #282: `<name>.md.bak`(`folder` 直下の通常ファイルのみ)をベストエフォートで完全削除する。
/// ロック中タブが絡むタブ間移動で、移動元の平文バックアップを運ばずに消す用。
pub(crate) fn remove_memo_bak(folder: &Path, name: &str, registry: &SelfWriteRegistry) {
    if let Some(path) = existing_direct_child_file(folder, &format!("{name}.md.bak")) {
        remove_bak_file(&path, registry);
    }
}

/// `.bak` の完全削除(失敗はログのみ)。
pub(crate) fn remove_bak_file(path: &Path, registry: &SelfWriteRegistry) {
    registry.register(path);
    if let Err(err) = with_retry(|| std::fs::remove_file(path)) {
        tracing::warn!(error = %err, path = %path.display(), "memo .bak: failed to remove a backup file (best-effort, not fatal)");
    }
}

// ---------------------------------------------------------------------------
// memo_delete
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn memo_delete(
    app: AppHandle,
    tab_id: String,
    paths: Vec<NodePath>,
    force_direct: bool,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
    search_index: State<SearchIndex>,
) -> AppResult<MemoMutationResult> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    let access = match tab_lock::resolve_tab_access(&settings, &sessions, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };
    let (folder, tab_name) = match access {
        // A locked-session memo's obfuscated file is always permanently deleted (`tab_lock.rs`'s
        // `session_delete_memo` doc comment); `force_direct` only selects trash vs. direct delete
        // for its random-named image folder (GitHub issue #271, same split as #263).
        tab_lock::TabAccess::Session { tab_id, folder } => {
            return tab_lock::session_delete_memo(&sessions, &registry, &folder, &tab_id, &paths, force_direct);
        }
        tab_lock::TabAccess::Plain { folder, tab_name } => (folder, tab_name),
    };

    let mut loaded = load_and_reconcile(&folder);

    // Full subtree of every requested top-level path (`[spec §2.2]`:
    // deleting a node deletes its entire subtree), de-duplicated so a
    // multi-select containing both a node and its own descendant doesn't
    // attempt to process the descendant twice.
    let mut all_names: Vec<String> = Vec::new();
    for p in &paths {
        if let Some(node) = find_node(&loaded.tree, p) {
            let mut names = Vec::new();
            collect_names(node, &mut names);
            for n in names {
                if !all_names.contains(&n) {
                    all_names.push(n);
                }
            }
        }
    }
    if all_names.is_empty() {
        return AppResult::err_code(ErrorCode::NotFound, "no matching memo to delete");
    }
    // GitHub issue #156: reject the whole batch if edit-locked memo is anywhere in scope — either
    // directly selected or a descendant of a selected node (deleting a node deletes its entire
    // subtree, so a locked descendant would otherwise be silently destroyed too). All-or-nothing
    // rather than silently excluding just the locked ones, so a multi-select delete never produces
    // a partial, surprising result.
    if all_names.iter().any(|n| find_node(&loaded.tree, n).and_then(|node| node.locked) == Some(true)) {
        return AppResult::err_code(ErrorCode::MemoEditLocked, "one or more memos in this delete are edit-locked");
    }

    // Sync order (`[detailed-spec §6.3]`): file operation first. On any
    // failure below, the tree/metadata are left untouched — any files
    // already deleted in this loop are self-healing via the next
    // `tree_open` reconciliation (a metadata entry with no matching file is
    // simply dropped, `[detailed-spec §3.5]`), and this call is safe to
    // retry (already-gone files are skipped, not re-attempted).
    let warning = match delete_memo_files(&folder, &all_names, force_direct, &registry) {
        Ok(w) => w,
        Err(code) => return AppResult::err_code(code, "failed to delete one or more memo files"),
    };

    for name in &all_names {
        remove_node(&mut loaded.tree, name);
        search_index.remove(&tab_id, name);
    }

    if let Err(err) = save_tab_meta(&folder, loaded.schema_version, loaded.app_version, &loaded.tree, loaded.last_viewed_memo.clone(), &registry) {
        tracing::error!(error = %err, "memo_delete: failed to save tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
    }

    AppResult::ok(MemoMutationResult {
        snapshot: TreeSnapshot {
            nodes: loaded.tree,
            tab_name,
            locked: false,
            recovery_pending: false,
            recovery_kind: None,
            last_viewed_memo: loaded.last_viewed_memo,
        },
        warning,
    })
}

/// File-operation half of `memo_delete`: for each memo, `<name>.md` first (primary action), then
/// its per-memo image folder `<name>/` (GitHub issue #263), both through the same trash /
/// force-direct split.
///
/// Partial-failure policy:
/// - Trash path: an image-folder trash failure returns `TrashFailed` exactly like a `.md` trash
///   failure, so the existing consent dialog -> force-direct retry flow (`[spec §5.8]`) covers
///   it. Already-trashed `.md` files are not rolled back; the retry skips them (absent) and
///   direct-deletes the remaining folder.
/// - Force-direct path: there is no further fallback, so an image-folder delete failure is
///   logged and returned as a non-fatal warning rather than leaving the tree out of sync with an
///   already-deleted `.md`. A `.md` failure is still fatal, as before.
fn delete_memo_files(folder: &Path, names: &[String], force_direct: bool, registry: &SelfWriteRegistry) -> Result<Option<MemoMutationWarning>, ErrorCode> {
    let mut warning = None;
    for name in names {
        let file_path = folder.join(format!("{name}.md"));
        if file_path.exists() {
            let result = if force_direct {
                trash::delete_directly(&file_path, registry).map_err(|_| ErrorCode::IoLocked)
            } else {
                trash::trash_file(&file_path, registry)
            };
            if let Err(code) = result {
                tracing::error!(?code, path = %file_path.display(), "memo_delete: failed to delete memo file");
                return Err(code);
            }
        }

        // 画像フォルダが無いメモ(添付なし)はスキップ。`name` は未検証のディスク上の stem でありうる
        // (`..md` → `.` 等)ため、タブ直下の実ディレクトリに解決できるものだけを対象にする。
        let Some(image_folder) = existing_memo_image_folder(folder, name) else {
            continue;
        };
        if force_direct {
            if let Err(err) = trash::delete_dir_directly(&image_folder, registry) {
                tracing::warn!(error = %err, path = %image_folder.display(), "memo_delete: failed to delete memo's image folder");
                warning = Some(MemoMutationWarning::ImageFolderDeleteFailed);
            }
        } else if let Err(code) = trash::trash_file(&image_folder, registry) {
            tracing::error!(?code, path = %image_folder.display(), "memo_delete: failed to trash memo's image folder");
            return Err(code);
        }
    }
    Ok(warning)
}

// ---------------------------------------------------------------------------
// memo_move
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn memo_move(
    app: AppHandle,
    tab_id: String,
    paths: Vec<NodePath>,
    target: MoveTarget,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
    search_index: State<SearchIndex>,
) -> AppResult<MemoMoveResult> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let source_access = match tab_lock::resolve_tab_access(&settings, &sessions, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };

    match target {
        // Same-tab moves never change a memo's name/tab/content, so no search-index patch is
        // needed here (`search-index/PLAN.md` §1 only lists create/rename/delete/move-*across*-
        // tabs and save as content-changing triggers).
        MoveTarget::WithinTab { new_parent, index } => match source_access {
            tab_lock::TabAccess::Session { tab_id, folder } => {
                match tab_lock::session_move_within(&sessions, &registry, &folder, &tab_id, &paths, new_parent.as_deref(), index) {
                    AppResult::Ok(source) => AppResult::ok(MemoMoveResult { source, target: None }),
                    AppResult::Err(err) => AppResult::err(err),
                }
            }
            tab_lock::TabAccess::Plain { folder, tab_name } => memo_move_within_tab(&folder, &tab_name, &paths, new_parent.as_deref(), index, &registry),
        },
        MoveTarget::OtherTab {
            target_tab_id,
            new_parent,
            index,
            copy,
        } => {
            // This is `tab-lock/PLAN.md` §"Cross-Tab DND"'s seam: dropping onto a locked
            // (no-session) target tab rejects with `TabLocked` here, before anything is touched,
            // letting the frontend trigger the password-at-drop-time flow that plan specifies
            // (an inline `tab_unlock` call, then retrying this same `memo_move`).
            let target_access = match tab_lock::resolve_tab_access(&settings, &sessions, &target_tab_id) {
                Ok(v) => v,
                Err(e) => return AppResult::err(e),
            };

            match (source_access, target_access) {
                (tab_lock::TabAccess::Plain { folder: source_folder, tab_name: source_tab_name }, tab_lock::TabAccess::Plain { folder: target_folder, tab_name: target_tab_name }) => {
                    memo_move_cross_tab(&tab_id, &source_folder, &source_tab_name, &target_tab_id, &target_folder, &target_tab_name, &paths, new_parent.as_deref(), index, copy, &registry, &search_index)
                }
                // Plain -> locked-session: the moved memo(s) leave the plain tab's index scope
                // (`!copy` only — a copy leaves the plain original's own indexed entry intact) and
                // never enter the locked target's (`search-index/PLAN.md` §1: unconditional
                // exclusion, no exception for the session case).
                (tab_lock::TabAccess::Plain { folder: source_folder, tab_name: source_tab_name }, tab_lock::TabAccess::Session { tab_id: target_tab_id, folder: target_folder }) => {
                    tab_lock::move_cross_tab_plain_to_session(&tab_id, &source_folder, &source_tab_name, &target_tab_id, &target_folder, &paths, new_parent.as_deref(), index, copy, &registry, &sessions, &search_index)
                }
                // Locked-session -> plain: the moved memo(s) become indexable in the plain target,
                // regardless of `copy` (the source stayed excluded either way, so there is nothing
                // to remove there).
                (tab_lock::TabAccess::Session { tab_id: source_tab_id, folder: source_folder }, tab_lock::TabAccess::Plain { folder: target_folder, tab_name: target_tab_name }) => {
                    tab_lock::move_cross_tab_session_to_plain(&source_tab_id, &source_folder, &target_tab_id, &target_folder, &target_tab_name, &paths, new_parent.as_deref(), index, copy, &registry, &sessions, &search_index)
                }
                (tab_lock::TabAccess::Session { tab_id: source_tab_id, folder: source_folder }, tab_lock::TabAccess::Session { tab_id: target_tab_id, folder: target_folder }) => {
                    tab_lock::move_cross_tab_session_to_session(&source_tab_id, &source_folder, &target_tab_id, &target_folder, &paths, new_parent.as_deref(), index, copy, &registry, &sessions)
                }
            }
        }
    }
}

fn memo_move_within_tab(
    folder: &Path,
    tab_name: &str,
    paths: &[NodePath],
    new_parent: Option<&str>,
    index: usize,
    registry: &SelfWriteRegistry,
) -> AppResult<MemoMoveResult> {
    let mut loaded = load_and_reconcile(folder);

    let mut removed = Vec::new();
    for p in paths {
        if let Some(node) = remove_node(&mut loaded.tree, p) {
            removed.push(node);
        }
    }
    if removed.is_empty() {
        return AppResult::err_code(ErrorCode::NotFound, "no matching memo to move");
    }

    // Cycle prevention: the moved subtrees were already removed above, so
    // `new_parent` no longer resolves if it *was* one of the moved nodes or
    // nested inside one of their subtrees — "move a node into its own
    // descendant" is rejected for free, with no separate ancestor-chain
    // check needed.
    if let Some(p) = new_parent {
        if find_node(&loaded.tree, p).is_none() {
            return AppResult::err_code(ErrorCode::NotFound, "target parent memo not found");
        }
    }

    for (i, node) in removed.into_iter().enumerate() {
        if let Err(code) = insert_node_at(&mut loaded.tree, new_parent, index + i, node) {
            return AppResult::err_code(code, "failed to move memo");
        }
    }

    if let Err(err) = save_tab_meta(folder, loaded.schema_version, loaded.app_version, &loaded.tree, loaded.last_viewed_memo.clone(), registry) {
        tracing::error!(error = %err, "memo_move: failed to save tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
    }

    AppResult::ok(MemoMoveResult {
        source: TreeSnapshot {
            nodes: loaded.tree,
            tab_name: tab_name.to_string(),
            locked: false,
            recovery_pending: false,
            recovery_kind: None,
            last_viewed_memo: loaded.last_viewed_memo,
        },
        target: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn memo_move_cross_tab(
    source_tab_id: &str,
    source_folder: &Path,
    source_tab_name: &str,
    target_tab_id: &str,
    target_folder: &Path,
    target_tab_name: &str,
    paths: &[NodePath],
    new_parent: Option<&str>,
    index: usize,
    copy: bool,
    registry: &SelfWriteRegistry,
    search_index: &SearchIndex,
) -> AppResult<MemoMoveResult> {
    let mut source_loaded = load_and_reconcile(source_folder);
    let mut target_loaded = load_and_reconcile(target_folder);

    if let Some(p) = new_parent {
        if find_node(&target_loaded.tree, p).is_none() {
            return AppResult::err_code(ErrorCode::NotFound, "target parent memo not found");
        }
    }

    let mut nodes_to_move = Vec::new();
    for p in paths {
        if let Some(node) = find_node(&source_loaded.tree, p) {
            nodes_to_move.push(node.clone());
        }
    }
    if nodes_to_move.is_empty() {
        return AppResult::err_code(ErrorCode::NotFound, "no matching memo to move");
    }

    // Every name-collision is checked before any file is touched, so a
    // rejected move never partially completes (`memo-tree/PLAN.md` §3).
    let mut all_names = Vec::new();
    for node in &nodes_to_move {
        collect_names(node, &mut all_names);
    }
    for name in &all_names {
        if contains_name(&target_loaded.tree, name) {
            return AppResult::err_code(ErrorCode::NameDuplicate, "a memo with this name already exists in the target tab");
        }
    }
    // GitHub issue #262: 画像フォルダ `<メモ名>/` も同名で運ぶ(名前が変わらないのでリンク書き換えは不要)。
    // 衝突は上の名前チェックと同じく、何も触る前に拒否する。
    let image_sources = all_names.iter().filter_map(|name| existing_memo_image_folder(source_folder, name).map(|src| (name.clone(), src))).collect();
    let image_transfers = match plan_image_folders_into_plain_tab(target_folder, image_sources) {
        Ok(v) => v,
        Err(err) => return AppResult::err(err),
    };
    // フォルダを先に運ぶ(`CRASH-RECOVERY.md` §10)。失敗時は運んだ分を戻し、何も変えずに中止。
    let transfers: Vec<FolderTransfer> = image_transfers.iter().map(|(_, t)| t.clone()).collect();
    if transfer_all(&transfers, copy, registry).is_err() {
        return AppResult::err_code(ErrorCode::ImageFolderRenameFailed, "failed to move a memo image folder between tabs");
    }

    for (i, name) in all_names.iter().enumerate() {
        let src_path = source_folder.join(format!("{name}.md"));
        let dst_path = target_folder.join(format!("{name}.md"));
        let op_result: std::io::Result<()> = if copy {
            // A brand-new file at the destination has no prior version to
            // `.bak`-swap, so a plain `fs::copy` (not `atomic_write`) is the
            // legitimate exception here too, same as image writes
            // (`persistence-fs/PLAN.md` §3).
            registry.register(&dst_path);
            std::fs::copy(&src_path, &dst_path).map(|_| ())
        } else {
            registry.register(&src_path);
            registry.register(&dst_path);
            with_retry(|| std::fs::rename(&src_path, &dst_path)).or_else(|_| {
                // Cross-volume rename fallback: copy + delete original
                // (`memo-tree/PLAN.md` §3).
                std::fs::copy(&src_path, &dst_path)?;
                std::fs::remove_file(&src_path)
            })
        };
        if let Err(err) = op_result {
            tracing::error!(error = %err, "memo_move: cross-tab file operation failed");
            // 本文をまだ運べていないメモ(このメモ以降)の画像フォルダは元へ戻す
            undo_transfers(&image_transfers_for(&image_transfers, &all_names[i..]), copy, registry);
            return AppResult::err_code(ErrorCode::Unknown, "failed to move memo file between tabs");
        }
        // GitHub issue #282: 移動なら `<メモ名>.md.bak` も移動先へ運ぶ(コピーでは運ばない)
        if !copy {
            carry_memo_bak(source_folder, name, target_folder, name, registry);
        }
    }

    for (i, p) in paths.iter().enumerate() {
        let node = if copy {
            find_node(&source_loaded.tree, p).cloned()
        } else {
            remove_node(&mut source_loaded.tree, p)
        };
        if let Some(node) = node {
            let _ = insert_node_at(&mut target_loaded.tree, new_parent, index + i, node);
        }
    }

    for name in &all_names {
        search_index.upsert(target_tab_id, name, &read_body_for_index(target_folder, name));
        if !copy {
            search_index.remove(source_tab_id, name);
        }
    }

    if let Err(err) = save_tab_meta(target_folder, target_loaded.schema_version, target_loaded.app_version, &target_loaded.tree, target_loaded.last_viewed_memo.clone(), registry) {
        tracing::error!(error = %err, "memo_move: failed to save target tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save target tab metadata");
    }
    if !copy {
        // The moved memo(s) no longer exist in the source tab's tree, so a
        // stale `last_viewed_memo` pointer must be cleared here rather than
        // left dangling in the persisted metadata (issue #239).
        if let Some(last_viewed) = &source_loaded.last_viewed_memo {
            if all_names.iter().any(|name| name == last_viewed) {
                source_loaded.last_viewed_memo = None;
            }
        }
        if let Err(err) = save_tab_meta(source_folder, source_loaded.schema_version, source_loaded.app_version, &source_loaded.tree, source_loaded.last_viewed_memo.clone(), registry) {
            tracing::error!(error = %err, "memo_move: failed to save source tab metadata");
            return AppResult::err_code(ErrorCode::Unknown, "failed to save source tab metadata");
        }
    }

    AppResult::ok(MemoMoveResult {
        source: TreeSnapshot {
            nodes: source_loaded.tree,
            tab_name: source_tab_name.to_string(),
            locked: false,
            recovery_pending: false,
            recovery_kind: None,
            last_viewed_memo: source_loaded.last_viewed_memo,
        },
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

/// GitHub issue #262: 平文タブへ移動/コピーする各メモの画像フォルダ(`sources`: メモ名 → 移動元の
/// 実フォルダ)について、移動先 `<target_folder>/<メモ名>/` を決める事前チェック。移動先に既に何か
/// あれば `NameDuplicate`、メモ名が #263 のガードを通らなければ `NameInvalid`(どちらも何も
/// 触る前に操作全体を拒否する)。
pub(crate) fn plan_image_folders_into_plain_tab(target_folder: &Path, sources: Vec<(String, PathBuf)>) -> Result<Vec<(String, FolderTransfer)>, AppErrorPayload> {
    let mut planned = Vec::with_capacity(sources.len());
    for (name, src) in sources {
        let Some(dst) = memo_image_folder_path(target_folder, &name) else {
            return Err(AppErrorPayload::new(ErrorCode::NameInvalid, "memo name is not usable as an image folder name in the target tab"));
        };
        if std::fs::symlink_metadata(&dst).is_ok() {
            return Err(AppErrorPayload::new(ErrorCode::NameDuplicate, "an image folder with this memo's name already exists in the target tab"));
        }
        planned.push((name, FolderTransfer { src, dst }));
    }
    Ok(planned)
}

/// `planned` のうち `names` に含まれるメモの転送だけを取り出す(部分取り消し用)。
pub(crate) fn image_transfers_for(planned: &[(String, FolderTransfer)], names: &[String]) -> Vec<FolderTransfer> {
    planned.iter().filter(|(n, _)| names.contains(n)).map(|(_, t)| t.clone()).collect()
}

// ---------------------------------------------------------------------------
// memo_set_expanded / memo_set_label / memo_set_editor_override / memo_set_edit_lock
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn memo_set_expanded(
    app: AppHandle,
    tab_id: String,
    path: NodePath,
    expanded: bool,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
) -> AppResult<TreeSnapshot> {
    mutate_node_field(&app, &tab_id, &path, &store, &registry, &sessions, |node| {
        node.expanded = expanded;
        Ok(())
    })
}

/// `color: None` clears the label to the "None" choice (`docs/plans/memo-tree/PLAN.md` §3's
/// confirmed 5-choice palette — "None" has no `LabelColor` variant of its own, it's simply the
/// absence of one). Never touches `label.kind` (reserved/unimplemented for v1).
#[tauri::command]
pub fn memo_set_label(
    app: AppHandle,
    tab_id: String,
    path: NodePath,
    color: Option<LabelColor>,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
) -> AppResult<TreeSnapshot> {
    mutate_node_field(&app, &tab_id, &path, &store, &registry, &sessions, |node| {
        match color {
            Some(c) => {
                let label = node.label.get_or_insert_with(|| Label { color: None, kind: None });
                label.color = Some(c.hex().to_string());
            }
            None => {
                if let Some(label) = node.label.as_mut() {
                    label.color = None;
                    if label.kind.is_none() {
                        node.label = None;
                    }
                }
            }
        }
        Ok(())
    })
}

#[tauri::command]
pub fn memo_set_editor_override(
    app: AppHandle,
    tab_id: String,
    path: NodePath,
    editor: Option<MemoEditorSettings>,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
) -> AppResult<TreeSnapshot> {
    mutate_node_field(&app, &tab_id, &path, &store, &registry, &sessions, |node| {
        node.editor = editor;
        Ok(())
    })
}

/// Toggles the per-memo **edit lock** (`locked?: boolean`) — body-edit-only block, no password,
/// distinct from tab lock (`[spec §2.2]`). This command only persists the flag; refusing edits
/// when set is `editor-save/PLAN.md`'s responsibility.
///
/// GitHub issue #165: also drives `MemoEditorSettings.preview_locked` (issue #149's preview-lock)
/// to the same value, symmetrically — an edit-locked memo has nothing editable to show, so it's
/// pinned to preview; unlocking releases the pin too. One-directional: the preview-lock toolbar
/// button remains independently togglable and never writes back to `locked`.
#[tauri::command]
pub fn memo_set_edit_lock(
    app: AppHandle,
    tab_id: String,
    path: NodePath,
    locked: bool,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
) -> AppResult<TreeSnapshot> {
    mutate_node_field(&app, &tab_id, &path, &store, &registry, &sessions, |node| {
        node.locked = Some(locked);
        node.editor.get_or_insert_with(Default::default).preview_locked = Some(locked);
        Ok(())
    })
}

/// Toggles the cross-tab favorite flag (`docs/plans/v1.1.0/favorites/PLAN.md` §2,
/// `[spec v1.1.0 §4.1]`). Structurally identical to `memo_set_edit_lock` above, plus one extra
/// side effect: patches the full-text index's mirrored `IndexEntry.favorite` on success
/// (`favorites/PLAN.md` §3) so `search_get_favorites` (out of scope on this branch) will see it
/// without a rebuild. `search_index.set_favorite` is a harmless no-op for locked tabs (no matching
/// index entry ever exists there, `search-index/PLAN.md` §1's unconditional locked-tab exclusion).
#[tauri::command]
pub fn memo_set_favorite(
    app: AppHandle,
    tab_id: String,
    path: NodePath,
    favorite: bool,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
    search_index: State<SearchIndex>,
) -> AppResult<TreeSnapshot> {
    let result = mutate_node_field(&app, &tab_id, &path, &store, &registry, &sessions, |node| {
        node.favorite = Some(favorite);
        Ok(())
    });
    if let AppResult::Ok(_) = &result {
        search_index.set_favorite(&tab_id, &path, favorite);
    }
    result
}

// ---------------------------------------------------------------------------
// tree_set_last_viewed
// ---------------------------------------------------------------------------

/// `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §4 — pure metadata mutation (no file-body I/O),
/// called from the same trigger points that already call `memo_set_editor_override` to persist the
/// outgoing memo's `scrollPosition` (memo click, arrow-key nav, `memo:` link navigation, tab
/// switch): this rides along, it does not introduce a new debounce/flush timer. Unlike
/// `memo_set_expanded`/`memo_set_edit_lock` etc. above, this does NOT return a fresh
/// `TreeSnapshot` — the frontend already holds the authoritative in-memory tree/selection state for
/// this narrow, tree-structure-independent mutation.
#[tauri::command]
pub fn tree_set_last_viewed(
    app: AppHandle,
    tab_id: String,
    memo_name: Option<String>,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
    view_store: State<TabViewStore>,
) -> AppResult<()> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    tree_set_last_viewed_impl(&settings, &sessions, &view_store, &registry, &tab_id, memo_name)
}

/// Core logic, split out from the `#[tauri::command]` wrapper above so it's directly unit-testable
/// against a manually-built `AppSettings` — `State<T>` extraction needs a live `tauri::App`, the
/// same constraint `commands/editor.rs`'s own tests document (`resolve_tab`/`load_and_reconcile`
/// below follow the identical split).
pub(crate) fn tree_set_last_viewed_impl(
    settings: &AppSettings,
    sessions: &LockSessionStore,
    view_store: &TabViewStore,
    registry: &SelfWriteRegistry,
    tab_id: &str,
    memo_name: Option<String>,
) -> AppResult<()> {
    let access = match tab_lock::resolve_tab_access(settings, sessions, tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };

    match access {
        // Session branch: re-encrypt+re-save the lock blob (fresh IV) — same "structure change ->
        // re-encrypt and re-save the blob" rule `tab-lock/PLAN.md` §3 "While a Session Is Active"
        // already states for any session-tree mutation.
        tab_lock::TabAccess::Session { tab_id: session_tab_id, folder } => {
            let result = tab_lock::session_set_last_viewed(sessions, registry, &folder, &session_tab_id, memo_name.clone());
            if matches!(result, AppResult::Ok(())) {
                view_store.set(&session_tab_id, memo_name);
            }
            result
        }
        tab_lock::TabAccess::Plain { folder, .. } => {
            let loaded = load_and_reconcile(&folder);
            if let Err(err) = save_tab_meta(&folder, loaded.schema_version, loaded.app_version, &loaded.tree, memo_name.clone(), registry) {
                tracing::error!(error = %err, "tree_set_last_viewed: failed to save tab metadata");
                return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
            }
            view_store.set(tab_id, memo_name);
            AppResult::ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn mutate_node_field(
    app: &AppHandle,
    tab_id: &str,
    path: &str,
    store: &AppSettingsStore,
    registry: &SelfWriteRegistry,
    sessions: &LockSessionStore,
    f: impl FnOnce(&mut MetaTreeNode) -> Result<(), ErrorCode>,
) -> AppResult<TreeSnapshot> {
    let (settings, outcome) = store.get(registry);
    emit_settings_recovery_event(app, outcome);

    let access = match tab_lock::resolve_tab_access(&settings, sessions, tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };
    let (folder, tab_name) = match access {
        tab_lock::TabAccess::Session { tab_id, folder } => {
            return tab_lock::session_mutate_node_field(sessions, registry, &folder, &tab_id, path, f);
        }
        tab_lock::TabAccess::Plain { folder, tab_name } => (folder, tab_name),
    };

    let mut loaded = load_and_reconcile(&folder);
    let Some(node) = find_node_mut(&mut loaded.tree, path) else {
        return AppResult::err_code(ErrorCode::NotFound, "memo not found");
    };
    if let Err(code) = f(node) {
        return AppResult::err_code(code, "failed to update memo");
    }

    if let Err(err) = save_tab_meta(&folder, loaded.schema_version, loaded.app_version, &loaded.tree, loaded.last_viewed_memo.clone(), registry) {
        tracing::error!(error = %err, "failed to save tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
    }

    AppResult::ok(TreeSnapshot {
        nodes: loaded.tree,
        tab_name,
        locked: false,
        recovery_pending: false,
        recovery_kind: None,
        last_viewed_memo: loaded.last_viewed_memo,
    })
}

// ---------------------------------------------------------------------------
// memo_get_properties
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn memo_get_properties(
    app: AppHandle,
    tab_id: String,
    path: NodePath,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
) -> AppResult<MemoProperties> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    let access = match tab_lock::resolve_tab_access(&settings, &sessions, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };
    let folder = match access {
        tab_lock::TabAccess::Session { tab_id, folder } => {
            return tab_lock::session_get_properties(&sessions, &folder, &tab_id, &path);
        }
        tab_lock::TabAccess::Plain { folder, .. } => folder,
    };

    let file_path = folder.join(format!("{path}.md"));
    let metadata = match std::fs::metadata(&file_path) {
        Ok(m) => m,
        Err(_) => return AppResult::err_code(ErrorCode::NotFound, "memo file not found"),
    };
    let bytes = match std::fs::read(&file_path) {
        Ok(b) => b,
        Err(_) => return AppResult::err_code(ErrorCode::NotFound, "memo file not found"),
    };

    let normalized = crate::store::memo_file_service::normalize_to_lf(&decode_memo_body(&bytes));
    let char_count = normalized.chars().count();
    let char_count_excluding_whitespace = normalized.chars().filter(|c| !c.is_whitespace()).count();
    let line_count = normalized.split('\n').count();
    let created_ms = metadata.created().ok().map(system_time_to_epoch_ms).unwrap_or_default();
    let modified_ms = metadata.modified().ok().map(system_time_to_epoch_ms).unwrap_or_default();
    let file_name = format!("{}.md", path.rsplit('/').next().unwrap_or(&path));

    AppResult::ok(MemoProperties {
        file_name,
        file_path: file_path.display().to_string(),
        file_size_bytes: metadata.len(),
        created_ms,
        modified_ms,
        char_count,
        char_count_excluding_whitespace,
        line_count,
    })
}

// ---------------------------------------------------------------------------
// memo_open_location
// ---------------------------------------------------------------------------

/// OS "reveal in file manager" for a single memo (`[spec §2.8]`'s pattern, same Shell API as
/// `commands/tab::tab_open_location`). Plain (non-locked) tabs reveal the actual memo file,
/// selected in Explorer. Locked/session tabs store the memo body under an obfuscated on-disk
/// token filename (`tab_lock::session_get_properties`'s same reasoning) — revealing that specific
/// file would show a confusingly-named file, so this branch reveals the tab's folder itself
/// instead, without selecting any file (same behavior as `tab_open_location`).
#[tauri::command]
pub fn memo_open_location(
    app: AppHandle,
    tab_id: String,
    path: NodePath,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
) -> AppResult<()> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    let access = match tab_lock::resolve_tab_access(&settings, &sessions, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };

    let target = match access {
        tab_lock::TabAccess::Session { folder, .. } => folder,
        tab_lock::TabAccess::Plain { folder, .. } => folder.join(format!("{path}.md")),
    };

    match app.opener().reveal_item_in_dir(&target) {
        Ok(()) => AppResult::ok(()),
        Err(err) => {
            tracing::error!(error = %err, path = %target.display(), "memo_open_location: failed to reveal location");
            AppResult::err_code(ErrorCode::NotFound, "could not open the memo's file location")
        }
    }
}

/// Encoding detection (`[detailed-spec §5.3]`): delegates to the shared
/// `store::memo_file_service` decoder (`editor-save/PLAN.md`), which `commands/editor.rs`'s
/// `editor_load`/`editor_save` also use — only the decoded text is needed here, not the
/// detected `Encoding` itself.
fn decode_memo_body(bytes: &[u8]) -> String {
    crate::store::memo_file_service::decode_memo_body(bytes).0
}

/// Converts a `SystemTime` to milliseconds since the Unix epoch, for the frontend to format
/// locally (weekday + local time + relative "n日前", matching the old app's client-side
/// `formatDateTime` — `src/shared/formatDateTime.ts`). `pub(crate)` so `commands/tab_lock.rs`'s
/// session-path `memo_get_properties` equivalent can reuse it.
pub(crate) fn system_time_to_epoch_ms(t: std::time::SystemTime) -> u64 {
    t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Search-index integration
// ---------------------------------------------------------------------------

/// Reads and decodes `<folder>/<memo_name>.md` for a `SearchIndex::upsert` call whose caller
/// (`memo_rename`, `memo_move`'s cross-tab path) doesn't already have the memo's content on hand
/// the way `editor_save` does — renaming/moving a file doesn't change its bytes, so the content
/// is simply re-read from its (already renamed/moved-to) new location. Any read failure falls
/// back to an empty body rather than failing the whole command: by this point the real file
/// operation the caller performed already succeeded, so a read error here would indicate trouble
/// already surfaced elsewhere, and an under-indexed (rather than un-movable) memo is the safer
/// degradation.
fn read_body_for_index(folder: &Path, memo_name: &str) -> String {
    let path = folder.join(format!("{memo_name}.md"));
    std::fs::read(&path).map(|bytes| decode_memo_body(&bytes)).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tab resolution
// ---------------------------------------------------------------------------

pub(crate) fn resolve_tab(settings: &AppSettings, tab_id: &str) -> Result<(PathBuf, String), AppErrorPayload> {
    let entry = settings
        .tabs
        .iter()
        .find(|t| t.id == tab_id)
        .ok_or_else(|| AppErrorPayload::new(ErrorCode::NotFound, "tab not found"))?;
    if entry.locked.unwrap_or(false) {
        // `tab-lock/PLAN.md`'s seam: once sessions exist, this needs to
        // allow a tab with an *active* session through instead of always
        // rejecting.
        return Err(AppErrorPayload::new(ErrorCode::TabLocked, "cannot modify a locked tab"));
    }
    Ok((PathBuf::from(&entry.folder_path), entry.name.clone()))
}

// ---------------------------------------------------------------------------
// Metadata load / reconcile / save
// ---------------------------------------------------------------------------

/// `pub(crate)` so `commands/editor.rs` (`editor-save/PLAN.md`) can reuse `load_and_reconcile`
/// without duplicating tree reconciliation logic.
pub(crate) struct LoadedTabMeta {
    pub(crate) schema_version: u32,
    pub(crate) app_version: String,
    pub(crate) tree: Vec<MetaTreeNode>,
    /// `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §1 — the raw persisted `lastViewedMemo`,
    /// carried through unchanged so callers that re-save this tab's metadata (every mutating
    /// command below) preserve it rather than silently clearing it back to `None`.
    pub(crate) last_viewed_memo: Option<String>,
}

/// `pub(crate)` so `commands/tab_lock.rs`'s crash-recovery Finish/Restore flow
/// (`docs/plans/tab-lock/CRASH-RECOVERY.md`) can discriminate "step 8 (lock blob write) already
/// succeeded" (`.tabtreenote.json` has a `lock` field) from "step 8 never ran" (it still has a
/// `tree` field) without duplicating the corruption-recovery cascade.
pub(crate) fn load_tab_meta_with_recovery(folder: &Path) -> (Option<TabMetaFile>, RecoveryOutcome) {
    let primary = folder.join(TAB_META_FILE_NAME);
    if let Some(meta) = try_parse_tab_meta(&primary) {
        return (Some(meta), RecoveryOutcome::Clean);
    }
    let backup = bak_path(&primary);
    if let Some(meta) = try_parse_tab_meta(&backup) {
        return (Some(meta), RecoveryOutcome::FromBackup);
    }
    (None, RecoveryOutcome::Regenerated)
}

fn try_parse_tab_meta(path: &Path) -> Option<TabMetaFile> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<TabMetaFile>(&bytes).ok()
}

/// Loads the persisted tree (or an empty one, on total corruption/loss —
/// the flat-rebuild recovery path, `[spec §5.9]`) and reconciles it against
/// a fresh scan of the tab folder (`[detailed-spec §3.5]`). Does not persist
/// anything itself — see this module's top-level doc comment for why that's
/// safe to defer.
pub(crate) fn load_and_reconcile(folder: &Path) -> LoadedTabMeta {
    let (parsed, _recovery) = load_tab_meta_with_recovery(folder);
    let (schema_version, app_version, persisted_tree, last_viewed_memo) = match parsed {
        Some(meta) => (meta.schema_version, meta.app_version, meta.tree.unwrap_or_default(), meta.last_viewed_memo),
        None => (TAB_META_SCHEMA_VERSION, env!("CARGO_PKG_VERSION").to_string(), Vec::new(), None),
    };
    let (tree, _changed) = reconcile_against_disk(folder, persisted_tree);
    LoadedTabMeta { schema_version, app_version, tree, last_viewed_memo }
}

/// `pub(crate)` so `commands/tab_lock.rs`'s crash-recovery Finish flow can write the plain-`tree`
/// shape back if recovery rolls back before ever building a lock blob for an unrelated tab (not
/// currently exercised, but keeps this the single write path for the `tree`-shaped
/// `.tabtreenote.json` rather than a second ad hoc one).
pub(crate) fn save_tab_meta(folder: &Path, schema_version: u32, app_version: String, tree: &[MetaTreeNode], last_viewed_memo: Option<String>, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    let meta = TabMetaFile {
        schema_version,
        app_version,
        tree: Some(tree.to_vec()),
        lock: None,
        last_viewed_memo,
    };
    let json = serde_json::to_vec_pretty(&meta)?;
    atomic_write(&folder.join(TAB_META_FILE_NAME), &json, registry)
}

/// Direct-child `.md` files of `folder`, by `memoName` (extension stripped). A missing folder
/// yields an empty list, not an error (`[detailed-spec §6.5]`). Symbolic links/junctions are not
/// followed (`[spec §2.2]`). `pub(crate)` so `commands/tab_lock.rs`'s permanent-unlock
/// crash-recovery trigger-point check (`tab-lock-permanent-unlock/CRASH-RECOVERY.md` §3.2) can
/// reuse this same password-free "any `.md` file present" scan, and so its Finish recovery
/// command can reindex from the actual on-disk file list (`PLAN.md` §4.6).
pub(crate) fn scan_real_memo_names(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else { continue };
        if !ext.eq_ignore_ascii_case("md") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            names.push(stem.to_string());
        }
    }
    names
}

/// Reconciliation rules (`[detailed-spec §3.5]`):
/// - a metadata entry with no matching file is dropped (not re-created, not kept as a
///   placeholder). Its children, if their own files still exist, are promoted to this level
///   rather than being dropped too — an edge case `detailed-spec.md` does not spell out beyond
///   "the entry is dropped," resolved here in favor of the "memo body is never lost" principle
///   (`[spec §3.2]`) over losing a still-real child's place in the tree.
/// - a real file with no matching metadata entry is appended to the end of the root level, with
///   default settings, sorted for determinism (directory read order is not guaranteed).
fn reconcile_against_disk(folder: &Path, persisted: Vec<MetaTreeNode>) -> (Vec<MetaTreeNode>, bool) {
    let original = persisted.clone();
    let mut real_names: BTreeSet<String> = scan_real_memo_names(folder).into_iter().collect();
    let mut reconciled = reconcile_tree(persisted, &mut real_names);
    for name in real_names {
        reconciled.push(MetaTreeNode::default_for_new_file(name));
    }
    let changed = reconciled != original;
    (reconciled, changed)
}

fn reconcile_tree(persisted: Vec<MetaTreeNode>, real_names: &mut BTreeSet<String>) -> Vec<MetaTreeNode> {
    let mut out = Vec::with_capacity(persisted.len());
    for mut node in persisted {
        if real_names.remove(&node.memo_name) {
            node.children = reconcile_tree(node.children, real_names);
            out.push(node);
        } else {
            out.extend(reconcile_tree(node.children, real_names));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tree node lookup / mutation helpers
// ---------------------------------------------------------------------------

/// `pub(crate)` so `commands/editor.rs` (`editor-save/PLAN.md`) can look up a memo's
/// `MetaTreeNode` (edit-lock flag, `MemoEditorSettings.scrollPosition`) without duplicating tree
/// lookup logic.
pub(crate) fn find_node<'a>(tree: &'a [MetaTreeNode], name: &str) -> Option<&'a MetaTreeNode> {
    for node in tree {
        if node.memo_name == name {
            return Some(node);
        }
        if let Some(found) = find_node(&node.children, name) {
            return Some(found);
        }
    }
    None
}

/// `pub(crate)` so `commands/tab_lock.rs` can mutate a locked-session tab's in-memory tree
/// (`docs/plans/tab-lock/PLAN.md` §3 "While a Session Is Active") without duplicating tree
/// lookup/mutation logic.
pub(crate) fn find_node_mut<'a>(tree: &'a mut Vec<MetaTreeNode>, name: &str) -> Option<&'a mut MetaTreeNode> {
    for node in tree.iter_mut() {
        if node.memo_name == name {
            return Some(node);
        }
        if let Some(found) = find_node_mut(&mut node.children, name) {
            return Some(found);
        }
    }
    None
}

/// Case-insensitive (ASCII) — matches the same convention used for tab-name uniqueness
/// (`commands/tab.rs`). `pub(crate)`: reused by `commands/tab_lock.rs`'s session-path memo
/// create/rename, same reason as `find_node_mut` above.
pub(crate) fn contains_name(tree: &[MetaTreeNode], name: &str) -> bool {
    tree.iter().any(|n| n.memo_name.eq_ignore_ascii_case(name) || contains_name(&n.children, name))
}

/// `pub(crate)`: reused by `commands/tab_lock.rs`'s session-path memo delete/move.
pub(crate) fn remove_node(tree: &mut Vec<MetaTreeNode>, name: &str) -> Option<MetaTreeNode> {
    if let Some(pos) = tree.iter().position(|n| n.memo_name == name) {
        return Some(tree.remove(pos));
    }
    for node in tree.iter_mut() {
        if let Some(found) = remove_node(&mut node.children, name) {
            return Some(found);
        }
    }
    None
}

/// `pub(crate)`: reused by `commands/tab_lock.rs`'s session-path memo delete/cross-tab move.
pub(crate) fn collect_names(node: &MetaTreeNode, out: &mut Vec<String>) {
    out.push(node.memo_name.clone());
    for child in &node.children {
        collect_names(child, out);
    }
}

fn children_of_mut<'a>(tree: &'a mut Vec<MetaTreeNode>, parent: Option<&str>) -> Result<&'a mut Vec<MetaTreeNode>, ErrorCode> {
    match parent {
        None => Ok(tree),
        Some(name) => find_node_mut(tree, name).map(|n| &mut n.children).ok_or(ErrorCode::NotFound),
    }
}

/// `pub(crate)`: reused by `commands/tab_lock.rs`'s session-path memo create/move.
pub(crate) fn insert_node_at(tree: &mut Vec<MetaTreeNode>, parent: Option<&str>, index: usize, node: MetaTreeNode) -> Result<(), ErrorCode> {
    let children = children_of_mut(tree, parent)?;
    let idx = index.min(children.len());
    children.insert(idx, node);
    Ok(())
}

/// `pub(crate)`: reused by `commands/tab_lock.rs`'s session-path memo create.
pub(crate) fn insert_node_at_end(tree: &mut Vec<MetaTreeNode>, parent: Option<&str>, node: MetaTreeNode) -> Result<(), ErrorCode> {
    let children = children_of_mut(tree, parent)?;
    children.push(node);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str) -> MetaTreeNode {
        MetaTreeNode::default_for_new_file(name.to_string())
    }

    fn node_with_children(name: &str, children: Vec<MetaTreeNode>) -> MetaTreeNode {
        let mut n = leaf(name);
        n.children = children;
        n
    }

    #[test]
    fn reconcile_drops_entries_with_no_matching_file() {
        let persisted = vec![leaf("Gone"), leaf("Still here")];
        let mut real = BTreeSet::from(["Still here".to_string()]);
        let reconciled = reconcile_tree(persisted, &mut real);
        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].memo_name, "Still here");
    }

    #[test]
    fn reconcile_promotes_children_of_a_dropped_parent() {
        let persisted = vec![node_with_children("Parent gone", vec![leaf("Child stays")])];
        let mut real = BTreeSet::from(["Child stays".to_string()]);
        let reconciled = reconcile_tree(persisted, &mut real);
        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].memo_name, "Child stays");
    }

    #[test]
    fn reconcile_against_disk_appends_new_real_files_sorted_to_root_end() {
        let dir = tempdir();
        std::fs::write(dir.join("Existing.md"), b"").unwrap();
        std::fs::write(dir.join("Zebra.md"), b"").unwrap();
        std::fs::write(dir.join("Apple.md"), b"").unwrap();

        let persisted = vec![leaf("Existing")];
        let (reconciled, changed) = reconcile_against_disk(&dir, persisted);
        assert!(changed);
        let names: Vec<&str> = reconciled.iter().map(|n| n.memo_name.as_str()).collect();
        assert_eq!(names, vec!["Existing", "Apple", "Zebra"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reconcile_against_disk_missing_folder_yields_empty_not_error() {
        let dir = std::env::temp_dir().join(format!("ttn-tree-missing-{}", rand::random::<u64>()));
        let (reconciled, changed) = reconcile_against_disk(&dir, vec![leaf("Anything")]);
        assert!(reconciled.is_empty());
        assert!(changed);
    }

    #[test]
    fn find_remove_insert_roundtrip() {
        let mut tree = vec![node_with_children("A", vec![leaf("B")]), leaf("C")];
        assert!(find_node(&tree, "B").is_some());

        let removed = remove_node(&mut tree, "B").unwrap();
        assert_eq!(removed.memo_name, "B");
        assert!(find_node(&tree, "B").is_none());

        insert_node_at_end(&mut tree, None, removed).unwrap();
        assert_eq!(tree.last().unwrap().memo_name, "B");
    }

    #[test]
    fn insert_node_at_missing_parent_is_not_found() {
        let mut tree = vec![leaf("A")];
        let err = insert_node_at_end(&mut tree, Some("Missing"), leaf("New")).unwrap_err();
        assert_eq!(err, ErrorCode::NotFound);
    }

    #[test]
    fn contains_name_is_case_insensitive_and_recursive() {
        let tree = vec![node_with_children("A", vec![leaf("Nested")])];
        assert!(contains_name(&tree, "nested"));
        assert!(!contains_name(&tree, "Missing"));
    }

    #[test]
    fn collect_names_includes_self_and_all_descendants() {
        let node = node_with_children("Root", vec![leaf("Child1"), node_with_children("Child2", vec![leaf("Grandchild")])]);
        let mut out = Vec::new();
        collect_names(&node, &mut out);
        assert_eq!(out, vec!["Root", "Child1", "Child2", "Grandchild"]);
    }

    #[test]
    fn move_target_deserializes_from_camel_case_wire_shape() {
        let within: MoveTarget = serde_json::from_value(serde_json::json!({
            "kind": "withinTab",
            "newParent": "Parent",
            "index": 2
        }))
        .unwrap();
        match within {
            MoveTarget::WithinTab { new_parent, index } => {
                assert_eq!(new_parent.as_deref(), Some("Parent"));
                assert_eq!(index, 2);
            }
            _ => panic!("expected WithinTab"),
        }

        let other: MoveTarget = serde_json::from_value(serde_json::json!({
            "kind": "otherTab",
            "targetTabId": "tab-2",
            "newParent": null,
            "index": 0,
            "copy": true
        }))
        .unwrap();
        match other {
            MoveTarget::OtherTab { target_tab_id, new_parent, index, copy } => {
                assert_eq!(target_tab_id, "tab-2");
                assert_eq!(new_parent, None);
                assert_eq!(index, 0);
                assert!(copy);
            }
            _ => panic!("expected OtherTab"),
        }
    }

    #[test]
    fn decode_memo_body_handles_bom_and_shift_jis_fallback() {
        assert_eq!(decode_memo_body("hello".as_bytes()), "hello");

        let mut with_bom = vec![0xEF, 0xBB, 0xBF];
        with_bom.extend_from_slice("bom".as_bytes());
        assert_eq!(decode_memo_body(&with_bom), "bom");

        // Shift_JIS encoding of "あ" (0x82 0xA0) is invalid UTF-8, so this
        // should fall back to Shift_JIS decoding rather than erroring.
        let shift_jis_a = [0x82, 0xA0];
        assert_eq!(decode_memo_body(&shift_jis_a), "あ");
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-tree-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // -----------------------------------------------------------------------
    // tree_set_last_viewed_impl (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §4)
    // -----------------------------------------------------------------------

    fn settings_with_plain_tab(tab_id: &str, folder: &Path) -> AppSettings {
        let mut settings = AppSettings::default();
        settings.tabs.push(crate::model::app_settings::TabSettingsEntry {
            id: tab_id.to_string(),
            name: "Test tab".to_string(),
            folder_path: folder.to_string_lossy().into_owned(),
            locked: Some(false),
            lock_label: None,
        });
        settings
    }

    #[test]
    fn tree_set_last_viewed_writes_the_field_and_updates_the_runtime_cache() {
        let dir = tempdir();
        std::fs::write(dir.join("Diary.md"), b"").unwrap();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        let view_store = TabViewStore::new();
        let settings = settings_with_plain_tab("tab-1", &dir);

        let result = tree_set_last_viewed_impl(&settings, &sessions, &view_store, &registry, "tab-1", Some("Diary".to_string()));
        assert!(matches!(result, AppResult::Ok(())));

        let (parsed, _recovery) = load_tab_meta_with_recovery(&dir);
        assert_eq!(parsed.unwrap().last_viewed_memo.as_deref(), Some("Diary"));
        assert_eq!(view_store.get("tab-1"), Some("Diary".to_string()));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tree_set_last_viewed_preserves_the_existing_tree_structure() {
        let dir = tempdir();
        std::fs::write(dir.join("Diary.md"), b"").unwrap();
        std::fs::write(dir.join("Notes.md"), b"").unwrap();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        let view_store = TabViewStore::new();
        let settings = settings_with_plain_tab("tab-1", &dir);

        // Establish an initial reconciled tree on disk first (mirrors an ordinary `tree_open`).
        let loaded = load_and_reconcile(&dir);
        save_tab_meta(&dir, loaded.schema_version, loaded.app_version, &loaded.tree, None, &registry).unwrap();

        let result = tree_set_last_viewed_impl(&settings, &sessions, &view_store, &registry, "tab-1", Some("Notes".to_string()));
        assert!(matches!(result, AppResult::Ok(())));

        let (parsed, _recovery) = load_tab_meta_with_recovery(&dir);
        let meta = parsed.unwrap();
        assert_eq!(meta.last_viewed_memo.as_deref(), Some("Notes"));
        assert_eq!(meta.tree.unwrap().len(), 2, "the tree structure must survive this metadata-only mutation");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tree_set_last_viewed_none_clears_a_previously_set_value() {
        let dir = tempdir();
        std::fs::write(dir.join("Diary.md"), b"").unwrap();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        let view_store = TabViewStore::new();
        let settings = settings_with_plain_tab("tab-1", &dir);

        tree_set_last_viewed_impl(&settings, &sessions, &view_store, &registry, "tab-1", Some("Diary".to_string()));
        let result = tree_set_last_viewed_impl(&settings, &sessions, &view_store, &registry, "tab-1", None);
        assert!(matches!(result, AppResult::Ok(())));

        let (parsed, _recovery) = load_tab_meta_with_recovery(&dir);
        assert_eq!(parsed.unwrap().last_viewed_memo, None);
        assert_eq!(view_store.get("tab-1"), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tree_set_last_viewed_unknown_tab_returns_not_found() {
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        let view_store = TabViewStore::new();
        let settings = AppSettings::default();

        let result = tree_set_last_viewed_impl(&settings, &sessions, &view_store, &registry, "missing-tab", Some("Diary".to_string()));
        match result {
            AppResult::Err(err) => assert_eq!(err.code, ErrorCode::NotFound),
            AppResult::Ok(()) => panic!("expected NotFound for an unregistered tab"),
        }
    }

    #[test]
    fn tree_set_last_viewed_locked_no_session_tab_returns_tab_locked() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        let sessions = LockSessionStore::new();
        let view_store = TabViewStore::new();
        let mut settings = settings_with_plain_tab("tab-1", &dir);
        settings.tabs[0].locked = Some(true);

        let result = tree_set_last_viewed_impl(&settings, &sessions, &view_store, &registry, "tab-1", Some("Diary".to_string()));
        match result {
            AppResult::Err(err) => assert_eq!(err.code, ErrorCode::TabLocked),
            AppResult::Ok(()) => panic!("expected TabLocked for a locked tab with no active session"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    // -----------------------------------------------------------------------
    // memo_move_cross_tab / stale last_viewed_memo clearing (issue #239)
    // -----------------------------------------------------------------------

    #[test]
    fn memo_move_cross_tab_clears_source_last_viewed_memo_when_the_moved_memo_was_it() {
        let source_dir = tempdir();
        let target_dir = tempdir();
        std::fs::write(source_dir.join("Diary.md"), b"").unwrap();
        let registry = SelfWriteRegistry::new();
        let search_index = SearchIndex::new();

        let loaded = load_and_reconcile(&source_dir);
        save_tab_meta(&source_dir, loaded.schema_version, loaded.app_version, &loaded.tree, Some("Diary".to_string()), &registry).unwrap();

        let result = memo_move_cross_tab(
            "tab-1",
            &source_dir,
            "Source",
            "tab-2",
            &target_dir,
            "Target",
            &["Diary".to_string()],
            None,
            0,
            false,
            &registry,
            &search_index,
        );
        let AppResult::Ok(result) = result else { panic!("expected Ok") };
        assert_eq!(result.source.last_viewed_memo, None, "the returned source snapshot must reflect the cleared pointer");

        let (parsed, _recovery) = load_tab_meta_with_recovery(&source_dir);
        assert_eq!(parsed.unwrap().last_viewed_memo, None, "the persisted source tab metadata must no longer point at the moved-away memo");

        std::fs::remove_dir_all(&source_dir).ok();
        std::fs::remove_dir_all(&target_dir).ok();
    }

    // -----------------------------------------------------------------------
    // GitHub issue #263: image folder delete / rename-collision warning
    // -----------------------------------------------------------------------

    fn make_memo_with_image_folder(dir: &Path, name: &str) {
        std::fs::write(dir.join(format!("{name}.md")), format!("![図]({name}/画像-1.png)")).unwrap();
        std::fs::create_dir_all(dir.join(name)).unwrap();
        std::fs::write(dir.join(name).join("画像-1.png"), b"png").unwrap();
    }

    #[test]
    fn delete_memo_files_force_direct_removes_md_and_image_folder() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        make_memo_with_image_folder(&dir, "日記");
        let result = delete_memo_files(&dir, &["日記".to_string()], true, &registry);
        assert_eq!(result, Ok(None));
        assert!(!dir.join("日記.md").exists());
        assert!(!dir.join("日記").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_memo_files_without_image_folder_is_not_an_error() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        std::fs::write(dir.join("メモ.md"), b"").unwrap();
        let result = delete_memo_files(&dir, &["メモ".to_string()], true, &registry);
        assert_eq!(result, Ok(None));
        assert!(!dir.join("メモ.md").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_memo_files_subtree_removes_each_memos_image_folder() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        make_memo_with_image_folder(&dir, "親メモ");
        make_memo_with_image_folder(&dir, "子メモ");
        std::fs::write(dir.join("孫メモ.md"), b"").unwrap();
        make_memo_with_image_folder(&dir, "無関係");
        // `memo_delete` が `collect_names` で集めるサブツリー全体と同じ形
        let parent = node_with_children("親メモ", vec![node_with_children("子メモ", vec![leaf("孫メモ")])]);
        let mut names = Vec::new();
        collect_names(&parent, &mut names);
        let result = delete_memo_files(&dir, &names, true, &registry);
        assert_eq!(result, Ok(None));
        for n in ["親メモ", "子メモ", "孫メモ"] {
            assert!(!dir.join(format!("{n}.md")).exists(), "{n}.md should be gone");
            assert!(!dir.join(n).exists(), "{n}/ should be gone");
        }
        assert!(dir.join("無関係.md").exists());
        assert!(dir.join("無関係").join("画像-1.png").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn follow_rename_image_folder_into_existing_folder_returns_warning() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        make_memo_with_image_folder(&dir, "旧名");
        // 以前削除されたメモの取り残しフォルダが新名を占有している状況
        std::fs::create_dir_all(dir.join("新名")).unwrap();
        std::fs::rename(dir.join("旧名.md"), dir.join("新名.md")).unwrap();
        let warning = follow_rename_image_folder(&dir, "旧名", "新名", &dir.join("新名.md"), &registry);
        assert_eq!(warning, Some(MemoMutationWarning::ImageFolderRenameFailed));
        // 画像は旧名フォルダに残り、本文リンクも書き換えない
        assert!(dir.join("旧名").join("画像-1.png").exists());
        assert_eq!(std::fs::read_to_string(dir.join("新名.md")).unwrap(), "![図](旧名/画像-1.png)");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn follow_rename_image_folder_success_returns_no_warning() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        make_memo_with_image_folder(&dir, "旧名");
        std::fs::rename(dir.join("旧名.md"), dir.join("新名.md")).unwrap();
        let warning = follow_rename_image_folder(&dir, "旧名", "新名", &dir.join("新名.md"), &registry);
        assert_eq!(warning, None);
        assert!(dir.join("新名").join("画像-1.png").exists());
        assert!(!dir.join("旧名").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_memo_files_dot_stems_never_touch_tab_folder_or_parent() {
        // `..md` / `...md` の stem は `.` / `..`。タブフォルダ自身や親フォルダを削除してはならない
        let parent = tempdir();
        let dir = parent.join("タブ");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(parent.join("外側.txt"), b"keep").unwrap();
        make_memo_with_image_folder(&dir, "日記");
        let registry = SelfWriteRegistry::new();
        let names = [".".to_string(), "..".to_string(), "a.".to_string()];
        let result = delete_memo_files(&dir, &names, true, &registry);
        assert_eq!(result, Ok(None));
        assert!(dir.is_dir());
        assert!(parent.join("外側.txt").exists());
        assert!(dir.join("日記").join("画像-1.png").exists());
        std::fs::remove_dir_all(&parent).ok();
    }

    #[test]
    fn follow_rename_image_folder_dot_names_never_touch_outside() {
        let parent = tempdir();
        let dir = parent.join("タブ");
        std::fs::create_dir_all(&dir).unwrap();
        make_memo_with_image_folder(&dir, "日記");
        let registry = SelfWriteRegistry::new();
        // 旧名が `..` → 対象フォルダなし扱い(親フォルダを移動しない)
        assert_eq!(follow_rename_image_folder(&dir, "..", "新名", &dir.join("新名.md"), &registry), None);
        assert!(!dir.join("新名").exists());
        assert!(dir.is_dir());
        // 新名が `..` → 警告、旧フォルダはそのまま
        assert_eq!(
            follow_rename_image_folder(&dir, "日記", "..", &dir.join("日記.md"), &registry),
            Some(MemoMutationWarning::ImageFolderRenameFailed)
        );
        assert!(dir.join("日記").join("画像-1.png").exists());
        std::fs::remove_dir_all(&parent).ok();
    }

    // -----------------------------------------------------------------------
    // GitHub issue #262: plain -> plain cross-tab move/copy carries image folders
    // -----------------------------------------------------------------------

    /// 親「日記　その1」(子「子メモ」)、両方に画像フォルダ。
    fn t262_plain_source(dir: &Path, registry: &SelfWriteRegistry) {
        make_memo_with_image_folder(dir, "日記　その1");
        make_memo_with_image_folder(dir, "子メモ");
        let mut parent = MetaTreeNode::default_for_new_file("日記　その1".to_string());
        parent.children.push(MetaTreeNode::default_for_new_file("子メモ".to_string()));
        save_tab_meta(dir, TAB_META_SCHEMA_VERSION, "test".to_string(), &[parent], None, registry).unwrap();
    }

    fn t262_move(source_dir: &Path, target_dir: &Path, copy: bool, registry: &SelfWriteRegistry) -> AppResult<MemoMoveResult> {
        memo_move_cross_tab("tab-1", source_dir, "元", "tab-2", target_dir, "先", &["日記　その1".to_string()], None, 0, copy, registry, &SearchIndex::new())
    }

    #[test]
    fn memo_move_cross_tab_plain_to_plain_carries_image_folders() {
        for copy in [false, true] {
            let (source_dir, target_dir) = (tempdir(), tempdir());
            let registry = SelfWriteRegistry::new();
            t262_plain_source(&source_dir, &registry);
            assert!(matches!(t262_move(&source_dir, &target_dir, copy, &registry), AppResult::Ok(_)), "copy={copy}");
            for name in ["日記　その1", "子メモ"] {
                // 名前が変わらないので本文リンクはそのまま
                assert_eq!(std::fs::read_to_string(target_dir.join(format!("{name}.md"))).unwrap(), format!("![図]({name}/画像-1.png)"));
                assert_eq!(std::fs::read(target_dir.join(name).join("画像-1.png")).unwrap(), b"png");
                assert_eq!(source_dir.join(name).join("画像-1.png").exists(), copy, "copy={copy}");
                assert_eq!(source_dir.join(format!("{name}.md")).exists(), copy);
            }
            std::fs::remove_dir_all(&source_dir).ok();
            std::fs::remove_dir_all(&target_dir).ok();
        }
    }

    #[test]
    fn memo_move_cross_tab_plain_to_plain_rejects_an_image_folder_collision_without_partial_changes() {
        for copy in [false, true] {
            let (source_dir, target_dir) = (tempdir(), tempdir());
            let registry = SelfWriteRegistry::new();
            t262_plain_source(&source_dir, &registry);
            // 子メモ側だけ衝突(親のフォルダも動いてはならない)
            std::fs::create_dir_all(target_dir.join("子メモ")).unwrap();
            let AppResult::Err(err) = t262_move(&source_dir, &target_dir, copy, &registry) else { panic!("expected rejection") };
            assert_eq!(err.code, ErrorCode::NameDuplicate);
            for name in ["日記　その1", "子メモ"] {
                assert!(source_dir.join(format!("{name}.md")).exists());
                assert!(source_dir.join(name).join("画像-1.png").exists());
                assert!(!target_dir.join(format!("{name}.md")).exists());
            }
            assert!(!target_dir.join("日記　その1").exists());
            assert_eq!(std::fs::read_dir(target_dir.join("子メモ")).unwrap().count(), 0);
            assert_eq!(load_and_reconcile(&source_dir).tree.len(), 1, "source tree unchanged");
            std::fs::remove_dir_all(&source_dir).ok();
            std::fs::remove_dir_all(&target_dir).ok();
        }
    }

    // -----------------------------------------------------------------------
    // GitHub issue #282: `<メモ名>.md.bak` が改名/タブ間移動に追従する
    // -----------------------------------------------------------------------

    #[test]
    fn carry_memo_bak_renames_the_backup_with_the_memo() {
        let dir = tempdir();
        let registry = SelfWriteRegistry::new();
        std::fs::write(dir.join("メモ　一.md.bak"), b"old body").unwrap();
        carry_memo_bak(&dir, "メモ　一", &dir, "メモ　二", &registry);
        assert!(!dir.join("メモ　一.md.bak").exists());
        assert_eq!(std::fs::read(dir.join("メモ　二.md.bak")).unwrap(), b"old body");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn memo_move_cross_tab_plain_to_plain_carries_bak_only_on_move() {
        for copy in [false, true] {
            let (source_dir, target_dir) = (tempdir(), tempdir());
            let registry = SelfWriteRegistry::new();
            t262_plain_source(&source_dir, &registry);
            for name in ["日記　その1", "子メモ"] {
                std::fs::write(source_dir.join(format!("{name}.md.bak")), format!("旧版 {name}")).unwrap();
            }
            assert!(matches!(t262_move(&source_dir, &target_dir, copy, &registry), AppResult::Ok(_)), "copy={copy}");
            for name in ["日記　その1", "子メモ"] {
                // 移動なら移動先へ運ばれ、コピーなら移動元にそのまま残り移動先には作らない
                assert_eq!(source_dir.join(format!("{name}.md.bak")).exists(), copy, "copy={copy}");
                assert_eq!(target_dir.join(format!("{name}.md.bak")).exists(), !copy, "copy={copy}");
                if !copy {
                    assert_eq!(std::fs::read_to_string(target_dir.join(format!("{name}.md.bak"))).unwrap(), format!("旧版 {name}"));
                }
            }
            std::fs::remove_dir_all(&source_dir).ok();
            std::fs::remove_dir_all(&target_dir).ok();
        }
    }

    #[test]
    fn carry_memo_bak_never_overwrites_an_existing_destination_backup() {
        let (source_dir, target_dir) = (tempdir(), tempdir());
        let registry = SelfWriteRegistry::new();
        std::fs::write(source_dir.join("日記　その1.md.bak"), b"source").unwrap();
        std::fs::write(target_dir.join("日記　その1.md.bak"), b"destination").unwrap();
        carry_memo_bak(&source_dir, "日記　その1", &target_dir, "日記　その1", &registry);
        assert!(!source_dir.join("日記　その1.md.bak").exists(), "source backup is removed instead");
        assert_eq!(std::fs::read(target_dir.join("日記　その1.md.bak")).unwrap(), b"destination");
        std::fs::remove_dir_all(&source_dir).ok();
        std::fs::remove_dir_all(&target_dir).ok();
    }

    #[test]
    fn carry_and_remove_memo_bak_are_no_ops_without_a_backup() {
        let (source_dir, target_dir) = (tempdir(), tempdir());
        let registry = SelfWriteRegistry::new();
        std::fs::write(source_dir.join("日記　その1.md"), b"body").unwrap();
        carry_memo_bak(&source_dir, "日記　その1", &target_dir, "日記　その2", &registry);
        remove_memo_bak(&source_dir, "日記　その1", &registry);
        assert!(source_dir.join("日記　その1.md").exists());
        assert_eq!(std::fs::read_dir(&target_dir).unwrap().count(), 0);
        std::fs::remove_dir_all(&source_dir).ok();
        std::fs::remove_dir_all(&target_dir).ok();
    }
}
