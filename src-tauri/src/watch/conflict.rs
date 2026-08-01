//! Composite mtime + size + hash conflict classification
//! (`docs/spec.md` §3.3, §5.5; `docs/detailed-spec.md` §6.4;
//! `docs/plans/persistence-fs/PLAN.md` §5).
//!
//! Distinguishes a genuine external content change from e.g. an editor or
//! backup tool touching a file's mtime without changing its content. This is
//! the watcher-side composite check only; the save-time conflict check
//! (`editor_save`, elaborated in `editor-save/PLAN.md`) is a separate, simpler
//! pure-hash comparison with no mtime/size involved.

use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSnapshot {
    pub mtime: SystemTime,
    pub size: u64,
    pub hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeClassification {
    /// Neither size, mtime, nor content differ from the previous snapshot.
    Unchanged,
    /// mtime (and/or size) differ, but the content hash is identical — an
    /// editor/backup tool touch, not a genuine change. Ignored for
    /// reconciliation/index/conflict-event purposes.
    TouchedOnly { mtime: SystemTime },
    /// Content hash differs from the previous snapshot: a genuine external
    /// change. Callers (memo-tree reconciliation, search index patch,
    /// `evt:fs:external-change`/`evt:editor:conflict`) act on this variant.
    GenuineChange { snapshot: FileSnapshot },
}

pub fn compute_snapshot(path: &Path) -> io::Result<FileSnapshot> {
    let metadata = fs::metadata(path)?;
    let mtime = metadata.modified()?;
    let size = metadata.len();
    let hash = hash_file(path)?;
    Ok(FileSnapshot { mtime, size, hash })
}

/// Classifies the current on-disk state of `path` against `previous`. mtime
/// and size are used as a fast pre-filter (both unchanged => skip hashing
/// entirely); otherwise the content hash is the decisive signal.
pub fn classify_change(previous: &FileSnapshot, path: &Path) -> io::Result<ChangeClassification> {
    let metadata = fs::metadata(path)?;
    let current_size = metadata.len();
    let current_mtime = metadata.modified()?;

    if current_size == previous.size && current_mtime == previous.mtime {
        return Ok(ChangeClassification::Unchanged);
    }

    let current_hash = hash_file(path)?;
    if current_hash == previous.hash {
        Ok(ChangeClassification::TouchedOnly { mtime: current_mtime })
    } else {
        Ok(ChangeClassification::GenuineChange {
            snapshot: FileSnapshot {
                mtime: current_mtime,
                size: current_size,
                hash: current_hash,
            },
        })
    }
}

fn hash_file(path: &Path) -> io::Result<[u8; 32]> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::thread::sleep;
    use std::time::Duration;

    fn tempfile(content: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("ttn-conflict-test-{}.md", rand::random::<u64>()));
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn identical_content_and_stat_is_unchanged() {
        let path = tempfile(b"hello");
        let snapshot = compute_snapshot(&path).unwrap();
        assert_eq!(classify_change(&snapshot, &path).unwrap(), ChangeClassification::Unchanged);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn touching_mtime_without_changing_content_is_touched_only() {
        let path = tempfile(b"hello");
        let snapshot = compute_snapshot(&path).unwrap();

        // Rewrite identical content; mtime advances but hash/size don't change.
        sleep(Duration::from_millis(10));
        fs::write(&path, b"hello").unwrap();

        match classify_change(&snapshot, &path).unwrap() {
            ChangeClassification::TouchedOnly { .. } => {}
            other => panic!("expected TouchedOnly, got {other:?}"),
        }
        fs::remove_file(&path).ok();
    }

    #[test]
    fn changed_content_is_genuine_change() {
        let path = tempfile(b"hello");
        let snapshot = compute_snapshot(&path).unwrap();

        fs::write(&path, b"hello world").unwrap();

        match classify_change(&snapshot, &path).unwrap() {
            ChangeClassification::GenuineChange { .. } => {}
            other => panic!("expected GenuineChange, got {other:?}"),
        }
        fs::remove_file(&path).ok();
    }
}
