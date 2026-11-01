//! App Settings JSON store: parse-with-corruption-recovery cascade +
//! deep-partial patch application (`docs/spec.md` §5.9, §7;
//! `docs/detailed-spec.md` §6.4; `docs/plans/persistence-fs/PLAN.md` §6).
//!
//! The cascade (bak -> regenerate-with-defaults -> notify) runs on every
//! `settings_get`/`settings_set` call, not just once at process startup, per
//! `docs/plans/persistence-fs/PLAN.md` §6.3 step 1 ("On `settings_get` (and
//! at startup)..."). A `Mutex<()>` serializes the read-modify-write critical
//! section so two near-simultaneous `settings_set` calls (e.g. rapid window
//! resize) cannot race and clobber each other's patch; `atomic_write`'s
//! rename-based swap already guarantees a concurrent reader never observes a
//! partially-written file.

use std::io;
use std::sync::Mutex;

use crate::fs::app_data_root::{ensure_app_data_root, settings_json_path};
use crate::fs::atomic_write::{atomic_write, bak_path};
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::AppSettings;
use crate::model::app_settings_patch::AppSettingsPatch;

/// Which branch of the corruption-recovery cascade produced the returned
/// settings. `FromBackup`/`Regenerated` should cause the caller to emit
/// `evt:meta:recovered { scope: 'app-settings' }` (this module has no Tauri
/// `AppHandle` access, so it only reports the outcome; emitting the event is
/// the command handler's job).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryOutcome {
    Clean,
    FromBackup,
    Regenerated,
}

pub struct AppSettingsStore {
    write_lock: Mutex<()>,
}

impl AppSettingsStore {
    pub fn new() -> Self {
        Self {
            write_lock: Mutex::new(()),
        }
    }

    pub fn get(&self, registry: &SelfWriteRegistry) -> (AppSettings, RecoveryOutcome) {
        let _guard = self.write_lock.lock().expect("app settings store lock poisoned");
        load_with_recovery(registry)
    }

    pub fn patch(&self, patch: AppSettingsPatch, registry: &SelfWriteRegistry) -> io::Result<(AppSettings, RecoveryOutcome)> {
        let _guard = self.write_lock.lock().expect("app settings store lock poisoned");
        let (mut settings, outcome) = load_with_recovery(registry);
        patch.apply_to(&mut settings);
        let json = serde_json::to_vec_pretty(&settings)?;
        atomic_write(&settings_json_path(), &json, registry)?;
        Ok((settings, outcome))
    }

    /// Read-modify-write in a single critical section: `f` receives a mutable
    /// reference to the currently-persisted settings and either mutates it in
    /// place and returns `Ok(())` (persisted immediately after `f` returns),
    /// or aborts without writing by returning `Err(...)`.
    ///
    /// Used by callers that need to validate against the *current* settings
    /// before writing (e.g. `tab-management/PLAN.md`'s duplicate-name and
    /// locked-tab checks) without the check-then-write race a separate
    /// `get()` call followed by a `patch()` call would leave open (each of
    /// those two methods only locks for the duration of its own call, so a
    /// second command could interleave between them).
    pub fn modify<F, E>(&self, registry: &SelfWriteRegistry, f: F) -> io::Result<Result<(AppSettings, RecoveryOutcome), E>>
    where
        F: FnOnce(&mut AppSettings) -> Result<(), E>,
    {
        let _guard = self.write_lock.lock().expect("app settings store lock poisoned");
        let (mut settings, outcome) = load_with_recovery(registry);
        if let Err(e) = f(&mut settings) {
            return Ok(Err(e));
        }
        let json = serde_json::to_vec_pretty(&settings)?;
        atomic_write(&settings_json_path(), &json, registry)?;
        Ok(Ok((settings, outcome)))
    }
}

impl Default for AppSettingsStore {
    fn default() -> Self {
        Self::new()
    }
}

fn load_with_recovery(registry: &SelfWriteRegistry) -> (AppSettings, RecoveryOutcome) {
    // First-launch bullet (`persistence-fs/PLAN.md` §2): ensure `data/`
    // exists before attempting any read/write below.
    let _ = ensure_app_data_root();

    load_at(&settings_json_path(), registry)
}

/// `load_with_recovery` の本体(パス指定、テスト用に分離)。
/// GitHub issue #318: プライマリも `.bak` も存在しない(初回起動)ときは復旧ではなく新規作成なので
/// `Clean`(通知しない)。プライマリが壊れている、または `.bak` だけ残っている(プライマリ消失)ときは
/// 利用者のデータが失われた状況なので `FromBackup`/`Regenerated`(通知する)。
fn load_at(primary_path: &std::path::Path, registry: &SelfWriteRegistry) -> (AppSettings, RecoveryOutcome) {
    let primary_path = primary_path.to_path_buf();
    if let Some(settings) = try_parse(&primary_path) {
        return (settings, RecoveryOutcome::Clean);
    }

    let backup_path = bak_path(&primary_path);
    if let Some(settings) = try_parse(&backup_path) {
        return (settings, RecoveryOutcome::FromBackup);
    }
    let first_launch = !primary_path.exists() && !backup_path.exists();

    // Total loss: regenerate hardcoded defaults and persist them, the same
    // code path as first-launch initialization (`persistence-fs/PLAN.md` §2).
    let defaults = AppSettings::default();
    if let Ok(json) = serde_json::to_vec_pretty(&defaults) {
        let _ = atomic_write(&primary_path, &json, registry);
    }
    (defaults, if first_launch { RecoveryOutcome::Clean } else { RecoveryOutcome::Regenerated })
}

fn try_parse(path: &std::path::Path) -> Option<AppSettings> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<AppSettings>(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Points `app_data_root` at a fresh temp directory for the duration of
    /// one test by relying on the fact that `settings_json_path()` is always
    /// `app_data_root().join("settings.json")`; since `app_data_root` caches
    /// a value derived from `current_exe()` process-wide, these tests instead
    /// exercise the cascade helpers directly against explicit temp paths
    /// rather than going through the process-wide cached root.
    ///
    /// Uses a random suffix (matching the style already used by
    /// `fs::atomic_write`, `watch::watcher`, and `watch::conflict` tests)
    /// rather than a process-local `AtomicU64` counter: a counter resets to 0
    /// on every `cargo test` invocation, so a leftover `settings.json` from a
    /// previous run (nothing here ever deletes its temp dir) can collide with
    /// a same-numbered directory a later run assigns to a different test,
    /// causing spurious failures that depend on prior run history.
    fn temp_settings_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-settings-store-test-{}", rand::random::<u64>()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn parses_clean_file() {
        let path = temp_settings_path();
        let defaults = AppSettings::default();
        fs::write(&path, serde_json::to_vec(&defaults).unwrap()).unwrap();
        assert!(try_parse(&path).is_some());
    }

    #[test]
    fn falls_back_to_backup_on_corrupt_primary() {
        let path = temp_settings_path();
        fs::write(&path, b"{ not json").unwrap();
        assert!(try_parse(&path).is_none());

        let backup = bak_path(&path);
        fs::write(&backup, serde_json::to_vec(&AppSettings::default()).unwrap()).unwrap();
        assert!(try_parse(&backup).is_some());
    }

    #[test]
    fn first_launch_without_files_is_clean() {
        let path = temp_settings_path();
        let (_, outcome) = load_at(&path, &SelfWriteRegistry::new());
        assert_eq!(outcome, RecoveryOutcome::Clean);
        assert!(path.exists());
    }

    #[test]
    fn corrupt_primary_without_backup_is_regenerated() {
        let path = temp_settings_path();
        fs::write(&path, b"{ not json").unwrap();
        let (_, outcome) = load_at(&path, &SelfWriteRegistry::new());
        assert_eq!(outcome, RecoveryOutcome::Regenerated);
    }

    #[test]
    fn corrupt_primary_with_backup_is_from_backup() {
        let path = temp_settings_path();
        fs::write(&path, b"{ not json").unwrap();
        fs::write(bak_path(&path), serde_json::to_vec(&AppSettings::default()).unwrap()).unwrap();
        let (_, outcome) = load_at(&path, &SelfWriteRegistry::new());
        assert_eq!(outcome, RecoveryOutcome::FromBackup);
    }

    #[test]
    fn missing_file_is_not_an_error_just_none() {
        let path = temp_settings_path();
        assert!(try_parse(&path).is_none());
    }
}
