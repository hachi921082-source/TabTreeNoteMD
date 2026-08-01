# PLAN: Tab Management

> Status: implemented (backend commands + `TabBar` frontend), except the 📌 always-on-top toggle button
> (spec resolved, see §4 below and "Open Items"; not yet implemented) and the "Lock tab" context-menu action (deferred to
> `tab-lock/PLAN.md`'s own implementation pass). Depends on `project-architecture/PLAN.md` (naming
> conventions) and `persistence-fs/PLAN.md` (atomic write, settings store, name validation). Tab Lock
> operations (`tab_lock`/`tab_unlock`/`tab_end_session`) are specified separately in
> `tab-lock/PLAN.md` — this file covers everything else in the tab domain and treats lock state as an
> opaque flag it reads/displays.

Sources: `docs/spec.md` §2.1, §4.1 pt.7; `docs/detailed-spec.md` §4.1, §5.1.

---

## 1. Data

`AppSettings.tabs: { id: string; name: string; folderPath: string; locked?: boolean }[]` — array order
**is** tab-bar order (no separate order field, `[detailed-spec §3.2]`). `id` is a stable identifier
generated once at tab-creation time (a random string, e.g. `nanoid`-style or a UUID-v4 string — exact
generation method is implementation discretion; must be stable across renames/reorders since it's the
join key used by `lastOpened`, window focus restoration, and cross-file references).

## 2. Commands

### `tab_list() -> AppResult<Vec<TabInfo>>`
```rust
struct TabInfo { id: String, name: String, locked: bool }
```
Reads `AppSettings.tabs` (via `persistence-fs`'s settings store) in array order. For a locked tab,
`name` is already the placeholder string stored in settings (§3 below) — this command does no
decryption and never touches a locked tab's `.tabtreenote.json`.

### `tab_create(initial_path: Option<String>, name: Option<String>) -> AppResult<Option<TabInfo>>`
> **Implemented signature note**: adds a `name: Option<String>` parameter beyond this section's
> original draft signature, and wraps the return type's success value in `Option` instead of using a
> separate cancel variant. Both are resolutions of discretion this section explicitly left open (see
> step 1 and step 2 below) — `name` is required for step 2's own "let the user pick a different
> display name before retrying" requirement to actually be implementable (the original signature had
> no way for a retry to supply that name for the *same* already-picked folder without re-opening the
> dialog).
1. If `initial_path` is `None`, open the OS folder-selection dialog (Tauri's dialog plugin,
   `tauri_plugin_dialog`'s `blocking_pick_folder`) — **never** a web `prompt()` (`[spec §2.1]`). If the
   user cancels the dialog, returns `AppResult::Ok(None)` (not an error); the frontend closes the flow
   silently on `None`. If `initial_path` is `Some(path)`, the dialog is skipped entirely and `path` is
   used directly (this is what `name`-retry calls and `tab_register_existing` both do internally).
2. Derive a proposed tab name from the folder's base name if `name` is `None`; otherwise use `name`
   (trimmed) as given. Validate via `path_safety::validate_name` (Strict) — folder base names are
   usually already valid, but symlink junctions or unusual folder names could collide/violate; on
   `NAME_DUPLICATE` against existing tab names (case-insensitive), the error's `path` field carries the
   already-picked folder path back to the frontend, which shows a prompt for an alternate name and
   retries via `tab_create(Some(path), Some(new_name))` — no second dialog. (Tab display name is
   independent of the folder's actual name on disk — only the *folder path* is fixed by the dialog, the
   tab's `name` field is a free label.)
2a. **Added safety check** (not in the original draft): rejects with `NotFound` if the resolved path is
    not a directory (e.g. a file was passed in), since folder registration only makes sense for a
    directory. Runs before the name-derivation step.
3. If `<folder>/.tabtreenote.json` does not already exist, initialize it with an empty `tree: []`
   (fresh tab). If it does exist (user picked a folder that was previously used, e.g. re-registering
   after a prior `tab_remove`), leave it untouched — `tree_open` (see `memo-tree/PLAN.md`) reconciles
   it against the folder's real files on first open.
4. Append `{ id: <new>, name, folderPath, locked: false }` to `AppSettings.tabs`. **Implemented via
   `AppSettingsStore::modify`** (a new read-modify-write-in-one-lock method added to
   `settings/app_settings_store.rs` alongside this feature, extending `persistence-fs/PLAN.md`'s
   store) rather than a separate `get()` + `settings_set`/`patch()` pair, so the duplicate-name check
   and the write happen atomically — a plain `get()`-then-`patch()` pair would leave a race window
   where a second concurrent tab command could interleave between the check and the write.
5. Return the new `TabInfo`, wrapped in `Some`.

### `tab_register_existing(path: String, name: Option<String>) -> AppResult<TabInfo>`
**Implemented as a separate command** (not folded into `tab_create`), since
`project-architecture/PLAN.md` §4's command-name inventory lists `tab_create` and
`tab_register_existing` as distinct entries. Internally, both commands call the same private helper
function with the resolved path and optional name — behaviorally identical to `tab_create(Some(path),
name)` (step 3's "existing `.tabtreenote.json` reconciled, not overwritten" already covers both "create
new" and "register existing" per `[detailed-spec §5.1]`). No dialog, no cancel path (the path is
already known to the caller — the OS-drag-and-drop-onto-tab-bar entry point).

### `tab_rename(id: String, new_name: String) -> AppResult<TabInfo>`
- Rejected with `TabLocked` if the tab is locked with no active session (renaming the *display* name
  of a locked tab is meaningless since the placeholder is shown, not a real edit target — if a UI
  surface for this exists at all, it should be disabled/hidden for locked tabs rather than relying on
  this rejection, but the backend check exists as defense-in-depth).
- If the tab has an active lock session, this command updates the **placeholder** is not applicable —
  an active-session tab's *real* name change goes through the lock-payload update path in
  `tab-lock/PLAN.md` (renaming a locked-but-unlocked-this-session tab's real name is a lock-domain
  operation, not this one). This command only ever touches `AppSettings.tabs[i].name` for unlocked
  tabs.
- Validates via `path_safety::validate_name` (Strict) + uniqueness check against other tabs
  (case-insensitive), excluding the tab being renamed itself.

### `tab_reorder(order: Vec<String>) -> AppResult<()>`
`order` = full list of tab `id`s in the new desired order. Validates the set of ids matches the
current registry exactly (defensive check against a stale frontend drag state); reorders
`AppSettings.tabs` to match, `settings_set`. No per-tab file touched (`[detailed-spec §5.1]`).

### `tab_remove(id: String) -> AppResult<()>`
- Rejected with `TabLocked` if locked with no active session (`[spec §4.1 pt.7]`).
- Otherwise: removes the entry from `AppSettings.tabs` only. **Never** deletes or trashes the folder or
  its contents (`[spec §2.1]`, `DECISIONS.md` "Tab Deletion Semantics"). No confirmation-dialog content
  needs to warn about data loss beyond "this will no longer appear in the tab list" — since files are
  never touched, this is a low-risk, easily-reversible (`tab_create`-with-same-folder) action, but a
  confirm step per the app's general custom-dialog convention is still appropriate UX (implementation
  discretion whether it's a lightweight inline confirm or the shared dialog component).
- If the removed tab was the currently-open tab, frontend selects another tab (or shows the empty
  state if none remain) — pure frontend-state concern, no backend involvement beyond this command.
- If `AppSettings.lastOpened.tabId` referenced the removed tab, clear it via a follow-up `settings_set`
  (or fold into the same transaction) so a later startup doesn't try to restore a nonexistent tab (see
  `editor-save/PLAN.md` §6 for the startup auto-open behavior this feeds).

### `tab_open_location(id: String) -> AppResult<()>`
OS "reveal in file manager" standard API (Tauri's `opener`/shell plugin, or platform `explorer.exe
/select,<path>` invoked via the OS-standard API rather than shell string concatenation, per
`[spec §2.8]`'s "no shell string concatenation" principle applying generally to external-process
launches). Works for both locked and unlocked tabs (revealing the folder doesn't reveal memo content —
locked tabs' files are obfuscated tokens on disk, which is fine to show in Explorer since that's
already the point of obfuscation).

## 3. `tabs[i].locked` Cache Semantics

Restates `[detailed-spec §3.2]`: this field is a display cache. `tab_list`'s `locked` field in
`TabInfo` is read directly from this cache for fast tab-bar rendering. `tab-lock/PLAN.md` is
responsible for keeping this cache in sync (it's written whenever `tab_lock`/`tab_unlock` change the
true state); if this plan's commands ever detect a mismatch (e.g. after an external folder copy swaps
in a differently-locked `.tabtreenote.json`), that reconciliation happens inside `tree_open`
(`memo-tree/PLAN.md`, which reads the real `.tabtreenote.json` and can correct the cache via
`settings_set` if it disagrees with what `AppSettings.tabs[i].locked` currently says) — not inside this
file's commands, which trust the cache for listing purposes only.

## 4. Frontend

- `components/TabBar/`: renders `TabInfo[]` in order, drag-to-reorder (calls `tab_reorder` on drop),
  click to switch active tab, keyboard-shortcut tab switching (`shared/shortcuts.ts` default keymap,
  `window-operation/PLAN.md` §3). **Mouse-wheel tab switching is removed from v1 scope** (user
  decision, `[detailed-spec §5.1]`, `DECISIONS.md` "Mouse-Wheel Tab Switching: Removed from v1 Scope")
  — do not implement a wheel handler on the tab bar.
  - **Drag-to-reorder implementation note**: uses pointer events (`mousedown`/`mouseenter`/`mouseup`),
    not native HTML5 `draggable`/`dragstart`/`drop`. Tauri's `tauri.conf.json` `dragDropEnabled` (default
    `true`) is required for the OS-folder-drop-onto-tab-bar feature below to receive real filesystem
    paths, but the Tauri config schema itself documents that setting as conflicting with the browser's
    own HTML5 drag-and-drop API on Windows ("Disabling it is required to use HTML5 drag and drop on the
    frontend on Windows"). Implementing tab-bar reordering with pointer events instead of HTML5 DnD
    avoids this conflict entirely.
  - **"Lock tab" context-menu action is NOT implemented in this file's scope** — it calls a
    not-yet-implemented `tab_lock` command (`tab-lock/PLAN.md`), so is deferred to that feature's own
    implementation pass rather than added here as a stub. Locked tabs currently only expose "Open tab
    location" in the context menu; "Rename"/"Remove tab" are shown but disabled per §2's rejection
    rules.
  - **📌 "Always-on-top" toggle button** — a single global button (not one per tab), placed in the tab
    bar's toolbar area immediately to the left of the search button (button order left-to-right: pin,
    search, theme toggle — matching the legacy repo's `TabBar.tsx`). Clicking it toggles OS-level
    "always on top" for the entire app window, not any per-tab or tab-bar-position effect;
    session-only React state, never persisted to `AppSettings` (`[spec §2.1]`, `[detailed-spec §4.1]`).
    **Implemented**: backend command `window_set_always_on_top(window: WebviewWindow, enabled: bool)`
    (`src-tauri/src/commands/window.rs`) calls Tauri v2's `WebviewWindow::set_always_on_top(bool)`
    (confirmed against the installed `tauri = "2.11.5"`); frontend toggle lives in
    `components/TabBar/TabBar.tsx` (`ttn-tab-bar-toolbar` group, optimistic local state reverted on IPC
    failure). Search/theme buttons don't exist yet (`search-index`/`appearance-theme` not yet
    implemented) — the toolbar group currently holds only the pin button, ready for those to be added
    to its right.
  - Right-click → shared context menu (`components/ContextMenu/`) with "Rename," "Open tab location,"
    "Remove tab" (Rename/Remove disabled for locked tabs, per §2's rejection rules — the backend check
    remains as defense-in-depth).
- Right-click outside any registered tab (empty tab-bar area) shows no context menu at all
  (`[spec §2.7]`, already-established behavior to preserve).
- Locked tabs render with the **confirmed placeholder display name: the 🔒 glyph alone**, no
  accompanying text (`[detailed-spec §3.2]`, `DECISIONS.md` "Tab-Lock Placeholder Display Name: 🔒
  Glyph Alone") — this is `AppSettings.tabs[i].name`'s literal value while locked, not a separate
  UI-only badge layered next to a different placeholder string.
- "Create tab" entry point: a `+`-style button at the end of the tab bar, or the app's empty-state
  screen if zero tabs are registered — invokes `tab_create`.
- Drag-and-drop of an OS folder from Explorer onto the tab bar invokes `tab_register_existing` with the
  dropped path (Tauri's file-drop event).

## Open Items

- **📌 "Always-on-top" toggle button** — implemented (see §4): `window_set_always_on_top` Tauri command
  + `TabBar.tsx` toolbar button, using `WebviewWindow::set_always_on_top(bool)` (Tauri v2.11.5).
- **"Lock tab" context-menu action** — intentionally not added here; `tab-lock/PLAN.md`'s own
  implementation pass should add it to `components/TabBar/TabBar.tsx`'s context-menu `items` array once
  `tab_lock`/`tab_unlock` exist.
- This implementation also introduced two shared, cross-feature UI primitives that did not exist yet at
  planning time but are referenced by `project-architecture/PLAN.md` §2's directory layout /
  `detailed-spec.md` §6.5 / `spec.md` §9.1: `components/dialogs/Dialog.tsx` (the shared non-blocking
  dialog shell, with `PromptDialog`/`ConfirmDialog` built on top) and `components/ContextMenu/ContextMenu.tsx`
  (the shared unified right-click menu). Later features (`memo-tree/PLAN.md`'s tree context menu,
  `editor-save/PLAN.md`'s editor context menu, `tab-lock/PLAN.md`'s password-entry dialog, etc.) should
  reuse these rather than building their own.
