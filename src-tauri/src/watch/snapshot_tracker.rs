//! Per-watcher baseline `FileSnapshot`s for `watch/conflict.rs`'s `classify_change`
//! (`docs/plans/persistence-fs/PLAN.md` §5, GitHub issue #211).
//!
//! Keyed by memo_name (file stem), not full path: a non-recursive watch means every path is
//! always a direct child of the one tab folder a given tracker instance is scoped to, so the file
//! stem alone is already unambiguous — matching `NodePath`/`memo_name`'s existing meaning
//! elsewhere (`model/tree.rs`'s `NodePath`, `commands/editor.rs`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use super::conflict::{compute_snapshot, FileSnapshot};

pub struct SnapshotTracker {
    snapshots: Mutex<HashMap<String, FileSnapshot>>,
}

impl SnapshotTracker {
    pub fn new() -> Self {
        Self { snapshots: Mutex::new(HashMap::new()) }
    }

    /// Scans `folder`'s direct-child `.md` files once, seeding a starting baseline for each —
    /// mirrors `SearchIndex::rebuild_all`'s "scan once at watch start" pattern. Best-effort: an
    /// unreadable file (permissions, vanished mid-scan) is simply left unseeded, same fallback
    /// `route_change`'s "no baseline yet" branch already handles for any other reason a baseline
    /// might be missing.
    pub fn seed(&self, folder: &Path) {
        let mut snapshots = self.snapshots.lock().expect("snapshot tracker poisoned");
        let Ok(read_dir) = std::fs::read_dir(folder) else { return };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !is_md_file(&path) {
                continue;
            }
            let Some(memo_name) = memo_name_of(&path) else { continue };
            if let Ok(snapshot) = compute_snapshot(&path) {
                snapshots.insert(memo_name, snapshot);
            }
        }
    }

    pub fn get(&self, memo_name: &str) -> Option<FileSnapshot> {
        self.snapshots.lock().expect("snapshot tracker poisoned").get(memo_name).cloned()
    }

    pub fn set(&self, memo_name: &str, snapshot: FileSnapshot) {
        self.snapshots.lock().expect("snapshot tracker poisoned").insert(memo_name.to_string(), snapshot);
    }

    pub fn remove(&self, memo_name: &str) {
        self.snapshots.lock().expect("snapshot tracker poisoned").remove(memo_name);
    }
}

impl Default for SnapshotTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// `true` for a `.md` path (case-insensitive extension match) — everything else (`.tabtreenote.json`,
/// temp files already filtered upstream by `watcher.rs`, etc.) is outside this pipeline's concern.
pub fn is_md_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("md")).unwrap_or(false)
}

/// The file stem, i.e. `memo_name` in this codebase's usual sense (`model/tree.rs`'s `NodePath`).
pub fn memo_name_of(path: &Path) -> Option<String> {
    path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-snapshot-tracker-test-{}", rand::random::<u64>()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn seed_only_picks_up_direct_child_md_files() {
        let dir = tempdir();
        fs::write(dir.join("a.md"), b"hello").unwrap();
        fs::write(dir.join(".tabtreenote.json"), b"{}").unwrap();
        fs::write(dir.join("b.txt"), b"not markdown").unwrap();

        let tracker = SnapshotTracker::new();
        tracker.seed(&dir);

        assert!(tracker.get("a").is_some());
        assert!(tracker.get(".tabtreenote").is_none());
        assert!(tracker.get("b").is_none());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn set_then_get_then_remove_round_trips() {
        let dir = tempdir();
        let path = dir.join("a.md");
        fs::write(&path, b"hello").unwrap();
        let snapshot = compute_snapshot(&path).unwrap();

        let tracker = SnapshotTracker::new();
        assert!(tracker.get("a").is_none());

        tracker.set("a", snapshot.clone());
        assert_eq!(tracker.get("a"), Some(snapshot));

        tracker.remove("a");
        assert!(tracker.get("a").is_none());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn is_md_file_is_case_insensitive_and_extension_only() {
        assert!(is_md_file(Path::new("a.md")));
        assert!(is_md_file(Path::new("a.MD")));
        assert!(!is_md_file(Path::new(".tabtreenote.json")));
        assert!(!is_md_file(Path::new("a")));
    }
}
