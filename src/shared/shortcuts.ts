// Default keyboard shortcut keymap (`docs/plans/window-operation/PLAN.md`
// §3; `docs/spec.md` §2.7/§2.1 "switch via click / shortcut"). Every action
// gets an action-id here; `AppSettings.shortcuts` overrides entries by
// action-id, merged over these defaults at startup — every consumer below
// calls `resolveShortcuts`/`comboFromEvent` rather than hardcoding a key
// check, so every entry here is rebindable via the Settings screen's
// Shortcuts tab (`components/Settings/SettingsDialog.tsx`).
//
// Combo strings use the same "Ctrl+Shift+X" style as the rest of the app's
// documentation. The 16 markdown-toolbar combos (plus `markdown.tocUpdate`'s
// pairing with "Update (TOC)") match `window-operation/PLAN.md` §3's
// explicit table verbatim; the rest (tab switching, save, rename,
// force-delete, find/replace) are implementation discretion not otherwise
// fixed by `spec.md` (`docs/detailed-spec.md` §9), chosen to not collide
// with the table or each other.

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

  // Markdown toolbar (`docs/plans/markdown-editing/PLAN.md` §4; `docs/detailed-spec.md` §5.10:
  // "every one of the 16 toolbar actions gets a default keyboard shortcut"; combos fixed by
  // `docs/plans/window-operation/PLAN.md` §3's table). Action ids match
  // `editor/toolbarSlots.ts`'s `ToolbarActionId`, namespaced under `markdown.`.
  // `markdown.toggleLiveView` is the runtime live/source toggle (`[detailed-spec §5.10]`) — not
  // one of the 16 fixed grid slots (so not in that table), but still gets a default binding for
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
  "markdown.rearrangeToggle": "Ctrl+Shift+R",
  "markdown.toggleLiveView": "Ctrl+Shift+M",
};

/** Merges user overrides (`AppSettings.shortcuts`) over the defaults, by
 * action-id. */
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
