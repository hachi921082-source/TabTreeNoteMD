//! Name sanitization / validation — strict-vs-lenient rule
//! (`docs/spec.md` §5.11; `docs/detailed-spec.md` §6.5;
//! `docs/plans/persistence-fs/PLAN.md` §8).

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
}
