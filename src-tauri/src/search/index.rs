//! Cross-tab full-text search index — `docs/plans/search-index/PLAN.md` §1-§3.
//!
//! In-memory only, no on-disk persistence (`[spec §2.4]`): built fresh at startup by scanning
//! every registered, currently-unlocked tab's direct-child `.md` files, then patched
//! incrementally afterward by the specific call sites `search-index/PLAN.md` §1 enumerates
//! (`memo_create`/`memo_rename`/`memo_delete`/`memo_move`/`editor_save` in `commands/tree.rs` and
//! `commands/editor.rs`, `tab_lock`'s bulk purge in `commands/tab_lock.rs`, and — once built,
//! `persistence-fs/PLAN.md` §5 — the file-watcher's external-change path) — never a full rebuild
//! on any of those.
//!
//! **Locked tabs are unconditionally excluded**, whether locked-with-no-session or
//! locked-with-an-active-unlock-session — no exception for the session case
//! (`[spec §2.4, §4.1 pt.6]`, `[detailed-spec §5.4]`). Every mutation that would add a
//! locked-session tab's content is simply never called for that case (see the session-path
//! helpers in `commands/tab_lock.rs`, none of which touch this module), and locking a
//! previously-unlocked tab bulk-purges its entries via `purge_tab`.

use std::path::Path;
use std::sync::RwLock;

use crate::commands::tree::{collect_names, find_node, load_and_reconcile};
use crate::model::app_settings::AppSettings;
use crate::store::memo_file_service::decode_memo_body;

/// One indexed memo (`search-index/PLAN.md` §2).
///
/// `plaintext`/`plaintext_name` preserve the original case of the converted body / memo name.
/// Case-(in)sensitivity is applied at query time instead (`commands/search.rs`'s `SearchOptions
/// .case_sensitive`), rather than committing to one case convention when the entry is
/// built/patched — a single stored copy then serves both a case-sensitive and a
/// case-insensitive query, and every patch site (`upsert`) stays a straightforward
/// "convert and store" with no separate case-fold step to keep in sync.
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub tab_id: String,
    pub memo_name: String,
    pub plaintext: String,
    pub plaintext_name: String,
    /// Mirrors `MetaTreeNode.favorite` (absence coerced to `false`) — `docs/plans/v1.1.0/favorites/PLAN.md`
    /// §3, `[spec v1.1.0 §4.3]`. Kept in sync by `set_favorite` and preserved across `upsert`'s
    /// in-place-update branch (see that method's doc comment).
    pub favorite: bool,
}

/// Linear scan over `entries` is acceptable at "hundreds to thousands of memos" scale
/// (`[spec §6]`); no inverted-index structure is required (`[detailed-spec §5.4]`). If profiling
/// ever shows this inadequate, an inverted index keyed on lowercased whitespace-tokenized words
/// is the natural upgrade path (`search-index/PLAN.md` §2) — not needed today.
pub struct SearchIndex {
    entries: RwLock<Vec<IndexEntry>>,
}

impl SearchIndex {
    pub fn new() -> Self {
        Self { entries: RwLock::new(Vec::new()) }
    }

    /// Startup index build (`search-index/PLAN.md` §1 "Built at startup"): scans every
    /// registered, currently-unlocked tab's direct-child `.md` files via the same
    /// tree-reconciliation logic `tree_open` uses (`commands/tree.rs::load_and_reconcile`), so a
    /// stale/corrupt `.tabtreenote.json` is handled identically here (self-healing against the
    /// real files on disk) rather than by a second, divergent folder scan. Replaces the entire
    /// in-memory `Vec` atomically under the lock.
    pub fn rebuild_all(&self, settings: &AppSettings) {
        let mut fresh = Vec::new();
        for tab in &settings.tabs {
            if tab.locked.unwrap_or(false) {
                continue; // `[spec §2.4, §4.1 pt.6]`: no exception for the session case either,
                          // and no active session exists yet at startup regardless.
            }
            let folder = Path::new(&tab.folder_path);
            let loaded = load_and_reconcile(folder);
            let mut names = Vec::new();
            for node in &loaded.tree {
                collect_names(node, &mut names);
            }
            for name in names {
                let file_path = folder.join(format!("{name}.md"));
                let Ok(bytes) = std::fs::read(&file_path) else { continue };
                let (decoded, _encoding) = decode_memo_body(&bytes);
                let favorite = find_node(&loaded.tree, &name).and_then(|n| n.favorite).unwrap_or(false);
                fresh.push(IndexEntry {
                    tab_id: tab.id.clone(),
                    memo_name: name.clone(),
                    plaintext: to_plaintext(&decoded),
                    plaintext_name: name,
                    favorite,
                });
            }
        }
        *self.entries.write().expect("search index lock poisoned") = fresh;
    }

    /// A cloned snapshot of the current entries, for `commands/search.rs::search_query` to match
    /// against without holding the lock for the whole (user-driven but still unbounded) matching
    /// pass.
    pub fn snapshot(&self) -> Vec<IndexEntry> {
        self.entries.read().expect("search index lock poisoned").clone()
    }

    /// Inserts a fresh entry, or replaces the existing one for `(tab_id, memo_name)`
    /// (case-insensitive on `memo_name`, matching this codebase's tab-wide memo-name-uniqueness
    /// convention — `commands/tree.rs::contains_name`) — `search-index/PLAN.md` §2. `raw_body` is
    /// the memo's decoded (not yet plaintext-converted) Markdown source; the conversion (§3,
    /// `to_plaintext` below) happens here, once, at the single choke point every patch call site
    /// goes through.
    ///
    /// `favorite` is left untouched on the existing-entry-update branch (rename, ordinary save, and
    /// `docs/plans/v1.1.0/folder-import/PLAN.md`'s Overwrite-collision re-import must not clear an
    /// already-favorited memo's status) — only the brand-new-entry branch defaults it to `false`,
    /// matching `MetaTreeNode.favorite`'s own absent-by-default rule
    /// (`docs/plans/v1.1.0/favorites/PLAN.md` §3).
    pub fn upsert(&self, tab_id: &str, memo_name: &str, raw_body: &str) {
        let plaintext = to_plaintext(raw_body);
        let mut entries = self.entries.write().expect("search index lock poisoned");
        match entries.iter_mut().find(|e| e.tab_id == tab_id && e.memo_name.eq_ignore_ascii_case(memo_name)) {
            Some(existing) => {
                existing.memo_name = memo_name.to_string();
                existing.plaintext = plaintext;
                existing.plaintext_name = memo_name.to_string();
            }
            None => entries.push(IndexEntry {
                tab_id: tab_id.to_string(),
                memo_name: memo_name.to_string(),
                plaintext,
                plaintext_name: memo_name.to_string(),
                favorite: false,
            }),
        }
    }

    /// Removes `(tab_id, memo_name)`'s entry, if present. A no-op (not an error) if it isn't —
    /// callers never need to check `upsert`/removal ordering against index membership first.
    pub fn remove(&self, tab_id: &str, memo_name: &str) {
        let mut entries = self.entries.write().expect("search index lock poisoned");
        entries.retain(|e| !(e.tab_id == tab_id && e.memo_name.eq_ignore_ascii_case(memo_name)));
    }

    /// Patches `(tab_id, memo_name)`'s `favorite` flag (`docs/plans/v1.1.0/favorites/PLAN.md` §2-3),
    /// same case-insensitive match `upsert`/`remove` use. A no-op if no matching entry exists — the
    /// locked-tab case (`commands/tree.rs::memo_set_favorite`'s only caller today still succeeds at
    /// the `MetaTreeNode.favorite` mutation itself; only this index side-effect is skipped).
    pub fn set_favorite(&self, tab_id: &str, memo_name: &str, favorite: bool) {
        let mut entries = self.entries.write().expect("search index lock poisoned");
        if let Some(existing) = entries.iter_mut().find(|e| e.tab_id == tab_id && e.memo_name.eq_ignore_ascii_case(memo_name)) {
            existing.favorite = favorite;
        }
    }

    /// Bulk-removes every entry for `tab_id` — called when a tab is locked
    /// (`commands/tab_lock.rs`'s `tab_lock`/`tab_lock_recovery_finish`). Unlocking a tab has no
    /// counterpart "add them back" call: locked tabs stay excluded even once an unlock session is
    /// active (`search-index/PLAN.md` §1's explicitly-called-out asymmetry — the index only ever
    /// shrinks around a lock/unlock transition, never grows from one).
    pub fn purge_tab(&self, tab_id: &str) {
        let mut entries = self.entries.write().expect("search index lock poisoned");
        entries.retain(|e| e.tab_id != tab_id);
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// §3: Plaintext conversion for indexing
// ---------------------------------------------------------------------------

/// Markdown -> searchable plaintext (`search-index/PLAN.md` §3, elaborating `[spec §4.2]`'s
/// "remove Markdown syntax/HTML tags... before indexing"): strips structural/decoration markers
/// and HTML tags line-by-line outside fenced code blocks, retaining fenced code block content
/// verbatim via a real fence-boundary scan rather than the marker-stripping pass (`DECISIONS.md`
/// "Code-Fence Awareness in Markdown Scanning" — `#`, `*`, `` ` ``, `>` are all common, legitimate
/// characters inside code).
fn to_plaintext(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut in_code_block = false;
    let mut fence_char = '`';
    let mut fence_len = 0usize;

    for line in body.lines() {
        if in_code_block {
            if is_closing_fence(line, fence_char, fence_len) {
                in_code_block = false;
            } else {
                out.push_str(line);
                out.push('\n');
            }
            continue;
        }
        if let Some((ch, len)) = detect_fence(line) {
            in_code_block = true;
            fence_char = ch;
            fence_len = len;
            continue; // the fence delimiter line itself carries no searchable content
        }
        if is_horizontal_rule(line) {
            continue; // dropped entirely rather than stripped down to a blank line
        }
        let without_tags = strip_html_tags(line);
        let stripped = strip_structural_markers(&without_tags);
        out.push_str(&stripped);
        out.push('\n');
    }
    out
}

/// A code-fence opening delimiter: 3+ of the same fence character (`` ` `` or `~`) at the start of
/// the line (after any leading indentation), per CommonMark's fenced-code-block rule.
fn detect_fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let ch = trimmed.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|&c| c == ch).count();
    if len < 3 {
        return None;
    }
    Some((ch, len))
}

/// A valid closing fence: the same character, at least as long as the opening fence, with
/// nothing but whitespace following it.
fn is_closing_fence(line: &str, fence_char: char, min_len: usize) -> bool {
    let trimmed = line.trim_start();
    let len = trimmed.chars().take_while(|&c| c == fence_char).count();
    len >= min_len && trimmed[len..].trim().is_empty()
}

/// A thematic-break/setext-heading-underline line made solely of 3+ `-` characters (optionally
/// space-separated). `*`/`_`-based horizontal rules need no separate case here:
/// `strip_structural_markers` already removes every `*`/`_` character unconditionally, so a line
/// like `"* * *"` is left empty either way.
fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let dash_count = trimmed.chars().filter(|&c| c == '-').count();
    dash_count >= 3 && trimmed.chars().all(|c| c == '-' || c.is_whitespace())
}

/// Removes `<...>` runs (`search-index/PLAN.md` §3: "HTML tags (including the allowlisted
/// `<img>`/`<span>`)... retaining human-readable text content" — every tag is stripped here
/// regardless of the frontend preview sanitizer's own allowlist, since only the surrounding text
/// nodes matter for search). An unterminated `<` with no matching `>` before line end consumes
/// the rest of the line — an acceptable simplification for malformed/partial markup, per
/// `[detailed-spec §5.4]`'s "implementation nicety" latitude on edge cases here.
fn strip_html_tags(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '<' {
            for c2 in chars.by_ref() {
                if c2 == '>' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Strips `#`, `*`, `_`, `` ` ``, `>`, and table-pipe `|` characters anywhere in the line
/// (`search-index/PLAN.md` §3's enumerated marker set) — a global character filter rather than
/// position-sensitive parsing, since none of these characters carry meaningful prose content on
/// their own: `"## Heading"` -> `"Heading"` (leading space trimmed away with the removed `#`s),
/// `"**bold**"` -> `"bold"`, `"> quoted"` -> `" quoted"`, `"| Col 1 | Col 2 |"` -> `" Col 1  Col 2 "`.
fn strip_structural_markers(line: &str) -> String {
    line.chars().filter(|c| !matches!(c, '#' | '*' | '_' | '`' | '>' | '|')).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-search-index-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ---- §3 plaintext conversion --------------------------------------------------------------

    #[test]
    fn strips_headings_emphasis_blockquote_and_table_markers() {
        let body = "# Heading\n\n**bold** and _italic_ and `code`\n\n> a quote\n\n| A | B |\n|---|---|\n";
        let plain = to_plaintext(body);
        assert!(plain.contains("Heading"));
        assert!(plain.contains("bold"));
        assert!(plain.contains("italic"));
        assert!(plain.contains("code"));
        assert!(plain.contains("a quote"));
        assert!(plain.contains("A"));
        assert!(plain.contains("B"));
        assert!(!plain.contains('#'));
        assert!(!plain.contains('*'));
        assert!(!plain.contains('_'));
        assert!(!plain.contains('`'));
        assert!(!plain.contains('>'));
        assert!(!plain.contains('|'));
    }

    #[test]
    fn drops_horizontal_rule_lines() {
        let plain = to_plaintext("above\n\n---\n\nbelow\n");
        assert!(plain.contains("above"));
        assert!(plain.contains("below"));
        assert!(!plain.contains('-'));
    }

    #[test]
    fn fenced_code_block_content_is_retained_verbatim_not_marker_stripped() {
        let body = "prose\n```rust\nfn main() { // comment > 1 * 2\n    let x = 1;\n}\n```\nmore prose\n";
        let plain = to_plaintext(body);
        // Code content survives untouched, including characters that would otherwise be stripped.
        assert!(plain.contains("fn main() { // comment > 1 * 2"));
        assert!(plain.contains("let x = 1;"));
        assert!(plain.contains("prose"));
        assert!(plain.contains("more prose"));
        // The fence delimiter lines themselves are not part of the output.
        assert!(!plain.contains("```"));
    }

    #[test]
    fn tilde_fence_is_recognized_independently_of_backtick_fence() {
        let body = "~~~\n# not a heading, this is code\n~~~\n";
        let plain = to_plaintext(body);
        assert!(plain.contains("# not a heading, this is code"));
    }

    #[test]
    fn html_tags_are_stripped_but_their_text_content_is_kept() {
        let plain = to_plaintext("before <img src=\"x.png\" alt=\"desc\"> after <span>inline</span> end\n");
        assert!(plain.contains("before"));
        assert!(plain.contains("after"));
        assert!(plain.contains("inline"));
        assert!(plain.contains("end"));
        assert!(!plain.contains('<'));
        assert!(!plain.contains('>'));
    }

    // ---- §2 patch operations -------------------------------------------------------------------

    #[test]
    fn upsert_then_remove_round_trip() {
        let index = SearchIndex::new();
        index.upsert("tab-1", "Diary", "hello world");
        let snap = index.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].tab_id, "tab-1");
        assert_eq!(snap[0].memo_name, "Diary");
        assert!(snap[0].plaintext.contains("hello world"));

        index.remove("tab-1", "Diary");
        assert!(index.snapshot().is_empty());
    }

    #[test]
    fn upsert_replaces_an_existing_entry_case_insensitively_by_name() {
        let index = SearchIndex::new();
        index.upsert("tab-1", "Diary", "first version");
        index.upsert("tab-1", "diary", "second version");
        let snap = index.snapshot();
        assert_eq!(snap.len(), 1, "case-insensitive match should replace, not duplicate");
        assert!(snap[0].plaintext.contains("second version"));
    }

    #[test]
    fn remove_of_a_missing_entry_is_a_harmless_no_op() {
        let index = SearchIndex::new();
        index.remove("tab-1", "Nonexistent");
        assert!(index.snapshot().is_empty());
    }

    #[test]
    fn upsert_of_a_brand_new_entry_defaults_favorite_to_false() {
        let index = SearchIndex::new();
        index.upsert("tab-1", "Diary", "hello world");
        assert!(!index.snapshot()[0].favorite);
    }

    #[test]
    fn upsert_preserves_favorite_across_an_in_place_update() {
        let index = SearchIndex::new();
        index.upsert("tab-1", "Diary", "first version");
        index.set_favorite("tab-1", "Diary", true);
        // Ordinary re-save / rename / Overwrite-collision re-import path — must not clear favorite.
        index.upsert("tab-1", "Diary", "second version");
        let snap = index.snapshot();
        assert_eq!(snap.len(), 1);
        assert!(snap[0].favorite, "favorite must survive an existing-entry upsert");
        assert!(snap[0].plaintext.contains("second version"));
    }

    #[test]
    fn set_favorite_matches_case_insensitively_like_upsert_and_remove() {
        let index = SearchIndex::new();
        index.upsert("tab-1", "Diary", "content");
        index.set_favorite("tab-1", "diary", true);
        assert!(index.snapshot()[0].favorite);
        index.set_favorite("tab-1", "DIARY", false);
        assert!(!index.snapshot()[0].favorite);
    }

    #[test]
    fn set_favorite_is_a_harmless_no_op_for_a_missing_entry() {
        let index = SearchIndex::new();
        index.set_favorite("tab-1", "Nonexistent", true);
        assert!(index.snapshot().is_empty());
    }

    #[test]
    fn purge_tab_removes_only_that_tabs_entries() {
        let index = SearchIndex::new();
        index.upsert("tab-1", "A", "content a");
        index.upsert("tab-2", "B", "content b");
        index.purge_tab("tab-1");
        let snap = index.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].tab_id, "tab-2");
    }

    // ---- §1 startup build -----------------------------------------------------------------------

    #[test]
    fn rebuild_all_scans_unlocked_tabs_and_skips_locked_ones() {
        let unlocked_dir = tempdir();
        std::fs::write(unlocked_dir.join("Diary.md"), "# Diary\n\nhello there").unwrap();
        let locked_dir = tempdir();
        std::fs::write(locked_dir.join("Secret.md"), "top secret content").unwrap();

        let mut settings = AppSettings::default();
        settings.tabs.push(crate::model::app_settings::TabSettingsEntry {
            id: "unlocked-tab".to_string(),
            name: "Open".to_string(),
            folder_path: unlocked_dir.to_string_lossy().into_owned(),
            locked: Some(false),
            lock_label: None,
        });
        settings.tabs.push(crate::model::app_settings::TabSettingsEntry {
            id: "locked-tab".to_string(),
            name: "\u{1F512}".to_string(),
            folder_path: locked_dir.to_string_lossy().into_owned(),
            locked: Some(true),
            lock_label: None,
        });

        let index = SearchIndex::new();
        index.rebuild_all(&settings);
        let snap = index.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].tab_id, "unlocked-tab");
        assert_eq!(snap[0].memo_name, "Diary");
        assert!(snap[0].plaintext.contains("Diary"));
        assert!(snap[0].plaintext.contains("hello there"));
        assert!(!snap[0].favorite, "no metadata file means favorite is absent, coerced to false");
        assert!(!snap.iter().any(|e| e.tab_id == "locked-tab"));

        std::fs::remove_dir_all(&unlocked_dir).ok();
        std::fs::remove_dir_all(&locked_dir).ok();
    }

    #[test]
    fn rebuild_all_reads_a_favorited_nodes_flag_from_persisted_metadata() {
        use crate::commands::tree::save_tab_meta;
        use crate::fs::self_write_registry::SelfWriteRegistry;
        use crate::model::tab_meta::TAB_META_SCHEMA_VERSION;
        use crate::model::tree::MetaTreeNode;

        let dir = tempdir();
        std::fs::write(dir.join("Starred.md"), "starred content").unwrap();
        let mut node = MetaTreeNode::default_for_new_file("Starred".to_string());
        node.favorite = Some(true);
        save_tab_meta(&dir, TAB_META_SCHEMA_VERSION, "test".to_string(), &[node], None, &SelfWriteRegistry::new()).unwrap();

        let mut settings = AppSettings::default();
        settings.tabs.push(crate::model::app_settings::TabSettingsEntry {
            id: "fav-tab".to_string(),
            name: "Fav".to_string(),
            folder_path: dir.to_string_lossy().into_owned(),
            locked: Some(false),
            lock_label: None,
        });

        let index = SearchIndex::new();
        index.rebuild_all(&settings);
        let snap = index.snapshot();
        assert_eq!(snap.len(), 1);
        assert!(snap[0].favorite);

        std::fs::remove_dir_all(&dir).ok();
    }
}
