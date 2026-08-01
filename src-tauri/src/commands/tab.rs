//! Tab domain commands — everything in the tab domain except lock
//! operations (`tab_lock`/`tab_unlock`/`tab_end_session`, which belong to
//! `tab-lock/PLAN.md` and treat `locked` here purely as an opaque
//! read/display cache) — `docs/plans/tab-management/PLAN.md`.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::commands::emit_settings_recovery_event;
use crate::fs::atomic_write::atomic_write;
use crate::fs::path_safety::{validate_name, ValidationMode};
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::TabSettingsEntry;
use crate::model::result::{AppErrorPayload, AppResult, ErrorCode};
use crate::model::tab_meta::TabMetaFile;
use crate::settings::app_settings_store::AppSettingsStore;

/// Tab-bar list entry (`docs/plans/tab-management/PLAN.md` §2 `tab_list`).
/// For a locked tab, `name` is already the 🔒-placeholder stored in
/// `AppSettings.tabs[i].name` (`tab-lock/PLAN.md` writes it there) — this
/// module never decrypts or otherwise inspects a locked tab's real name.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabInfo {
    pub id: String,
    pub name: String,
    pub locked: bool,
}

impl From<&TabSettingsEntry> for TabInfo {
    fn from(entry: &TabSettingsEntry) -> Self {
        Self {
            id: entry.id.clone(),
            name: entry.name.clone(),
            locked: entry.locked.unwrap_or(false),
        }
    }
}

#[tauri::command]
pub fn tab_list(app: AppHandle, store: State<AppSettingsStore>, registry: State<SelfWriteRegistry>) -> AppResult<Vec<TabInfo>> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);
    AppResult::ok(settings.tabs.iter().map(TabInfo::from).collect())
}

/// Opens the OS folder-selection dialog when `initial_path` is `None`
/// (`[spec §2.1]`, never a web `prompt()`); `initial_path` lets a caller
/// (currently unused by the frontend, reserved for future callers) skip the
/// dialog and register a folder path it already has. `name`, if given,
/// overrides the folder-basename-derived proposed name — this is how the
/// frontend retries after a `NAME_DUPLICATE` rejection without re-opening the
/// dialog (`tab-management/PLAN.md` §2 step 2 requires this retry path but
/// the plan's literal signature omits the parameter needed to support it;
/// this is a plan refinement made during implementation).
///
/// Returns `Ok(None)` (not an error) if the user cancels the dialog.
///
/// This command is `async` and uses the non-blocking `pick_folder` (callback
/// + `await`ed `oneshot`) rather than `blocking_pick_folder`. The blocking
/// variant synchronously occupies the thread Tauri uses to service this IPC
/// call for as long as the native dialog stays open, which was observed to
/// stall *every other* `invoke()` (e.g. `tab_list`) until the dialog closed —
/// see the "Implementation note (post-review fix)" under
/// `docs/plans/tab-management/PLAN.md` §2 `tab_create` step 1. Awaiting a
/// channel instead yields the thread back to the async runtime while the
/// dialog is open.
///
/// `store`/`registry` are fetched from `app.state()` inside the body rather
/// than taken as `State<'_, _>` parameters: Tauri's async-command macro
/// requires any async command with a reference/lifetime-bearing parameter to
/// return a literal `std::result::Result`, which this codebase's `AppResult`
/// wrapper (needed for its custom JSON shape) is not.
#[tauri::command]
pub async fn tab_create(app: AppHandle, initial_path: Option<String>, name: Option<String>) -> AppResult<Option<TabInfo>> {
    let path = match initial_path {
        Some(p) => PathBuf::from(p),
        None => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            app.dialog().file().pick_folder(move |file_path| {
                let _ = tx.send(file_path);
            });
            match rx.await {
                Ok(Some(file_path)) => match file_path.into_path() {
                    Ok(p) => p,
                    Err(err) => {
                        tracing::error!(error = %err, "tab_create: picked folder was not a plain filesystem path");
                        return AppResult::err_code(ErrorCode::Unknown, "invalid folder selection");
                    }
                },
                Ok(None) => return AppResult::ok(None), // user cancelled: no-op, not an error
                Err(_) => {
                    tracing::error!("tab_create: folder-picker callback channel dropped without a response");
                    return AppResult::err_code(ErrorCode::Unknown, "folder selection failed");
                }
            }
        }
    };

    let store = app.state::<AppSettingsStore>();
    let registry = app.state::<SelfWriteRegistry>();
    match register_tab_at_path(&app, path, name, &store, &registry) {
        Ok(info) => AppResult::ok(Some(info)),
        Err(payload) => AppResult::err(payload),
    }
}

/// Registers a folder already known to the frontend (no dialog) — the
/// OS-drag-and-drop-onto-tab-bar entry point (`docs/plans/tab-management/PLAN.md`
/// §4). Internally identical to `tab_create`'s "existing `.tabtreenote.json`
/// reconciled, not overwritten" behavior (`[detailed-spec §5.1]`); kept as a
/// separate command name because `project-architecture/PLAN.md` §4's command
/// inventory lists `tab_create` and `tab_register_existing` as distinct
/// entries.
#[tauri::command]
pub fn tab_register_existing(
    app: AppHandle,
    path: String,
    name: Option<String>,
    store: State<AppSettingsStore>,
    registry: State<SelfWriteRegistry>,
) -> AppResult<TabInfo> {
    match register_tab_at_path(&app, PathBuf::from(path), name, &store, &registry) {
        Ok(info) => AppResult::ok(info),
        Err(payload) => AppResult::err(payload),
    }
}

fn register_tab_at_path(
    app: &AppHandle,
    path: PathBuf,
    requested_name: Option<String>,
    store: &AppSettingsStore,
    registry: &SelfWriteRegistry,
) -> Result<TabInfo, AppErrorPayload> {
    let folder_path = path.to_string_lossy().to_string();

    if !path.is_dir() {
        return Err(AppErrorPayload::new(ErrorCode::NotFound, "the selected item is not a folder").with_path(folder_path));
    }

    let proposed_name = match requested_name {
        Some(n) => n.trim().to_string(),
        None => derive_name_from_path(&path),
    };

    validate_name(&proposed_name, ValidationMode::Strict)
        .map_err(|code| AppErrorPayload::new(code, "invalid tab name").with_path(folder_path.clone()))?;

    // Initialize `.tabtreenote.json` if absent; leave an existing one
    // untouched (`tab-management/PLAN.md` §2 step 3 — reconciliation against
    // the folder's real files happens later, in `tree_open`). Done before the
    // settings mutation below so a failure here never leaves a
    // half-registered tab in `AppSettings.tabs`.
    let meta_path = path.join(".tabtreenote.json");
    if !meta_path.exists() {
        let fresh = TabMetaFile::fresh();
        let json = serde_json::to_vec_pretty(&fresh).map_err(|err| {
            tracing::error!(error = %err, "tab create: failed to serialize fresh .tabtreenote.json");
            AppErrorPayload::new(ErrorCode::Unknown, "failed to initialize tab metadata")
        })?;
        atomic_write(&meta_path, &json, registry).map_err(|err| {
            tracing::error!(error = %err, path = %meta_path.display(), "tab create: failed to write .tabtreenote.json");
            AppErrorPayload::new(ErrorCode::Unknown, "failed to initialize tab metadata")
        })?;
    }

    let id = generate_tab_id();
    let new_entry = TabSettingsEntry {
        id: id.clone(),
        name: proposed_name.clone(),
        folder_path: folder_path.clone(),
        locked: Some(false),
    };

    let result = store.modify(registry, |settings| {
        let duplicate = settings.tabs.iter().any(|t| t.name.eq_ignore_ascii_case(&proposed_name));
        if duplicate {
            // `path` is attached so the frontend can retry with a different
            // name for the same already-picked folder without re-opening the
            // OS folder dialog (`tab-management/PLAN.md` §2 step 2's retry
            // requirement).
            return Err(AppErrorPayload::new(ErrorCode::NameDuplicate, "a tab with this name already exists").with_path(folder_path.clone()));
        }
        settings.tabs.push(new_entry.clone());
        Ok(())
    });

    match result {
        Ok(Ok((_settings, outcome))) => {
            emit_settings_recovery_event(app, outcome);
            Ok(TabInfo { id, name: proposed_name, locked: false })
        }
        Ok(Err(payload)) => Err(payload),
        Err(io_err) => {
            tracing::error!(error = %io_err, "tab create: failed to persist new tab");
            Err(AppErrorPayload::new(ErrorCode::Unknown, "failed to save new tab"))
        }
    }
}

fn derive_name_from_path(path: &Path) -> String {
    path.file_name().and_then(|s| s.to_str()).unwrap_or("New tab").to_string()
}

/// 128-bit random hex id, stable across renames/reorders
/// (`docs/plans/tab-management/PLAN.md` §1) — generation method is
/// implementation discretion.
fn generate_tab_id() -> String {
    format!("{:016x}{:016x}", rand::random::<u64>(), rand::random::<u64>())
}

#[tauri::command]
pub fn tab_rename(
    app: AppHandle,
    id: String,
    new_name: String,
    store: State<AppSettingsStore>,
    registry: State<SelfWriteRegistry>,
) -> AppResult<TabInfo> {
    let trimmed = new_name.trim().to_string();
    if let Err(code) = validate_name(&trimmed, ValidationMode::Strict) {
        return AppResult::err_code(code, "invalid tab name");
    }

    let result = store.modify(&registry, |settings| {
        // `locked` is treated purely as an opaque cache here — a `true`
        // value always rejects, since this feature has no lock-session
        // concept of its own (`tab-lock/PLAN.md` owns that distinction and
        // must update this check if/when it introduces one).
        let target = settings.tabs.iter().find(|t| t.id == id);
        match target {
            None => Err(AppErrorPayload::new(ErrorCode::NotFound, "tab not found")),
            Some(t) if t.locked.unwrap_or(false) => Err(AppErrorPayload::new(ErrorCode::TabLocked, "cannot rename a locked tab")),
            Some(_) => {
                let duplicate = settings.tabs.iter().any(|t| t.id != id && t.name.eq_ignore_ascii_case(&trimmed));
                if duplicate {
                    return Err(AppErrorPayload::new(ErrorCode::NameDuplicate, "a tab with this name already exists"));
                }
                if let Some(entry) = settings.tabs.iter_mut().find(|t| t.id == id) {
                    entry.name = trimmed.clone();
                }
                Ok(())
            }
        }
    });

    match result {
        Ok(Ok((settings, outcome))) => {
            emit_settings_recovery_event(&app, outcome);
            match settings.tabs.iter().find(|t| t.id == id) {
                Some(entry) => AppResult::ok(TabInfo::from(entry)),
                None => AppResult::err_code(ErrorCode::NotFound, "tab not found"),
            }
        }
        Ok(Err(payload)) => AppResult::err(payload),
        Err(io_err) => {
            tracing::error!(error = %io_err, "tab_rename: failed to persist rename");
            AppResult::err_code(ErrorCode::Unknown, "failed to rename tab")
        }
    }
}

#[tauri::command]
pub fn tab_reorder(app: AppHandle, order: Vec<String>, store: State<AppSettingsStore>, registry: State<SelfWriteRegistry>) -> AppResult<()> {
    let result = store.modify(&registry, |settings| {
        let mut current_ids: Vec<&str> = settings.tabs.iter().map(|t| t.id.as_str()).collect();
        let mut requested_ids: Vec<&str> = order.iter().map(|s| s.as_str()).collect();
        current_ids.sort_unstable();
        requested_ids.sort_unstable();
        if current_ids != requested_ids {
            return Err(AppErrorPayload::new(ErrorCode::NotFound, "tab order does not match the currently registered tab set"));
        }

        let mut reordered = Vec::with_capacity(settings.tabs.len());
        for id in &order {
            if let Some(pos) = settings.tabs.iter().position(|t| &t.id == id) {
                reordered.push(settings.tabs.remove(pos));
            }
        }
        settings.tabs = reordered;
        Ok(())
    });

    match result {
        Ok(Ok((_settings, outcome))) => {
            emit_settings_recovery_event(&app, outcome);
            AppResult::ok(())
        }
        Ok(Err(payload)) => AppResult::err(payload),
        Err(io_err) => {
            tracing::error!(error = %io_err, "tab_reorder: failed to persist order");
            AppResult::err_code(ErrorCode::Unknown, "failed to reorder tabs")
        }
    }
}

#[tauri::command]
pub fn tab_remove(app: AppHandle, id: String, store: State<AppSettingsStore>, registry: State<SelfWriteRegistry>) -> AppResult<()> {
    let result = store.modify(&registry, |settings| {
        let target = settings.tabs.iter().find(|t| t.id == id);
        match target {
            None => Err(AppErrorPayload::new(ErrorCode::NotFound, "tab not found")),
            Some(t) if t.locked.unwrap_or(false) => Err(AppErrorPayload::new(ErrorCode::TabLocked, "cannot remove a locked tab")),
            Some(_) => {
                // Never deletes/trashes the folder or its files — unregister
                // only (`DECISIONS.md` "Tab Deletion Semantics").
                settings.tabs.retain(|t| t.id != id);
                if let Some(last_opened) = &settings.last_opened {
                    if last_opened.tab_id == id {
                        settings.last_opened = None;
                    }
                }
                Ok(())
            }
        }
    });

    match result {
        Ok(Ok((_settings, outcome))) => {
            emit_settings_recovery_event(&app, outcome);
            AppResult::ok(())
        }
        Ok(Err(payload)) => AppResult::err(payload),
        Err(io_err) => {
            tracing::error!(error = %io_err, "tab_remove: failed to persist removal");
            AppResult::err_code(ErrorCode::Unknown, "failed to remove tab")
        }
    }
}

/// OS "reveal in file manager," via the OS-standard Shell API
/// (`tauri_plugin_opener`'s `reveal_item_in_dir`, which uses
/// `SHOpenFolderAndSelectItems` on Windows) rather than shell string
/// concatenation (`[spec §2.8]`). Works for both locked and unlocked tabs —
/// revealing the folder doesn't reveal memo content.
#[tauri::command]
pub fn tab_open_location(app: AppHandle, id: String, store: State<AppSettingsStore>, registry: State<SelfWriteRegistry>) -> AppResult<()> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    let Some(entry) = settings.tabs.iter().find(|t| t.id == id) else {
        return AppResult::err_code(ErrorCode::NotFound, "tab not found");
    };

    match app.opener().reveal_item_in_dir(&entry.folder_path) {
        Ok(()) => AppResult::ok(()),
        Err(err) => {
            tracing::error!(error = %err, path = %entry.folder_path, "tab_open_location: failed to reveal folder");
            AppResult::err_code(ErrorCode::NotFound, "could not open the tab's folder location")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_name_from_path_uses_final_component() {
        assert_eq!(derive_name_from_path(Path::new("C:/Users/me/Shopping list")), "Shopping list");
    }

    #[test]
    fn generate_tab_id_produces_distinct_32_char_hex_strings() {
        let a = generate_tab_id();
        let b = generate_tab_id();
        assert_eq!(a.len(), 32);
        assert_eq!(b.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn tab_info_from_entry_defaults_missing_locked_to_false() {
        let entry = TabSettingsEntry {
            id: "abc".into(),
            name: "Shopping list".into(),
            folder_path: "C:/tab".into(),
            locked: None,
        };
        let info = TabInfo::from(&entry);
        assert!(!info.locked);
    }
}
