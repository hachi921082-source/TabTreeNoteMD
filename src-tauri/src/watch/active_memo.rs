//! Tracks which memo is currently open in the editor, so the watcher's
//! conflict pipeline can decide whether a genuine external change should
//! trigger `evt:editor:conflict` (in addition to, or instead of, generic
//! reconciliation) — `docs/plans/persistence-fs/PLAN.md` §5.
//!
//! Set by `editor_load`, cleared by `editor_close` (`docs/plans/editor-save/PLAN.md` §2) —
//! called on every real "memo closed" transition (switch to another memo, switch tab, or no
//! memo open at all), so `is_active_tab` never keeps pointing at a memo the frontend has already
//! flushed and closed.

use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveMemoKey {
    pub tab_id: String,
    pub memo_name: String,
}

pub struct ActiveMemoTracker {
    current: Mutex<Option<ActiveMemoKey>>,
}

impl ActiveMemoTracker {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(None),
        }
    }

    pub fn set(&self, key: ActiveMemoKey) {
        *self.current.lock().expect("active memo tracker poisoned") = Some(key);
    }

    pub fn clear(&self) {
        *self.current.lock().expect("active memo tracker poisoned") = None;
    }

    /// Clears the tracked slot only if it still points at `(tab_id, memo_name)` — used by
    /// `editor_close` (`docs/plans/editor-save/PLAN.md`) so a real "memo closed" transition never
    /// clobbers some *other* memo that may have raced in and become the tracked one in the
    /// meantime (defense-in-depth, same spirit as `tab_lock`'s own defensive `is_active_tab`
    /// check this exists to make actually work — see `docs/plans/tab-lock/PLAN.md` §3 step 2 and
    /// the fix recorded in git history for the bug where nothing ever called `clear()`).
    pub fn clear_if(&self, tab_id: &str, memo_name: &str) {
        let mut guard = self.current.lock().expect("active memo tracker poisoned");
        if matches!(&*guard, Some(key) if key.tab_id == tab_id && key.memo_name == memo_name) {
            *guard = None;
        }
    }

    pub fn is_active(&self, tab_id: &str, memo_name: &str) -> bool {
        match &*self.current.lock().expect("active memo tracker poisoned") {
            Some(key) => key.tab_id == tab_id && key.memo_name == memo_name,
            None => false,
        }
    }

    /// `true` if *any* memo of `tab_id` is currently the active editor memo, regardless of which
    /// one. Used by `tab_lock` (`docs/plans/tab-lock/PLAN.md` §3 step 2) as a defensive check that
    /// the frontend already force-saved-and-closed any open editor for this tab before locking it
    /// — locking a tab out from under a live edit would corrupt/lose that unsaved content.
    pub fn is_active_tab(&self, tab_id: &str) -> bool {
        match &*self.current.lock().expect("active memo tracker poisoned") {
            Some(key) => key.tab_id == tab_id,
            None => false,
        }
    }
}

impl Default for ActiveMemoTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_set_clear_and_membership() {
        let tracker = ActiveMemoTracker::new();
        assert!(!tracker.is_active("t1", "Shopping list"));

        tracker.set(ActiveMemoKey {
            tab_id: "t1".into(),
            memo_name: "Shopping list".into(),
        });
        assert!(tracker.is_active("t1", "Shopping list"));
        assert!(!tracker.is_active("t1", "Other"));

        tracker.clear();
        assert!(!tracker.is_active("t1", "Shopping list"));
    }

    #[test]
    fn is_active_tab_ignores_which_memo_within_the_tab() {
        let tracker = ActiveMemoTracker::new();
        assert!(!tracker.is_active_tab("t1"));

        tracker.set(ActiveMemoKey {
            tab_id: "t1".into(),
            memo_name: "Shopping list".into(),
        });
        assert!(tracker.is_active_tab("t1"));
        assert!(!tracker.is_active_tab("t2"));
    }

    #[test]
    fn clear_if_only_clears_a_matching_key() {
        let tracker = ActiveMemoTracker::new();
        tracker.set(ActiveMemoKey {
            tab_id: "t1".into(),
            memo_name: "Shopping list".into(),
        });

        // A stale/mismatched close request must not clobber a since-changed slot.
        tracker.clear_if("t1", "Other");
        assert!(tracker.is_active("t1", "Shopping list"));

        tracker.clear_if("t1", "Shopping list");
        assert!(!tracker.is_active("t1", "Shopping list"));
        assert!(!tracker.is_active_tab("t1"));
    }
}
