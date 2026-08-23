//! Maps tab id -> currently-active `FolderWatcher`, owning the start/stop lifecycle for
//! ordinary (non-locked) tabs (`docs/plans/persistence-fs/PLAN.md` §5; GitHub issue #209).
//!
//! Wired at three points: app startup (one watcher per registered, non-locked tab), new-tab
//! registration, and tab removal. Locked-tab session-scoped start/stop is a separate concern
//! (issue #210), not handled here — locked tabs simply never get an entry in this map.
//!
//! The `on_external_change` callback passed to `FolderWatcher::watch` here is a no-op logging
//! placeholder: the conflict-check/event-emission pipeline it should eventually drive
//! (`watch/conflict.rs`, `evt:fs:external-change`, `evt:editor:conflict`) is issue #211's job.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::watch::watcher::FolderWatcher;

pub struct WatcherRegistry {
    active: Mutex<HashMap<String, FolderWatcher>>,
}

impl WatcherRegistry {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(HashMap::new()),
        }
    }

    /// Starts watching `folder` for `tab_id`, replacing any watcher already tracked for it.
    /// Best-effort: a failure to start the underlying OS watch is logged and otherwise ignored,
    /// same as other non-critical startup/registration side effects in this codebase.
    pub fn start(&self, tab_id: &str, folder: &Path, self_write_registry: Arc<SelfWriteRegistry>) {
        let owned_tab_id = tab_id.to_string();
        let watcher = match FolderWatcher::watch(folder, self_write_registry, move |event| {
            // Placeholder pending issue #211's conflict-check/event-emission wiring.
            tracing::debug!(tab_id = %owned_tab_id, ?event, "external change detected (not yet wired to conflict check)");
        }) {
            Ok(watcher) => watcher,
            Err(err) => {
                tracing::error!(error = %err, path = %folder.display(), tab_id, "failed to start folder watcher for tab");
                return;
            }
        };
        self.active.lock().expect("watcher registry poisoned").insert(tab_id.to_string(), watcher);
    }

    /// Stops (drops) the watcher tracked for `tab_id`, if any. A no-op if none is tracked.
    pub fn stop(&self, tab_id: &str) {
        self.active.lock().expect("watcher registry poisoned").remove(tab_id);
    }

    /// `true` if a watcher is currently tracked for `tab_id`.
    pub fn is_watching(&self, tab_id: &str) -> bool {
        self.active.lock().expect("watcher registry poisoned").contains_key(tab_id)
    }
}

impl Default for WatcherRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-watcher-registry-test-{}", rand::random::<u64>()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn start_then_stop_tracks_membership() {
        let dir = tempdir();
        let registry = WatcherRegistry::new();
        assert!(!registry.is_watching("tab-1"));

        registry.start("tab-1", &dir, Arc::new(SelfWriteRegistry::new()));
        assert!(registry.is_watching("tab-1"));

        registry.stop("tab-1");
        assert!(!registry.is_watching("tab-1"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn starting_again_for_the_same_tab_replaces_the_previous_watcher() {
        let dir = tempdir();
        let registry = WatcherRegistry::new();
        registry.start("tab-1", &dir, Arc::new(SelfWriteRegistry::new()));
        assert!(registry.is_watching("tab-1"));

        // Re-registration (e.g. re-running the startup loop) must not leave two watchers
        // stacked or panic — the previous entry is simply replaced.
        registry.start("tab-1", &dir, Arc::new(SelfWriteRegistry::new()));
        assert!(registry.is_watching("tab-1"));

        registry.stop("tab-1");
        assert!(!registry.is_watching("tab-1"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stop_on_an_untracked_tab_is_a_no_op() {
        let registry = WatcherRegistry::new();
        registry.stop("missing");
        assert!(!registry.is_watching("missing"));
    }

    #[test]
    fn lookup_is_independent_per_tab_id() {
        let dir_a = tempdir();
        let dir_b = tempdir();
        let registry = WatcherRegistry::new();

        registry.start("tab-a", &dir_a, Arc::new(SelfWriteRegistry::new()));
        assert!(registry.is_watching("tab-a"));
        assert!(!registry.is_watching("tab-b"));

        registry.start("tab-b", &dir_b, Arc::new(SelfWriteRegistry::new()));
        assert!(registry.is_watching("tab-a"));
        assert!(registry.is_watching("tab-b"));

        registry.stop("tab-a");
        assert!(!registry.is_watching("tab-a"));
        assert!(registry.is_watching("tab-b"));

        fs::remove_dir_all(&dir_a).ok();
        fs::remove_dir_all(&dir_b).ok();
    }
}
