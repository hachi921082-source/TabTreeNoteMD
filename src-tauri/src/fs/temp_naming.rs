//! Fixed temp-file naming pattern shared by `atomic_write` and the self-write
//! registry / file watcher, which filter out temp-file churn entirely before
//! even considering registry lookup (`docs/plans/persistence-fs/PLAN.md` §3).

use rand::RngExt;

/// Prefix for atomic-write scratch files. Exact spelling is
/// implementation-discretion (`docs/detailed-spec.md` §9).
pub const TEMP_PREFIX: &str = ".ttn-tmp-";

/// `true` if `file_name` (just the final path component, not a full path) is
/// one of `atomic_write`'s scratch files and should never be treated as a
/// real content change by the watcher.
pub fn is_temp_file_name(file_name: &str) -> bool {
    file_name.starts_with(TEMP_PREFIX)
}

/// Generates a fresh `<TEMP_PREFIX><random-suffix>` file name for use in the
/// same directory as `target_file_name` (the suffix does not need to be
/// derived from the target name, it only needs to be unique per write).
pub fn generate_temp_file_name() -> String {
    let suffix: u64 = rand::rng().random();
    format!("{TEMP_PREFIX}{suffix:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_temp_prefix() {
        assert!(is_temp_file_name(".ttn-tmp-abc123"));
        assert!(!is_temp_file_name("Shopping list.md"));
    }

    #[test]
    fn generated_names_carry_prefix_and_differ() {
        let a = generate_temp_file_name();
        let b = generate_temp_file_name();
        assert!(is_temp_file_name(&a));
        assert!(is_temp_file_name(&b));
        assert_ne!(a, b);
    }
}
