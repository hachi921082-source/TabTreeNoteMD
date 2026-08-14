//! Creation-time duplicate-name auto-numbering (GitHub issue #97): `base`, `base(1)`, `base(2)`,
//! ... until `exists` reports the candidate free. Scoped to *creation* only — memo creation
//! (`commands/tree.rs`'s `memo_create_impl`) and tab creation (`commands/tab.rs`'s
//! `tab_create_new`/`register_tab_at_path`). Rename operations are unaffected (out of scope for
//! #97) and keep blocking on a duplicate name with `NameDuplicate`, exactly as before.

pub fn resolve_unique_name(base: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(base) {
        return base.to_string();
    }
    let mut n: u32 = 1;
    loop {
        let candidate = format!("{base}({n})");
        if !exists(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_base_unchanged_when_free() {
        assert_eq!(resolve_unique_name("Shopping list", |_| false), "Shopping list");
    }

    #[test]
    fn appends_first_free_suffix() {
        let taken = ["Shopping list", "Shopping list(1)"];
        let result = resolve_unique_name("Shopping list", |n| taken.contains(&n));
        assert_eq!(result, "Shopping list(2)");
    }
}
