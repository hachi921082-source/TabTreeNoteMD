//! `settings_get` / `settings_set` Tauri commands
//! (`docs/plans/project-architecture/PLAN.md` §4; `docs/plans/persistence-fs/PLAN.md` §6).

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::commands::emit_settings_recovery_event;
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::AppSettings;
use crate::model::app_settings_patch::AppSettingsPatch;
use crate::model::result::{AppResult, ErrorCode};
use crate::settings::app_settings_store::AppSettingsStore;

#[tauri::command]
pub fn settings_get(app: AppHandle, store: State<AppSettingsStore>, registry: State<Arc<SelfWriteRegistry>>) -> AppResult<AppSettings> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    AppResult::ok(settings)
}

#[tauri::command]
pub fn settings_set(
    app: AppHandle,
    patch: AppSettingsPatch,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
) -> AppResult<AppSettings> {
    match store.patch(patch, &registry) {
        Ok((settings, outcome)) => {
            emit_settings_recovery_event(&app, outcome);
            AppResult::ok(settings)
        }
        Err(err) => {
            tracing::error!(error = %err, "settings_set: failed to persist settings.json");
            AppResult::err_code(ErrorCode::Unknown, "failed to save settings")
        }
    }
}
