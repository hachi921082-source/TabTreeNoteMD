//! Static "About"-style read-only info exposed to the frontend. Currently just the third-party
//! license notices (Settings > General > license link at the very bottom, `SettingsDialog.tsx`),
//! required so a closed-source build still satisfies the MIT/Apache-2.0/ISC/BSD-3-Clause notice-
//! preservation terms of its bundled dependencies.
//!
//! The notices text is baked into the binary at compile time via `include_str!` rather than read
//! from disk at runtime: `THIRD-PARTY-NOTICES.md` lives at the repo root, outside anything Tauri's
//! `bundle.resources` would otherwise need wiring up, and a static compliance document has no
//! reason to be editable/re-readable after the app is built anyway.

use crate::model::result::AppResult;

/// Never actually fails (the text is compiled in), but still wrapped in `AppResult` per this
/// crate's "every command returns `AppResult<T>`" convention (`model/result.rs`), so the frontend
/// side can use the same `Result<T>` handling as every other IPC call instead of a special case.
#[tauri::command]
pub fn about_get_third_party_notices() -> AppResult<String> {
    AppResult::ok(include_str!("../../../THIRD-PARTY-NOTICES.md").to_string())
}
