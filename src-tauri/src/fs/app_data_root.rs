//! Portable app-data layout (`docs/spec.md` §2.6; `docs/detailed-spec.md` §3.1;
//! `docs/plans/persistence-fs/PLAN.md` §2).
//!
//! Resolves a folder next to the running executable (`<exe_dir>/data/`), not
//! `%APPDATA%`, so copying the exe alone makes settings/tab-registration a
//! fully independent, portable profile.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static APP_DATA_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Resolves (on first call) and then reuses the `data/` folder next to the
/// running executable. Computed once and cached — a portable copy's exe
/// location doesn't change while running, so there is no need to repeat the
/// `current_exe()` lookup mid-session.
pub fn app_data_root() -> &'static Path {
    APP_DATA_ROOT.get_or_init(|| resolve_app_data_root().expect("failed to resolve app-adjacent data directory"))
}

fn resolve_app_data_root() -> io::Result<PathBuf> {
    let exe_path = std::env::current_exe()?;
    // `current_exe()` resolves symlinks/shortcuts down to the real exe path
    // per the standard library's documented behavior, so this is correct
    // whether launched via a shortcut or directly.
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "executable has no parent directory"))?;
    Ok(exe_dir.join("data"))
}

/// Path to the app settings JSON file under the resolved data root.
pub fn settings_json_path() -> PathBuf {
    app_data_root().join("settings.json")
}

/// Ensures the data root directory exists (first-launch case), creating it
/// if necessary. Does not create `settings.json` itself — that is the
/// corruption-recovery cascade's job (`settings/app_settings_store.rs`),
/// reusing the same "regenerate with defaults" code path either way.
pub fn ensure_app_data_root() -> io::Result<()> {
    std::fs::create_dir_all(app_data_root())
}

/// Default parent folder for a brand-new "Create new tab" folder (`docs/plans/tab-management/PLAN.md`
/// §2 `tab_create_new`, GitHub issue #2) — `data/tabs/`, kept separate from `data/settings.json` so a
/// user's tab folders don't sit directly alongside the app's own internal settings file.
pub fn tabs_root_dir() -> PathBuf {
    app_data_root().join("tabs")
}
