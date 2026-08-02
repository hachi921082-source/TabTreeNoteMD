//! Image save command (`docs/plans/image-handling/PLAN.md` §3). Images are saved directly under
//! the tab's folder, sibling to `.md` files (`[spec §4.2]`) — they are never scanned as memos,
//! never subject to tab-lock filename obfuscation, and never indexed by full-text search
//! (`PLAN.md` §1). This command's write path is deliberately simpler than
//! `fs/atomic_write.rs`'s temp -> bak -> target dance: a freshly-generated image filename has no
//! prior version to protect against losing, so this only needs temp -> target (no `.bak`
//! rotation) — the "reasonable middle ground" `PLAN.md` §3 step 3 leaves to `developer-lite`,
//! reusing `fs/retry.rs`'s Windows lock-retry policy and registering the final path with the
//! self-write registry for consistency with every other write in the app
//! (`docs/plans/persistence-fs/PLAN.md` §3, §4).

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use image::GenericImageView;
use rand::RngExt;
use tauri::{AppHandle, State};

use crate::commands::emit_settings_recovery_event;
use crate::commands::tab_lock::{resolve_tab_access, TabAccess};
use crate::fs::retry::{is_lock_class_error, with_retry};
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::fs::temp_naming::generate_temp_file_name;
use crate::model::result::{AppResult, ErrorCode};
use crate::settings::app_settings_store::AppSettingsStore;
use crate::store::lock_session_store::LockSessionStore;

/// Return type of `image_save_pasted` (`docs/plans/image-handling/PLAN.md` §3).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedImage {
    /// Filename only, relative to the tab folder — the Markdown body always references images
    /// relatively (`[spec §4.2]`).
    pub relative_path: String,
    pub width: u32,
    pub height: u32,
}

/// Astronomically unlikely to ever loop (128 bits of randomness across the 8-byte suffix), but
/// bounded defensively rather than looping forever (`PLAN.md` §2).
const MAX_FILENAME_COLLISION_ATTEMPTS: u32 = 10;

/// `docs/plans/image-handling/PLAN.md` §3.
#[tauri::command]
pub fn image_save_pasted(
    app: AppHandle,
    tab_id: String,
    bytes: Vec<u8>,
    mime_hint: Option<String>,
    store: State<AppSettingsStore>,
    registry: State<SelfWriteRegistry>,
    sessions: State<LockSessionStore>,
) -> AppResult<SavedImage> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    // Step 1: resolve the tab's target folder. A locked tab with an active session still saves
    // the image as a plaintext, unobfuscated file directly in the folder — images are out of
    // scope for tab-lock's `.md`-only encryption/obfuscation mechanism (`PLAN.md` §3 step 1) — so
    // both branches below resolve to the same plain folder path. A locked tab with no session is
    // not reachable here (the editor for that tab isn't open at all), so `resolve_tab_access`'s
    // `Err(TabLocked)` case is simply propagated like any other command needs no special casing.
    let folder = match resolve_tab_access(&settings, &sessions, &tab_id) {
        Ok(TabAccess::Plain { folder, .. }) => folder,
        Ok(TabAccess::Session { folder, .. }) => folder,
        Err(e) => return AppResult::err(e),
    };

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

    // Step 3 (filename): generate `img_<16 lowercase hex chars>.<ext>`, regenerating on the
    // (astronomically unlikely) collision.
    let mut target: Option<PathBuf> = None;
    for _ in 0..MAX_FILENAME_COLLISION_ATTEMPTS {
        let candidate = folder.join(generate_image_filename(&ext));
        if !candidate.exists() {
            target = Some(candidate);
            break;
        }
    }
    let Some(target) = target else {
        return AppResult::err_code(ErrorCode::Unknown, "failed to generate a unique image filename");
    };

    // Step 3 (write): temp file in the same directory, then rename into place.
    if let Err(err) = write_image_file(&target, &bytes, &registry) {
        tracing::error!(error = %err, path = %target.display(), "image_save_pasted: failed to write image file");
        let code = if is_lock_class_error(&err) { ErrorCode::IoLocked } else { ErrorCode::Unknown };
        return AppResult::err_code(code, "failed to save the pasted image");
    }

    // Step 5: relative (filename-only) path, since images always live directly in the tab folder.
    let relative_path = target.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();

    AppResult::ok(SavedImage { relative_path, width, height })
}

fn generate_image_filename(ext: &str) -> String {
    let mut bytes = [0u8; 8];
    rand::rng().fill(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("img_{hex}.{ext}")
}

/// Magic-number sniff first (`image::guess_format`), MIME-hint fallback, `png` default
/// (`docs/plans/image-handling/PLAN.md` §2).
fn detect_extension(bytes: &[u8], mime_hint: Option<&str>) -> String {
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
fn write_image_file(target: &Path, bytes: &[u8], registry: &SelfWriteRegistry) -> io::Result<()> {
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
    fn generated_filenames_carry_prefix_extension_and_16_hex_chars() {
        let name = generate_image_filename("png");
        assert!(name.starts_with("img_"));
        assert!(name.ends_with(".png"));
        let hex_part = &name["img_".len()..name.len() - ".png".len()];
        assert_eq!(hex_part.len(), 16);
        assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn generated_filenames_differ() {
        assert_ne!(generate_image_filename("png"), generate_image_filename("png"));
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
}
