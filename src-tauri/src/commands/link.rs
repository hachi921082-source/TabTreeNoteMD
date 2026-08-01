//! Minimal external-URL-open primitive, reserved for `link/PLAN.md` (not yet implemented) in the
//! command master inventory (`docs/plans/project-architecture/PLAN.md` §4: `link_open_external`).
//!
//! Only the OS-standard-open pass-through is implemented here — `docs/plans/editor-save/PLAN.md`
//! §4's "Search with Google" right-click action is the one consumer today. Everything else
//! `link/PLAN.md` actually owns (`memo:` link resolution, `AppSettings.links.confirmBeforeOpen`
//! gating, email/file-path special-casing) is intentionally NOT implemented here — this is a
//! narrow forward seam, not `link/PLAN.md`'s feature itself.

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::model::result::{AppResult, ErrorCode};

/// Opens `url` with the OS's default handler (`tauri_plugin_opener`'s `open_url`, matching
/// `commands/tab.rs`'s existing `tab_open_location` use of the same plugin for "reveal in file
/// manager"), rather than any shell string concatenation (`[spec §2.1]`).
#[tauri::command]
pub fn link_open_external(app: AppHandle, url: String) -> AppResult<()> {
    match app.opener().open_url(&url, None::<String>) {
        Ok(()) => AppResult::ok(()),
        Err(err) => {
            tracing::error!(error = %err, url, "link_open_external: failed to open URL");
            AppResult::err_code(ErrorCode::Unknown, "failed to open the external link")
        }
    }
}
