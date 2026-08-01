//! Editor load/save commands — `docs/plans/editor-save/PLAN.md` §2.
//!
//! The Rust backend never holds a "live" in-memory copy of the currently-open memo's buffer
//! between calls — it only reads/writes the file on disk at `editor_load`/`editor_save` call
//! boundaries (`editor-save/PLAN.md` §1). CodeMirror 6 on the frontend is the single source of
//! truth for the buffer between those calls; undo/redo history is entirely a frontend concern.
//!
//! Both commands re-derive the memo's tree metadata (`MetaTreeNode`, for the edit-lock flag and
//! `MemoEditorSettings.scrollPosition`) via `commands::tree`'s `load_and_reconcile`/`find_node`
//! rather than duplicating tree-reconciliation logic — the same pattern every mutating
//! `commands/tree.rs` command already follows.

use tauri::{AppHandle, State};

use crate::commands::emit_settings_recovery_event;
use crate::commands::tree::{find_node, load_and_reconcile, resolve_tab};
use crate::fs::atomic_write::atomic_write;
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::EolPolicy;
use crate::model::result::{AppResult, ErrorCode};
use crate::model::tree::NodePath;
use crate::settings::app_settings_store::AppSettingsStore;
use crate::store::memo_file_service::{decode_memo_body, detect_eol, encode_for_save, hash_bytes, normalize_to_lf, restore_eol, Encoding, Eol};
use crate::watch::active_memo::{ActiveMemoKey, ActiveMemoTracker};

/// Return type of `editor_load` (`docs/plans/editor-save/PLAN.md` §2).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoContent {
    /// LF-normalized for the editor buffer.
    pub text: String,
    pub encoding: Encoding,
    pub eol: Eol,
    /// Hash of the on-disk byte sequence at load time (`[spec §5.5]` scope: whole file bytes).
    pub hash: String,
    /// This memo's per-memo Edit Lock flag, read from tree metadata.
    pub edit_locked: bool,
    /// `MemoEditorSettings.scrollPosition` — 1-indexed top-visible document line number, `None`
    /// if never recorded (open at the top).
    pub scroll_position: Option<u32>,
}

/// Return type of `editor_save` (`docs/plans/editor-save/PLAN.md` §2).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    /// Hash of the just-written bytes, so the frontend can update its `expected_hash` for the
    /// next save without a round-trip re-read.
    pub hash: String,
}

// ---------------------------------------------------------------------------
// editor_load
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn editor_load(
    app: AppHandle,
    tab_id: String,
    path: NodePath,
    store: State<AppSettingsStore>,
    registry: State<SelfWriteRegistry>,
    active_memo: State<ActiveMemoTracker>,
) -> AppResult<MemoContent> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let (folder, _tab_name) = match resolve_tab(&settings, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };

    let loaded = load_and_reconcile(&folder);
    let Some(node) = find_node(&loaded.tree, &path) else {
        return AppResult::err_code(ErrorCode::NotFound, "memo not found");
    };
    let edit_locked = node.locked.unwrap_or(false);
    let scroll_position = node.editor.as_ref().and_then(|e| e.scroll_position);

    let file_path = folder.join(format!("{path}.md"));
    let bytes = match std::fs::read(&file_path) {
        Ok(b) => b,
        Err(err) => {
            tracing::error!(error = %err, path = %file_path.display(), "editor_load: failed to read memo file");
            return AppResult::err_code(ErrorCode::NotFound, "failed to read memo file");
        }
    };

    let (decoded, encoding) = decode_memo_body(&bytes);
    let eol = detect_eol(&decoded);
    let text = normalize_to_lf(&decoded);
    let hash = hash_bytes(&bytes);

    // `docs/plans/persistence-fs/PLAN.md` §5: marks this memo as the one the (not-yet-wired-up)
    // folder watcher should raise `evt:editor:conflict` for, if an external change lands while
    // it's open.
    active_memo.set(ActiveMemoKey { tab_id, memo_name: path });

    AppResult::ok(MemoContent {
        text,
        encoding,
        eol,
        hash,
        edit_locked,
        scroll_position,
    })
}

// ---------------------------------------------------------------------------
// editor_save
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn editor_save(
    app: AppHandle,
    tab_id: String,
    path: NodePath,
    text: String,
    expected_hash: String,
    store: State<AppSettingsStore>,
    registry: State<SelfWriteRegistry>,
) -> AppResult<SaveResult> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let (folder, _tab_name) = match resolve_tab(&settings, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };

    let loaded = load_and_reconcile(&folder);
    let Some(node) = find_node(&loaded.tree, &path) else {
        return AppResult::err_code(ErrorCode::NotFound, "memo not found");
    };

    // Step 1 (`editor-save/PLAN.md` §2 `editor_save`, "Open Items"): the frontend already makes
    // the editor read-only for an edit-locked memo, so this defensive check is not expected to
    // be reachable in practice. No dedicated `ErrorCode` exists for this scenario in `[spec
    // §3.4]`'s enum, so `Unknown` is used here per the plan's own explicit resolution — if this
    // branch is ever found to be reachable in a way that matters to the user (not just a
    // defensive guard), that should be escalated to `designer-lite`, not reinterpreted here.
    if node.locked.unwrap_or(false) {
        return AppResult::err_code(ErrorCode::Unknown, "cannot save: this memo is edit-locked");
    }

    let file_path = folder.join(format!("{path}.md"));
    let on_disk = match std::fs::read(&file_path) {
        Ok(b) => b,
        Err(err) => {
            tracing::error!(error = %err, path = %file_path.display(), "editor_save: failed to read memo file for conflict check");
            return AppResult::err_code(ErrorCode::NotFound, "memo file no longer exists");
        }
    };

    // Step 2: hash-only conflict check against current on-disk bytes, no mtime/size
    // (`[spec §5.5]`).
    let current_hash = hash_bytes(&on_disk);
    if current_hash != expected_hash {
        return AppResult::err_code(ErrorCode::Conflict, "the memo was changed on disk since it was loaded");
    }

    // Step 3: restore the EOL style detected at load. Since the hash check above just proved
    // `on_disk` is byte-identical to what was loaded, its EOL style is re-derived from those same
    // bytes here rather than requiring a separate client-supplied `eol` parameter (`editor_save`'s
    // signature intentionally has none, per `editor-save/PLAN.md` §2).
    let (decoded_on_disk, _encoding) = decode_memo_body(&on_disk);
    let detected_eol = detect_eol(&decoded_on_disk);
    let output_eol = match settings.eol.policy {
        EolPolicy::Preserve => detected_eol,
        EolPolicy::Crlf => Eol::Crlf,
        EolPolicy::Lf => Eol::Lf,
        EolPolicy::Cr => Eol::Cr,
    };
    let restored = restore_eol(&text, output_eol);

    // Step 4: re-encode per `AppSettings.encoding.defaultSave` (always UTF-8 today, `[spec §3.4]`).
    let bytes_to_write = encode_for_save(&restored, &settings.encoding.default_save);

    // Step 5: atomic write.
    if let Err(err) = atomic_write(&file_path, &bytes_to_write, &registry) {
        tracing::error!(error = %err, path = %file_path.display(), "editor_save: failed to write memo file");
        return AppResult::err_code(ErrorCode::Unknown, "failed to save memo file");
    }

    // Step 6 (seam): patch the full-text index for this memo (`search-index/PLAN.md`, not yet
    // implemented).
    touch_search_index_upsert(&tab_id, &path, &restored);

    // Step 7: hash of the just-written bytes.
    let new_hash = hash_bytes(&bytes_to_write);

    // Step 8 (`tab-lock/PLAN.md`, not yet implemented): a locked tab's active-session
    // encrypt-before-write path would branch here instead of steps 3-5's plain path.
    // `resolve_tab` above already rejects every locked tab with `TabLocked` today (there is
    // never an active session yet), so no separate branch is needed until that plan lands.

    AppResult::ok(SaveResult { hash: new_hash })
}

// ---------------------------------------------------------------------------
// Search-index integration seam (`editor-save/PLAN.md` §2 step 6)
// ---------------------------------------------------------------------------

/// Mirrors `commands/tree.rs`'s own local `touch_search_index_upsert`/`_remove` stub pair
/// (kept local to each module rather than shared, matching this codebase's existing per-module
/// seam convention) — `editor_save` is the one call site that always has the real saved
/// plaintext on hand, so unlike `commands/tree.rs`'s `memo_rename`/`memo_move` call sites, no
/// placeholder `""` is involved here.
fn touch_search_index_upsert(_tab_id: &str, _memo_name: &str, _plaintext: &str) {
    // TODO(search-index): call `SearchIndex::upsert(tab_id, memo_name, plaintext)` once
    // `search/index.rs` exists (`search-index/PLAN.md` §2, §4).
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::self_write_registry::SelfWriteRegistry;
    use crate::model::app_settings::{AppSettings, TabSettingsEntry};
    use crate::model::tree::{MemoEditorSettings, MetaTreeNode};
    use std::path::PathBuf;

    // These tests exercise `resolve_tab`/`load_and_reconcile`/the encode-decode helpers
    // directly rather than the full `editor_load`/`editor_save` command functions, since the
    // latter need live `State<T>` extraction that isn't easily constructed outside a running
    // `tauri::App` — the same style already used by `commands/tree.rs`'s own unit tests.

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-editor-cmd-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_tab_and_load_and_reconcile_find_a_memo_with_editor_settings() {
        let dir = tempdir();
        std::fs::write(dir.join("Shopping list.md"), "buy milk\r\nbuy eggs\r\n").unwrap();

        let mut node = MetaTreeNode::default_for_new_file("Shopping list".to_string());
        node.locked = Some(false);
        node.editor = Some(MemoEditorSettings {
            scroll_position: Some(3),
            ..Default::default()
        });
        let meta = crate::model::tab_meta::TabMetaFile {
            schema_version: 1,
            app_version: "0.1.0".to_string(),
            tree: Some(vec![node]),
            lock: None,
        };
        std::fs::write(dir.join(".tabtreenote.json"), serde_json::to_vec(&meta).unwrap()).unwrap();

        let mut settings = AppSettings::default();
        settings.tabs.push(TabSettingsEntry {
            id: "t1".to_string(),
            name: "Test tab".to_string(),
            folder_path: dir.to_string_lossy().into_owned(),
            locked: Some(false),
        });

        let (folder, _tab_name) = resolve_tab(&settings, "t1").unwrap();
        let loaded = load_and_reconcile(&folder);
        let node = find_node(&loaded.tree, "Shopping list").unwrap();
        assert_eq!(node.locked, Some(false));
        assert_eq!(node.editor.as_ref().and_then(|e| e.scroll_position), Some(3));

        let bytes = std::fs::read(folder.join("Shopping list.md")).unwrap();
        let (decoded, encoding) = decode_memo_body(&bytes);
        assert_eq!(encoding, Encoding::Utf8);
        assert_eq!(detect_eol(&decoded), Eol::Crlf);
        assert_eq!(normalize_to_lf(&decoded), "buy milk\nbuy eggs\n");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn atomic_write_round_trip_matches_editor_save_pipeline() {
        let dir = tempdir();
        let target = dir.join("Note.md");
        std::fs::write(&target, "first\n").unwrap();
        let registry = SelfWriteRegistry::new();

        let original_bytes = std::fs::read(&target).unwrap();
        let expected_hash = hash_bytes(&original_bytes);
        assert_eq!(hash_bytes(&std::fs::read(&target).unwrap()), expected_hash);

        let new_text = "first\nsecond\n"; // LF-normalized editor buffer content
        let restored = restore_eol(new_text, Eol::Crlf);
        let bytes_to_write = encode_for_save(&restored, "utf-8");
        atomic_write(&target, &bytes_to_write, &registry).unwrap();

        let on_disk = std::fs::read(&target).unwrap();
        assert_eq!(on_disk, b"first\r\nsecond\r\n");
        assert_eq!(hash_bytes(&on_disk), hash_bytes(&bytes_to_write));

        std::fs::remove_dir_all(&dir).ok();
    }
}
