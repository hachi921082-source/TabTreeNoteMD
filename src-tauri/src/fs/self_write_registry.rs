//! Self-write registry — suppresses false external-change events caused by
//! the app's own writes (`docs/spec.md` §5.4; `docs/detailed-spec.md` §6.2;
//! `docs/plans/persistence-fs/PLAN.md` §4).
//!
//! Registration is centralized: every FS-mutating call in `fs/`,
//! `crypto/obfuscated_naming.rs`, and `fs/trash.rs` registers its target
//! path(s) immediately before performing the operation. Command handlers
//! never call the underlying FS primitives directly without going through
//! one of those already-registering functions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// TTL fallback for entries that are registered but never consumed (e.g. the
/// watcher backend coalesces/drops the corresponding event) — `2.5s` per
/// `docs/detailed-spec.md` §3.7.
const SELF_WRITE_TTL: Duration = Duration::from_millis(2500);

pub struct SelfWriteRegistry {
    entries: Mutex<HashMap<PathBuf, Instant>>,
}

impl SelfWriteRegistry {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Call BEFORE performing the FS operation on `path`.
    pub fn register(&self, path: &Path) {
        let mut entries = self.entries.lock().expect("self-write registry poisoned");
        entries.insert(path.to_path_buf(), Instant::now());
        Self::sweep_expired_locked(&mut entries);
    }

    /// Called by the watcher when a raw event for `path` arrives. Returns
    /// `true` (and consumes the entry) if this event is attributable to our
    /// own write, i.e. should be discarded rather than treated as an
    /// external change.
    pub fn consume_if_present(&self, path: &Path) -> bool {
        let mut entries = self.entries.lock().expect("self-write registry poisoned");
        Self::sweep_expired_locked(&mut entries);
        entries.remove(path).is_some()
    }

    fn sweep_expired_locked(entries: &mut HashMap<PathBuf, Instant>) {
        let now = Instant::now();
        entries.retain(|_, inserted_at| now.duration_since(*inserted_at) < SELF_WRITE_TTL);
    }
}

impl Default for SelfWriteRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn consume_matches_registered_path() {
        let registry = SelfWriteRegistry::new();
        let path = PathBuf::from("C:/tab/Shopping list.md");
        registry.register(&path);
        assert!(registry.consume_if_present(&path));
        // Second consume for the same path finds nothing (already removed).
        assert!(!registry.consume_if_present(&path));
    }

    #[test]
    fn unregistered_path_is_not_consumed() {
        let registry = SelfWriteRegistry::new();
        let path = PathBuf::from("C:/tab/Untouched.md");
        assert!(!registry.consume_if_present(&path));
    }

    #[test]
    fn ttl_expiry_drops_stale_entries() {
        let registry = SelfWriteRegistry::new();
        let path = PathBuf::from("C:/tab/Slow watcher.md");
        {
            let mut entries = registry.entries.lock().unwrap();
            // Simulate a registration that is already older than the TTL.
            entries.insert(path.clone(), Instant::now() - Duration::from_millis(3000));
        }
        sleep(Duration::from_millis(1));
        assert!(!registry.consume_if_present(&path));
    }
}
