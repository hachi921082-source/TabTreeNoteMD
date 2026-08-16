//! `TabViewStore` — in-memory runtime cache of each tab's current `lastViewedMemo`
//! (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §3). Exists so a future feature
//! (`docs/plans/v1.1.0/quick-open/PLAN.md`, `docs/plans/v1.1.0/favorites/PLAN.md`) can sort
//! results by "currently this tab's last-viewed memo" without re-reading every registered tab's
//! `.tabtreenote.json` from disk on every keystroke.
//!
//! Kept in sync at exactly two call sites, both already-existing: `commands::tree::tree_open`
//! (populates/refreshes the cache on read) and `commands::tree::tree_set_last_viewed` (updates the
//! cache in the same call that persists the new value) — same "runtime cache mirroring persisted
//! state, updated at both the read and write call sites" pattern already established for
//! `AppSettings.tabs[i].locked` (`docs/plans/tab-management/PLAN.md` §3).

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct TabViewStore {
    /// `tab_id` -> that tab's current `lastViewedMemo`. A missing key means "never populated this
    /// run" (distinct from a populated `None`, meaning "populated, and the tab genuinely has no
    /// last-viewed memo") — `get` collapses both to `None` since no call site so far needs to tell
    /// them apart.
    last_viewed: Mutex<HashMap<String, Option<String>>>,
}

impl TabViewStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, tab_id: &str, memo_name: Option<String>) {
        self.last_viewed.lock().expect("tab view store poisoned").insert(tab_id.to_string(), memo_name);
    }

    /// `None` if `tab_id` was never populated, OR if it was populated with `None` (the tab
    /// genuinely has no last-viewed memo yet) — this store's callers only ever need "is there a
    /// memo name to show," not the absent-vs-null distinction.
    pub fn get(&self, tab_id: &str) -> Option<String> {
        self.last_viewed.lock().expect("tab view store poisoned").get(tab_id).cloned().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpopulated_tab_returns_none() {
        let store = TabViewStore::new();
        assert_eq!(store.get("tab-1"), None);
    }

    #[test]
    fn set_then_get_round_trips() {
        let store = TabViewStore::new();
        store.set("tab-1", Some("Diary".to_string()));
        assert_eq!(store.get("tab-1"), Some("Diary".to_string()));
        assert_eq!(store.get("tab-2"), None, "other tabs are unaffected");
    }

    #[test]
    fn set_none_is_distinct_from_unpopulated_but_both_read_as_none() {
        let store = TabViewStore::new();
        store.set("tab-1", Some("Diary".to_string()));
        store.set("tab-1", None);
        assert_eq!(store.get("tab-1"), None, "a populated-but-empty entry reads back as None too");
    }
}
