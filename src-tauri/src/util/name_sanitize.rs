//! Core name-shape checks shared by both validation modes in
//! `fs/path_safety.rs` (`docs/spec.md` §5.11; `docs/detailed-spec.md` §6.5).
//! These checks (forbidden characters, control characters, reserved names)
//! can never be valid NTFS entries regardless of mode, unlike the
//! strict-only leading/trailing-space and trailing-period rules, which live
//! in `fs/path_safety.rs` since they depend on the validation mode.

const FORBIDDEN_CHARS: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

const RESERVED_BASE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1",
    "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub fn has_forbidden_char(name: &str) -> bool {
    name.chars().any(|c| FORBIDDEN_CHARS.contains(&c) || c.is_control())
}

/// Windows reserved device names are reserved case-insensitively, with or
/// without a trailing extension (e.g. `NUL`, `nul.txt`).
pub fn is_reserved_name(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or(name);
    RESERVED_BASE_NAMES.iter().any(|reserved| reserved.eq_ignore_ascii_case(base))
}

/// Best-effort replacement suggestion for a name that failed validation:
/// forbidden characters are swapped for visually-similar allowed characters,
/// and leading/trailing whitespace/periods are trimmed. Does not itself
/// guarantee the result passes `is_reserved_name` (a reserved-name collision
/// needs a different kind of user-visible resolution, not a silent rename).
pub fn suggest_sanitized(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' => '-',
            ':' => '_',
            '*' => '_',
            '?' => '_',
            '"' => '\'',
            '<' => '(',
            '>' => ')',
            '|' => '_',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();

    replaced.trim_matches(|c: char| c.is_whitespace() || c == '.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_forbidden_and_control_chars() {
        assert!(has_forbidden_char("a/b"));
        assert!(has_forbidden_char("a\u{0007}b"));
        assert!(!has_forbidden_char("Shopping list"));
    }

    #[test]
    fn detects_reserved_names_case_insensitively_with_or_without_extension() {
        assert!(is_reserved_name("CON"));
        assert!(is_reserved_name("con"));
        assert!(is_reserved_name("NUL.txt"));
        assert!(is_reserved_name("lpt1"));
        assert!(!is_reserved_name("Console"));
    }

    #[test]
    fn suggests_replacements_and_trims_edges() {
        assert_eq!(suggest_sanitized("a/b:c"), "a-b_c");
        assert_eq!(suggest_sanitized("  Shopping list.  "), "Shopping list");
    }
}
