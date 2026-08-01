//! Tracks which memo is currently open in the editor, so the watcher's
//! conflict pipeline can decide whether a genuine external change should
//! trigger `evt:editor:conflict` (in addition to, or instead of, generic
//! reconciliation) — `docs/plans/persistence-fs/PLAN.md` §5.
//!
//! Set by `editor_load` and cleared on switch-away
//! (`docs/plans/editor-save/PLAN.md`, not yet implemented — this module only
//! provides the shared tracking primitive those commands will call into).

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

    pub fn is_active(&self, tab_id: &str, memo_name: &str) -> bool {
        match &*self.current.lock().expect("active memo tracker poisoned") {
            Some(key) => key.tab_id == tab_id && key.memo_name == memo_name,
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
}
