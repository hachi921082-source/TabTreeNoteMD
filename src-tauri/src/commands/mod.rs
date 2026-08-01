//! One module per domain, Tauri `#[command]` fns
//! (`docs/plans/project-architecture/PLAN.md` §2). Domains other than
//! `settings`/`tab`/`window` (tree, editor, search, link, image, lock) are
//! added by their own feature `PLAN.md` when implemented.

pub mod settings;
pub mod tab;
pub mod window;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::settings::app_settings_store::RecoveryOutcome;

#[derive(Serialize, Clone)]
struct MetaRecoveredPayload {
    scope: &'static str,
}

/// Emits `evt:meta:recovered { scope: 'app-settings' }`
/// (`docs/plans/persistence-fs/PLAN.md` §6.3) whenever a command's read of
/// `AppSettings` via `AppSettingsStore` had to fall back to `.bak` or
/// hardcoded defaults. Shared by every command module that touches the
/// settings store (originally only `commands::settings`, now also
/// `commands::tab`), since the corruption-recovery cascade can trigger on
/// any access, not just `settings_get`/`settings_set`.
pub(crate) fn emit_settings_recovery_event(app: &AppHandle, outcome: RecoveryOutcome) {
    if outcome == RecoveryOutcome::Clean {
        return;
    }
    if let Err(err) = app.emit("evt:meta:recovered", MetaRecoveredPayload { scope: "app-settings" }) {
        tracing::error!(error = %err, "failed to emit evt:meta:recovered");
    }
}
