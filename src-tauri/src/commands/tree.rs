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

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::commands::{emit_meta_recovered, emit_settings_recovery_event};
use crate::fs::atomic_write::{atomic_write, bak_path};
use crate::fs::path_safety::{validate_name, ValidationMode};
use crate::fs::retry::{is_lock_class_error, with_retry};
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::fs::trash;
use crate::model::app_settings::AppSettings;
use crate::model::result::{AppErrorPayload, AppResult, ErrorCode};
use crate::model::tab_meta::{TabMetaFile, TAB_META_FILE_NAME, TAB_META_SCHEMA_VERSION};
use crate::model::tree::{Label, LabelColor, MemoEditorSettings, MetaTreeNode, NodePath};
use crate::settings::app_settings_store::{AppSettingsStore, RecoveryOutcome};

/// Crash-mid-lock recovery marker (`tab-lock/PLAN.md`, `tab-lock/CRASH-RECOVERY.md`). This file
/// only detects its presence (step 0 of `tree_open`, `memo-tree/PLAN.md` §2) — the marker's
/// content and the actual Finish/Restore recovery flow belong to `tab-lock/PLAN.md`'s own
/// implementation pass.
const LOCK_INPROGRESS_MARKER_NAME: &str = ".tabtreenote.lock-inprogress";

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
    /// `true` when a crash-mid-lock marker was found (`tab-lock/CRASH-RECOVERY.md`) — the
    /// frontend should show that feature's recovery dialog instead of the normal tree view.
    pub recovery_pending: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoProperties {
    pub file_size_bytes: u64,
    pub created: String,
    pub modified: String,
    pub char_count: usize,
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
pub fn tree_open(app: AppHandle, tab_id: String, store: State<AppSettingsStore>, registry: State<SelfWriteRegistry>) -> AppResult<TreeSnapshot> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    let Some(entry) = settings.tabs.iter().find(|t| t.id == tab_id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };
    let folder = PathBuf::from(&entry.folder_path);
    let tab_name = entry.name.clone();
    let cached_locked = entry.locked.unwrap_or(false);

    // Step 0 (`tab-lock/CRASH-RECOVERY.md`): crash-mid-lock marker check,
    // before anything else. The actual recovery flow is `tab-lock/PLAN.md`'s
    // own implementation; this is only the detection seam.
    if folder.join(LOCK_INPROGRESS_MARKER_NAME).exists() {
        return AppResult::ok(TreeSnapshot {
            nodes: vec![],
            tab_name,
            locked: false,
            recovery_pending: true,
        });
    }

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
            }
            Ok(())
        });
    }

    // Step 1: locked tab, no active session. `tab-lock/PLAN.md` is not yet
    // implemented, so there is never an active session today — this branch
    // is always taken for a locked tab.
    if really_locked {
        return AppResult::ok(TreeSnapshot {
            nodes: vec![],
            tab_name,
            locked: true,
            recovery_pending: false,
        });
    }

    let persisted_tree = parsed.and_then(|m| m.tree).unwrap_or_default();
    let (reconciled, _changed) = reconcile_against_disk(&folder, persisted_tree);

    if recovery_outcome != RecoveryOutcome::Clean {
        emit_meta_recovered(&app, "tab", Some(tab_id));
    }

    AppResult::ok(TreeSnapshot {
        nodes: reconciled,
        tab_name,
        locked: false,
        recovery_pending: false,
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
    registry: State<SelfWriteRegistry>,
) -> AppResult<TreeSnapshot> {
    memo_create_impl(&app, &tab_id, &name, parent.as_deref(), &store, &registry)
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
    registry: State<SelfWriteRegistry>,
) -> AppResult<TreeSnapshot> {
    memo_create_impl(&app, &tab_id, &name, Some(parent.as_str()), &store, &registry)
}

fn memo_create_impl(
    app: &AppHandle,
    tab_id: &str,
    name: &str,
    parent: Option<&str>,
    store: &AppSettingsStore,
    registry: &SelfWriteRegistry,
) -> AppResult<TreeSnapshot> {
    let (settings, outcome) = store.get(registry);
    emit_settings_recovery_event(app, outcome);
    let (folder, tab_name) = match resolve_tab(&settings, tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };

    let trimmed = name.trim();
    if let Err(code) = validate_name(trimmed, ValidationMode::Strict) {
        return AppResult::err_code(code, "invalid memo name");
    }

    let mut loaded = load_and_reconcile(&folder);
    if contains_name(&loaded.tree, trimmed) {
        return AppResult::err_code(ErrorCode::NameDuplicate, "a memo with this name already exists in this tab");
    }
    if let Some(p) = parent {
        if find_node(&loaded.tree, p).is_none() {
            return AppResult::err_code(ErrorCode::NotFound, "parent memo not found");
        }
    }

    let file_path = folder.join(format!("{trimmed}.md"));
    if let Err(err) = atomic_write(&file_path, b"", registry) {
        tracing::error!(error = %err, path = %file_path.display(), "memo_create: failed to create memo file");
        return AppResult::err_code(ErrorCode::Unknown, "failed to create memo file");
    }

    let node = MetaTreeNode::default_for_new_file(trimmed.to_string());
    if let Err(code) = insert_node_at_end(&mut loaded.tree, parent, node) {
        return AppResult::err_code(code, "failed to insert memo into tree");
    }

    if let Err(err) = save_tab_meta(&folder, loaded.schema_version, loaded.app_version, &loaded.tree, registry) {
        tracing::error!(error = %err, "memo_create: failed to save tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
    }

    touch_search_index_upsert(tab_id, trimmed, "");

    AppResult::ok(TreeSnapshot {
        nodes: loaded.tree,
        tab_name,
        locked: false,
        recovery_pending: false,
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
    registry: State<SelfWriteRegistry>,
) -> AppResult<TreeSnapshot> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let (folder, tab_name) = match resolve_tab(&settings, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };

    let trimmed = new_name.trim();
    if let Err(code) = validate_name(trimmed, ValidationMode::Strict) {
        return AppResult::err_code(code, "invalid memo name");
    }

    let mut loaded = load_and_reconcile(&folder);
    if find_node(&loaded.tree, &path).is_none() {
        return AppResult::err_code(ErrorCode::NotFound, "memo not found");
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

    if let Some(node) = find_node_mut(&mut loaded.tree, &path) {
        node.memo_name = trimmed.to_string();
    }

    if let Err(err) = save_tab_meta(&folder, loaded.schema_version, loaded.app_version, &loaded.tree, &registry) {
        tracing::error!(error = %err, "memo_rename: failed to save tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
    }

    // `[spec §3.1]`: any `memo:` link pointing at the old name now breaks —
    // known/accepted consequence, no automatic rewrite (out of scope).
    touch_search_index_remove(&tab_id, &path);
    touch_search_index_upsert(&tab_id, trimmed, "");

    AppResult::ok(TreeSnapshot {
        nodes: loaded.tree,
        tab_name,
        locked: false,
        recovery_pending: false,
    })
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
    registry: State<SelfWriteRegistry>,
) -> AppResult<TreeSnapshot> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let (folder, tab_name) = match resolve_tab(&settings, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
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

    // Sync order (`[detailed-spec §6.3]`): file operation first. On any
    // failure below, the tree/metadata are left untouched — any files
    // already deleted in this loop are self-healing via the next
    // `tree_open` reconciliation (a metadata entry with no matching file is
    // simply dropped, `[detailed-spec §3.5]`), and this call is safe to
    // retry (already-gone files are skipped, not re-attempted).
    for name in &all_names {
        let file_path = folder.join(format!("{name}.md"));
        if !file_path.exists() {
            continue;
        }
        let result = if force_direct {
            trash::delete_directly(&file_path, &registry).map_err(|_| ErrorCode::IoLocked)
        } else {
            trash::trash_file(&file_path, &registry)
        };
        if let Err(code) = result {
            tracing::error!(?code, path = %file_path.display(), "memo_delete: failed to delete memo file");
            return AppResult::err_code(code, "failed to delete one or more memo files");
        }
    }

    for name in &all_names {
        remove_node(&mut loaded.tree, name);
        touch_search_index_remove(&tab_id, name);
    }

    if let Err(err) = save_tab_meta(&folder, loaded.schema_version, loaded.app_version, &loaded.tree, &registry) {
        tracing::error!(error = %err, "memo_delete: failed to save tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
    }

    AppResult::ok(TreeSnapshot {
        nodes: loaded.tree,
        tab_name,
        locked: false,
        recovery_pending: false,
    })
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
    registry: State<SelfWriteRegistry>,
) -> AppResult<MemoMoveResult> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let (source_folder, source_tab_name) = match resolve_tab(&settings, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };

    match target {
        MoveTarget::WithinTab { new_parent, index } => {
            memo_move_within_tab(&source_folder, &source_tab_name, &paths, new_parent.as_deref(), index, &registry)
        }
        MoveTarget::OtherTab {
            target_tab_id,
            new_parent,
            index,
            copy,
        } => {
            // Reusing `resolve_tab` here is also this command's seam for
            // `tab-lock/PLAN.md` §"Cross-Tab DND": dropping onto a locked
            // (no-session) target tab rejects with `TabLocked` here, before
            // anything is touched, letting the frontend trigger the
            // password-at-drop-time flow that plan specifies. Once
            // `tab-lock/PLAN.md` introduces session state, `resolve_tab`
            // (or its own equivalent check) is where that distinction needs
            // to be threaded through.
            let (target_folder, target_tab_name) = match resolve_tab(&settings, &target_tab_id) {
                Ok(v) => v,
                Err(e) => return AppResult::err(e),
            };
            memo_move_cross_tab(
                &tab_id,
                &source_folder,
                &source_tab_name,
                &target_tab_id,
                &target_folder,
                &target_tab_name,
                &paths,
                new_parent.as_deref(),
                index,
                copy,
                &registry,
            )
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

    if let Err(err) = save_tab_meta(folder, loaded.schema_version, loaded.app_version, &loaded.tree, registry) {
        tracing::error!(error = %err, "memo_move: failed to save tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
    }

    AppResult::ok(MemoMoveResult {
        source: TreeSnapshot {
            nodes: loaded.tree,
            tab_name: tab_name.to_string(),
            locked: false,
            recovery_pending: false,
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

    for name in &all_names {
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
            return AppResult::err_code(ErrorCode::Unknown, "failed to move memo file between tabs");
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
        touch_search_index_upsert(target_tab_id, name, "");
        if !copy {
            touch_search_index_remove(source_tab_id, name);
        }
    }

    if let Err(err) = save_tab_meta(target_folder, target_loaded.schema_version, target_loaded.app_version, &target_loaded.tree, registry) {
        tracing::error!(error = %err, "memo_move: failed to save target tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save target tab metadata");
    }
    if !copy {
        if let Err(err) = save_tab_meta(source_folder, source_loaded.schema_version, source_loaded.app_version, &source_loaded.tree, registry) {
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
        },
        target: Some(TreeSnapshot {
            nodes: target_loaded.tree,
            tab_name: target_tab_name.to_string(),
            locked: false,
            recovery_pending: false,
        }),
    })
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
    registry: State<SelfWriteRegistry>,
) -> AppResult<TreeSnapshot> {
    mutate_node_field(&app, &tab_id, &path, &store, &registry, |node| {
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
    registry: State<SelfWriteRegistry>,
) -> AppResult<TreeSnapshot> {
    mutate_node_field(&app, &tab_id, &path, &store, &registry, |node| {
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
    registry: State<SelfWriteRegistry>,
) -> AppResult<TreeSnapshot> {
    mutate_node_field(&app, &tab_id, &path, &store, &registry, |node| {
        node.editor = editor;
        Ok(())
    })
}

/// Toggles the per-memo **edit lock** (`locked?: boolean`) — body-edit-only block, no password,
/// distinct from tab lock (`[spec §2.2]`). This command only persists the flag; refusing edits
/// when set is `editor-save/PLAN.md`'s responsibility.
#[tauri::command]
pub fn memo_set_edit_lock(
    app: AppHandle,
    tab_id: String,
    path: NodePath,
    locked: bool,
    store: State<AppSettingsStore>,
    registry: State<SelfWriteRegistry>,
) -> AppResult<TreeSnapshot> {
    mutate_node_field(&app, &tab_id, &path, &store, &registry, |node| {
        node.locked = Some(locked);
        Ok(())
    })
}

fn mutate_node_field(
    app: &AppHandle,
    tab_id: &str,
    path: &str,
    store: &AppSettingsStore,
    registry: &SelfWriteRegistry,
    f: impl FnOnce(&mut MetaTreeNode) -> Result<(), ErrorCode>,
) -> AppResult<TreeSnapshot> {
    let (settings, outcome) = store.get(registry);
    emit_settings_recovery_event(app, outcome);
    let (folder, tab_name) = match resolve_tab(&settings, tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };

    let mut loaded = load_and_reconcile(&folder);
    let Some(node) = find_node_mut(&mut loaded.tree, path) else {
        return AppResult::err_code(ErrorCode::NotFound, "memo not found");
    };
    if let Err(code) = f(node) {
        return AppResult::err_code(code, "failed to update memo");
    }

    if let Err(err) = save_tab_meta(&folder, loaded.schema_version, loaded.app_version, &loaded.tree, registry) {
        tracing::error!(error = %err, "failed to save tab metadata");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
    }

    AppResult::ok(TreeSnapshot {
        nodes: loaded.tree,
        tab_name,
        locked: false,
        recovery_pending: false,
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
    registry: State<SelfWriteRegistry>,
) -> AppResult<MemoProperties> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let (folder, _tab_name) = match resolve_tab(&settings, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
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

    let char_count = decode_memo_body(&bytes).chars().count();
    let created = metadata.created().ok().map(format_system_time_utc).unwrap_or_default();
    let modified = metadata.modified().ok().map(format_system_time_utc).unwrap_or_default();

    AppResult::ok(MemoProperties {
        file_size_bytes: metadata.len(),
        created,
        modified,
        char_count,
    })
}

/// Encoding detection (`[detailed-spec §5.3]`): delegates to the shared
/// `store::memo_file_service` decoder (`editor-save/PLAN.md`), which `commands/editor.rs`'s
/// `editor_load`/`editor_save` also use — only the decoded text is needed here, not the
/// detected `Encoding` itself.
fn decode_memo_body(bytes: &[u8]) -> String {
    crate::store::memo_file_service::decode_memo_body(bytes).0
}

/// Formats a `SystemTime` as a UTC `"YYYY-MM-DD HH:MM:SS"` string, without pulling in a
/// date/time crate for this single formatting need.
fn format_system_time_utc(t: std::time::SystemTime) -> String {
    let duration = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let total_secs = duration.as_secs();
    let days = (total_secs / 86400) as i64;
    let secs_of_day = total_secs % 86400;
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

/// Howard Hinnant's `civil_from_days` algorithm (public domain), converting a day count since
/// the Unix epoch (1970-01-01) into a proleptic-Gregorian `(year, month, day)`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

// ---------------------------------------------------------------------------
// Search-index integration seam
// ---------------------------------------------------------------------------

/// Seam for `search-index/PLAN.md` (not yet implemented): every mutation above that changes
/// indexable content (create/rename/delete/move/copy) calls one of these two narrow functions
/// instead of touching a search index directly, so wiring in the real index later is a small,
/// localized change rather than a re-audit of every command in this file.
fn touch_search_index_upsert(_tab_id: &str, _memo_name: &str, _plaintext: &str) {
    // TODO(search-index): call `SearchIndex::upsert(tab_id, memo_name, plaintext)` once
    // `search/index.rs` exists (`search-index/PLAN.md` §2, §4).
    //
    // NOTE: `memo_create`'s call site passes `""` legitimately (a brand-new file has no
    // content yet). `memo_rename`/`memo_move`'s call sites also currently pass `""` as a
    // placeholder since this function is a no-op stub — once a real index is wired in, those
    // two call sites must be updated to pass the memo's actual file content instead.
}

fn touch_search_index_remove(_tab_id: &str, _memo_name: &str) {
    // TODO(search-index): call `SearchIndex::remove(tab_id, memo_name)` once `search/index.rs`
    // exists (`search-index/PLAN.md` §2).
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
}

fn load_tab_meta_with_recovery(folder: &Path) -> (Option<TabMetaFile>, RecoveryOutcome) {
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
    let (schema_version, app_version, persisted_tree) = match parsed {
        Some(meta) => (meta.schema_version, meta.app_version, meta.tree.unwrap_or_default()),
        None => (TAB_META_SCHEMA_VERSION, env!("CARGO_PKG_VERSION").to_string(), Vec::new()),
    };
    let (tree, _changed) = reconcile_against_disk(folder, persisted_tree);
    LoadedTabMeta { schema_version, app_version, tree }
}

fn save_tab_meta(folder: &Path, schema_version: u32, app_version: String, tree: &[MetaTreeNode], registry: &SelfWriteRegistry) -> std::io::Result<()> {
    let meta = TabMetaFile {
        schema_version,
        app_version,
        tree: Some(tree.to_vec()),
        lock: None,
    };
    let json = serde_json::to_vec_pretty(&meta)?;
    atomic_write(&folder.join(TAB_META_FILE_NAME), &json, registry)
}

/// Direct-child `.md` files of `folder`, by `memoName` (extension stripped). A missing folder
/// yields an empty list, not an error (`[detailed-spec §6.5]`). Symbolic links/junctions are not
/// followed (`[spec §2.2]`).
fn scan_real_memo_names(folder: &Path) -> Vec<String> {
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

fn find_node_mut<'a>(tree: &'a mut Vec<MetaTreeNode>, name: &str) -> Option<&'a mut MetaTreeNode> {
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
/// (`commands/tab.rs`).
fn contains_name(tree: &[MetaTreeNode], name: &str) -> bool {
    tree.iter().any(|n| n.memo_name.eq_ignore_ascii_case(name) || contains_name(&n.children, name))
}

fn remove_node(tree: &mut Vec<MetaTreeNode>, name: &str) -> Option<MetaTreeNode> {
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

fn collect_names(node: &MetaTreeNode, out: &mut Vec<String>) {
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

fn insert_node_at(tree: &mut Vec<MetaTreeNode>, parent: Option<&str>, index: usize, node: MetaTreeNode) -> Result<(), ErrorCode> {
    let children = children_of_mut(tree, parent)?;
    let idx = index.min(children.len());
    children.insert(idx, node);
    Ok(())
}

fn insert_node_at_end(tree: &mut Vec<MetaTreeNode>, parent: Option<&str>, node: MetaTreeNode) -> Result<(), ErrorCode> {
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
    fn civil_from_days_epoch_and_known_date() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-08-01 is 20666 days after the epoch.
        assert_eq!(civil_from_days(20666), (2026, 8, 1));
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
}
