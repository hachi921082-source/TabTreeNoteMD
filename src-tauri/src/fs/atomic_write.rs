//! Atomic write — temp -> bak -> target (`docs/spec.md` §5.2;
//! `docs/detailed-spec.md` §6.1; `docs/plans/persistence-fs/PLAN.md` §3).
//!
//! Single shared function used by every persistence call site in the app
//! (memo body writes, tab metadata JSON, app settings JSON, tab-lock
//! obfuscated files). No ad-hoc `fs::write`/`rename` anywhere else in the
//! codebase for these file classes — the only legitimate exceptions are
//! brand-new image writes (no prior version to `.bak`-swap) and trash
//! operations (a delete, not a write), see `fs/trash.rs`.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use super::retry::with_retry;
use super::self_write_registry::SelfWriteRegistry;
use super::temp_naming::generate_temp_file_name;

/// `p` with `.bak` appended to the full file name, e.g.
/// `Shopping list.md` -> `Shopping list.md.bak`.
pub fn bak_path(p: &Path) -> PathBuf {
    let mut name = p.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    p.with_file_name(name)
}

/// Writes `content` to `target_path`, going through a temp file in the same
/// directory and rotating the previous target into a single-generation
/// `.bak`. Registers both the temp file's rename destinations with
/// `registry` *before* performing each FS operation, so the watcher can
/// recognize these as self-writes rather than external changes.
pub fn atomic_write(target_path: &Path, content: &[u8], registry: &SelfWriteRegistry) -> io::Result<()> {
    let dir = target_path.parent().unwrap_or_else(|| Path::new("."));
    let temp_path = dir.join(generate_temp_file_name());

    // 1-2. Write + flush/fsync the temp file. Temp-file churn is never
    // registered (fs/temp_naming.rs is filtered out before registry lookup
    // by the watcher), so no `registry.register` call for `temp_path` itself.
    {
        let mut file = File::create(&temp_path)?;
        use std::io::Write;
        file.write_all(content)?;
        file.sync_all()?;
    }

    let bak = bak_path(target_path);

    // 3-4. Rotate the previous target into `.bak` (only one generation is
    // ever kept; ignore NotFound for a first-ever save).
    registry.register(target_path);
    registry.register(&bak);
    match with_retry(|| fs::rename(target_path, &bak)) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }

    // 5. Promote the temp file to the target path.
    registry.register(target_path);
    with_retry(|| fs::rename(&temp_path, target_path))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn bak_path_appends_suffix_to_full_file_name() {
        assert_eq!(bak_path(Path::new("C:/tab/Shopping list.md")), PathBuf::from("C:/tab/Shopping list.md.bak"));
        assert_eq!(
            bak_path(Path::new("C:/tab/.tabtreenote.json")),
            PathBuf::from("C:/tab/.tabtreenote.json.bak")
        );
    }

    #[test]
    fn first_write_creates_target_with_no_prior_bak() {
        let dir = tempdir();
        let target = dir.join("note.md");
        let registry = SelfWriteRegistry::new();

        atomic_write(&target, b"hello", &registry).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"hello");
        assert!(!bak_path(&target).exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn second_write_rotates_previous_content_into_bak() {
        let dir = tempdir();
        let target = dir.join("note.md");
        let registry = SelfWriteRegistry::new();

        atomic_write(&target, b"first", &registry).unwrap();
        atomic_write(&target, b"second", &registry).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"second");
        assert_eq!(fs::read(bak_path(&target)).unwrap(), b"first");

        fs::remove_dir_all(&dir).ok();
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-atomic-write-test-{}", rand::random::<u64>()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
