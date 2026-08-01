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

    let primary_path = settings_json_path();
    if let Some(settings) = try_parse(&primary_path) {
        return (settings, RecoveryOutcome::Clean);
    }

    let backup_path = bak_path(&primary_path);
    if let Some(settings) = try_parse(&backup_path) {
        return (settings, RecoveryOutcome::FromBackup);
    }

    // Total loss: regenerate hardcoded defaults and persist them, the same
    // code path as first-launch initialization (`persistence-fs/PLAN.md` §2).
    let defaults = AppSettings::default();
    if let Ok(json) = serde_json::to_vec_pretty(&defaults) {
        let _ = atomic_write(&primary_path, &json, registry);
    }
    (defaults, RecoveryOutcome::Regenerated)
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Points `app_data_root` at a fresh temp directory for the duration of
    /// one test by relying on the fact that `settings_json_path()` is always
    /// `app_data_root().join("settings.json")`; since `app_data_root` caches
    /// a value derived from `current_exe()` process-wide, these tests instead
    /// exercise the cascade helpers directly against explicit temp paths
    /// rather than going through the process-wide cached root.
    fn temp_settings_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ttn-settings-store-test-{n}"));
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
    fn missing_file_is_not_an_error_just_none() {
        let path = temp_settings_path();
        assert!(try_parse(&path).is_none());
    }
}
