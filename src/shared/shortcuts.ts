// Default keyboard shortcut keymap (`docs/plans/window-operation/PLAN.md`
// §3; `docs/spec.md` §2.7/§2.1 "switch via click / shortcut"). Every action
// gets an action-id here; `AppSettings.shortcuts` overrides entries by
// action-id, merged over these defaults at startup — every consumer below
// calls `resolveShortcuts`/`comboFromEvent` rather than hardcoding a key
// check, so every entry here is rebindable via the Settings screen's
// Shortcuts tab (`components/Settings/SettingsDialog.tsx`).
//
// Combo strings use the same "Ctrl+Shift+X" style as the rest of the app's
// documentation. The 16 original markdown-toolbar combos (plus
// `markdown.tocUpdate`'s pairing with "Update (TOC)") match
// `window-operation/PLAN.md` §3's explicit table verbatim; the rest (tab
// switching, save, rename, force-delete, find/replace, and GitHub issue
// #176's `markdown.clearFormatting`/`markdown.hardBreak` additions) are
// implementation discretion not otherwise fixed by `spec.md`
// (`docs/detailed-spec.md` §9), chosen to not collide with the table or
// each other.

export const DEFAULT_SHORTCUTS: Record<string, string> = {
  // Tab bar (`docs/plans/tab-management/PLAN.md` §4). Mouse-wheel switching
  // is intentionally NOT bound to anything (`docs/DECISIONS.md`
  // "Mouse-Wheel Tab Switching: Removed from v1 Scope").
  "tab.switchNext": "Ctrl+Tab",
  "tab.switchPrev": "Ctrl+Shift+Tab",

  // `docs/plans/editor-save/PLAN.md` §6 (save), §5 (find/replace);
  // `docs/plans/memo-tree/PLAN.md` §3 (rename), §4 (force-delete).
  "editor.save": "Ctrl+S",
  "editor.findReplace": "Ctrl+F",
  "tree.rename": "F2",
  "tree.forceDelete": "Ctrl+Delete",

  // Cross-Tab Quick Open (`docs/plans/v1.1.0/quick-open/PLAN.md` §4, `[spec v1.1.0 §3.1]`) — opens
  // the memo-name jump-open overlay. `Ctrl+O` verified unused against this very map as of this
  // writing (not `Ctrl+P`: taken by `markdown.pdfExport` below, also the native browser-print
  // shortcut; not `Ctrl+Shift+P`: taken by `markdown.paneModeCycle` below).
  "search.quickOpen": "Ctrl+O",

  // Markdown toolbar (`docs/plans/markdown-editing/PLAN.md` §4; `docs/detailed-spec.md` §5.10:
  // "every toolbar action gets a default keyboard shortcut", now 18 actions after GitHub issue
  // #176; the original 16's combos fixed by `docs/plans/window-operation/PLAN.md` §3's table).
  // Action ids match `editor/toolbarSlots.ts`'s `ToolbarActionId`, namespaced under `markdown.`.
  // `markdown.toggleLiveView` is the runtime live/source toggle (`[detailed-spec §5.10]`) — not
  // one of the 18 fixed grid slots (so not in that table), but still gets a default binding for
  // the same reason.
  "markdown.undo": "Ctrl+Z",
  "markdown.redo": "Ctrl+Y",
  "markdown.color": "Ctrl+Shift+C",
  "markdown.fontSize": "Ctrl+Shift+F",
  "markdown.heading": "Ctrl+Shift+H",
  "markdown.list": "Ctrl+Shift+L",
  "markdown.bold": "Ctrl+B",
  "markdown.italic": "Ctrl+I",
  "markdown.strikethrough": "Ctrl+Shift+X",
  "markdown.quote": "Ctrl+Shift+Q",
  "markdown.link": "Ctrl+K",
  "markdown.codeBlock": "Ctrl+Shift+K",
  "markdown.tocUpdate": "Ctrl+Shift+O",
  "markdown.table": "Ctrl+Shift+T",
  "markdown.image": "Ctrl+Shift+I",
  "markdown.clearFormatting": "Ctrl+Shift+E",
  "markdown.hardBreak": "Shift+Enter",
  "markdown.rearrangeToggle": "Ctrl+Shift+R",
  "markdown.toggleLiveView": "Ctrl+Shift+M",

  // `docs/plans/markdown/markdown-preview/PLAN.md` §6.3 — cycles the 3-state pane-mode toggle
  // (edit -> split -> preview -> edit -> ...). Not one of the 16 fixed grid slots (lives in the
  // toolbar header instead, same treatment as `markdown.toggleLiveView` above) and dispatched by a
  // separate, focus-unconstrained effect in `EditorPane.tsx` rather than through
  // `MarkdownToolbar`'s `invokeAction` switch.
  "markdown.paneModeCycle": "Ctrl+Shift+P",

  // `docs/plans/markdown/pdf-export/PLAN.md` §4 (`docs/MDEditor-spec.md` §8) — hidden-iframe print
  // flow. Not one of the 16 fixed grid slots (toolbar-header button instead, same treatment as
  // `markdown.paneModeCycle` above) and, like it, dispatched by a separate, focus-unconstrained
  // effect in `EditorPane.tsx`. `Ctrl+P` is also WebView2/Chromium's native browser-print shortcut —
  // the handler must call `event.preventDefault()` to suppress that native dialog (`MDEditor-spec.md`
  // §8's "`Ctrl+P` native-print conflict" note).
  "markdown.pdfExport": "Ctrl+P",

  // `docs/plans/v1.2.0/preview-zoom/PLAN.md` §5.1 — ± buttons in the preview-mode toolbar header
  // (`paneMode === "preview"` only). Not one of the 16 fixed grid slots and, like
  // `markdown.paneModeCycle`/`markdown.pdfExport` above, dispatched by a separate,
  // focus-unconstrained effect in `EditorPane.tsx` (must fire even while focus sits inside the
  // preview pane's own DOM subtree). `Ctrl+=`/`Ctrl+-` are also WebView2/Chromium's native
  // page-zoom shortcuts — the handler must call `event.preventDefault()` to suppress that native
  // behavior, same reasoning as `markdown.pdfExport`'s native-print-dialog conflict. The
  // `Ctrl+NumpadAdd`/`Ctrl+NumpadSubtract` numpad aliases are handled separately in `EditorPane.tsx`
  // (§5.3: `Ctrl+NumpadSubtract`'s `e.key` already equals `Ctrl+-`'s via `comboFromEvent`, so it
  // needs no extra entry here; `Ctrl+NumpadAdd`'s `e.key` differs from `Ctrl+=`'s and is checked via
  // `e.code` outside this rebindable map instead).
  "preview.zoomIn": "Ctrl+=",
  "preview.zoomOut": "Ctrl+-",
};

/** Action ids that are rebindable via the Settings screen's Shortcuts tab but ship with NO default
 * combo in `DEFAULT_SHORTCUTS` above — so the row starts unbound and only fires once (if ever) a
 * user configures one. Kept as its own list rather than adding `undefined`-valued entries to
 * `DEFAULT_SHORTCUTS` (whose every value is a real combo string, matched exactly against
 * `comboFromEvent`'s output elsewhere).
 *
 * - `favorites.open` — Cross-Tab Favorites list (`docs/plans/v1.1.0/favorites/PLAN.md` §6,
 *   `[spec v1.1.0 §4.6]`): always reachable via its own toolbar ★ button, so no default keyboard
 *   shortcut is required; this only exists so the action is still rebindable for users who want one.
 */
export const UNBOUND_ACTION_IDS: string[] = ["favorites.open"];

/** Every rebindable action id, in the order the Settings screen's Shortcuts tab should list them —
 * `DEFAULT_SHORTCUTS`'s own keys (each with a real default combo) followed by `UNBOUND_ACTION_IDS`
 * (no default combo, `undefined` unless user-bound). */
export const REBINDABLE_ACTION_IDS: string[] = [...Object.keys(DEFAULT_SHORTCUTS), ...UNBOUND_ACTION_IDS];

/** Merges user overrides (`AppSettings.shortcuts`) over the defaults, by
 * action-id. An action id in `UNBOUND_ACTION_IDS` resolves to `undefined` here unless `overrides`
 * itself supplies a combo for it — `DEFAULT_SHORTCUTS` has no entry for those ids to spread in. */
export function resolveShortcuts(overrides: Record<string, string> | undefined): Record<string, string> {
  return { ...DEFAULT_SHORTCUTS, ...(overrides ?? {}) };
}

/** Minimal shape needed from a keyboard event to normalize it into the same
 * combo-string format as the keymap, without depending on `react`'s
 * `KeyboardEvent` type in this framework-agnostic `shared/` module. */
export interface KeyComboLike {
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  key: string;
}

/** Normalizes a keyboard event into a "Ctrl+Shift+X"-style combo string for
 * matching against the keymap. */
export function comboFromEvent(e: KeyComboLike): string {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (!["Control", "Alt", "Shift"].includes(e.key)) {
    parts.push(normalizeKeyName(e.key));
  }
  return parts.join("+");
}

function normalizeKeyName(key: string): string {
  if (key === " ") return "Space";
  if (key.length === 1) return key.toUpperCase();
  return key; // e.g. "Tab", "Delete", "F2", "Escape"
}

/** Every combo this app's own shortcut system has claimed — used to keep CodeMirror's own
 * built-in `@codemirror/commands` keymaps (`defaultKeymap`/`historyKeymap`) from silently
 * shadowing them (GitHub issue #140: several, including a destructive one, do). Static, built from
 * `DEFAULT_SHORTCUTS` rather than the live per-session `AppSettings.shortcuts` override — a user
 * who rebinds one of these 16+ actions to a combo that happens to newly collide with a CodeMirror
 * built-in is a separate, out-of-scope follow-up (`AppSettings.shortcuts`'s live value isn't
 * threaded into the CodeMirror extension-building layer at all today). */
export const RESERVED_COMBOS = new Set(Object.values(DEFAULT_SHORTCUTS));

/** Converts one CodeMirror `KeyBinding.key`/`.win` string (e.g. `"Mod-i"`, `"Shift-Mod-k"`) into
 * this app's own `"Ctrl+I"`/`"Ctrl+Shift+K"` combo format, for comparison against
 * `RESERVED_COMBOS`/`comboFromEvent`'s output. Windows-only translation (`Mod` -> `Ctrl`,
 * `Ctrl` passed through as-is) — this app's primary target platform (`docs/spec.md` §6: "Windows
 * 11 primary target"); a binding's separate `.mac`/`.linux` fields are irrelevant here since they
 * never apply on Windows regardless of this app's own combo choices. */
export function normalizeCodeMirrorKey(cmKey: string): string {
  const segments = cmKey.split("-");
  const keyPart = segments[segments.length - 1];
  const modifiers = segments.slice(0, -1);
  const parts: string[] = [];
  if (modifiers.includes("Mod") || modifiers.includes("Ctrl")) parts.push("Ctrl");
  if (modifiers.includes("Alt")) parts.push("Alt");
  if (modifiers.includes("Shift")) parts.push("Shift");
  parts.push(normalizeKeyName(keyPart));
  return parts.join("+");
}
