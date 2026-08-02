//! Auto-launch-to-tray-on-Windows-login setting
//! (`docs/plans/window-operation/PLAN.md` §4). Wraps `tauri-plugin-autostart`'s
//! `AutoLaunchManager` (which registers a `HKCU\...\Run` registry entry
//! pointing at the exe with a `--tray` argument on Windows — see
//! `Cargo.toml`'s dependency comment for why a registry Run-key is
//! acceptable here per that plan's own fallback clause) behind this app's
//! own `AppResult`/settings-store conventions, rather than exposing the
//! plugin's own JS-callable commands directly, matching how every other OS
//! integration in this app (dialogs, opener) is wrapped.
//!
//! The persisted `AppSettings.autostart.enabled` flag mirrors "what the user
//! last asked for" — it is written here on every successful toggle, but is
//! not read back from the OS to detect drift (e.g. a user manually deleting
//! the registry entry outside the app). Treated the same as
//! `TabSettingsEntry.locked`'s "display cache only" caveat.

use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::commands::emit_settings_recovery_event;
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::AppSettings;
use crate::model::app_settings_patch::{AppSettingsPatch, AutostartSettingsPatch};
use crate::model::result::{AppResult, ErrorCode};
use crate::settings::app_settings_store::AppSettingsStore;

#[tauri::command]
pub fn autostart_set_enabled(
    app: AppHandle,
    enabled: bool,
    store: State<AppSettingsStore>,
    registry: State<SelfWriteRegistry>,
) -> AppResult<AppSettings> {
    let manager = app.autolaunch();
    let os_result = if enabled { manager.enable() } else { manager.disable() };
    if let Err(err) = os_result {
        tracing::error!(error = %err, enabled, "autostart_set_enabled: OS registration failed");
        return AppResult::err_code(ErrorCode::Unknown, "failed to change the auto-launch setting");
    }

    let patch = AppSettingsPatch {
        autostart: Some(AutostartSettingsPatch { enabled: Some(enabled) }),
        ..Default::default()
    };
    match store.patch(patch, &registry) {
        Ok((settings, outcome)) => {
            emit_settings_recovery_event(&app, outcome);
            AppResult::ok(settings)
        }
        Err(err) => {
            tracing::error!(error = %err, "autostart_set_enabled: failed to persist settings.json");
            AppResult::err_code(ErrorCode::Unknown, "failed to save settings")
        }
    }
}
