// Toolbar-invoked commands (`docs/plans/markdown-editing/PLAN.md` §4): each produces one
// CodeMirror transaction (one undo step), applying to the current selection or inserting at the
// cursor, per standard Markdown-editor toolbar conventions. Undo/Redo reuse
// `editor/textCommands.ts`'s existing `undoCommand`/`redoCommand` re-exports rather than
// duplicating them — only import those from the toolbar UI layer directly.
//
// "Image insert" is an explicit stub: `image-handling/PLAN.md` (the plan that actually owns
// `image_save_pasted`) is not implemented yet. It logs a clearly-labeled no-op rather than silently
// doing nothing unexplained, so its forward seam is visible in the console during manual testing;
// wiring the real behavior in later is just replacing this function body. "Update" (TOC) is real —
// see `tocGenerator.ts`'s `tocUpdateCommand`, imported directly by
// `components/Editor/Toolbar/MarkdownToolbar.tsx` rather than re-exported from here, since it's
// `table-of-contents/PLAN.md`'s own module, not this plan's.
import type { EditorView } from "@codemirror/view";
import { findColorSpans } from "./markdownSpanParser";

function currentSelectionText(view: EditorView): { from: number; to: number; text: string } {
  const sel = view.state.selection.main;
  return { from: sel.from, to: sel.to, text: view.state.sliceDoc(sel.from, sel.to) };
}

// ---------------------------------------------------------------------------
// Bold / Italic / Strikethrough — toggle-wrap the current selection in the CommonMark marker.
// Known limitation (implementation discretion, not spec-mandated): a naive single-`*` italic
// toggle can misfire at the boundary of an already-bold `**text**` selection (a half-match of the
// `**` pair) — an acceptable edge case for a toolbar convenience action, not a data-integrity
// concern (worst case is one extra/missing `*`, trivially fixed by re-clicking or hand-editing).
// ---------------------------------------------------------------------------

function toggleWrap(view: EditorView, marker: string): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const before = state.sliceDoc(Math.max(0, sel.from - marker.length), sel.from);
  const after = state.sliceDoc(sel.to, Math.min(state.doc.length, sel.to + marker.length));
  if (before === marker && after === marker) {
    view.dispatch({
      changes: [
        { from: sel.from - marker.length, to: sel.from, insert: "" },
        { from: sel.to, to: sel.to + marker.length, insert: "" },
      ],
      selection: { anchor: sel.from - marker.length, head: sel.to - marker.length },
    });
  } else {
    const inner = state.sliceDoc(sel.from, sel.to);
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert: `${marker}${inner}${marker}` },
      selection: sel.empty ? { anchor: sel.from + marker.length } : { anchor: sel.from + marker.length, head: sel.to + marker.length },
    });
  }
  view.focus();
  return true;
}

export function boldCommand(view: EditorView): boolean {
  return toggleWrap(view, "**");
}
export function italicCommand(view: EditorView): boolean {
  return toggleWrap(view, "*");
}
export function strikethroughCommand(view: EditorView): boolean {
  return toggleWrap(view, "~~");
}

// ---------------------------------------------------------------------------
// Heading — cycles the current line's `#`..`######` prefix (none -> H1 -> H2 -> ... -> H6 -> none).
// ---------------------------------------------------------------------------

export function headingCommand(view: EditorView): boolean {
  const { state } = view;
  const line = state.doc.lineAt(state.selection.main.head);
  const match = /^(#{1,6})\s/.exec(line.text);
  const level = match ? match[1].length : 0;
  const next = level >= 6 ? 0 : level + 1;
  const oldLen = match ? match[0].length : 0;
  const newPrefix = next === 0 ? "" : "#".repeat(next) + " ";
  view.dispatch({ changes: { from: line.from, to: line.from + oldLen, insert: newPrefix } });
  view.focus();
  return true;
}

// ---------------------------------------------------------------------------
// List / Quote — toggle a `- `/`> ` prefix on every line the selection spans.
// ---------------------------------------------------------------------------

function linesInSelection(view: EditorView) {
  const { state } = view;
  const lineNumbers = new Set<number>();
  for (const range of state.selection.ranges) {
    const startLine = state.doc.lineAt(range.from).number;
    const endLine = state.doc.lineAt(range.to).number;
    for (let n = startLine; n <= endLine; n++) lineNumbers.add(n);
  }
  return [...lineNumbers].sort((a, b) => a - b).map((n) => state.doc.line(n));
}

function toggleLinePrefix(view: EditorView, re: RegExp, makePrefix: (indent: string) => string): boolean {
  const lines = linesInSelection(view);
  const allPrefixed = lines.every((l) => re.test(l.text));
  const changes = lines.map((l) => {
    if (allPrefixed) {
      const m = re.exec(l.text);
      return { from: l.from, to: l.from + (m ? m[0].length : 0), insert: "" };
    }
    const indent = /^(\s*)/.exec(l.text)?.[1] ?? "";
    return { from: l.from, to: l.from + indent.length, insert: makePrefix(indent) };
  });
  view.dispatch({ changes });
  view.focus();
  return true;
}

export function listCommand(view: EditorView): boolean {
  return toggleLinePrefix(view, /^(\s*)[-*+]\s/, (indent) => `${indent}- `);
}

export function quoteCommand(view: EditorView): boolean {
  return toggleLinePrefix(view, /^(\s*)>\s/, (indent) => `${indent}> `);
}

// ---------------------------------------------------------------------------
// Code block — wraps the selection in a fenced code block, or inserts an empty skeleton with the
// cursor placed inside if there is no selection.
// ---------------------------------------------------------------------------

export function codeBlockCommand(view: EditorView): boolean {
  const { from, to, text } = currentSelectionText(view);
  if (text === "") {
    const insert = "```\n\n```";
    view.dispatch({ changes: { from, insert }, selection: { anchor: from + 4 } });
  } else {
    const insert = "```\n" + text + "\n```";
    view.dispatch({ changes: { from, to, insert }, selection: { anchor: from + insert.length } });
  }
  view.focus();
  return true;
}

// ---------------------------------------------------------------------------
// Table insert — a 2-column skeleton at the cursor (exact starter shape is implementation
// discretion; `[spec §4.2]` only specifies "table insert" as a toolbar action, not a fixed size).
// ---------------------------------------------------------------------------

export function tableCommand(view: EditorView): boolean {
  const pos = view.state.selection.main.from;
  const skeleton = "| Header 1 | Header 2 |\n| --- | --- |\n| Cell 1 | Cell 2 |\n";
  view.dispatch({ changes: { from: pos, insert: skeleton }, selection: { anchor: pos + skeleton.length } });
  view.focus();
  return true;
}

// ---------------------------------------------------------------------------
// Link insert — wraps the selection as the link label (or a placeholder label if there is no
// selection), leaving `url` selected so the user can immediately type/paste over it.
// ---------------------------------------------------------------------------

export function linkCommand(view: EditorView): boolean {
  const { from, to, text } = currentSelectionText(view);
  const label = text === "" ? "リンク" : text;
  const insert = `[${label}](url)`;
  const urlStart = from + label.length + 3; // "[" + label + "]("
  view.dispatch({ changes: { from, to, insert }, selection: { anchor: urlStart, head: urlStart + 3 } });
  view.focus();
  return true;
}

// ---------------------------------------------------------------------------
// Color / Font-size — wraps the selection in `<span style="...">` (closed 5-choice color palette
// / 3-choice size palette, `docs/plans/markdown-editing/PLAN.md` §4). Deliberately does not search
// for/merge into an existing enclosing span — a second wrap nests (`<span style="font-size:.."
// ><span style="color:..">...</span></span>`), which composes correctly via CSS exactly like this
// plan's mark-decoration approach elsewhere (`markdownDecorations.ts`'s header comment), rather
// than needing bespoke "merge two style attributes" logic for a toolbar convenience action.
// ---------------------------------------------------------------------------

function wrapSelectionInSpan(view: EditorView, styleDecl: string): boolean {
  const { from, to, text } = currentSelectionText(view);
  const openTag = `<span style="${styleDecl}">`;
  const insert = `${openTag}${text}</span>`;
  view.dispatch({
    changes: { from, to, insert },
    selection: text === "" ? { anchor: from + openTag.length } : { anchor: from, head: from + insert.length },
  });
  view.focus();
  return true;
}

export function applyColorSpan(view: EditorView, hex: string): boolean {
  return wrapSelectionInSpan(view, `color:${hex}`);
}

export function applyFontSizeSpan(view: EditorView, px: string): boolean {
  return wrapSelectionInSpan(view, `font-size:${px}`);
}

/** Removes exactly `prop` (`"color"` or `"font-size"`) from the nearest `<span style="...">` whose
 * full tag range encloses the current selection; unwraps the `<span>` entirely (keeping its inner
 * text) if no allowlisted property remains. No-op (returns `false`) if the selection isn't inside
 * any such span. */
function clearSpanProperty(view: EditorView, prop: "color" | "font-size"): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const spans = findColorSpans(state.doc.toString());
  const target = spans.find((s) => s.openFrom <= sel.from && s.closeTo >= sel.to);
  if (!target) return false;
  const remaining = target.style
    .split(";")
    .map((d) => d.trim())
    .filter(Boolean)
    .filter((d) => !d.toLowerCase().startsWith(prop));
  const inner = state.sliceDoc(target.contentFrom, target.contentTo);
  const insert = remaining.length ? `<span style="${remaining.join("; ")}">${inner}</span>` : inner;
  view.dispatch({ changes: { from: target.openFrom, to: target.closeTo, insert } });
  view.focus();
  return true;
}

export function clearColorSpan(view: EditorView): boolean {
  return clearSpanProperty(view, "color");
}

export function clearFontSizeSpan(view: EditorView): boolean {
  return clearSpanProperty(view, "font-size");
}

// ---------------------------------------------------------------------------
// Stub — `image-handling/PLAN.md` (`image_save_pasted`) is not implemented yet. See this module's
// header comment for why this logs-and-no-ops rather than silently doing nothing.
// ---------------------------------------------------------------------------

export function imageInsertStub(view: EditorView): boolean {
  console.info("[markdown-editing] Image insert is not implemented yet (see image-handling/PLAN.md) — no image was inserted.");
  view.focus();
  return true;
}
