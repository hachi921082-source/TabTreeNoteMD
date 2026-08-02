//! Cross-tab full-text search command — `docs/plans/search-index/PLAN.md` §4.
//!
//! Distinct from the in-memo search/replace feature (`editor-save/PLAN.md` §5, a pure
//! frontend/CodeMirror concern with no backend command of its own) — this is the top-bar-button-
//! triggered search across every unlocked tab's indexed memos (`search-index/PLAN.md` §5).

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::result::{AppResult, ErrorCode};
use crate::search::index::SearchIndex;
use crate::settings::app_settings_store::AppSettingsStore;

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
        let m = build_matcher("hello", &SearchOptions { case_sensitive: false, regex: false }).unwrap();
        assert!(m("say HELLO world"));
        assert!(m("say hello world"));
        assert!(!m("say goodbye"));
    }

    #[test]
    fn build_matcher_plain_substring_case_sensitive() {
        let m = build_matcher("Hello", &SearchOptions { case_sensitive: true, regex: false }).unwrap();
        assert!(m("say Hello world"));
        assert!(!m("say hello world"));
    }

    #[test]
    fn build_matcher_regex_case_insensitive_by_default() {
        let m = build_matcher("h[ae]llo", &SearchOptions { case_sensitive: false, regex: true }).unwrap();
        assert!(m("HALLO there"));
        assert!(m("hello there"));
        assert!(!m("nothing here"));
    }

    #[test]
    fn build_matcher_regex_case_sensitive() {
        let m = build_matcher("Hello", &SearchOptions { case_sensitive: true, regex: true }).unwrap();
        assert!(m("Hello world"));
        assert!(!m("hello world"));
    }

    #[test]
    fn build_matcher_invalid_regex_is_an_error() {
        assert!(build_matcher("(unclosed", &SearchOptions { case_sensitive: false, regex: true }).is_err());
    }
}
