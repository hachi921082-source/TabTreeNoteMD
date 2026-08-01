// Default keyboard shortcut keymap (`docs/plans/window-operation/PLAN.md`
// §3; `docs/spec.md` §2.7/§2.1 "switch via click / shortcut"). Every action
// gets an action-id here; `AppSettings.shortcuts` overrides entries by
// action-id at startup (not wired up yet — no Settings screen exists to
// edit it, `window-operation/PLAN.md`'s own scope). Only the entries needed
// by already-implemented features are populated so far; later features add
// their own action-ids here rather than duplicating this file.
//
// Combo strings use the same "Ctrl+Shift+X" style as the rest of the app's
// documentation (`docs/plans/window-operation/PLAN.md` §3's toolbar-action
// table). Exact key combinations not already fixed by `spec.md` are
// implementation discretion (`docs/detailed-spec.md` §9).

export const DEFAULT_SHORTCUTS: Record<string, string> = {
  // Tab bar (`docs/plans/tab-management/PLAN.md` §4). Mouse-wheel switching
  // is intentionally NOT bound to anything (`docs/DECISIONS.md`
  // "Mouse-Wheel Tab Switching: Removed from v1 Scope").
  "tab.switchNext": "Ctrl+Tab",
  "tab.switchPrev": "Ctrl+Shift+Tab",
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
