//! Name sanitization / validation — strict-vs-lenient rule
//! (`docs/spec.md` §5.11; `docs/detailed-spec.md` §6.5;
//! `docs/plans/persistence-fs/PLAN.md` §8).
//!
//! Also holds an unrelated-but-thematically-similar concern (`docs/plans/markdown/image-handling/
//! PLAN.md` §2, §3.1): `has_parent_dir_segment`/`resolve_relative_within_base`, hoisted out of
//! `commands/link.rs` so `commands/image.rs`'s `image_read_bytes` can reuse the same path-
//! containment check instead of duplicating it. Both concerns are "is this path safe" checks, but
//! solve different problems — this file's original `validate_name`/`ValidationMode`/`is_long_path`
//! content above is untouched by that addition.

use std::path::{Path, PathBuf};

use crate::model::result::ErrorCode;
use crate::util::name_sanitize::{has_forbidden_char, is_reserved_name};

// Re-exported so callers only need `fs::path_safety::{validate_name, suggest_sanitized}`.
// Not yet called from this crate (no command uses it until tab/memo create-rename
// commands are implemented) — see the crate-level `allow(dead_code)` note in `main.rs`.
#[allow(unused_imports)]
pub use crate::util::name_sanitize::suggest_sanitized;

/// Long-path warning threshold: full resulting path length > 260 chars
/// (`docs/detailed-spec.md` §3.7). Warning only — callers may still proceed.
pub const LONG_PATH_WARNING_THRESHOLD: usize = 260;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationMode {
    /// Tab create/rename, memo create/rename, image-filename-generation
    /// collision retry.
    Strict,
    /// §3.5 tree reconciliation scan, corruption-recovery flat rebuild —
    /// accepts whatever already exists on disk without blocking/renaming it.
    Lenient,
}

/// Validates a single path component's *name* (not a full path). All modes
/// reject: empty names, Windows-forbidden characters, control characters,
/// and Windows reserved device names. `Strict` additionally rejects
/// leading/trailing spaces and a trailing period, since NTFS silently
/// strips/rejects those for *new* entries even though a pre-existing entity
/// with such a name is technically valid (hence accepted under `Lenient`).
pub fn validate_name(name: &str, mode: ValidationMode) -> Result<(), ErrorCode> {
    if name.is_empty() {
        return Err(ErrorCode::NameInvalid);
    }
    if has_forbidden_char(name) {
        return Err(ErrorCode::NameInvalid);
    }
    if is_reserved_name(name) {
        return Err(ErrorCode::NameInvalid);
    }
    if mode == ValidationMode::Strict {
        let starts_or_ends_with_space = name.starts_with(' ') || name.ends_with(' ');
        let ends_with_period = name.ends_with('.');
        if starts_or_ends_with_space || ends_with_period {
            return Err(ErrorCode::NameInvalid);
        }
    }
    Ok(())
}

/// `true` if `full_path`'s length exceeds the long-path warning threshold.
/// The caller decides whether to surface `ErrorCode::LongPath` as a
/// non-blocking warning; this function never blocks the operation itself.
pub fn is_long_path(full_path: &str) -> bool {
    full_path.chars().count() > LONG_PATH_WARNING_THRESHOLD
}

// ---------------------------------------------------------------------------
// Path-containment ("resolve relative-to-base, reject traversal") helpers
// (`docs/plans/markdown/image-handling/PLAN.md` §3.1) — hoisted out of
// `commands/link.rs::resolve_file_path`, which now delegates to `resolve_relative_within_base`.
// ---------------------------------------------------------------------------

/// `true` if any component of `path` is a `..` parent-directory segment (parses both `/` and `\`
/// separators on Windows). Hoisted verbatim from `commands/link.rs`'s formerly-private
/// `has_parent_dir_segment`, so `image_read_bytes` (`commands/image.rs`) can reuse the identical
/// check instead of duplicating it.
pub fn has_parent_dir_segment(path: &Path) -> bool {
    path.components().any(|c| matches!(c, std::path::Component::ParentDir))
}

/// How `resolve_relative_within_base` treats an absolute `value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbsolutePathPolicy {
    /// Returned as-is, unresolved against `base` — `link.rs`'s `link_open_external` (`FilePath`
    /// kind) needs this: a user-typed absolute path is a legitimate, supported open target.
    AllowPassthrough,
    /// Rejected (`ErrorCode::NameInvalid`) — any command whose `value` must always resolve to
    /// somewhere *inside* `base` (e.g. `image_read_bytes`, where an image path is never
    /// legitimately absolute) needs this instead. Without this policy, an absolute `value` would
    /// silently discard `base` entirely when joined (`PathBuf::join`'s own semantics on Windows),
    /// defeating the containment check this function exists for.
    Reject,
}

/// Resolves `value` against `base`, rejecting directory traversal. Generalizes the containment
/// logic that used to be private to `commands/link.rs::resolve_file_path` (that function's own
/// doc comment, describing the Windows `has_root()`-without-`is_absolute()` edge case in full, is
/// not repeated here — read it for the complete rationale). Rejects: any `..` component
/// (`has_parent_dir_segment`), and a "rooted but not absolute" value (leading `\` or `/` with no
/// drive-letter prefix). A `None` base returns `value` unresolved-but-validated (the "no known tab
/// context" case, e.g. a `memo:` link typed with no tab currently open) — `image_read_bytes` never
/// calls this with `None`, since an image read always has a known tab/folder.
pub fn resolve_relative_within_base(value: &str, base: Option<&Path>, absolute_policy: AbsolutePathPolicy) -> Result<PathBuf, ErrorCode> {
    let path = Path::new(value);
    if path.is_absolute() {
        return match absolute_policy {
            AbsolutePathPolicy::AllowPassthrough => Ok(PathBuf::from(value)),
            AbsolutePathPolicy::Reject => Err(ErrorCode::NameInvalid),
        };
    }
    if has_parent_dir_segment(path) {
        return Err(ErrorCode::NameInvalid);
    }
    if path.has_root() {
        return Err(ErrorCode::NameInvalid);
    }
    Ok(match base {
        Some(b) => b.join(path),
        None => PathBuf::from(value),
    })
}

// ---------------------------------------------------------------------------
// Per-memo image folder path guard (GitHub issue #263)
// ---------------------------------------------------------------------------

/// Builds `<tab_folder>/<memo_name>` — a memo's per-memo image folder (#139) — only if
/// `memo_name` is safe to use as exactly one direct child component of `tab_folder`. Returns
/// `None` otherwise. The folder need not exist (see `existing_memo_image_folder` for that).
///
/// `memo_name` can come from a raw `.md` stem on disk (`scan_real_memo_names`, never validated)
/// or from IPC, so e.g. `..md` / `...md` would yield stems `.` / `..` and `join` would point at
/// the tab folder itself or its parent. Uses `ValidationMode::Strict` rather than `Lenient`:
/// Win32 path normalization silently strips trailing periods/spaces, so a lenient-only name like
/// `a.` would resolve to sibling memo `a`'s folder, and `.`/`..`/`...` would resolve outside
/// `<tab>/<name>`. Strict rejects all of those (trailing period/space), and is also the rule every
/// path that *creates* an image folder (`image_save_pasted`, `memo_rename`'s new name) already
/// enforces, so no legitimately created image folder is ever excluded by it.
pub fn memo_image_folder_path(tab_folder: &Path, memo_name: &str) -> Option<PathBuf> {
    if memo_name.is_empty() || memo_name == "." || memo_name == ".." {
        return None;
    }
    if validate_name(memo_name, ValidationMode::Strict).is_err() {
        return None;
    }
    let path = tab_folder.join(memo_name);
    if path.parent() != Some(tab_folder) || path.file_name() != Some(std::ffi::OsStr::new(memo_name)) {
        return None;
    }
    Some(path)
}

/// `memo_image_folder_path`, additionally requiring that the folder exists as a real directory
/// — a symlink/junction is never treated as an image folder, consistent with
/// `scan_real_memo_names` excluding links (`docs/spec.md` §2.2), so delete/rename never follow a
/// link out of the tab folder.
pub fn existing_memo_image_folder(tab_folder: &Path, memo_name: &str) -> Option<PathBuf> {
    let path = memo_image_folder_path(tab_folder, memo_name)?;
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if is_link_or_reparse_point(&meta) || !meta.is_dir() {
        return None;
    }
    Some(path)
}

/// `true` for a symlink or junction (`symlink_metadata` result). Rust's Windows
/// `FileType::is_symlink()` already covers both (name-surrogate reparse tags) — same criterion as
/// `scan_real_memo_names`. Other reparse points (OneDrive Files On-Demand placeholders, dedup,
/// etc.) are deliberately NOT treated as links, so a OneDrive-synced data folder still works.
pub fn is_link_or_reparse_point(meta: &std::fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_forbidden_and_reserved_in_both_modes() {
        for mode in [ValidationMode::Strict, ValidationMode::Lenient] {
            assert_eq!(validate_name("", mode), Err(ErrorCode::NameInvalid));
            assert_eq!(validate_name("a/b", mode), Err(ErrorCode::NameInvalid));
            assert_eq!(validate_name("CON", mode), Err(ErrorCode::NameInvalid));
        }
    }

    #[test]
    fn strict_rejects_edge_space_and_trailing_period_lenient_accepts() {
        assert_eq!(validate_name(" Shopping list", ValidationMode::Strict), Err(ErrorCode::NameInvalid));
        assert_eq!(validate_name("Shopping list ", ValidationMode::Strict), Err(ErrorCode::NameInvalid));
        assert_eq!(validate_name("Shopping list.", ValidationMode::Strict), Err(ErrorCode::NameInvalid));

        assert_eq!(validate_name(" Shopping list", ValidationMode::Lenient), Ok(()));
        assert_eq!(validate_name("Shopping list ", ValidationMode::Lenient), Ok(()));
        assert_eq!(validate_name("Shopping list.", ValidationMode::Lenient), Ok(()));
    }

    #[test]
    fn accepts_ordinary_names() {
        assert_eq!(validate_name("Shopping list", ValidationMode::Strict), Ok(()));
    }

    #[test]
    fn long_path_threshold() {
        let short = "C:/tab/note.md";
        let long = format!("C:/tab/{}.md", "a".repeat(300));
        assert!(!is_long_path(short));
        assert!(is_long_path(&long));
    }

    // GitHub issue #263: per-memo image folder path guard

    fn guard_tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-path-safety-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn memo_image_folder_path_accepts_japanese_name_as_direct_child() {
        let tab = Path::new("C:\\データ\\タブ");
        assert_eq!(memo_image_folder_path(tab, "日記"), Some(tab.join("日記")));
    }

    #[test]
    fn memo_image_folder_path_rejects_dot_names_separators_and_trailing_period_or_space() {
        let tab = Path::new("C:\\データ\\タブ");
        for bad in ["", ".", "..", "...", "a.", "a ", " a", "a/b", "a\\b", "../日記", "CON"] {
            assert_eq!(memo_image_folder_path(tab, bad), None, "{bad:?} must be rejected");
        }
    }

    #[test]
    fn existing_memo_image_folder_requires_a_real_directory() {
        let tab = guard_tempdir();
        std::fs::create_dir_all(tab.join("日記")).unwrap();
        std::fs::write(tab.join("ファイル"), b"x").unwrap();
        assert_eq!(existing_memo_image_folder(&tab, "日記"), Some(tab.join("日記")));
        assert_eq!(existing_memo_image_folder(&tab, "ファイル"), None);
        assert_eq!(existing_memo_image_folder(&tab, "無い"), None);
        // `.`/`..` はタブフォルダ自身/親フォルダ(実在ディレクトリ)だが対象外
        assert_eq!(existing_memo_image_folder(&tab, "."), None);
        assert_eq!(existing_memo_image_folder(&tab, ".."), None);
        std::fs::remove_dir_all(&tab).ok();
    }
}
