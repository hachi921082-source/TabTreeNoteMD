// Click handler on a rendered `☐`/`☑` decoration widget (`docs/plans/markdown-editing/PLAN.md`
// §2): toggles the underlying `[ ]`/`[x]` text in place via a CodeMirror transaction. This is the
// "direct click" toggle path (`[spec §2.3]`), distinct from `editor/textCommands.ts`'s right-click
// "Check" menu action (which cycles a bare `☐`/`☑` glyph already present in the *rendered* text —
// that plan's own plain-text-layer scope, not this one).
//
// Implemented as a `WidgetType`/`Decoration.replace` (not a mark) — deliberately, since a checkbox
// is a genuine "replace this exact 3-character range with a rendered glyph" case, one of the two
// widget-decoration usages this plan calls out as needing the high-risk-zone test coverage in §9
// item 3 ("clicking to place the cursor on/near a decoration widget — a rendered checkbox glyph").
// The replaced range is always exactly the literal `[ ]`/`[x]`/`[X]` text (3 characters), so its
// visual-to-source length mismatch is fixed and small, unlike a heading/emphasis marker whose
// hidden length can vary — kept unconditionally visible (not gated on cursor presence) since
// toggling is the primary interaction and the underlying text is still fully intact/navigable.
import { Decoration, EditorView, WidgetType } from "@codemirror/view";

/** Matches a GFM task-list marker at the very start of a string (used against the text
 * immediately following a list-item marker + one space). Accepts `[ ]`, `[x]`, `[X]`. */
export const TASK_MARKER_RE = /^\[([ xX])\]/;

class CheckboxWidget extends WidgetType {
  constructor(
    readonly checked: boolean,
    readonly from: number,
    readonly to: number,
  ) {
    super();
  }

  eq(other: CheckboxWidget): boolean {
    return other.checked === this.checked && other.from === this.from && other.to === this.to;
  }

  toDOM(view: EditorView): HTMLElement {
    const el = document.createElement("span");
    el.className = "cm-md-checkbox";
    el.textContent = this.checked ? "☑" : "☐";
    el.setAttribute("role", "checkbox");
    el.setAttribute("aria-checked", this.checked ? "true" : "false");
    el.title = this.checked ? "クリックでチェックを外す" : "クリックでチェック";
    el.addEventListener("mousedown", (e) => {
      // Prevent CodeMirror from also placing the cursor via its own mousedown handling before
      // our toggle transaction runs — the widget fully owns this click.
      e.preventDefault();
      e.stopPropagation();
      toggleCheckboxRange(view, this.from, this.to, this.checked);
    });
    return el;
  }

  ignoreEvent(): boolean {
    return false;
  }
}

/** Replaces the exact `[ ]`/`[x]`/`[X]` text at `[from, to)` with its opposite state. */
export function toggleCheckboxRange(view: EditorView, from: number, to: number, currentlyChecked: boolean): void {
  view.dispatch({ changes: { from, to, insert: currentlyChecked ? "[ ]" : "[x]" } });
}

/** Builds the replace-decoration for one task-marker range. `checked` reflects the *current*
 * document text at the time of the decoration pass (recomputed every pass, so it always matches
 * the buffer — there is no separately-tracked widget state to go stale). */
export function checkboxDecoration(checked: boolean, from: number, to: number) {
  return Decoration.replace({ widget: new CheckboxWidget(checked, from, to), inclusive: false });
}
