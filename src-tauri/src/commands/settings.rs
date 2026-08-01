//! `settings_get` / `settings_set` Tauri commands
//! (`docs/plans/project-architecture/PLAN.md` §4; `docs/plans/persistence-fs/PLAN.md` §6).

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::AppSettings;
use crate::model::app_settings_patch::AppSettingsPatch;
use crate::model::result::{AppResult, ErrorCode};
use crate::settings::app_settings_store::{AppSettingsStore, RecoveryOutcome};

#[derive(Serialize, Clone)]
struct MetaRecoveredPayload {
    scope: &'static str,
}

/// Emits `evt:meta:recovered { scope: 'app-settings' }`
/// (`docs/plans/persistence-fs/PLAN.md` §6.3) when the cascade had to fall
/// back to `.bak` or hardcoded defaults. A `Clean` read is silent.
fn emit_recovery_event(app: &AppHandle, outcome: RecoveryOutcome) {
    if outcome == RecoveryOutcome::Clean {
        return;
    }
    if let Err(err) = app.emit("evt:meta:recovered", MetaRecoveredPayload { scope: "app-settings" }) {
        tracing::error!(error = %err, "failed to emit evt:meta:recovered");
    }
}

#[tauri::command]
pub fn settings_get(app: AppHandle, store: State<AppSettingsStore>, registry: State<SelfWriteRegistry>) -> AppResult<AppSettings> {
    let (settings, outcome) = store.get(&registry);
    emit_recovery_event(&app, outcome);
    AppResult::ok(settings)
}

#[tauri::command]
pub fn settings_set(
    app: AppHandle,
    patch: AppSettingsPatch,
    store: State<AppSettingsStore>,
    registry: State<SelfWriteRegistry>,
) -> AppResult<AppSettings> {
    match store.patch(patch, &registry) {
        Ok((settings, outcome)) => {
            emit_recovery_event(&app, outcome);
            AppResult::ok(settings)
        }
        Err(err) => {
            tracing::error!(error = %err, "settings_set: failed to persist settings.json");
            AppResult::err_code(ErrorCode::Unknown, "failed to save settings")
        }
    }
}
