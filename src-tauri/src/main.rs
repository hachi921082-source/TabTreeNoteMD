// Prevents an additional console window on Windows in release builds, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// This crate wires up `settings_get`/`settings_set` (persistence-fs/PLAN.md),
// the tab domain (tab-management/PLAN.md and tab-lock/PLAN.md's lock/unlock/
// session commands), the session-only window always-on-top toggle
// (tab-management/PLAN.md §4.1), the tree/memo domain (memo-tree/PLAN.md,
// including tab-lock/PLAN.md's locked-active-session branch),
// `editor_load`/`editor_save` (editor-save/PLAN.md, ditto),
// `search_query`/the cross-tab full-text index (search-index/PLAN.md, built
// at startup below and patched incrementally by the tree/editor/tab-lock
// commands), and window-operation/PLAN.md's app-shell integrations:
// second-instance prevention (§5, `tauri_plugin_single_instance`, registered
// first), auto-launch-to-tray (§4, `tauri_plugin_autostart` +
// `commands::autostart`), and the tray icon (§4, built in `.setup()`). The
// external-change watcher (`watch::watcher`/`watch::watcher_registry`) is now
// wired up for ordinary, non-locked tabs (startup, new-tab registration, tab
// removal — `persistence-fs/PLAN.md` §5, GitHub issue #209) with its
// conflict-check + `evt:fs:external-change`/`evt:editor:conflict` emission
// pipeline (`watch::conflict`, `watch::watcher_registry::route_change`,
// GitHub issue #211) wired in, plus frontend handling of both events (issue
// #212). Locked tabs are deliberately never watched, including while a
// session is active (issue #210, closed as won't-fix: the pipeline's
// `is_md_file` filter would silently ignore every event for a locked tab's
// obfuscated files regardless, and the app is the only writer to them
// anyway). This allow stays regardless — it also covers
// unrelated pre-existing dead code reserved for not-yet-wired-up future
// features (e.g. `fs::path_safety::ValidationMode::Lenient`, `model::result::ErrorCode::LongPath` —
// 長いパスの警告は `evt:path:long-warning` / `ImportFileResult.long_path` で出すため、このコードは
// 結果として返さない) elsewhere in this crate.
#![allow(dead_code)]

mod commands;
mod crypto;
mod fs;
mod model;
mod search;
mod settings;
mod store;
mod util;
mod watch;

use std::path::Path;
use std::sync::Arc;

use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

use fs::self_write_registry::SelfWriteRegistry;
use search::index::SearchIndex;
use settings::app_settings_store::AppSettingsStore;
use store::lock_session_store::LockSessionStore;
use store::tab_store::TabViewStore;
use watch::active_memo::ActiveMemoTracker;
use watch::watcher_registry::WatcherRegistry;

fn main() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        // Must be the first plugin registered (`docs/plans/window-operation/PLAN.md` §5): when a
        // second instance is launched, this callback runs inside the FIRST (already-running)
        // instance, bringing its window to the foreground; the second process then exits
        // immediately, before this app ever reaches `.setup()`/opens a window/touches
        // settings.json of its own.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // Auto-launch-to-tray-on-Windows-login (`docs/plans/window-operation/PLAN.md` §4). `--tray`
        // is the argument the OS-relaunched process receives; `commands::window::window_is_tray_launch`
        // reads it back to decide whether to reveal the main window at startup.
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--tray"])))
        .manage(Arc::new(SelfWriteRegistry::new()))
        .manage(AppSettingsStore::new())
        .manage(commands::PendingMetaRecovery::new())
        .manage(ActiveMemoTracker::new())
        .manage(LockSessionStore::new())
        .manage(SearchIndex::new())
        .manage(TabViewStore::new())
        .manage(WatcherRegistry::new())
        .setup(|app| {
            // Main window: created here programmatically rather than via `tauri.conf.json`'s
            // declarative `app.windows` array (which sets `"create": false` on the "main" entry
            // for exactly this reason), so debug builds can attach
            // `--remote-debugging-port=9222` via `WebviewWindowBuilder::additional_browser_args`
            // (`docs/plans/project-architecture/PLAN.md` §7: exposes WebView2's CDP port so
            // reviewer-lite can drive the app). Not a product feature; has no effect in release
            // builds. The previous approach — setting the
            // `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` env var before `tauri::Builder` — never
            // worked: wry always passes an explicit, non-null `additional_browser_arguments`
            // string to `CoreWebView2EnvironmentOptions`, and WebView2 ignores the env var
            // whenever that's non-null (see `wry::webview2::create_environment`, wry 0.55.1).
            // `WebviewWindowBuilder::from_config` reads every other property (title, size,
            // decorations, starting hidden, ...) straight from that same `tauri.conf.json`
            // config, so it stays the single source of truth for the window's static shape in
            // both debug and release builds; only the CDP arg differs between the two.
            {
                let window_config = app
                    .config()
                    .app
                    .windows
                    .iter()
                    .find(|w| w.label == "main")
                    .expect("tauri.conf.json must declare a \"main\" window")
                    .clone();
                let builder = tauri::WebviewWindowBuilder::from_config(app, &window_config)?;
                // Setting `additional_browser_args` at all replaces wry's own default
                // `additional_browser_arguments` outright (it does not merge), so this
                // reproduces that default verbatim and only appends the new CDP arg, to avoid
                // silently losing the mini-menu/PDF-OOUI/SmartScreen suppression and autoplay
                // policy debug builds would otherwise still get from wry's default.
                #[cfg(debug_assertions)]
                let builder = builder.additional_browser_args(
                    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required --remote-debugging-port=9222",
                );
                builder.build()?;
            }

            // Cross-tab full-text search index build (`search-index/PLAN.md` §1 "Built at
            // startup"): scans every registered, currently-unlocked tab's direct-child `.md`
            // files once, up front. No unlock session can be active yet at this point in
            // startup, so there is nothing to special-case for the locked-with-session
            // exclusion rule here beyond the plain `locked` check `rebuild_all` already applies.
            {
                let store = app.state::<AppSettingsStore>();
                let registry = app.state::<Arc<SelfWriteRegistry>>();
                let index = app.state::<SearchIndex>();
                let (settings, outcome) = store.get(&registry);
                // GitHub issue #318: 起動時の復旧はフロント未準備なので、保留フラグに残す。
                commands::emit_settings_recovery_event(app.handle(), outcome);
                index.rebuild_all(&settings);

                // External-change watcher startup (`docs/plans/persistence-fs/PLAN.md` §5; GitHub
                // issue #209): one `FolderWatcher` per registered, non-locked tab. Locked tabs are
                // never watched here — session-scoped start/stop for those is issue #210.
                let watchers = app.state::<WatcherRegistry>();
                let app_handle = app.handle().clone();
                for tab in settings.tabs.iter().filter(|t| !t.locked.unwrap_or(false)) {
                    watchers.start(&app_handle, &tab.id, Path::new(&tab.folder_path), registry.inner().clone());
                }
            }

            // Tray icon: left-/double-click restores the window
            // (`docs/plans/window-operation/PLAN.md` §4), handled directly in Rust rather than a
            // round-trip event, per `project-architecture/PLAN.md` §5's "only add events actually
            // needed" note.
            if let Some(icon) = app.default_window_icon().cloned() {
                TrayIconBuilder::new()
                    .icon(icon)
                    .tooltip("TabTreeNoteMD")
                    .on_tray_icon_event(|tray, event| {
                        let restore = matches!(
                            event,
                            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                        ) || matches!(event, TrayIconEvent::DoubleClick { button: MouseButton::Left, .. });
                        if !restore {
                            return;
                        }
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    })
                    .build(app)?;
            } else {
                tracing::warn!("no default window icon available; tray icon not created");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::tab::tab_list,
            commands::tab::tab_create,
            commands::tab::tab_create_new,
            commands::tab::tab_register_existing,
            commands::tab::tab_rename,
            commands::tab::tab_set_lock_label,
            commands::tab::tab_reorder,
            commands::tab::tab_remove,
            commands::tab::tab_open_location,
            commands::tab_lock::tab_lock,
            commands::tab_lock::tab_unlock,
            commands::tab_lock::tab_end_session,
            commands::tab_lock::tab_lock_recovery_status,
            commands::tab_lock::tab_lock_recovery_finish,
            commands::tab_lock::tab_lock_recovery_restore,
            commands::tab_lock::tab_permanent_unlock,
            commands::tab_lock::tab_permanent_unlock_recovery_status,
            commands::tab_lock::tab_permanent_unlock_recovery_finish,
            commands::tab_lock::tab_permanent_unlock_recovery_restore,
            commands::window::window_set_always_on_top,
            commands::window::window_is_tray_launch,
            commands::autostart::autostart_set_enabled,
            commands::tree::tree_open,
            commands::tree::memo_create,
            commands::tree::memo_create_child,
            commands::tree::memo_rename,
            commands::tree::memo_delete,
            commands::tree::memo_move,
            commands::tree::memo_set_expanded,
            commands::tree::memo_set_label,
            commands::tree::memo_set_editor_override,
            commands::tree::memo_set_edit_lock,
            commands::tree::memo_set_favorite,
            commands::tree::memo_get_properties,
            commands::tree::memo_open_location,
            commands::tree::tree_set_last_viewed,
            commands::editor::editor_load,
            commands::editor::editor_save,
            commands::editor::editor_close,
            commands::search::search_query,
            commands::search::search_quick_open,
            commands::search::search_get_favorites,
            commands::link::link_resolve,
            commands::link::link_open_external,
            commands::pdf_export::pdf_export_write,
            commands::image::image_save_pasted,
            commands::image::image_read_bytes,
            commands::image::image_repair_legacy_links,
            commands::image::image_read_external_file,
            commands::import::import_file,
            commands::about::about_get_third_party_notices,
            commands::meta_take_pending_recovery,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
