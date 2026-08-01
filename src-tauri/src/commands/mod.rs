//! One module per domain, Tauri `#[command]` fns
//! (`docs/plans/project-architecture/PLAN.md` §2). Domains other than
//! `settings`/`tab`/`window`/`tree` (editor, search, link, image, lock) are
//! added by their own feature `PLAN.md` when implemented.

pub mod settings;
pub mod tab;
pub mod tree;
pub mod window;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::settings::app_settings_store::RecoveryOutcome;

#[derive(Serialize, Clone)]
struct MetaRecoveredPayload {
    scope: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tab_id: Option<String>,
}

/// Emits `evt:meta:recovered` (`docs/plans/persistence-fs/PLAN.md` §6.3,
/// `docs/plans/project-architecture/PLAN.md` §5) with the given `scope`
/// (`'app-settings'` or `'tab'`) and, for the `'tab'` scope, the affected
/// `tab_id`. Shared by every command module whose corruption-recovery
/// cascade can trigger on any read/write, not just the domain's own
/// "get"/"set" command.
pub(crate) fn emit_meta_recovered(app: &AppHandle, scope: &'static str, tab_id: Option<String>) {
    if let Err(err) = app.emit("evt:meta:recovered", MetaRecoveredPayload { scope, tab_id }) {
        tracing::error!(error = %err, "failed to emit evt:meta:recovered");
    }
}

/// `emit_meta_recovered(app, "app-settings", None)`, gated on the
/// `AppSettingsStore` recovery outcome — kept as its own function since this
/// is by far the most common call site (every command that touches the
/// settings store).
pub(crate) fn emit_settings_recovery_event(app: &AppHandle, outcome: RecoveryOutcome) {
    if outcome == RecoveryOutcome::Clean {
        return;
    }
    emit_meta_recovered(app, "app-settings", None);
}
