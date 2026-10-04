//! OS-folder -> tree-pane drag-and-drop import — `docs/plans/v1.1.0/folder-import/PLAN.md` §3.
//!
//! Mirrors `commands/tree.rs`'s `memo_create_impl` closely: content comes from `source_path`
//! instead of being empty, and an `overwrite` branch exists for the collision-resolved "Overwrite"
//! choice. Collision detection/resolution itself is entirely client-side (`state/useImport.ts`) —
//! this command only ever executes one already-resolved item per call, sequentially, one call per
//! dropped file (`PLAN.md` §2's "client-orchestrated collision loop, backend executes one resolved
//! item at a time" design).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::commands::emit_settings_recovery_event;
use crate::commands::image::{detect_extension, next_sequence_number, plain_tab_image_dir, write_image_file, IMAGE_FILE_EXTENSIONS, MAX_FILENAME_COLLISION_ATTEMPTS};
use crate::commands::tab_lock::{self, TabAccess};
use crate::commands::tree::{contains_name, find_node, insert_node_at_end, load_and_reconcile, save_tab_meta, TreeSnapshot};
use crate::fs::atomic_write::atomic_write;
use crate::fs::retry::with_retry;
use crate::fs::path_safety::is_link_or_reparse_point;
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::EolPolicy;
use crate::model::result::{AppResult, ErrorCode};
use crate::model::tree::{MetaTreeNode, NodePath};
use crate::search::index::SearchIndex;
use crate::settings::app_settings_store::AppSettingsStore;
use crate::store::lock_session_store::LockSessionStore;
use crate::store::memo_file_service::{decode_memo_body, detect_eol, encode_for_save, normalize_to_lf, restore_eol, Eol};
use crate::util::image_link_rewrite::{decode_destination, rewrite_image_destinations_with_legacy};

/// Return type of `import_file`: the tree snapshot (flattened, same wire fields as before) plus
/// the number of relative image links whose source file was not found (GitHub issue #265). The
/// frontend sums this over a drop batch and shows one toast when it is non-zero.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFileResult {
    #[serde(flatten)]
    pub snapshot: TreeSnapshot,
    pub missing_images: u32,
}

/// 1枚あたりのコピー上限(GitHub issue #265)。超えた画像はコピーせずリンクも元のまま残す。
const MAX_IMPORT_IMAGE_BYTES: u64 = 50 * 1024 * 1024;

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
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
    search_index: State<SearchIndex>,
) -> AppResult<ImportFileResult> {
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
            // ロック中タブへのインポートでは画像をコピーしない(#264 のレイアウト決定まで対象外、#265)
            return match tab_lock::session_import_file(&sessions, &registry, &folder, &tab_id, &target_name, &bytes_to_write, overwrite, parent.as_deref()) {
                AppResult::Ok(snapshot) => AppResult::ok(ImportFileResult { snapshot, missing_images: 0 }),
                AppResult::Err(e) => AppResult::err(e),
            };
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
    // re-encode. Between decode and re-encode, relative image links are copied into the memo's
    // image folder and rewritten (GitHub issue #265).
    let (decoded_text, source_eol) = match read_normalized(&source_path) {
        Ok(v) => v,
        Err(code) => return AppResult::err_code(code, "failed to read source file"),
    };
    let source_dir = Path::new(&source_path).parent().map(Path::to_path_buf).unwrap_or_default();
    let (rewritten, missing_images) = copy_relative_images(&decoded_text, &source_dir, &folder, &target_name, &registry);
    let normalized_text = rewritten.unwrap_or(decoded_text);
    let bytes_to_write = encode_normalized(&normalized_text, source_eol, settings.eol.policy, &settings.encoding.default_save);

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
        if let Err(err) = save_tab_meta(&folder, loaded.schema_version, loaded.app_version, &loaded.tree, loaded.last_viewed_memo.clone(), &registry) {
            tracing::error!(error = %err, "import_file: failed to save tab metadata");
            return AppResult::err_code(ErrorCode::Unknown, "failed to save tab metadata");
        }
    }

    // Step 10: called in both branches — the Overwrite case re-upserts a potentially-already-
    // favorited memo, so `SearchIndex::upsert` must preserve any existing `favorite` value (see
    // that fn's own doc comment once `docs/plans/v1.1.0/favorites/PLAN.md` extends `IndexEntry`).
    search_index.upsert(&tab_id, &target_name, &normalized_text);

    // Step 11. `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §4 "Not called by": an imported memo
    // never becomes a tab's `lastViewedMemo`, so this is simply the preserved pre-existing value.
    AppResult::ok(ImportFileResult {
        snapshot: TreeSnapshot {
            nodes: loaded.tree,
            tab_name,
            locked: false,
            recovery_pending: false,
            recovery_kind: None,
            last_viewed_memo: loaded.last_viewed_memo,
        },
        missing_images,
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
    let (normalized_text, source_eol) = read_normalized(source_path)?;
    let bytes_to_write = encode_normalized(&normalized_text, source_eol, eol_policy, default_save);
    Ok((bytes_to_write, normalized_text))
}

/// `read_and_prepare_bytes` の前半: 読み込み・デコード・LF正規化。元ファイルの改行形式も返す。
fn read_normalized(source_path: &str) -> Result<(String, Eol), ErrorCode> {
    let raw_bytes = read_source_bytes(source_path)?;
    let (decoded, _encoding) = decode_memo_body(&raw_bytes);
    let source_eol = detect_eol(&decoded);
    Ok((normalize_to_lf(&decoded), source_eol))
}

/// `read_and_prepare_bytes` の後半: タブの改行ポリシーに従って改行を戻し、保存用に再エンコード。
fn encode_normalized(normalized_text: &str, source_eol: Eol, eol_policy: EolPolicy, default_save: &str) -> Vec<u8> {
    let output_eol = match eol_policy {
        EolPolicy::Preserve => source_eol,
        EolPolicy::Crlf => Eol::Crlf,
        EolPolicy::Lf => Eol::Lf,
        EolPolicy::Cr => Eol::Cr,
    };
    let restored = restore_eol(normalized_text, output_eol);
    encode_for_save(&restored, default_save)
}

/// GitHub issue #265: インポートする本文中の相対画像リンクを、元 `.md` のフォルダ基準で解決して
/// `<tab_folder>/<memo_name>/NNN.ext` へコピーし、リンク先を `<memo_name>/NNN.ext` に書き換える。
/// 戻り値は (書き換え後の本文 — 変更なしなら `None`, 見つからなかった相対画像の件数)。
///
/// 対象外(リンクはそのまま): スキーム付き(`http:`/`file:`/`data:` 等)・絶対パス・`/` 始まり、
/// 対応外の拡張子、50MB超、読み込み/書き込み失敗。対応画像拡張子の相対パスで、通常ファイルとして存在しないものだけを
/// 「見つからない」として数える。同じ元ファイルを同じメモ内で複数回参照した場合は1回だけコピーする。
/// 書き込み先は `plain_tab_image_dir`(#263 のパスガード)経由に限定する。
fn copy_relative_images(body: &str, source_dir: &Path, tab_folder: &Path, memo_name: &str, registry: &SelfWriteRegistry) -> (Option<String>, u32) {
    let mut missing = 0u32;
    // 元ファイル(正規化パス) -> 書き換え後のリンク先
    let mut copied: HashMap<PathBuf, String> = HashMap::new();
    // 画像フォルダ: 未準備なら None、準備失敗なら Some(Err)
    let mut image_dir: Option<Result<PathBuf, ()>> = None;

    // #266: 空白入り裸形のレガシーリンクは、元 `.md` 基準で対応画像の通常ファイル(リンク以外)が
    // 実在する場合のみ対象にする
    let legacy_exists = |path: &str| {
        let Some(rel) = relative_image_path(path) else { return false };
        let source = source_dir.join(&rel);
        let ext_ok = source.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGE_FILE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
        ext_ok && std::fs::symlink_metadata(&source).is_ok_and(|m| m.is_file() && !is_link_or_reparse_point(&m))
    };
    let rewritten = rewrite_image_destinations_with_legacy(body, |dest| {
        let rel = relative_image_path(dest)?;
        let source = source_dir.join(&rel);
        // 拡張子判定を先に行い、対応画像拡張子のリンクだけを「見つからない」対象にする
        let source_ext = source.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)?;
        if !IMAGE_FILE_EXTENSIONS.contains(&source_ext.as_str()) {
            return None;
        }
        let meta = match std::fs::metadata(&source) {
            Ok(m) if m.is_file() => m,
            _ => {
                missing += 1;
                return None;
            }
        };
        let key = std::fs::canonicalize(&source).unwrap_or_else(|_| source.clone());
        if let Some(existing) = copied.get(&key) {
            return Some(existing.clone());
        }
        if meta.len() > MAX_IMPORT_IMAGE_BYTES {
            tracing::warn!(path = %source.display(), size = meta.len(), "import_file: image exceeds size cap, link left as-is");
            return None;
        }
        let dir = match image_dir.get_or_insert_with(|| prepare_image_dir(tab_folder, memo_name)) {
            Ok(d) => d.clone(),
            Err(()) => return None,
        };
        let bytes = match std::fs::read(&source) {
            Ok(b) => b,
            Err(err) => {
                tracing::error!(error = %err, path = %source.display(), "import_file: failed to read image");
                return None;
            }
        };
        let ext = detect_extension(&bytes, Some(&format!("image/{source_ext}")));
        let filename = (0..MAX_FILENAME_COLLISION_ATTEMPTS).map(|_| format!("{:03}.{ext}", next_sequence_number(&dir))).find(|name| !dir.join(name).exists())?;
        let target = dir.join(&filename);
        if let Err(err) = write_image_file(&target, &bytes, registry) {
            tracing::error!(error = %err, path = %target.display(), "import_file: failed to write image");
            return None;
        }
        let new_dest = format!("{memo_name}/{filename}");
        copied.insert(key, new_dest.clone());
        Some(new_dest)
    }, legacy_exists);
    (rewritten, missing)
}

/// 画像リンク先が「元 `.md` 基準の相対パス」ならデコード済みパスを返す。それ以外は `None`。
fn relative_image_path(dest: &str) -> Option<String> {
    // クエリ・フラグメントはデコード前に落とす(%23 等はファイル名の一部として残す)
    let end = dest.find(['?', '#']).unwrap_or(dest.len());
    let raw = &dest[..end];
    if raw.is_empty() || has_scheme(raw) {
        return None;
    }
    let decoded = decode_destination(raw)?;
    let path = Path::new(&decoded);
    if decoded.is_empty() || decoded.starts_with(['/', '\\']) || path.is_absolute() || path.has_root() || has_scheme(&decoded) {
        return None;
    }
    Some(decoded)
}

/// `scheme:` 形式(RFC 3986)で始まるか。`C:` のようなドライブレターもここで弾かれる。
fn has_scheme(s: &str) -> bool {
    let Some(colon) = s.find(':') else { return false };
    let mut chars = s[..colon].chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// メモの画像フォルダを #263 のガード経由で用意する。失敗時はその本文の画像コピーをすべて見送る。
fn prepare_image_dir(tab_folder: &Path, memo_name: &str) -> Result<PathBuf, ()> {
    let dir = plain_tab_image_dir(tab_folder, memo_name).map_err(|_| {
        tracing::error!(memo = %memo_name, "import_file: memo image folder path rejected by guard");
    })?;
    std::fs::create_dir_all(&dir).map_err(|err| {
        tracing::error!(error = %err, path = %dir.display(), "import_file: failed to create memo image folder");
    })?;
    Ok(dir)
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
            favorite: None,
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

    // --- GitHub issue #265: 相対画像のコピーとリンク書き換え ---

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake-png-body";

    /// 元フォルダ `元メモ/`(中に `資料.md` がある想定)とタブフォルダ `タブ/` を作る。
    fn import_dirs() -> (PathBuf, PathBuf, PathBuf) {
        let root = tempdir();
        let src = root.join("元メモ");
        let tab = root.join("タブ");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&tab).unwrap();
        (root, src, tab)
    }

    #[test]
    fn copies_sibling_subfolder_and_parent_images_and_rewrites_links() {
        let (root, src, tab) = import_dirs();
        std::fs::write(src.join("図.png"), PNG).unwrap();
        std::fs::create_dir_all(src.join("画像")).unwrap();
        std::fs::write(src.join("画像").join("写真.jpg"), b"\xFF\xD8\xFFjpeg").unwrap();
        std::fs::write(root.join("共通.png"), PNG).unwrap();

        let body = "![a](図.png)\n![b](<画像/写真.jpg> \"題\")\n![c](../共通.png)\n";
        let (got, missing) = copy_relative_images(body, &src, &tab, "日記", &SelfWriteRegistry::new());
        assert_eq!(got.unwrap(), "![a](日記/001.png)\n![b](<日記/002.jpg> \"題\")\n![c](日記/003.png)\n");
        assert_eq!(missing, 0);
        assert_eq!(std::fs::read(tab.join("日記").join("001.png")).unwrap(), PNG);
        assert!(tab.join("日記").join("002.jpg").is_file());
        assert!(tab.join("日記").join("003.png").is_file());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn percent_encoded_japanese_destination_is_decoded() {
        let (root, src, tab) = import_dirs();
        std::fs::write(src.join("図 1.png"), PNG).unwrap();
        // 図 = E5 9B B3
        let body = "![](%E5%9B%B3%201.png#frag)\n";
        let (got, missing) = copy_relative_images(body, &src, &tab, "メモ", &SelfWriteRegistry::new());
        assert_eq!(got.unwrap(), "![](メモ/001.png)\n");
        assert_eq!(missing, 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn spaced_memo_name_emits_angle_bracket_destination() {
        // 半角スペースを含むメモ名は CommonMark で有効な山括弧形で書く(GitHub issue #266)
        let (root, src, tab) = import_dirs();
        std::fs::write(src.join("図.png"), PNG).unwrap();
        let body = "![](図.png) ![](<図.png>)\n";
        let (got, missing) = copy_relative_images(body, &src, &tab, "新 メモ", &SelfWriteRegistry::new());
        assert_eq!(got.unwrap(), "![](<新 メモ/001.png>) ![](<新 メモ/001.png>)\n");
        assert_eq!(missing, 0);
        assert!(tab.join("新 メモ").join("001.png").is_file());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn legacy_spaced_link_is_copied_only_when_source_exists() {
        // #266: 空白入り裸形のレガシーリンクも、元ファイルが実在すれば取り込む
        let (root, src, tab) = import_dirs();
        std::fs::create_dir_all(src.join("旧 メモ")).unwrap();
        std::fs::write(src.join("旧 メモ").join("001.png"), PNG).unwrap();
        let body = "![](旧 メモ/001.png \"題\") ![](旧 メモ/無い.png)
";
        let (got, missing) = copy_relative_images(body, &src, &tab, "新 メモ", &SelfWriteRegistry::new());
        assert_eq!(got.unwrap(), "![](<新 メモ/001.png> \"題\") ![](旧 メモ/無い.png)
");
        assert_eq!(missing, 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_image_is_counted_and_left_untouched() {
        let (root, src, tab) = import_dirs();
        // 画像拡張子でないリンク(フォルダ・存在しない .txt/.svg)は数えない
        let body = "![](無い.png) ![](画像/) ![](無い.txt) ![](無い.svg)\n";
        let (got, missing) = copy_relative_images(body, &src, &tab, "メモ", &SelfWriteRegistry::new());
        assert_eq!(got, None);
        assert_eq!(missing, 1);
        // 何もコピーしないときは画像フォルダも作らない
        assert!(!tab.join("メモ").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn remote_absolute_and_unsupported_links_are_untouched_and_not_counted() {
        let (root, src, tab) = import_dirs();
        std::fs::write(src.join("メモ.txt"), b"text").unwrap();
        let abs = src.join("x.png");
        std::fs::write(&abs, PNG).unwrap();
        let body = format!("![](https://example.com/a.png) ![](file:///C:/a.png) ![](data:image/png;base64,AA==) ![](/a.png) ![]({}) ![](メモ.txt)\n", abs.display());
        let (got, missing) = copy_relative_images(&body, &src, &tab, "メモ", &SelfWriteRegistry::new());
        assert_eq!(got, None);
        assert_eq!(missing, 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn duplicate_reference_is_copied_once() {
        let (root, src, tab) = import_dirs();
        std::fs::create_dir_all(src.join("画像")).unwrap();
        std::fs::write(src.join("画像").join("図.png"), PNG).unwrap();
        let body = "![](画像/図.png)\n![](./画像/%E5%9B%B3.png)\n";
        let (got, _) = copy_relative_images(body, &src, &tab, "メモ", &SelfWriteRegistry::new());
        assert_eq!(got.unwrap(), "![](メモ/001.png)\n![](メモ/001.png)\n");
        assert_eq!(std::fs::read_dir(tab.join("メモ")).unwrap().count(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn code_fence_and_inline_code_are_untouched() {
        let (root, src, tab) = import_dirs();
        std::fs::write(src.join("図.png"), PNG).unwrap();
        let body = "```\n![](図.png)\n```\n`![](図.png)`\n";
        let (got, missing) = copy_relative_images(body, &src, &tab, "メモ", &SelfWriteRegistry::new());
        assert_eq!(got, None);
        assert_eq!(missing, 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn overwrite_continues_existing_sequence_without_deleting() {
        let (root, src, tab) = import_dirs();
        std::fs::create_dir_all(tab.join("メモ")).unwrap();
        std::fs::write(tab.join("メモ").join("001.png"), b"old").unwrap();
        std::fs::write(src.join("図.png"), PNG).unwrap();
        let (got, _) = copy_relative_images("![](図.png)\n", &src, &tab, "メモ", &SelfWriteRegistry::new());
        assert_eq!(got.unwrap(), "![](メモ/002.png)\n");
        assert_eq!(std::fs::read(tab.join("メモ").join("001.png")).unwrap(), b"old");
        std::fs::remove_dir_all(&root).ok();
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
