//! Tab domain commands — everything in the tab domain except the lock/unlock/session commands
//! themselves (`tab_lock`/`tab_unlock`/`tab_end_session`, which belong to
//! `commands/tab_lock.rs`/`tab-lock/PLAN.md`). This module treats `locked` as an opaque
//! read/display cache for most commands, except `tab_remove`, which also consults
//! `LockSessionStore` for `tab-lock/PLAN.md` §4's "active session lifts the deletion
//! restriction" rule — `commands/tab_lock.rs`'s `resolve_tab_access` isn't reused here since
//! `tab_remove` doesn't need a resolved folder/tree, only the locked+session boolean check.
//! (`docs/plans/tab-management/PLAN.md`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::commands::emit_settings_recovery_event;
use crate::commands::tab_lock::LOCKED_PLACEHOLDER_NAME;
use crate::fs::atomic_write::atomic_write;
use crate::fs::path_safety::{validate_name, ValidationMode};
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::TabSettingsEntry;
use crate::model::result::{AppErrorPayload, AppResult, ErrorCode};
use crate::model::tab_meta::TabMetaFile;
use crate::settings::app_settings_store::AppSettingsStore;
use crate::store::lock_session_store::LockSessionStore;
use crate::util::unique_name::resolve_unique_name;
use crate::watch::watcher_registry::WatcherRegistry;

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
    /// User-set distinguishing label for a locked tab (`docs/spec.v1.2.0.md` §10); `None` when
    /// unset, in which case the frontend falls back to the bare 🔒 glyph as before.
    pub lock_label: Option<String>,
}

impl From<&TabSettingsEntry> for TabInfo {
    fn from(entry: &TabSettingsEntry) -> Self {
        Self {
            id: entry.id.clone(),
            name: entry.name.clone(),
            locked: entry.locked.unwrap_or(false),
            lock_label: entry.lock_label.clone(),
        }
    }
}

#[tauri::command]
pub fn tab_list(app: AppHandle, store: State<AppSettingsStore>, registry: State<Arc<SelfWriteRegistry>>) -> AppResult<Vec<TabInfo>> {
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
    let registry = app.state::<Arc<SelfWriteRegistry>>();
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
    registry: State<Arc<SelfWriteRegistry>>,
) -> AppResult<TabInfo> {
    match register_tab_at_path(&app, PathBuf::from(path), name, &store, &registry) {
        Ok(info) => AppResult::ok(info),
        Err(payload) => AppResult::err(payload),
    }
}

/// Creates a brand-new, empty folder under the default tabs root (`fs::app_data_root::tabs_root_dir`,
/// `data/tabs/<name>/`) and registers it as a tab — no OS dialog, no existing folder required
/// (`docs/plans/tab-management/PLAN.md` §2, GitHub issue #2's "create new" vs "register existing"
/// distinction). `tab_create`/`tab_register_existing` both require an already-existing folder
/// (`register_tab_at_path`'s `path.is_dir()` check); this command is the one path that actually
/// creates new on-disk content.
#[tauri::command]
pub fn tab_create_new(app: AppHandle, name: String, store: State<AppSettingsStore>, registry: State<Arc<SelfWriteRegistry>>) -> AppResult<TabInfo> {
    let trimmed = name.trim().to_string();
    if let Err(code) = validate_name(&trimmed, ValidationMode::Strict) {
        return AppResult::err_code(code, "invalid tab name");
    }

    let tabs_root = crate::fs::app_data_root::tabs_root_dir();
    if let Err(err) = std::fs::create_dir_all(&tabs_root) {
        tracing::error!(error = %err, path = %tabs_root.display(), "tab_create_new: failed to prepare the default tabs root");
        return AppResult::err_code(ErrorCode::Unknown, "failed to prepare the default tabs folder");
    }

    // GitHub issue #97: creation auto-numbers past a duplicate name (`trimmed`, `trimmed(1)`, ...)
    // instead of blocking — rename operations are unaffected (`tab_rename` below still blocks).
    let resolved = resolve_unique_name(&trimmed, |candidate| tabs_root.join(candidate).exists());
    let folder_path = tabs_root.join(&resolved);

    if let Err(err) = std::fs::create_dir(&folder_path) {
        tracing::error!(error = %err, path = %folder_path.display(), "tab_create_new: failed to create the new tab folder");
        return AppResult::err_code(ErrorCode::Unknown, "failed to create the new tab folder");
    }

    match register_tab_at_path(&app, folder_path.clone(), Some(resolved), &store, &registry) {
        Ok(info) => AppResult::ok(info),
        Err(payload) => {
            // Don't leave an orphaned folder on disk if registration failed after we already
            // created it (e.g. the locked-placeholder-name exemption path below, or an
            // unexpected settings-persistence error — a display-name collision against another
            // registered tab no longer lands here, issue #97, since `register_tab_at_path` now
            // auto-numbers past it too). Must be `remove_dir_all`, not `remove_dir`: by this
            // point `register_tab_at_path` has already written `.tabtreenote.json` into
            // `folder_path` (its own "initialize meta file" step runs first), so the folder is
            // never actually empty here — `remove_dir` (empty-directories-only) silently no-ops
            // with an "ディレクトリが空ではありません" IO error in this exact case, verified via
            // manual reproduction during implementation.
            if let Err(err) = std::fs::remove_dir_all(&folder_path) {
                tracing::error!(error = %err, path = %folder_path.display(), "tab_create_new: failed to clean up orphaned folder after a failed registration");
            }
            AppResult::err(payload)
        }
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

    // Initialize `.tabtreenote.json` if absent; leave an existing one untouched
    // (`tab-management/PLAN.md` §2 step 3 — reconciliation against the folder's real files
    // happens later, in `tree_open`). Done before the settings mutation below so a failure here
    // never leaves a half-registered tab in `AppSettings.tabs`.
    let meta_path = path.join(".tabtreenote.json");

    // `tab-lock/PLAN.md` §7 "Portability": a folder copied from elsewhere may already be a
    // locked tab (a `.tabtreenote.json` with a `lock` field instead of `tree`) — this must be
    // recognized *before* deriving/accepting a display name, since a locked tab's real name must
    // never be read from disk or shown here (it lives only inside the encrypted blob).
    let already_locked = meta_path.exists() && is_locked_meta_file(&meta_path);

    let proposed_name = if already_locked {
        LOCKED_PLACEHOLDER_NAME.to_string()
    } else {
        let name = match requested_name {
            Some(n) => n.trim().to_string(),
            None => derive_name_from_path(&path),
        };
        validate_name(&name, ValidationMode::Strict).map_err(|code| AppErrorPayload::new(code, "invalid tab name").with_path(folder_path.clone()))?;
        name
    };

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

    let result = store.modify(registry, |settings| {
        // GitHub issue #97: creation auto-numbers past a duplicate display name instead of
        // blocking — rename operations are unaffected (`tab_rename` below still blocks). The
        // placeholder name is allowed to collide with another locked tab's regardless of this —
        // `tab_lock`'s own step 10 placeholder swap on an already-registered tab doesn't run this
        // check either, so only the non-locked branch resolves uniqueness.
        let final_name = if already_locked {
            proposed_name.clone()
        } else {
            resolve_unique_name(&proposed_name, |candidate| settings.tabs.iter().any(|t| t.name.eq_ignore_ascii_case(candidate)))
        };
        settings.tabs.push(TabSettingsEntry {
            id: id.clone(),
            name: final_name,
            folder_path: folder_path.clone(),
            locked: Some(already_locked),
            lock_label: None,
        });
        Ok(())
    });

    match result {
        Ok(Ok((settings, outcome))) => {
            emit_settings_recovery_event(app, outcome);
            // The just-pushed entry's final (possibly auto-numbered) name, looked up by `id`
            // rather than threaded out of the closure directly (`AppSettingsStore::modify`'s
            // closure return type is a plain `Result<(), E>`).
            let name = settings.tabs.iter().find(|t| t.id == id).map(|t| t.name.clone()).unwrap_or(proposed_name);

            // External-change watcher start on new tab registration (`docs/plans/persistence-fs/PLAN.md`
            // §5; GitHub issue #209). Locked tabs are never watched here — their session-scoped
            // start/stop is issue #210's job.
            if !already_locked {
                let watchers = app.state::<WatcherRegistry>();
                let self_write_registry = app.state::<Arc<SelfWriteRegistry>>();
                watchers.start(app, &id, &path, self_write_registry.inner().clone());
            }

            // GitHub issue #319: 登録した(または作った)タブフォルダ配下の最長の固定ファイルが長いパスなら警告のみ
            // (ロック済みタブも登録時点ではフォルダパスは平文なので同じく判定する)
            crate::commands::warn_if_long_path(app, [path.join(".tabtreenote.json")]);

            Ok(TabInfo { id, name, locked: already_locked, lock_label: None })
        }
        Ok(Err(payload)) => Err(payload),
        Err(io_err) => {
            tracing::error!(error = %io_err, "tab create: failed to persist new tab");
            Err(AppErrorPayload::new(ErrorCode::Unknown, "failed to save new tab"))
        }
    }
}

/// `true` if `meta_path` parses as a `TabMetaFile` with a `lock` field present (`tab-lock/PLAN.md`
/// §2's on-disk shape) — any read/parse failure is treated as "not locked" so a corrupt or
/// unrelated file doesn't block ordinary registration (the existing `load_tab_meta_with_recovery`
/// machinery, not this best-effort check, is what actually handles that case once the tab is
/// opened).
fn is_locked_meta_file(meta_path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(meta_path) else {
        return false;
    };
    let Ok(meta) = serde_json::from_slice::<TabMetaFile>(&bytes) else {
        return false;
    };
    meta.lock.is_some()
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
    registry: State<Arc<SelfWriteRegistry>>,
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

/// Sets or clears (`label: None`) the distinguishing label shown for a locked tab in place of the
/// bare 🔒 glyph (`docs/spec.v1.2.0.md` §10, `docs/detailed-spec.v1.2.0.md` §10.2). Unlike
/// `tab_rename` above, this deliberately does NOT reject on `locked`/session state — `lockLabel`
/// is a separate plaintext field, readable and writable regardless of whether the tab is
/// unlocked, locked-with-no-session, or locked-with-an-active-session. `label` is free text and
/// is intentionally not run through `validate_name` (that guards filesystem-safe names, not
/// applicable to a display label).
#[tauri::command]
pub fn tab_set_lock_label(
    app: AppHandle,
    id: String,
    label: Option<String>,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
) -> AppResult<TabInfo> {
    let result = store.modify(&registry, |settings| match settings.tabs.iter_mut().find(|t| t.id == id) {
        None => Err(AppErrorPayload::new(ErrorCode::NotFound, "tab not found")),
        Some(entry) => {
            entry.lock_label = label.clone();
            Ok(())
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
            tracing::error!(error = %io_err, "tab_set_lock_label: failed to persist lock label");
            AppResult::err_code(ErrorCode::Unknown, "failed to save the lock label")
        }
    }
}

#[tauri::command]
pub fn tab_reorder(app: AppHandle, order: Vec<String>, store: State<AppSettingsStore>, registry: State<Arc<SelfWriteRegistry>>) -> AppResult<()> {
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
pub fn tab_remove(
    app: AppHandle,
    id: String,
    store: State<AppSettingsStore>,
    registry: State<Arc<SelfWriteRegistry>>,
    sessions: State<LockSessionStore>,
    watchers: State<WatcherRegistry>,
) -> AppResult<()> {
    // `tab-lock/PLAN.md` §4: locked-with-no-active-session rejects; locked-with-an-active-session
    // behaves like removing any other tab (the still-encrypted files on disk are untouched
    // either way, same "unregister only" rule).
    let has_session = sessions.has_session(&id);

    let result = store.modify(&registry, |settings| {
        let target = settings.tabs.iter().find(|t| t.id == id);
        match target {
            None => Err(AppErrorPayload::new(ErrorCode::NotFound, "tab not found")),
            Some(t) if t.locked.unwrap_or(false) && !has_session => Err(AppErrorPayload::new(ErrorCode::TabLocked, "cannot remove a locked tab")),
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
            // Nothing left to reference this session's in-memory plaintext once the tab is
            // unregistered.
            sessions.end(&id);
            // External-change watcher stop on tab deletion (`docs/plans/persistence-fs/PLAN.md`
            // §5; GitHub issue #209) — only once removal is confirmed, never for a rejected
            // (e.g. `TabLocked`) removal attempt. A no-op if this tab was never watched (locked
            // tabs never have an entry here in the first place).
            watchers.stop(&id);
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
pub fn tab_open_location(app: AppHandle, id: String, store: State<AppSettingsStore>, registry: State<Arc<SelfWriteRegistry>>) -> AppResult<()> {
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
            lock_label: None,
        };
        let info = TabInfo::from(&entry);
        assert!(!info.locked);
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-tab-cmd-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // `tab-lock/PLAN.md` §7 "Portability": `register_tab_at_path` must recognize an already-
    // locked `.tabtreenote.json` (a `lock` field instead of `tree`) at registration time, since
    // its real name must never be read from disk or displayed — these tests exercise
    // `is_locked_meta_file` directly, the pure piece of that logic that doesn't need a live
    // `AppHandle` to construct (same rationale `commands::editor::tests` documents for testing
    // its own helpers directly rather than the full command functions).
    #[test]
    fn is_locked_meta_file_detects_a_lock_field() {
        let dir = tempdir();
        let meta_path = dir.join(".tabtreenote.json");
        std::fs::write(
            &meta_path,
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": 1,
                "appVersion": "0.1.0",
                "lock": { "formatVersion": 1 }
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(is_locked_meta_file(&meta_path));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn is_locked_meta_file_is_false_for_an_unlocked_tree_shape() {
        let dir = tempdir();
        let meta_path = dir.join(".tabtreenote.json");
        std::fs::write(&meta_path, serde_json::to_vec(&TabMetaFile::fresh()).unwrap()).unwrap();

        assert!(!is_locked_meta_file(&meta_path));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn is_locked_meta_file_is_false_when_the_file_is_missing_or_unparsable() {
        let dir = tempdir();
        let missing = dir.join(".tabtreenote.json");
        assert!(!is_locked_meta_file(&missing));

        std::fs::write(&missing, b"not json").unwrap();
        assert!(!is_locked_meta_file(&missing));

        std::fs::remove_dir_all(&dir).ok();
    }
}
