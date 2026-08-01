//! Window-level commands that are not tied to the tab domain's persisted
//! state. Currently just the global 📌 always-on-top toggle
//! (`docs/plans/tab-management/PLAN.md` §4.1, "Toggle window always-on-top";
//! `docs/DECISIONS.md` "'Pin' Button: Global Window Always-On-Top Toggle,
//! Not Per-Tab"). Session-only: the caller (frontend) owns the boolean state
//! and this command never reads or writes `AppSettings`/settings.json.

use tauri::WebviewWindow;

use crate::model::result::{AppResult, ErrorCode};

/// Toggles OS-level "always on top" for the whole application window
/// (Tauri v2's `WebviewWindow::set_always_on_top`, the equivalent of
/// Electron's `BrowserWindow.setAlwaysOnTop` used session-only by the legacy
/// repo). Not per-tab, not persisted — `enabled` is the frontend's current
/// session-only toggle state, sent as-is on every click.
#[tauri::command]
pub fn window_set_always_on_top(window: WebviewWindow, enabled: bool) -> AppResult<()> {
    match window.set_always_on_top(enabled) {
        Ok(()) => AppResult::ok(()),
        Err(err) => {
            tracing::error!(error = %err, enabled, "window_set_always_on_top: OS call failed");
            AppResult::err_code(ErrorCode::Unknown, "failed to change the always-on-top window state")
        }
    }
}
