# PLAN: Window / Operation

> Status: planning complete, not yet implemented. Depends on `project-architecture/PLAN.md` and
> `persistence-fs/PLAN.md` (window geometry persists through `settings_get`/`settings_set`). Covers the
> app shell (frameless window, panes, splitter, shortcuts, tray/autostart, multi-launch prevention) —
> the container everything else in `docs/plans/` runs inside.

Sources: `docs/spec.md` §2.7, §9.1 item 1, §9; `docs/detailed-spec.md` §5.7.

---

## 1. Frameless Window (`[spec §9.1 item 1]`)

- Tauri window config: `decorations: false`.
- Empty area of the tab bar carries a drag-region attribute (Tauri's `data-tauri-drag-region`
  equivalent) so dragging it moves the window, replacing the native title bar's drag affordance.
- Minimize/maximize/close buttons are self-implemented UI elements (`components/TitleBarControls/` or
  folded into `TabBar`), calling Tauri's window API (`appWindow.minimize()`/`toggleMaximize()`/`close()`).
- Windows 11's snap-layout flyout (hover on the native maximize button) is not shown by default since
  there is no native maximize button — this is a natural consequence of `decorations: false`, not
  separate work. Edge-drag snapping (dragging the window to a screen edge) continues to work since
  that's an OS-level window-manager behavior independent of window decorations.
- `close()` must route through the same "flush pending saves before exit" logic as
  `editor-save/PLAN.md` §6's exit trigger — intercept the close-requested event, await the flush, then
  allow the actual close.

## 2. Three-Pane Layout

- Tab bar (top, full width) / Tree (left or right per `AppSettings.window.treeSide`) / Editor (fills
  remaining space) + status bar (bottom, editor-pane-width or full-width — `spec.md` doesn't specify,
  reasonable default is full-width since it shows both memo-specific and possibly app-wide info).
- Splitter between tree and editor: drag to resize (`AppSettings.window.treeWidth`, persisted via
  `settings_set`, debounced during drag per `persistence-fs/PLAN.md` §6.2's window-geometry note),
  double-click or a toggle button to hide the tree pane entirely (editor takes full width), a setting
  or toolbar button to flip `treeSide` between `'left'`/`'right'`.
- Window geometry (`x, y, width, height, maximized`) persisted on resize/move (debounced) and restored
  at startup via `settings_get`. If the persisted position would place the window off-screen (e.g.
  after a monitor configuration change), clamp to a visible area at startup — not explicitly specified
  by `spec.md`, but a reasonable defensive default consistent with the app never becoming unreachable
  through no fault of the user's own data; not a blocking ambiguity.

## 3. Keyboard Shortcuts

- `AppSettings.shortcuts: Record<string, string>` (`[spec §3.4]`) — user-configurable
  (`[spec §2.7]`). `shared/shortcuts.ts` defines the default keymap (action-id → key-combo string);
  `AppSettings.shortcuts` overrides entries by action-id, merged over the defaults at startup.
- Default keymap covers at minimum: `Ctrl+S` (save), F2 (rename), the dedicated force-delete shortcut
  (`memo-tree/PLAN.md` §4, `Ctrl+Delete`), find/replace open, and any other action explicitly named in
  `spec.md` as needing a shortcut. Full enumerated default keymap is an implementation-discretion
  detail (`[detailed-spec §9]`).
- **All 16 Markdown-toolbar actions get a default binding** — confirmed policy
  (`[detailed-spec §5.10, §9]`): every toolbar action must have *some* default shortcut, not just be
  toolbar-click-only; exact combos are this plan's call. Defaults (none collide with each other or with
  the entries above):

  | Action | Shortcut | | Action | Shortcut |
  |---|---|---|---|---|
  | Undo | `Ctrl+Z` | | Redo | `Ctrl+Y` |
  | Bold | `Ctrl+B` | | Italic | `Ctrl+I` |
  | Strikethrough | `Ctrl+Shift+X` | | Color | `Ctrl+Shift+C` |
  | Font size | `Ctrl+Shift+F` | | Heading | `Ctrl+Shift+H` |
  | List | `Ctrl+Shift+L` | | Quote | `Ctrl+Shift+Q` |
  | Code block | `Ctrl+Shift+K` | | Link insert | `Ctrl+K` |
  | Table insert | `Ctrl+Shift+T` | | Image insert | `Ctrl+Shift+I` |
  | Update (TOC) | `Ctrl+Shift+O` | | Rearrange toggle | `Ctrl+Shift+R` |

  All rebindable via `AppSettings.shortcuts` like any other entry (no new mechanism).
- A Settings screen "Shortcuts" tab lets the user rebind entries, writing to `AppSettings.shortcuts`
  via `settings_set`.

## 4. Auto-Launch to Tray on Windows Login

- Setting: a boolean in `AppSettings` (not currently named in `[spec §3.4]`'s schema — additive field,
  e.g. `autostart: { enabled: boolean }`, implementation discretion for exact shape since `spec.md`
  only says "setting available").
- Mechanism: a Windows Startup-folder shortcut (preferred, per `[detailed-spec §5.7]`'s reasoning
  about not touching the registry for *data*) pointing at the exe with a launch argument/flag (e.g.
  `--tray`) that causes the app to start minimized-to-tray instead of showing its window immediately.
  A Registry Run-key is also acceptable if the Startup-folder approach proves impractical in Tauri
  (`[detailed-spec §9]`) — either way, no user data/settings are stored in the registry, preserving the
  portability principle (`[spec §2.6]`), which is what actually matters here, not the autostart
  mechanism itself.
- Tray icon: standard Tauri tray API, left/double-click restores the window
  (`evt:tray:show-request` → frontend requests `appWindow.show()`/`unminimize()`/`setFocus()` — or this
  can be done directly in Rust without a round-trip event, `developer-lite`'s call per
  `project-architecture/PLAN.md` §5's note that new events only get added if actually needed).

## 5. Multi-Launch Prevention

Elaborates `[spec §2.7]`, `[detailed-spec §5.7]`: second launch must (a) not open a second window, (b)
not write App Settings JSON from two processes, (c) bring the existing window to the foreground.

- Mechanism: Tauri's single-instance plugin (if available for the Tauri version in use) is the natural
  first choice (OS-level named-mutex/pipe based, exactly matches this requirement out of the box); a
  hand-rolled named-pipe or lock-file check is the fallback if the plugin isn't suitable — exact choice
  is implementation discretion (`[detailed-spec §9]`).
- On detecting a second-instance launch attempt, the **first** (already-running) instance receives a
  signal and emits `evt:app:focus-request`; frontend or a direct Rust window-API call brings the window
  to the foreground (restoring from tray/minimized if needed). The second process then exits
  immediately without initializing any settings/data access.

## 6. Right-Click Suppression / Native Menu Removal

Already-established behavior to preserve (`[spec §2.7, §9]`, "issue.md Delegated Response" table marks
this "Implemented" in the legacy app, carried forward as a requirement here, not new work to design):
right-click outside registered tabs on the tab bar shows no context menu; native WebView2/OS
context-menu defaults and title-bar right-click items are suppressed (Tauri equivalent of
`Menu.setApplicationMenu(null)` — set `menu: None`/no default menu in the Tauri window config, and
intercept/`preventDefault()` the browser's native `contextmenu` event globally except where the app's
own custom context menu (`memo-tree/PLAN.md` §4, shared component per `[spec §9.1 item 2]`) is
explicitly invoked).

## 7. Settings Screen Shell

`components/Settings/`: houses Appearance (`appearance-theme/PLAN.md`), Editor defaults
(`editorDefaults.*`, referenced by `editor-save/PLAN.md`/`markdown-editing/PLAN.md`), Shortcuts (§3
above), Links (`link/PLAN.md`'s `openTrigger`/`confirmBeforeOpen`), Autostart (§4 above) —
General/Editor two-tab layout with checkbox/dropdown vertical-margin conventions and Esc support
already established as required (`[spec §9]`, issue.md carryover, "Implemented" in legacy, preserved
here). All fields read via `settings_get` on open, write via `settings_set` per-field or as a single
patch on a "Save"/immediate-apply action — either interaction pattern is implementation discretion,
`spec.md` doesn't mandate one.

## Open Items

None.
