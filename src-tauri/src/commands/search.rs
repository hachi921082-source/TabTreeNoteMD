//! Cross-tab full-text search command — `docs/plans/search-index/PLAN.md` §4.
//!
//! Distinct from the in-memo search/replace feature (`editor-save/PLAN.md` §5, a pure
//! frontend/CodeMirror concern with no backend command of its own) — this is the top-bar-button-
//! triggered search across every unlocked tab's indexed memos (`search-index/PLAN.md` §5).

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::AppSettings;
use crate::model::result::{AppResult, ErrorCode};
use crate::search::index::{IndexEntry, SearchIndex};
use crate::settings::app_settings_store::AppSettingsStore;
use crate::store::tab_store::TabViewStore;

/// `SearchOptions` (`search-index/PLAN.md` §4) mirrors the in-memo search's own option set where
/// applicable to a global query; the exact set is implementation discretion beyond what
/// `spec.md` requires (name+body substring match at minimum).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub regex: bool,
    /// Scope selector (GitHub issue #4): `Some(tab_id)` restricts hits to that one tab
    /// ("現在のタブ" in the UI); `None` searches every unlocked tab ("全タブ"). Absent from
    /// older callers thanks to `#[serde(default)]`, which defaults to `None` (all tabs) —
    /// matches this command's pre-existing behavior for any caller that doesn't send it.
    #[serde(default)]
    pub scope_tab_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchLocation {
    Name,
    Body,
    Both,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub tab_id: String,
    pub tab_name: String,
    pub memo_name: String,
    pub matched_in: MatchLocation,
}

/// Searches `SearchIndex`'s entries against both `plaintextName` and `plaintext`
/// (`[spec §2.4]`: "Targets memo name and body"). An empty `query` yields no hits, rather than
/// matching everything (an empty substring pattern trivially matches any text) — the frontend
/// only ever calls this with a non-empty query in practice, but this keeps the command's own
/// contract well-defined regardless of caller behavior.
#[tauri::command]
pub fn search_query(query: String, options: SearchOptions, index: State<SearchIndex>, store: State<AppSettingsStore>, registry: State<SelfWriteRegistry>) -> AppResult<Vec<SearchHit>> {
    if query.is_empty() {
        return AppResult::ok(Vec::new());
    }

    let matcher = match build_matcher(&query, &options) {
        Ok(m) => m,
        Err(()) => return AppResult::err_code(ErrorCode::Unknown, "invalid search pattern"),
    };

    let (settings, _outcome) = store.get(&registry);
    let entries = index.snapshot();

    let mut hits = Vec::new();
    for entry in &entries {
        // Defense in depth (`search-index/PLAN.md` §1): a locked tab's entries should already be
        // absent from the index at this point (purged on lock, never added back on unlock), but
        // this command re-checks against the live tab registry anyway, since "no results from
        // locked tabs ever appear" is the one invariant this whole feature exists to uphold.
        let Some(tab) = settings.tabs.iter().find(|t| t.id == entry.tab_id) else {
            continue; // e.g. the tab was unregistered (`tab_remove`) since this entry was indexed
        };
        if tab.locked.unwrap_or(false) {
            continue;
        }
        if let Some(scope_tab_id) = &options.scope_tab_id {
            if &entry.tab_id != scope_tab_id {
                continue;
            }
        }

        let name_match = matcher(&entry.plaintext_name);
        let body_match = matcher(&entry.plaintext);
        let matched_in = match (name_match, body_match) {
            (true, true) => Some(MatchLocation::Both),
            (true, false) => Some(MatchLocation::Name),
            (false, true) => Some(MatchLocation::Body),
            (false, false) => None,
        };
        if let Some(matched_in) = matched_in {
            hits.push(SearchHit {
                tab_id: entry.tab_id.clone(),
                tab_name: tab.name.clone(),
                memo_name: entry.memo_name.clone(),
                matched_in,
            });
        }
    }

    AppResult::ok(hits)
}

// ---------------------------------------------------------------------------
// §3 Cross-Tab Quick Memo-Name Search ("Quick Open") — `docs/plans/v1.1.0/quick-open/PLAN.md` §2,
// `[spec v1.1.0 §3]`. Distinct from `search_query` above: matches memo DISPLAY NAMES only, never
// body content — a different search surface (VS Code's "Quick Open" vs. "Find in Files"
// distinction, `[spec v1.1.0 §3.1]`).
// ---------------------------------------------------------------------------

/// Result cap for `search_quick_open` (`docs/plans/v1.1.0/quick-open/PLAN.md` §2's "implementation
/// discretion" callout — spec does not mandate an exact count).
const QUICK_OPEN_RESULT_CAP: usize = 50;

/// One Quick Open result row (`docs/plans/v1.1.0/quick-open/PLAN.md` §2).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickOpenHit {
    pub tab_id: String,
    pub tab_name: String,
    pub memo_name: String,
    pub is_last_viewed: bool,
    pub is_favorite: bool,
}

/// Searches `SearchIndex`'s entries against `plaintext_name` only (never `plaintext`, unlike
/// `search_query` above). An empty `query` matches every candidate — not an empty result list
/// (`docs/plans/v1.1.0/quick-open/PLAN.md` §2 Q3-1) — since the Quick Open overlay shows the full,
/// sorted candidate list the moment it opens, before the user has typed anything.
#[tauri::command]
pub fn search_quick_open(
    query: String,
    index: State<SearchIndex>,
    store: State<AppSettingsStore>,
    registry: State<SelfWriteRegistry>,
    tab_view: State<TabViewStore>,
) -> AppResult<Vec<QuickOpenHit>> {
    let (settings, _outcome) = store.get(&registry);
    let entries = index.snapshot();
    AppResult::ok(search_quick_open_impl(&query, &entries, &settings, &tab_view))
}

/// Core logic, split out from the `#[tauri::command]` wrapper above so it's directly unit-testable
/// against a manually-built `AppSettings`/`Vec<IndexEntry>` — `State<T>` extraction needs a live
/// `tauri::App` (`commands/tree.rs::tree_set_last_viewed_impl`'s doc comment states the same
/// constraint; this follows the identical split).
pub(crate) fn search_quick_open_impl(query: &str, entries: &[IndexEntry], settings: &AppSettings, tab_view: &TabViewStore) -> Vec<QuickOpenHit> {
    let needle = query.to_lowercase();

    let mut hits: Vec<QuickOpenHit> = Vec::new();
    for entry in entries {
        if !needle.is_empty() && !entry.plaintext_name.to_lowercase().contains(&needle) {
            continue;
        }
        // Defense in depth (same rationale as `search_query` above): a locked tab's entries should
        // already be absent from the index (`docs/plans/v1.1.0/quick-open/PLAN.md` §2's "Locked-tab
        // exclusion is automatic" note), but re-check against the live tab registry anyway.
        let Some(tab) = settings.tabs.iter().find(|t| t.id == entry.tab_id) else {
            continue; // e.g. the tab was unregistered (`tab_remove`) since this entry was indexed
        };
        if tab.locked.unwrap_or(false) {
            continue;
        }

        let is_last_viewed = tab_view.get(&entry.tab_id).as_deref() == Some(entry.memo_name.as_str());
        hits.push(QuickOpenHit {
            tab_id: entry.tab_id.clone(),
            tab_name: tab.name.clone(),
            memo_name: entry.memo_name.clone(),
            is_last_viewed,
            is_favorite: entry.favorite,
        });
    }

    // Ordering (`docs/plans/v1.1.0/quick-open/PLAN.md` §2, `[spec v1.1.0 §3.2]`): `is_last_viewed`
    // first, tie-broken alphabetically by `memo_name` (case-insensitive) — applies identically to
    // the empty-query full list and any filtered subset.
    sort_quick_open_hits(&mut hits);
    hits.truncate(QUICK_OPEN_RESULT_CAP);
    hits
}

/// Shared ordering rule for `QuickOpenHit` lists — `docs/plans/v1.1.0/quick-open/PLAN.md` §2,
/// `[spec v1.1.0 §3.2]`: own-tab `is_last_viewed` first, then alphabetical by `memo_name`
/// (case-insensitive). Reused verbatim by `search_get_favorites_impl` below
/// (`docs/plans/v1.1.0/favorites/PLAN.md` §5 — "reuse the sorting logic, don't re-derive it").
fn sort_quick_open_hits(hits: &mut [QuickOpenHit]) {
    hits.sort_by(|a, b| b.is_last_viewed.cmp(&a.is_last_viewed).then_with(|| a.memo_name.to_lowercase().cmp(&b.memo_name.to_lowercase())));
}

// ---------------------------------------------------------------------------
// §5 Cross-Tab Favorites List — `docs/plans/v1.1.0/favorites/PLAN.md` §5, `[spec v1.1.0 §4.5]`.
// Distinct from `search_quick_open` above: no query string input, filters by `IndexEntry.favorite
// == true` instead of a name-substring match, and does NOT truncate (every favorited,
// index-eligible memo is returned — `[spec v1.1.0 §4.3]` Q4-2).
// ---------------------------------------------------------------------------

/// Reuses `QuickOpenHit` verbatim (`docs/plans/v1.1.0/favorites/PLAN.md` §5) — `is_favorite` is
/// always `true` for every row here, kept in the shape for row-rendering consistency with Quick
/// Open, since every entry returned already passed the `favorite == true` filter below.
#[tauri::command]
pub fn search_get_favorites(index: State<SearchIndex>, store: State<AppSettingsStore>, registry: State<SelfWriteRegistry>, tab_view: State<TabViewStore>) -> AppResult<Vec<QuickOpenHit>> {
    let (settings, _outcome) = store.get(&registry);
    let entries = index.snapshot();
    AppResult::ok(search_get_favorites_impl(&entries, &settings, &tab_view))
}

/// Core logic, split out from the `#[tauri::command]` wrapper above for the same direct-unit-test
/// reason `search_quick_open_impl` above is split out.
pub(crate) fn search_get_favorites_impl(entries: &[IndexEntry], settings: &AppSettings, tab_view: &TabViewStore) -> Vec<QuickOpenHit> {
    let mut hits: Vec<QuickOpenHit> = Vec::new();
    for entry in entries {
        if !entry.favorite {
            continue;
        }
        // Defense in depth (same rationale as `search_quick_open_impl` above): a locked tab's
        // entries should already be absent from the index (`favorites/PLAN.md` §7 "Tab Lock
        // Exclusion is automatic via the shared index"), but re-check against the live tab
        // registry anyway.
        let Some(tab) = settings.tabs.iter().find(|t| t.id == entry.tab_id) else {
            continue; // e.g. the tab was unregistered (`tab_remove`) since this entry was indexed
        };
        if tab.locked.unwrap_or(false) {
            continue;
        }

        let is_last_viewed = tab_view.get(&entry.tab_id).as_deref() == Some(entry.memo_name.as_str());
        hits.push(QuickOpenHit {
            tab_id: entry.tab_id.clone(),
            tab_name: tab.name.clone(),
            memo_name: entry.memo_name.clone(),
            is_last_viewed,
            is_favorite: true,
        });
    }

    sort_quick_open_hits(&mut hits);
    hits
}

/// Builds a `text -> bool` matcher closure for `query`/`options`, so the matching loop above
/// doesn't need to branch on `options.regex`/`case_sensitive` per entry. `Err(())` only for an
/// invalid regex pattern (`options.regex: true` with unparsable `query`).
fn build_matcher(query: &str, options: &SearchOptions) -> Result<Box<dyn Fn(&str) -> bool>, ()> {
    if options.regex {
        let pattern = if options.case_sensitive { query.to_string() } else { format!("(?i){query}") };
        let re = regex::Regex::new(&pattern).map_err(|_| ())?;
        return Ok(Box::new(move |text: &str| re.is_match(text)));
    }
    if options.case_sensitive {
        let needle = query.to_string();
        Ok(Box::new(move |text: &str| text.contains(&needle)))
    } else {
        let needle = query.to_lowercase();
        Ok(Box::new(move |text: &str| text.to_lowercase().contains(&needle)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_matcher_plain_substring_is_case_insensitive_by_default() {
        let m = build_matcher("hello", &SearchOptions { case_sensitive: false, regex: false, scope_tab_id: None }).unwrap();
        assert!(m("say HELLO world"));
        assert!(m("say hello world"));
        assert!(!m("say goodbye"));
    }

    #[test]
    fn build_matcher_plain_substring_case_sensitive() {
        let m = build_matcher("Hello", &SearchOptions { case_sensitive: true, regex: false, scope_tab_id: None }).unwrap();
        assert!(m("say Hello world"));
        assert!(!m("say hello world"));
    }

    #[test]
    fn build_matcher_regex_case_insensitive_by_default() {
        let m = build_matcher("h[ae]llo", &SearchOptions { case_sensitive: false, regex: true, scope_tab_id: None }).unwrap();
        assert!(m("HALLO there"));
        assert!(m("hello there"));
        assert!(!m("nothing here"));
    }

    #[test]
    fn build_matcher_regex_case_sensitive() {
        let m = build_matcher("Hello", &SearchOptions { case_sensitive: true, regex: true, scope_tab_id: None }).unwrap();
        assert!(m("Hello world"));
        assert!(!m("hello world"));
    }

    #[test]
    fn build_matcher_invalid_regex_is_an_error() {
        assert!(build_matcher("(unclosed", &SearchOptions { case_sensitive: false, regex: true, scope_tab_id: None }).is_err());
    }

    // ---- search_quick_open_impl (`docs/plans/v1.1.0/quick-open/PLAN.md` §2) ---------------------

    fn entry(tab_id: &str, memo_name: &str, favorite: bool) -> IndexEntry {
        IndexEntry { tab_id: tab_id.to_string(), memo_name: memo_name.to_string(), plaintext: String::new(), plaintext_name: memo_name.to_string(), favorite }
    }

    fn settings_with_tab(tab_id: &str, name: &str, locked: bool) -> AppSettings {
        let mut settings = AppSettings::default();
        settings.tabs.push(crate::model::app_settings::TabSettingsEntry {
            id: tab_id.to_string(),
            name: name.to_string(),
            folder_path: String::new(),
            locked: Some(locked),
        });
        settings
    }

    #[test]
    fn empty_query_matches_every_candidate() {
        let entries = vec![entry("tab-1", "Diary", false), entry("tab-1", "Notes", false)];
        let settings = settings_with_tab("tab-1", "Work", false);
        let tab_view = TabViewStore::new();

        let hits = search_quick_open_impl("", &entries, &settings, &tab_view);
        assert_eq!(hits.len(), 2, "empty query is every entry, not zero results (Q3-1)");
    }

    #[test]
    fn non_empty_query_substring_matches_plaintext_name_case_insensitively() {
        let entries = vec![entry("tab-1", "Diary", false), entry("tab-1", "Notes", false)];
        let settings = settings_with_tab("tab-1", "Work", false);
        let tab_view = TabViewStore::new();

        let hits = search_quick_open_impl("dia", &entries, &settings, &tab_view);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].memo_name, "Diary");

        let hits_upper = search_quick_open_impl("DIA", &entries, &settings, &tab_view);
        assert_eq!(hits_upper.len(), 1);
        assert_eq!(hits_upper[0].memo_name, "Diary");
    }

    #[test]
    fn locked_tabs_are_excluded() {
        let entries = vec![entry("tab-1", "Secret", false)];
        let settings = settings_with_tab("tab-1", "Locked", true);
        let tab_view = TabViewStore::new();

        assert!(search_quick_open_impl("", &entries, &settings, &tab_view).is_empty());
    }

    #[test]
    fn entries_for_an_unregistered_tab_are_excluded() {
        let entries = vec![entry("gone-tab", "Orphan", false)];
        let settings = AppSettings::default();
        let tab_view = TabViewStore::new();

        assert!(search_quick_open_impl("", &entries, &settings, &tab_view).is_empty());
    }

    #[test]
    fn enrichment_resolves_tab_name_last_viewed_and_favorite() {
        let entries = vec![entry("tab-1", "Diary", true)];
        let settings = settings_with_tab("tab-1", "Work", false);
        let tab_view = TabViewStore::new();
        tab_view.set("tab-1", Some("Diary".to_string()));

        let hits = search_quick_open_impl("", &entries, &settings, &tab_view);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].tab_id, "tab-1");
        assert_eq!(hits[0].tab_name, "Work");
        assert!(hits[0].is_last_viewed);
        assert!(hits[0].is_favorite);
    }

    #[test]
    fn is_last_viewed_requires_an_exact_match_not_a_substring() {
        let entries = vec![entry("tab-1", "Diary2", false)];
        let settings = settings_with_tab("tab-1", "Work", false);
        let tab_view = TabViewStore::new();
        tab_view.set("tab-1", Some("Diary".to_string()));

        let hits = search_quick_open_impl("", &entries, &settings, &tab_view);
        assert!(!hits[0].is_last_viewed);
    }

    #[test]
    fn ordering_puts_last_viewed_first_then_alphabetical_case_insensitive() {
        let entries = vec![entry("tab-1", "zebra", false), entry("tab-1", "Apple", false), entry("tab-1", "banana", false)];
        let settings = settings_with_tab("tab-1", "Work", false);
        let tab_view = TabViewStore::new();
        tab_view.set("tab-1", Some("zebra".to_string()));

        let hits = search_quick_open_impl("", &entries, &settings, &tab_view);
        let names: Vec<&str> = hits.iter().map(|h| h.memo_name.as_str()).collect();
        assert_eq!(names, vec!["zebra", "Apple", "banana"], "last-viewed sorts first; remainder alphabetical, case-insensitive");
    }

    #[test]
    fn ordering_tie_breaks_multiple_last_viewed_matches_alphabetically() {
        // Two different tabs can each independently have this memo name as their own
        // `lastViewedMemo` — there is no cross-tab timestamp to rank them (`[spec v1.1.0 §3.2]`).
        let entries = vec![entry("tab-2", "Zeta", false), entry("tab-1", "Alpha", false)];
        let settings = {
            let mut s = settings_with_tab("tab-1", "Work1", false);
            s.tabs.push(crate::model::app_settings::TabSettingsEntry { id: "tab-2".to_string(), name: "Work2".to_string(), folder_path: String::new(), locked: Some(false) });
            s
        };
        let tab_view = TabViewStore::new();
        tab_view.set("tab-1", Some("Alpha".to_string()));
        tab_view.set("tab-2", Some("Zeta".to_string()));

        let hits = search_quick_open_impl("", &entries, &settings, &tab_view);
        let names: Vec<&str> = hits.iter().map(|h| h.memo_name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Zeta"]);
    }

    #[test]
    fn result_list_is_capped_at_quick_open_result_cap() {
        let entries: Vec<IndexEntry> = (0..(QUICK_OPEN_RESULT_CAP + 10)).map(|i| entry("tab-1", &format!("memo{i:03}"), false)).collect();
        let settings = settings_with_tab("tab-1", "Work", false);
        let tab_view = TabViewStore::new();

        let hits = search_quick_open_impl("", &entries, &settings, &tab_view);
        assert_eq!(hits.len(), QUICK_OPEN_RESULT_CAP);
    }

    // ---- search_get_favorites_impl (`docs/plans/v1.1.0/favorites/PLAN.md` §5) --------------------

    #[test]
    fn only_favorited_entries_are_returned() {
        let entries = vec![entry("tab-1", "Diary", true), entry("tab-1", "Notes", false)];
        let settings = settings_with_tab("tab-1", "Work", false);
        let tab_view = TabViewStore::new();

        let hits = search_get_favorites_impl(&entries, &settings, &tab_view);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].memo_name, "Diary");
        assert!(hits[0].is_favorite, "is_favorite is always true for every row here (§5)");
    }

    #[test]
    fn favorites_list_locked_tabs_are_excluded() {
        let entries = vec![entry("tab-1", "Secret", true)];
        let settings = settings_with_tab("tab-1", "Locked", true);
        let tab_view = TabViewStore::new();

        assert!(search_get_favorites_impl(&entries, &settings, &tab_view).is_empty());
    }

    #[test]
    fn favorites_list_entries_for_an_unregistered_tab_are_excluded() {
        let entries = vec![entry("gone-tab", "Orphan", true)];
        let settings = AppSettings::default();
        let tab_view = TabViewStore::new();

        assert!(search_get_favorites_impl(&entries, &settings, &tab_view).is_empty());
    }

    #[test]
    fn favorites_list_enrichment_resolves_tab_name_and_last_viewed() {
        let entries = vec![entry("tab-1", "Diary", true)];
        let settings = settings_with_tab("tab-1", "Work", false);
        let tab_view = TabViewStore::new();
        tab_view.set("tab-1", Some("Diary".to_string()));

        let hits = search_get_favorites_impl(&entries, &settings, &tab_view);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].tab_id, "tab-1");
        assert_eq!(hits[0].tab_name, "Work");
        assert!(hits[0].is_last_viewed);
    }

    #[test]
    fn favorites_list_ordering_puts_last_viewed_first_then_alphabetical_case_insensitive() {
        let entries = vec![entry("tab-1", "zebra", true), entry("tab-1", "Apple", true), entry("tab-1", "banana", true)];
        let settings = settings_with_tab("tab-1", "Work", false);
        let tab_view = TabViewStore::new();
        tab_view.set("tab-1", Some("zebra".to_string()));

        let hits = search_get_favorites_impl(&entries, &settings, &tab_view);
        let names: Vec<&str> = hits.iter().map(|h| h.memo_name.as_str()).collect();
        assert_eq!(names, vec!["zebra", "Apple", "banana"]);
    }

    #[test]
    fn favorites_list_is_not_truncated_past_the_quick_open_result_cap() {
        let entries: Vec<IndexEntry> = (0..(QUICK_OPEN_RESULT_CAP + 10)).map(|i| entry("tab-1", &format!("memo{i:03}"), true)).collect();
        let settings = settings_with_tab("tab-1", "Work", false);
        let tab_view = TabViewStore::new();

        let hits = search_get_favorites_impl(&entries, &settings, &tab_view);
        assert_eq!(hits.len(), QUICK_OPEN_RESULT_CAP + 10, "no result cap (§5 Q4-2), unlike search_quick_open_impl");
    }
}
