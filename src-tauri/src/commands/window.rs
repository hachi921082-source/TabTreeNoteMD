//! Window-level commands that are not tied to the tab domain's persisted
//! state. The global 📌 always-on-top toggle
//! (`docs/plans/tab-management/PLAN.md` §4.1, "Toggle window always-on-top";
//! `docs/DECISIONS.md` "'Pin' Button: Global Window Always-On-Top Toggle,
//! Not Per-Tab") is session-only: the caller (frontend) owns the boolean
//! state and that command never reads or writes `AppSettings`/settings.json.
//! `window_is_tray_launch` (`docs/plans/window-operation/PLAN.md` §4) is a
//! stateless read of the process's own argv.

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

/// Whether this process was launched with the `--tray` argument
/// (`docs/plans/window-operation/PLAN.md` §4: the Startup-entry-registered
/// launch that should start hidden instead of showing the window
/// immediately). Read directly from `std::env::args()` — process-global,
/// no shared state needed. `state/useWindowGeometry.ts` checks this before
/// revealing the main window once it has restored its persisted
/// position/size.
#[tauri::command]
pub fn window_is_tray_launch() -> AppResult<bool> {
    AppResult::ok(std::env::args().any(|arg| arg == "--tray"))
}
