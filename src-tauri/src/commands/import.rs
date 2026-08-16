//! OS-folder -> tree-pane drag-and-drop import — `docs/plans/v1.1.0/folder-import/PLAN.md` §3.
//!
//! Mirrors `commands/tree.rs`'s `memo_create_impl` closely: content comes from `source_path`
//! instead of being empty, and an `overwrite` branch exists for the collision-resolved "Overwrite"
//! choice. Collision detection/resolution itself is entirely client-side (`state/useImport.ts`) —
//! this command only ever executes one already-resolved item per call, sequentially, one call per
//! dropped file (`PLAN.md` §2's "client-orchestrated collision loop, backend executes one resolved
//! item at a time" design).

use tauri::{AppHandle, State};

use crate::commands::emit_settings_recovery_event;
use crate::commands::tab_lock::{self, TabAccess};
use crate::commands::tree::{contains_name, find_node, insert_node_at_end, load_and_reconcile, save_tab_meta, TreeSnapshot};
use crate::fs::atomic_write::atomic_write;
use crate::fs::retry::with_retry;
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::EolPolicy;
use crate::model::result::{AppResult, ErrorCode};
use crate::model::tree::{MetaTreeNode, NodePath};
use crate::search::index::SearchIndex;
use crate::settings::app_settings_store::AppSettingsStore;
use crate::store::lock_session_store::LockSessionStore;
use crate::store::memo_file_service::{decode_memo_body, detect_eol, encode_for_save, normalize_to_lf, restore_eol, Eol};

// ---------------------------------------------------------------------------
// import_file
// ---------------------------------------------------------------------------

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn import_file(
    app: AppHandle,
    tab_id: String,
    parent: Option<NodePath>,
    source_path: String,
    target_name: String,
    overwrite: bool,
    store: State<AppSettingsStore>,
    registry: State<SelfWriteRegistry>,
    sessions: State<LockSessionStore>,
    search_index: State<SearchIndex>,
) -> AppResult<TreeSnapshot> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    // Step 1 (`PLAN.md` §3): on `TabAccess::Session`, delegate to `tab_lock::session_import_file`
    // (`PLAN.md` §5); everything below this match is the `TabAccess::Plain` path.
    let access = match tab_lock::resolve_tab_access(&settings, &sessions, &tab_id) {
        Ok(v) => v,
        Err(e) => return AppResult::err(e),
    };
    let (folder, tab_name) = match access {
        TabAccess::Session { tab_id, folder } => {
            let (bytes_to_write, _normalized_text) = match read_and_prepare_bytes(&source_path, settings.eol.policy, &settings.encoding.default_save) {
                Ok(v) => v,
                Err(code) => return AppResult::err_code(code, "failed to read source file"),
            };
            return tab_lock::session_import_file(&sessions, &registry, &folder, &tab_id, &target_name, &bytes_to_write, overwrite, parent.as_deref());
        }
        TabAccess::Plain { folder, tab_name } => (folder, tab_name),
    };

    // Step 2: defensive — the frontend only ever passes a `parent` it just hit-tested against the
    // live tree.
    let mut loaded = load_and_reconcile(&folder);
    if let Some(p) = &parent {
        if find_node(&loaded.tree, p).is_none() {
            return AppResult::err_code(ErrorCode::NotFound, "parent memo not found");
        }
    }

    // Step 3: defensive re-check against a race window since the frontend's own collision check
    // (`state/useImport.ts`).
    if !overwrite {
        if contains_name(&loaded.tree, &target_name) {
            return AppResult::err_code(ErrorCode::NameDuplicate, "a memo with this name already exists in this tab");
        }
    } else {
        match find_node_ci(&loaded.tree, &target_name) {
            None => return AppResult::err_code(ErrorCode::NotFound, "memo to overwrite not found"),
            Some(existing) if existing.locked == Some(true) => {
                // `[detailed-spec v1.1.0 §2.1]` step 3d, `PLAN.md` §3 step 3: the frontend already
                // disables the Overwrite control for an edit-locked collision target — this is a
                // defensive fallback only, reusing `Unknown` per the same policy
                // `editor-save/PLAN.md` §2 step 1 already applies to its own analogous case (no
                // dedicated `ErrorCode` exists for this).
                return AppResult::err_code(ErrorCode::Unknown, "cannot overwrite an edit-locked memo");
            }
            Some(_) => {}
        }
    }

    // Steps 4-6: read the source file's bytes, decode, then restore this tab's target EOL style
    // (preserving the *source file's own* detected style under `EolPolicy::Preserve`) and
    // re-encode.
    let (bytes_to_write, normalized_text) = match read_and_prepare_bytes(&source_path, settings.eol.policy, &settings.encoding.default_save) {
        Ok(v) => v,
        Err(code) => return AppResult::err_code(code, "failed to read source file"),
    };

    // Step 7: used uniformly for both branches (including a brand-new file), matching what
    // `memo_create_impl` itself actually does today.
    let target_path = folder.join(format!("{target_name}.md"));
    registry.register(&target_path);
    if let Err(err) = atomic_write(&target_path, &bytes_to_write, &registry) {
        tracing::error!(error = %err, path = %target_path.display(), "import_file: failed to write memo file");
        return AppResult::err_code(ErrorCode::Unknown, "failed to import memo file");
    }

    // Steps 8-9: tree/metadata only touched for the "new file" (non-overwrite) branch.
    if !overwrite {
        let node = MetaTreeNode::default_for_new_file(target_name.clone());
        if let Err(code) = insert_node_at_end(&mut loaded.tree, parent.as_deref(), node) {
            return AppResult::err_code(code, "failed to insert memo into tree");
        }
        if let Err(err) = save_tab_meta(&folder, loaded.schema_version, loaded.app_version, &loaded.tree, &registry) {
            tracing::error!(error = %err, "import_file: failed to save tab metadata");
            return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
        }
    }

    // Step 10: called in both branches — the Overwrite case re-upserts a potentially-already-
    // favorited memo, so `SearchIndex::upsert` must preserve any existing `favorite` value (see
    // that fn's own doc comment once `docs/plans/v1.1.0/favorites/PLAN.md` extends `IndexEntry`).
    search_index.upsert(&tab_id, &target_name, &normalized_text);

    // Step 11.
    AppResult::ok(TreeSnapshot {
        nodes: loaded.tree,
        tab_name,
        locked: false,
        recovery_pending: false,
        recovery_kind: None,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Steps 4-6 (`PLAN.md` §3): read `source_path`'s bytes, decode, then restore this tab's target
/// EOL style and re-encode. Returns `(bytes_to_write, normalized_text)` — `normalized_text` is the
/// LF-normalized decoded text, reused as-is for `search_index.upsert`'s `raw_body` (step 10) so the
/// index isn't re-decoded from the just-written bytes a second time. Shared by both the
/// `TabAccess::Plain` and `TabAccess::Session` branches of `import_file` above (the session path's
/// own write step, `PLAN.md` §5, differs only in *where* these final bytes end up — encrypted into
/// a token file rather than written plain) — called after the `TabAccess::Plain` path's own
/// steps 2-3 (parent/collision checks) so a doomed-to-fail call never reads the source file
/// needlessly, matching `PLAN.md` §3's literal step order for that path.
fn read_and_prepare_bytes(source_path: &str, eol_policy: EolPolicy, default_save: &str) -> Result<(Vec<u8>, String), ErrorCode> {
    let raw_bytes = read_source_bytes(source_path)?;
    let (decoded, _encoding) = decode_memo_body(&raw_bytes);
    let source_eol = detect_eol(&decoded);
    let normalized_text = normalize_to_lf(&decoded);
    let output_eol = match eol_policy {
        EolPolicy::Preserve => source_eol,
        EolPolicy::Crlf => Eol::Crlf,
        EolPolicy::Lf => Eol::Lf,
        EolPolicy::Cr => Eol::Cr,
    };
    let restored = restore_eol(&normalized_text, output_eol);
    let bytes_to_write = encode_for_save(&restored, default_save);
    Ok((bytes_to_write, normalized_text))
}

/// Step 4: `fs::retry::with_retry(|| std::fs::read(&source_path))`. On failure after the existing
/// retry schedule (`persistence-fs/PLAN.md` §7) -> `IoLocked` (the frontend treats this as a skip,
/// tallied in the summary toast, batch continues — `[detailed-spec v1.1.0 §2.1]` step 6).
fn read_source_bytes(source_path: &str) -> Result<Vec<u8>, ErrorCode> {
    with_retry(|| std::fs::read(source_path)).map_err(|err| {
        tracing::error!(error = %err, path = %source_path, "import_file: failed to read source file");
        ErrorCode::IoLocked
    })
}

/// Case-insensitive node lookup for the Overwrite branch's collision target (`PLAN.md` §3 step 3)
/// — distinct from `commands/tree.rs::find_node`, which matches by exact case. Windows NTFS is
/// itself case-insensitive for file paths, so `target_name`'s casing (from the just-dropped
/// source file) need not match the existing node's own stored `memoName` casing for this lookup to
/// find it; the tree entry's own name/casing is left untouched either way (Overwrite never renames).
fn find_node_ci<'a>(tree: &'a [MetaTreeNode], name: &str) -> Option<&'a MetaTreeNode> {
    for node in tree {
        if node.memo_name.eq_ignore_ascii_case(name) {
            return Some(node);
        }
        if let Some(found) = find_node_ci(&node.children, name) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-import-cmd-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn find_node_ci_matches_regardless_of_case_and_recurses() {
        let tree = vec![MetaTreeNode {
            memo_name: "Parent".to_string(),
            expanded: true,
            label: None,
            editor: None,
            locked: Some(false),
            children: vec![MetaTreeNode::default_for_new_file("Child Memo".to_string())],
        }];
        assert!(find_node_ci(&tree, "parent").is_some());
        assert!(find_node_ci(&tree, "CHILD MEMO").is_some());
        assert!(find_node_ci(&tree, "missing").is_none());
    }

    #[test]
    fn read_and_prepare_bytes_preserves_source_eol_under_preserve_policy() {
        let dir = tempdir();
        let source = dir.join("Source.md");
        std::fs::write(&source, "line one\r\nline two\r\n").unwrap();

        let (bytes, normalized) = read_and_prepare_bytes(source.to_str().unwrap(), EolPolicy::Preserve, "utf-8").unwrap();
        assert_eq!(bytes, b"line one\r\nline two\r\n");
        assert_eq!(normalized, "line one\nline two\n");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_and_prepare_bytes_applies_forced_eol_policy_regardless_of_source() {
        let dir = tempdir();
        let source = dir.join("Source.md");
        std::fs::write(&source, "line one\r\nline two\r\n").unwrap();

        let (bytes, _normalized) = read_and_prepare_bytes(source.to_str().unwrap(), EolPolicy::Lf, "utf-8").unwrap();
        assert_eq!(bytes, b"line one\nline two\n");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_source_bytes_maps_missing_file_to_io_locked() {
        let dir = tempdir();
        let missing = dir.join("Nonexistent.md");
        let err = read_source_bytes(missing.to_str().unwrap()).unwrap_err();
        assert_eq!(err, ErrorCode::IoLocked);
        std::fs::remove_dir_all(&dir).ok();
    }
}
