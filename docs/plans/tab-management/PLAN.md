# PLAN: Tab Management

> Status: planning complete, not yet implemented. Depends on `project-architecture/PLAN.md` (naming
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

### `tab_create(initial_path: Option<String>) -> AppResult<TabInfo>`
1. If `initial_path` is `None`, open the OS folder-selection dialog (Tauri's dialog plugin) — **never**
   a web `prompt()` (`[spec §2.1]`). If the user cancels the dialog, return a no-op (not an error) —
   exact signaling of "user cancelled" vs. a real error is implementation discretion (e.g. an `Option`
   wrapping the `AppResult`, or a dedicated cancel variant); front end simply closes the flow silently
   on cancel either way.
2. Derive a proposed tab name from the folder's base name; validate via `path_safety::validate_name`
   (Strict) — folder base names are usually already valid, but symlink junctions or unusual folder
   names could collide/violate; on `NAME_DUPLICATE` against existing tab names (case-insensitive,
   `[detailed-spec §5.1]`), the frontend must let the user pick a different display name before
   retrying (tab display name is independent of the folder's actual name on disk — only the *folder
   path* is fixed by the dialog, the tab's `name` field is a free label).
3. If `<folder>/.tabtreenote.json` does not already exist, initialize it with an empty `tree: []`
   (fresh tab). If it does exist (user picked a folder that was previously used, e.g. re-registering
   after a prior `tab_remove`), leave it untouched — `tree_open` (see `memo-tree/PLAN.md`) reconciles
   it against the folder's real files on first open.
4. Append `{ id: <new>, name, folderPath, locked: false }` to `AppSettings.tabs` via `settings_set`.
5. Return the new `TabInfo`.

### `tab_register_existing(...)`
Same flow as `tab_create` — per `[detailed-spec §5.1]`, "create new" and "register existing folder"
differ only in whether a pre-existing `.tabtreenote.json`/memo files are expected, and step 3 above
already handles both cases uniformly. **This may be implemented as a single command
(`tab_create`) with no behavioral branch needed**, rather than two separate commands — the distinction
in `spec.md` §2.1 ("drag-register existing folders") is a *UI entry-point* distinction (menu action vs.
drag-and-drop of a folder from Explorer onto the tab bar), not a backend-logic distinction. If the
frontend needs a separate command purely to receive a dropped-folder path instead of opening a picker
dialog, name it `tab_register_existing(path: String)` (skips step 1, dialog) — `developer-lite`
decides which shape is cleaner at implementation time; both are compatible with this plan.

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
  — do not implement a wheel handler on the tab bar. 📌 pin button per tab (session-only React state, not
  persisted, sits left of the search button per `[spec §2.1]`), right-click → shared context menu with
  "Rename," "Open tab location," "Remove tab," and (if unlocked) "Lock tab" / (if locked+session)
  "Lock now" is not offered as a separate action since session end is switch-triggered only
  (`tab-lock/PLAN.md`).
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

None.
