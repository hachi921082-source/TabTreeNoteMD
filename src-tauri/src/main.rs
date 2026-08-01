// Prevents an additional console window on Windows in release builds, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// This crate wires up `settings_get`/`settings_set` (persistence-fs/PLAN.md),
// the tab domain (tab-management/PLAN.md, everything except lock
// operations), and the session-only window always-on-top toggle
// (tab-management/PLAN.md §4.1). Several foundational primitives built here for OTHER
// features to call — the folder watcher
// (`watch::watcher`/`watch::conflict`/`watch::active_memo`), `fs::trash`,
// and the `IoLocked`/`TrashFailed`/`DecryptFailed`/`Conflict`/`MetaCorrupt`/
// `LongPath` `ErrorCode` variants — are intentionally not yet referenced;
// they are consumed once `memo-tree/PLAN.md`, `editor-save/PLAN.md`, and
// `tab-lock/PLAN.md` are implemented on top of this foundation
// (`docs/plans/persistence-fs/PLAN.md` §10 "Cross-References"). Remove this
// allow once those call sites exist.
#![allow(dead_code)]

mod commands;
mod fs;
mod model;
mod settings;
mod util;
mod watch;

use fs::self_write_registry::SelfWriteRegistry;
use settings::app_settings_store::AppSettingsStore;

fn main() {
    tracing_subscriber::fmt::init();

    // `docs/plans/project-architecture/PLAN.md` §7: expose WebView2 remote
    // debugging in debug builds so reviewer-lite can drive the app via CDP.
    // Not a product feature; has no effect in release builds.
    #[cfg(debug_assertions)]
    {
        // SAFETY: called once, single-threaded, before any other thread is
        // spawned (Tauri's async runtime/webview creation happens later in
        // `tauri::Builder::default().run(...)` below).
        unsafe {
            std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "--remote-debugging-port=9222");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(SelfWriteRegistry::new())
        .manage(AppSettingsStore::new())
        .invoke_handler(tauri::generate_handler![
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::tab::tab_list,
            commands::tab::tab_create,
            commands::tab::tab_register_existing,
            commands::tab::tab_rename,
            commands::tab::tab_reorder,
            commands::tab::tab_remove,
            commands::tab::tab_open_location,
            commands::window::window_set_always_on_top,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
