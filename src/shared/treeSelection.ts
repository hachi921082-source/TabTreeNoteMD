// Pure, framework-agnostic tree selection logic (`docs/plans/memo-tree/PLAN.md`
// §4): "Standard OS-file-explorer-style semantics: flatten the currently-
// visible tree (respecting collapsed nodes) into an ordered list; click sets
// a single 'anchor' and clears any other selection; Ctrl-click adds/removes
// just the clicked node from the selection set without moving the anchor;
// Shift-click selects every node between the anchor and the clicked node in
// that flattened order (inclusive), replacing the previous selection."
// Deliberately independent of rendering/React so this logic is easy to
// reason about and unit-test in isolation (no test runner is configured in
// this repo yet, so these are exercised indirectly via `components/Tree/`
// today; the pure-function shape keeps that option open).
import type { TreeNode } from "./models";

/** Flattens the currently-visible tree (respecting collapsed nodes: a node's children are only
 * included when `expanded` is `true`) into an ordered list of `memoName`s, matching on-screen
 * row order top-to-bottom. */
export function flattenVisible(nodes: readonly TreeNode[]): string[] {
  const out: string[] = [];
  const walk = (list: readonly TreeNode[]) => {
    for (const n of list) {
      out.push(n.memoName);
      if (n.expanded && n.children.length > 0) walk(n.children);
    }
  };
  walk(nodes);
  return out;
}

export interface SelectionState {
  selected: ReadonlySet<string>;
  /** The most recent plain-click (or arrow-nav) target; Shift-click ranges from this id and
   * does not itself move it. `null` when nothing has ever been selected. */
  anchor: string | null;
}

export interface ClickModifiers {
  ctrlKey: boolean;
  shiftKey: boolean;
}

export const EMPTY_SELECTION: SelectionState = { selected: new Set(), anchor: null };

/** Applies one row click to the current selection state. `flatOrder` must be the current
 * visible-order id list (`flattenVisible`'s output) so Shift-range resolution is correct. */
export function applyClick(current: SelectionState, flatOrder: readonly string[], clickedId: string, modifiers: ClickModifiers): SelectionState {
  if (modifiers.shiftKey && current.anchor !== null) {
    const from = flatOrder.indexOf(current.anchor);
    const to = flatOrder.indexOf(clickedId);
    if (from === -1 || to === -1) {
      return { selected: new Set([clickedId]), anchor: clickedId };
    }
    const lo = Math.min(from, to);
    const hi = Math.max(from, to);
    return { selected: new Set(flatOrder.slice(lo, hi + 1)), anchor: current.anchor };
  }

  if (modifiers.ctrlKey) {
    const next = new Set(current.selected);
    if (next.has(clickedId)) next.delete(clickedId);
    else next.add(clickedId);
    return { selected: next, anchor: current.anchor ?? clickedId };
  }

  return { selected: new Set([clickedId]), anchor: clickedId };
}

/** Click on empty area (below the last row): clears the selection entirely
 * (`docs/plans/memo-tree/PLAN.md` §4). */
export function clearSelection(): SelectionState {
  return EMPTY_SELECTION;
}

/** ↑/↓ arrow-key navigation target: the previous/next id in `flatOrder` relative to
 * `currentId`. If nothing is currently selected, jumps to the first (↓) or last (↑) row.
 * Returns `null` only when `flatOrder` is empty. */
export function moveSelection(flatOrder: readonly string[], currentId: string | null, direction: "up" | "down"): string | null {
  if (flatOrder.length === 0) return null;
  const idx = currentId === null ? -1 : flatOrder.indexOf(currentId);
  if (idx === -1) {
    return direction === "down" ? flatOrder[0] : flatOrder[flatOrder.length - 1];
  }
  const nextIdx = direction === "down" ? Math.min(idx + 1, flatOrder.length - 1) : Math.max(idx - 1, 0);
  return flatOrder[nextIdx];
}
