// Toolbar-invoked commands (`docs/plans/markdown-editing/PLAN.md` §4): each produces one
// CodeMirror transaction (one undo step), applying to the current selection or inserting at the
// cursor, per standard Markdown-editor toolbar conventions. Undo/Redo reuse
// `editor/textCommands.ts`'s existing `undoCommand`/`redoCommand` re-exports rather than
// duplicating them — only import those from the toolbar UI layer directly.
//
// "Image insert" is NOT one of the commands defined here — it's async (file picker + IPC
// round-trip, not a synchronous `(view) => boolean` transaction) and lives in
// `imagePaste.ts`'s `insertImageViaFilePicker` instead (`docs/plans/image-handling/PLAN.md` §6),
// imported directly by `components/Editor/Toolbar/MarkdownToolbar.tsx`. "Update" (TOC) is the same
// shape of exception — see `tocGenerator.ts`'s `tocUpdateCommand`, also imported directly by that
// component rather than re-exported from here, since it's `table-of-contents/PLAN.md`'s own
// module, not this plan's.
import type { EditorView } from "@codemirror/view";
import { findColorSpans } from "./markdownSpanParser";
import { mergeStyleDeclaration } from "./markdownSanitize";

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
  // Map the pre-edit selection through the changes explicitly (assoc = 1, i.e. "stick to the
  // right") instead of relying on CodeMirror's default dispatch-time mapping (assoc = -1, "stick
  // to the left"). Without this, a collapsed cursor sitting exactly at a line's start — the common
  // case, e.g. clicking Quote/List on an empty or just-typed line — maps to *before* the inserted
  // "> "/"- " prefix rather than after it, leaving the cursor in the wrong place to keep typing.
  const changeSet = view.state.changes(changes);
  const selection = view.state.selection.map(changeSet, 1);
  view.dispatch({ changes: changeSet, selection });
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
// / 3-choice size palette, `docs/plans/markdown-editing/PLAN.md` §4). Never nests a new `<span>`
// around an existing one — `markdownSpanParser.ts`'s `SPAN_RE` doesn't support nested `<span>`s, so
// a naive second wrap mis-pairs open/close tags and corrupts rendering (GitHub issue #87). Any
// existing span overlapping the selection has `prop` merged into its own `style` instead of being
// re-wrapped; only text outside every overlapping span gets a fresh `<span>`.
// ---------------------------------------------------------------------------

function wrapSelectionInSpan(view: EditorView, prop: "color" | "font-size", value: string): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const styleDecl = `${prop}: ${value}`;

  const overlapping = findColorSpans(state.doc.toString())
    .filter((s) => s.openFrom < sel.to && s.closeTo > sel.from)
    .sort((a, b) => a.openFrom - b.openFrom);

  if (overlapping.length === 0) {
    const text = state.sliceDoc(sel.from, sel.to);
    const openTag = `<span style="${styleDecl}">`;
    const insert = `${openTag}${text}</span>`;
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert },
      selection: text === "" ? { anchor: sel.from + openTag.length } : { anchor: sel.from, head: sel.from + insert.length },
    });
    view.focus();
    return true;
  }

  // At least one existing span overlaps — expand the edited range to fully cover every overlapping
  // span first, so a span's own tag characters are never split mid-tag, then rebuild that whole
  // range: text already inside an overlapping span merges `prop` into that span's existing style
  // (reusing its tags, never re-wrapping); everything else gets a fresh `<span style="...">`.
  let effFrom = sel.from;
  let effTo = sel.to;
  for (const s of overlapping) {
    effFrom = Math.min(effFrom, s.openFrom);
    effTo = Math.max(effTo, s.closeTo);
  }

  let cursor = effFrom;
  let insert = "";
  for (const s of overlapping) {
    if (s.openFrom > cursor) {
      const plain = state.sliceDoc(cursor, s.openFrom);
      insert += `<span style="${styleDecl}">${plain}</span>`;
    }
    const mergedStyle = mergeStyleDeclaration(s.style, prop, value);
    const inner = state.sliceDoc(s.contentFrom, s.contentTo);
    insert += `<span style="${mergedStyle}">${inner}</span>`;
    cursor = s.closeTo;
  }
  if (cursor < effTo) {
    const plain = state.sliceDoc(cursor, effTo);
    insert += `<span style="${styleDecl}">${plain}</span>`;
  }

  view.dispatch({
    changes: { from: effFrom, to: effTo, insert },
    selection: { anchor: effFrom, head: effFrom + insert.length },
  });
  view.focus();
  return true;
}

export function applyColorSpan(view: EditorView, hex: string): boolean {
  return wrapSelectionInSpan(view, "color", hex);
}

export function applyFontSizeSpan(view: EditorView, px: string): boolean {
  return wrapSelectionInSpan(view, "font-size", px);
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
