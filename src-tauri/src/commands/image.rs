//! Image save command (`docs/plans/image-handling/PLAN.md` §3). For a plain (non-locked) tab,
//! images are saved under a per-memo subfolder of the tab's folder (`<tab_folder>/<memo_name>/`,
//! GitHub issue #139) rather than flat alongside `.md` files; a locked tab with an active unlock
//! session uses a per-memo folder with a random name instead (`<tab_folder>/<random>/`, GitHub
//! issue #271 — see `image_save_pasted`'s own doc comment for why). Either way, images are never scanned as memos (only direct-child `.md`
//! files are, `[spec §3.1]`), never subject to tab-lock's own `.md`-body obfuscation, and never
//! indexed by full-text search (`PLAN.md` §1). This command's write path is deliberately simpler
//! than
//! `fs/atomic_write.rs`'s temp -> bak -> target dance: a freshly-generated image filename has no
//! prior version to protect against losing, so this only needs temp -> target (no `.bak`
//! rotation) — the "reasonable middle ground" `PLAN.md` §3 step 3 leaves to `developer-lite`,
//! reusing `fs/retry.rs`'s Windows lock-retry policy and registering the final path with the
//! self-write registry for consistency with every other write in the app
//! (`docs/plans/persistence-fs/PLAN.md` §3, §4).

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::GenericImageView;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State};

use crate::commands::emit_settings_recovery_event;
use crate::commands::tab_lock::{resolve_tab_access, session_memo_image_dir, TabAccess};
use crate::fs::path_safety::{is_link_or_reparse_point, memo_image_folder_path, resolve_relative_within_base, validate_name, AbsolutePathPolicy, ValidationMode};
use crate::fs::retry::{is_lock_class_error, with_retry};
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::fs::temp_naming::generate_temp_file_name;
use crate::model::result::{AppErrorPayload, AppResult, ErrorCode};
use crate::settings::app_settings_store::AppSettingsStore;
use crate::store::lock_session_store::LockSessionStore;
use crate::util::image_link_rewrite::{decode_destination, regular_file_without_links, rewrite_image_destinations_with_legacy};

/// Return type of `image_save_pasted` (`docs/plans/image-handling/PLAN.md` §3).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedImage {
    /// `<memo_name>/<NNN>.<ext>` for a plain tab (GitHub issue #139), or `<random>/<NNN>.<ext>`
    /// for a locked tab's active unlock session (GitHub issue #271) — relative to the tab folder
    /// either way. The Markdown body always references images relatively (`[spec §4.2]`).
    pub relative_path: String,
    pub width: u32,
    pub height: u32,
}

/// Defensive bound on `next_sequence_number` re-scans below — in practice this command's calls
/// are effectively serialized per-memo (a single desktop user pasting into one editor), so a
/// freshly computed "one past the max" number is already guaranteed free; this only guards
/// against looping forever in some unanticipated race (`PLAN.md` §2's collision-retry precedent).
pub(crate) const MAX_FILENAME_COLLISION_ATTEMPTS: u32 = 10;

/// 画像として受け付けるファイル拡張子(小文字)。フロントエンドの `editor/imagePaste.ts` の
/// `IMAGE_FILE_EXTENSIONS` と同じ集合。フォルダインポート時の相対画像コピー(GitHub issue #265)で
/// コピー対象かどうかの判定に使う。
pub(crate) const IMAGE_FILE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];

/// GitHub issue #139: for a **plain** (non-locked) tab, images now live in a per-memo subfolder
/// (`<tab_folder>/<memo_name>/`) instead of flat/random-hex directly under the tab folder, named
/// sequentially (`001.png`, `002.png`, ...) so a memo's attachments are identifiable in the
/// filesystem. The memo body itself stays flat (unaffected, out of scope — see the issue's
/// resolution comment on why: it would conflict with `[spec §3.2]`'s flat-rebuild-on-metadata-loss
/// crash recovery guarantee).
///
/// A **locked tab with an active unlock session** uses the same per-memo folder + sequential
/// `NNN.<ext>` scheme, but the folder is named with a random 16-hex string instead of the memo
/// name (GitHub issue #271), kept in a memo name -> folder map inside the tab's encrypted lock
/// payload (`LockPayload::image_folders`; `tab_lock::session_memo_image_dir`). Why not the memo
/// name: during a session the frontend deals in real memo names (`[spec §4.1]` point 4:
/// "retain real tab name, structure... in memory as a lock session"), and `.md` bodies themselves
/// are stored under obfuscated random-hex tokens specifically so nothing about a locked tab's
/// real names is ever recoverable from the plain filesystem without the password
/// (`session_rename_memo`'s own "the obfuscated token filename never reflects `memoName`" — same
/// invariant `search_index` upholds by excluding locked/session tabs entirely, `[spec §2.4]`). A
/// folder literally named after the real memo name would leak exactly that. Images saved before
/// #271 under the old flat `img_<hex>.<ext>` scheme are left in place (no migration) and still
/// display. `docs/plans/image-handling/PLAN.md` §3.
#[tauri::command]
pub fn image_save_pasted(
    app: AppHandle,
    tab_id: String,
    memo_name: String,
    bytes: Vec<u8>,
    mime_hint: Option<String>,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
) -> AppResult<SavedImage> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    // `memo_name` is used as a filesystem path component below (plain-tab branch) — an IPC
    // command can be invoked directly, bypassing the frontend's own memo-open state
    // (`docs/MDEditor-spec.md` §5), so it needs the same server-side validation
    // `memo_create`/`memo_rename` apply to memo names.
    if validate_name(&memo_name, ValidationMode::Strict).is_err() {
        return AppResult::err_code(ErrorCode::NameInvalid, "invalid memo name");
    }

    // Step 1: resolve the memo's image folder — `<tab_folder>/<memo_name>` for a plain tab
    // (#139), `<tab_folder>/<random>` for a locked tab's session (#271, see this function's doc
    // comment). A locked tab with no session is not reachable here (the editor for that tab isn't
    // open at all), so `resolve_tab_access`'s `Err(TabLocked)` case is simply propagated like any
    // other command needs no special casing.
    // `link_dir` is the folder-name component written into the body link.
    let (save_dir, link_dir, is_plain_tab) = match resolve_tab_access(&settings, &sessions, &tab_id) {
        Ok(TabAccess::Plain { folder, .. }) => match plain_tab_image_dir(&folder, &memo_name) {
            Ok(dir) => (dir, memo_name.clone(), true),
            Err(code) => return AppResult::err_code(code, "invalid memo image folder"),
        },
        Ok(TabAccess::Session { tab_id, folder }) => match session_memo_image_dir(&sessions, &registry, &folder, &tab_id, &memo_name) {
            Ok((dir, link)) => (dir, link, false),
            Err(e) => return AppResult::err(e),
        },
        Err(e) => return AppResult::err(e),
    };
    if let Err(err) = fs::create_dir_all(&save_dir) {
        tracing::error!(error = %err, path = %save_dir.display(), "image_save_pasted: failed to create the memo's image folder");
        let code = if is_lock_class_error(&err) { ErrorCode::IoLocked } else { ErrorCode::Unknown };
        return AppResult::err_code(code, "failed to create the memo's image folder");
    }

    // Step 2: sniff the image format from the actual bytes first (magic-number detection); fall
    // back to the frontend-supplied MIME hint; default to `png` if neither yields an answer
    // (`PLAN.md` §2, `[detailed-spec §3.6]`'s "raw bitmap clipboard data with no format info").
    let ext = detect_extension(&bytes, mime_hint.as_deref());

    // Step 4 (decode dimensions) is done before the write so a corrupt/undecodable payload never
    // reaches disk at all.
    let (width, height) = match image::load_from_memory(&bytes) {
        Ok(img) => img.dimensions(),
        Err(err) => {
            tracing::error!(error = %err, "image_save_pasted: failed to decode pasted image bytes");
            return AppResult::err_code(ErrorCode::Unknown, "failed to decode pasted image data");
        }
    };

    // Step 3 (filename): sequential `NNN.<ext>` (one past the largest existing number in
    // `save_dir`, regardless of extension), for both the plain-tab and locked-session cases.
    let mut target: Option<(PathBuf, String)> = None;
    for _ in 0..MAX_FILENAME_COLLISION_ATTEMPTS {
        let filename = format!("{:03}.{ext}", next_sequence_number(&save_dir));
        let candidate = save_dir.join(&filename);
        if !candidate.exists() {
            target = Some((candidate, filename));
            break;
        }
    }
    let Some((target, filename)) = target else {
        return AppResult::err_code(ErrorCode::Unknown, "failed to generate a unique image filename");
    };

    // Step 3 (write): temp file in the same directory, then rename into place.
    if let Err(err) = write_image_file(&target, &bytes, &registry) {
        tracing::error!(error = %err, path = %target.display(), "image_save_pasted: failed to write image file");
        let code = if is_lock_class_error(&err) { ErrorCode::IoLocked } else { ErrorCode::Unknown };
        return AppResult::err_code(code, "failed to save the pasted image");
    }

    // Step 5: relative path — `<link_dir>/<filename>` (always `/`-separated regardless of
    // platform: this is embedded directly into `![](...)` Markdown, not used as a filesystem path
    // again until `image_read_bytes`/`resolve_relative_within_base` re-parses it, which already
    // accepts `/` on Windows too).
    let relative_path = format!("{link_dir}/{filename}");

    // GitHub issue #319: 平文タブで作った画像ファイルのパスが長ければ警告のみ(保存済み)。ロック中タブの
    // 画像フォルダは乱数名で短いので対象外。
    if is_plain_tab {
        crate::commands::warn_if_long_path(&app, [&target]);
    }

    AppResult::ok(SavedImage { relative_path, width, height })
}

/// Scans `dir` for filenames whose stem is entirely ASCII digits (any extension), and returns one
/// past the largest found — or `1` if the folder has none yet (GitHub issue #139).
pub(crate) fn next_sequence_number(dir: &Path) -> u32 {
    let max = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let stem = entry.path().file_stem()?.to_str()?.to_string();
            if stem.is_empty() || !stem.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            stem.parse::<u32>().ok()
        })
        .max();
    max.map_or(1, |m| m + 1)
}

/// GitHub issue #266: 本文中のレガシー画像リンク(#266 以前に書かれた半角スペース入りの裸形
/// `![](新 メモ/001.png)` — CommonMark ではリンクにならない)を山括弧形 `![](<新 メモ/001.png>)` に
/// 直した本文を返す。参照先がタブフォルダ基準で実在する通常ファイル(途中にリンク/リパースポイントを
/// 含まない、`..`・絶対パス不可)の場合のみ修復する。修復が無ければ `None`。ファイルには書き込まない
/// (フロントエンドが通常の保存経路で保存する — `.bak`・ロックセッションの暗号化もそのまま効く)。
/// メモを開くたびに呼ばれるため、メインスレッドを止めないよう async コマンドにする(`tab_lock` と
/// 同じく State は `app.state()` で取得)。
#[tauri::command]
pub async fn image_repair_legacy_links(app: AppHandle, tab_id: String, body: String) -> AppResult<Option<String>> {
    let store = app.state::<AppSettingsStore>();
    let registry = app.state::<Arc<SelfWriteRegistry>>();
    let sessions = app.state::<LockSessionStore>();
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    let folder = match resolve_tab_access(&settings, &sessions, &tab_id) {
        Ok(TabAccess::Plain { folder, .. }) => folder,
        Ok(TabAccess::Session { folder, .. }) => folder,
        Err(e) => return AppResult::err(e),
    };
    AppResult::ok(repair_legacy_links(&body, &folder))
}

/// `image_repair_legacy_links` の本体。実在確認が通ったレガシーリンクだけを置き換え、それ以外の
/// リンクは1バイトも変えない(`replace` は直前に実在確認が通ったパスにだけ `Some` を返す)。
fn repair_legacy_links(body: &str, folder: &Path) -> Option<String> {
    let accepted: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
    let repaired = rewrite_image_destinations_with_legacy(
        body,
        |dest| {
            // レガシー分岐で実在確認が通った直後の呼び出しのみ対象(通常リンクは None で素通し)
            let path = accepted.borrow_mut().take()?;
            (path == dest).then(|| dest.to_string())
        },
        |path| {
            let ok = decode_destination(path).is_some_and(|rel| regular_file_without_links(folder, &rel));
            *accepted.borrow_mut() = ok.then(|| path.to_string());
            ok
        },
    );
    repaired.filter(|s| s != body)
}

/// Reads an image file's raw bytes and returns them as a raw-byte IPC response (Tauri 2.11.5's
/// `tauri::ipc::Response`), not base64/JSON-wrapped (`docs/MDEditor-spec.md` §5). See
/// `docs/plans/markdown/image-handling/PLAN.md` §3.3 for why this command's return type deviates
/// from the project's usual `AppResult<T>` JSON-envelope convention
/// (`project-architecture/PLAN.md` §3) — a backend `Err` here surfaces as a **rejected** JS
/// promise from `invoke()`, not as an `{ok: false, error}` value, the opposite of every other
/// command in this codebase. This narrow, documented exception is the only command in the
/// codebase that behaves this way; do not generalize the pattern to other commands.
#[tauri::command]
pub fn image_read_bytes(
    app: AppHandle,
    tab_id: String,
    relative_path: String,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
) -> Result<Response, AppErrorPayload> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    // Same tab-folder resolution `image_save_pasted` uses (`image-handling/PLAN.md` §3 step 1):
    // a locked tab with an active session still resolves to the tab's own plain, unobfuscated
    // folder, since images are out of scope for tab-lock's `.md`-only encryption/obfuscation.
    let folder = match resolve_tab_access(&settings, &sessions, &tab_id) {
        Ok(TabAccess::Plain { folder, .. }) => folder,
        Ok(TabAccess::Session { folder, .. }) => folder,
        Err(e) => return Err(e),
    };

    // Server-side path validation, independent of the frontend's own `isSafeRelativeImageSrc`
    // gate (`docs/MDEditor-spec.md` §5: "an IPC command can be invoked directly, bypassing any
    // frontend gate"). `AbsolutePathPolicy::Reject` — unlike `link.rs`'s file-path resolution, an
    // image path is never legitimately absolute.
    let resolved = resolve_relative_within_base(&relative_path, Some(&folder), AbsolutePathPolicy::Reject)
        .map_err(|code| AppErrorPayload::new(code, "image path is invalid"))?;

    let bytes = std::fs::read(&resolved).map_err(|err| {
        tracing::error!(error = %err, path = %resolved.display(), "image_read_bytes: failed to read image file");
        AppErrorPayload::new(ErrorCode::NotFound, "failed to read image file")
    })?;

    Ok(Response::new(bytes))
}

/// Reads raw bytes from an arbitrary absolute filesystem path (GitHub issue #244's re-fix — real
/// OS drag-and-drop). `components/Editor/EditorPane.tsx`'s native-OS-drag-drop listener
/// (`getCurrentWebview().onDragDropEvent`) only ever receives real absolute filesystem paths from
/// Tauri, never `File` objects the way clipboard paste/the toolbar file-picker do, so there is no
/// client-side way to turn a dropped path into bytes — this command is that missing step. The
/// returned bytes are fed straight into the EXISTING, unchanged `image_save_pasted` (via
/// `src/editor/imagePaste.ts`'s `saveAndInsertDroppedPaths` -> `saveAndInsertImage`), so this
/// command does no format-sniffing/dimension-decoding of its own.
///
/// This is inherently an arbitrary-filesystem-read operation by design, at the SAME trust level as
/// the existing toolbar file-picker (`insertImageViaFilePicker` in `imagePaste.ts`), which already
/// lets the user read any file on disk via the OS file dialog — the user is explicitly dragging
/// this exact path in via an OS-level gesture, so there is no "base folder" to contain it to the
/// way `resolve_relative_within_base` contains a *relative* path within a tab folder (`image_
/// read_bytes` above); that containment model simply doesn't apply to an already-absolute,
/// user-supplied path. The only validation performed is that `path` actually looks like a real
/// absolute filesystem path — a `onDragDropEvent` payload should never hand over anything else, so
/// a non-absolute value here indicates a malformed/forged call, not a legitimate drop. Actual
/// readability (existence, permissions, lock state) is left entirely to `std::fs::read` itself.
#[tauri::command]
pub fn image_read_external_file(path: String) -> AppResult<Vec<u8>> {
    let candidate = Path::new(&path);
    if path.is_empty() || !candidate.is_absolute() {
        return AppResult::err_code(ErrorCode::NameInvalid, "path must be an absolute file path");
    }

    match with_retry(|| fs::read(candidate)) {
        Ok(bytes) => AppResult::ok(bytes),
        Err(err) => {
            tracing::error!(error = %err, path = %path, "image_read_external_file: failed to read dropped file");
            let code = if is_lock_class_error(&err) { ErrorCode::IoLocked } else { ErrorCode::NotFound };
            AppResult::err_code(code, "failed to read dropped file")
        }
    }
}

/// Magic-number sniff first (`image::guess_format`), MIME-hint fallback, `png` default
/// (`docs/plans/image-handling/PLAN.md` §2).
pub(crate) fn detect_extension(bytes: &[u8], mime_hint: Option<&str>) -> String {
    if let Ok(format) = image::guess_format(bytes) {
        if let Some(ext) = format.extensions_str().first() {
            return (*ext).to_string();
        }
    }
    if let Some(ext) = extension_from_mime(mime_hint) {
        return ext;
    }
    "png".to_string()
}

fn extension_from_mime(mime_hint: Option<&str>) -> Option<String> {
    let raw = mime_hint?;
    let subtype = raw.split('/').nth(1)?.trim().to_ascii_lowercase();
    if subtype.is_empty() {
        return None;
    }
    Some(match subtype.as_str() {
        "jpeg" | "jpg" => "jpg".to_string(),
        other => other.to_string(),
    })
}

/// Temp-file-then-rename (`docs/plans/image-handling/PLAN.md` §3 step 3): a fresh image file has
/// no prior version to protect against losing, so this reuses `fs/atomic_write.rs`'s crash-safety
/// technique (write to a temp file in the same directory, then rename into place) minus its
/// `.bak`-rotation step, which only makes sense for overwriting an *existing* file.
pub(crate) fn write_image_file(target: &Path, bytes: &[u8], registry: &SelfWriteRegistry) -> io::Result<()> {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let temp_path = dir.join(generate_temp_file_name());
    {
        let mut file = File::create(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    // Self-write-register the target so the folder watcher doesn't misclassify this as an
    // external addition needing tree reconciliation (`PLAN.md` §3 step 3) — harmless-but-
    // consistent, since images aren't subject to `.md` tree reconciliation regardless.
    registry.register(target);
    with_retry(|| fs::rename(&temp_path, target))
}

/// GitHub issue #263: the plain-tab per-memo image folder for `memo_name`, guarded so it is
/// always exactly `<tab_folder>/<memo_name>` (`fs::path_safety::memo_image_folder_path`) — never
/// the tab folder itself, its parent, or a path through a symlink/junction already occupying that
/// name. Errors instead of writing anywhere else.
pub(crate) fn plain_tab_image_dir(folder: &Path, memo_name: &str) -> Result<PathBuf, ErrorCode> {
    let dir = memo_image_folder_path(folder, memo_name).ok_or(ErrorCode::NameInvalid)?;
    if let Ok(meta) = fs::symlink_metadata(&dir) {
        // 既存エントリがリンク/ファイルならタブ外への書き込みを防ぐため拒否
        if is_link_or_reparse_point(&meta) || !meta.is_dir() {
            return Err(ErrorCode::NameInvalid);
        }
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-image-cmd-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // A minimal valid 1x1 PNG (magic bytes + IHDR/IDAT/IEND), used across tests below.
    const ONE_PX_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn next_sequence_number_starts_at_one_for_an_empty_folder() {
        let dir = tempdir();
        assert_eq!(next_sequence_number(&dir), 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn next_sequence_number_is_one_past_the_existing_max_regardless_of_extension() {
        let dir = tempdir();
        fs::write(dir.join("001.png"), b"").unwrap();
        fs::write(dir.join("003.jpg"), b"").unwrap();
        assert_eq!(next_sequence_number(&dir), 4);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn next_sequence_number_ignores_non_numeric_stems() {
        let dir = tempdir();
        fs::write(dir.join("001.png"), b"").unwrap();
        fs::write(dir.join("notes.md"), b"").unwrap();
        assert_eq!(next_sequence_number(&dir), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detect_extension_sniffs_png_magic_bytes_regardless_of_mime_hint() {
        assert_eq!(detect_extension(ONE_PX_PNG, Some("image/jpeg")), "png");
    }

    #[test]
    fn detect_extension_falls_back_to_mime_hint_when_sniff_fails() {
        assert_eq!(detect_extension(b"not an image", Some("image/jpeg")), "jpg");
        assert_eq!(detect_extension(b"not an image", Some("image/webp")), "webp");
    }

    #[test]
    fn detect_extension_defaults_to_png_when_nothing_is_known() {
        assert_eq!(detect_extension(b"not an image", None), "png");
        assert_eq!(detect_extension(b"not an image", Some("garbage")), "png");
    }

    // `image_read_bytes`'s server-side path validation (`AbsolutePathPolicy::Reject`) — the
    // command itself takes `AppHandle`/`State<...>` params that aren't practical to construct in a
    // unit test, so these exercise `resolve_relative_within_base` directly with the exact
    // arguments `image_read_bytes` passes it, mirroring `link.rs`'s own `resolve_file_path_*` tests
    // for the sibling `AllowPassthrough` policy. Traversal/absolute/rooted rejection here was also
    // manually verified via live CDP testing (`docs/RESOLVED-ISSUES.md` #75) but had no automated
    // coverage until now.
    #[test]
    fn image_read_bytes_rejects_parent_dir_traversal() {
        let base = tempdir();
        let err = resolve_relative_within_base("../secret.png", Some(&base), AbsolutePathPolicy::Reject).unwrap_err();
        assert_eq!(err, ErrorCode::NameInvalid);
        let err = resolve_relative_within_base("sub\\..\\..\\secret.png", Some(&base), AbsolutePathPolicy::Reject).unwrap_err();
        assert_eq!(err, ErrorCode::NameInvalid);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn image_read_bytes_rejects_absolute_paths() {
        let base = tempdir();
        let err = resolve_relative_within_base("C:\\Windows\\System32\\cmd.exe", Some(&base), AbsolutePathPolicy::Reject).unwrap_err();
        assert_eq!(err, ErrorCode::NameInvalid);
        let err = resolve_relative_within_base("C:/Windows/System32/cmd.exe", Some(&base), AbsolutePathPolicy::Reject).unwrap_err();
        assert_eq!(err, ErrorCode::NameInvalid);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn image_read_bytes_rejects_rooted_but_not_absolute_paths() {
        let base = tempdir();
        // Leading `\` with no drive-letter prefix — `has_root()` true, `is_absolute()` false on
        // Windows, so `base.join(...)` would otherwise discard `base` entirely and escape it.
        let err = resolve_relative_within_base("\\secret.png", Some(&base), AbsolutePathPolicy::Reject).unwrap_err();
        assert_eq!(err, ErrorCode::NameInvalid);
        let err = resolve_relative_within_base("/secret.png", Some(&base), AbsolutePathPolicy::Reject).unwrap_err();
        assert_eq!(err, ErrorCode::NameInvalid);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn image_read_bytes_resolves_valid_relative_paths_within_the_tab_folder() {
        let base = tempdir();
        let resolved = resolve_relative_within_base("img_0011223344556677.png", Some(&base), AbsolutePathPolicy::Reject).unwrap();
        assert_eq!(resolved, base.join("img_0011223344556677.png"));
        fs::remove_dir_all(&base).ok();
    }

    // `image_read_external_file`'s own path-shape validation (GitHub issue #244's re-fix) —
    // the command itself takes no `State<...>` params, so it's callable directly in a unit test,
    // unlike most other commands in this module.
    #[test]
    fn image_read_external_file_reads_an_absolute_path() {
        let dir = tempdir();
        let file = dir.join("dropped.png");
        fs::write(&file, ONE_PX_PNG).unwrap();

        match image_read_external_file(file.to_str().unwrap().to_string()) {
            AppResult::Ok(bytes) => assert_eq!(bytes, ONE_PX_PNG),
            AppResult::Err(err) => panic!("expected success, got {err:?}"),
        }

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn image_read_external_file_rejects_a_relative_path() {
        match image_read_external_file("relative/dropped.png".to_string()) {
            AppResult::Err(err) => assert_eq!(err.code, ErrorCode::NameInvalid),
            AppResult::Ok(_) => panic!("expected a relative path to be rejected"),
        }
    }

    #[test]
    fn image_read_external_file_rejects_an_empty_path() {
        match image_read_external_file(String::new()) {
            AppResult::Err(err) => assert_eq!(err.code, ErrorCode::NameInvalid),
            AppResult::Ok(_) => panic!("expected an empty path to be rejected"),
        }
    }

    #[test]
    fn image_read_external_file_reports_not_found_for_a_missing_absolute_path() {
        let dir = tempdir();
        let missing = dir.join("missing.png");
        fs::remove_dir_all(&dir).ok();

        match image_read_external_file(missing.to_str().unwrap().to_string()) {
            AppResult::Err(err) => assert_eq!(err.code, ErrorCode::NotFound),
            AppResult::Ok(_) => panic!("expected a missing path to report NotFound"),
        }
    }

    #[test]
    fn write_image_file_writes_bytes_via_temp_then_rename() {
        let dir = tempdir();
        let target = dir.join("img_0000000000000000.png");
        let registry = SelfWriteRegistry::new();

        write_image_file(&target, ONE_PX_PNG, &registry).unwrap();

        assert_eq!(fs::read(&target).unwrap(), ONE_PX_PNG);
        // No leftover temp files in the directory once the rename completes.
        let leftover_temp = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with(".ttn-tmp-"));
        assert!(!leftover_temp);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn plain_tab_image_dir_accepts_japanese_name_and_rejects_dot_names() {
        let dir = tempdir();
        assert_eq!(plain_tab_image_dir(&dir, "日記"), Ok(dir.join("日記")));
        for bad in [".", "..", "...", "../日記", "a\\b"] {
            assert_eq!(plain_tab_image_dir(&dir, bad), Err(ErrorCode::NameInvalid), "{bad:?} must be rejected");
        }
        // 同名のファイルが既にある場合も拒否
        std::fs::write(dir.join("ファイル"), b"x").unwrap();
        assert_eq!(plain_tab_image_dir(&dir, "ファイル"), Err(ErrorCode::NameInvalid));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn repair_legacy_links_brackets_only_existing_files() {
        // #266: 実在する画像を指すレガシーリンクだけを山括弧形に直す。通常リンクは不変
        let dir = tempdir();
        std::fs::create_dir_all(dir.join("新 メモ")).unwrap();
        std::fs::write(dir.join("新 メモ").join("001.png"), ONE_PX_PNG).unwrap();
        let body = "![](新 メモ/001.png) ![](新 メモ/002.png) ![](日記/001.png)\n";
        assert_eq!(repair_legacy_links(body, &dir).as_deref(), Some("![](<新 メモ/001.png>) ![](新 メモ/002.png) ![](日記/001.png)\n"));
        assert_eq!(repair_legacy_links("![](<新 メモ/001.png>)\n", &dir), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn repair_legacy_links_leaves_unresolved_candidates_byte_identical() {
        // ファイルが無いレガシー候補は通常経路に落ちても山括弧を付けない(開いただけで本文を変えない)
        let dir = tempdir();
        let body = "![](メモ(1 2)/001.png)\n";
        assert_eq!(repair_legacy_links(body, &dir), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
