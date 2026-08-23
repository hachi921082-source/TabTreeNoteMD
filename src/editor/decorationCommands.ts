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
import type { EditorState } from "@codemirror/state";
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

/** `defaultLabel` is the placeholder inserted when there's no selection — the caller (`Markdown
 * Toolbar.tsx`'s "link" toolbar action, a React component with `useT()` access) supplies the
 * translated `t("editor.defaultLinkLabel")` value rather than this function owning a hardcoded
 * Japanese default (`docs/plans/v1.2.0/i18n/PLAN.md` §6/§7 follow-up) — this module itself has no
 * React context of its own. */
export function linkCommand(view: EditorView, defaultLabel: string): boolean {
  const { from, to, text } = currentSelectionText(view);
  const label = text === "" ? defaultLabel : text;
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
//
// Heading-marker exclusion (issue #204): a wrapped range's leading ATX `#` marker is cut out and left raw, since wrapping it stops `@lezer/markdown` parsing the line as a heading (dropping it from the TOC).
// ---------------------------------------------------------------------------

/** ATX heading marker (`#` through `######` + trailing whitespace) — same shape as `headingCommand` above. */
const HEADING_MARKER_RE = /^#{1,6}\s+/;

/** Line-start ATX heading-marker ranges inside `[from, to)`, clipped to that range; empty if no line in range starts with a marker. */
function headingMarkerExclusions(state: EditorState, from: number, to: number): { from: number; to: number }[] {
  const exclusions: { from: number; to: number }[] = [];
  if (from >= to) return exclusions;
  const startLine = state.doc.lineAt(from).number;
  const endLine = state.doc.lineAt(to).number;
  for (let n = startLine; n <= endLine; n++) {
    const line = state.doc.line(n);
    const match = HEADING_MARKER_RE.exec(line.text);
    if (!match) continue;
    const markerEnd = line.from + match[0].length;
    // Also absorb the newline before a non-first heading line, or the preceding span's close tag would land right before the marker and break parsing the same way.
    const exFrom = Math.max(line.from > from ? line.from - 1 : line.from, from);
    const exTo = Math.min(markerEnd, to);
    if (exFrom < exTo) exclusions.push({ from: exFrom, to: exTo });
    // If range content follows this heading line, exclude the newline right after it too, so a
    // reopened span never lands before the line break (GitHub issue #204 residual).
    if (line.to < to) exclusions.push({ from: line.to, to: Math.min(line.to + 1, to) });
  }
  return exclusions;
}

/** Wraps `[from, to)` in `<span style="${styleDecl}">...</span>`, splitting around any `headingMarkerExclusions` so markers stay raw. */
function wrapPlainRange(state: EditorState, from: number, to: number, styleDecl: string): string {
  const exclusions = headingMarkerExclusions(state, from, to);
  if (exclusions.length === 0) {
    return `<span style="${styleDecl}">${state.sliceDoc(from, to)}</span>`;
  }
  let cursor = from;
  let insert = "";
  for (const ex of exclusions) {
    if (ex.from > cursor) {
      insert += `<span style="${styleDecl}">${state.sliceDoc(cursor, ex.from)}</span>`;
    }
    insert += state.sliceDoc(ex.from, ex.to); // raw marker text — never wrapped
    cursor = ex.to;
  }
  if (cursor < to) {
    insert += `<span style="${styleDecl}">${state.sliceDoc(cursor, to)}</span>`;
  }
  return insert;
}

function wrapSelectionInSpan(view: EditorView, prop: "color" | "font-size", value: string): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const styleDecl = `${prop}: ${value}`;

  const overlapping = findColorSpans(state.doc.toString())
    .filter((s) => s.openFrom < sel.to && s.closeTo > sel.from)
    .sort((a, b) => a.openFrom - b.openFrom);

  if (overlapping.length === 0) {
    const text = state.sliceDoc(sel.from, sel.to);
    if (text === "") {
      const openTag = `<span style="${styleDecl}">`;
      const insert = `${openTag}</span>`;
      view.dispatch({ changes: { from: sel.from, to: sel.to, insert }, selection: { anchor: sel.from + openTag.length } });
      view.focus();
      return true;
    }
    const insert = wrapPlainRange(state, sel.from, sel.to, styleDecl);
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert },
      selection: { anchor: sel.from, head: sel.from + insert.length },
    });
    view.focus();
    return true;
  }

  // At least one existing span overlaps — expand the edited range to fully cover every overlapping
  // span first, so a span's own tag characters are never split mid-tag, then rebuild that whole
  // range: text already inside an overlapping span merges `prop` into that span's existing style
  // (reusing its tags, never re-wrapping); everything else gets a fresh `<span style="...">`
  // (marker-excluded via `wrapPlainRange`, same as the no-overlap case above).
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
      insert += wrapPlainRange(state, cursor, s.openFrom, styleDecl);
    }
    const mergedStyle = mergeStyleDeclaration(s.style, prop, value);
    const inner = state.sliceDoc(s.contentFrom, s.contentTo);
    insert += `<span style="${mergedStyle}">${inner}</span>`;
    cursor = s.closeTo;
  }
  if (cursor < effTo) {
    insert += wrapPlainRange(state, cursor, effTo, styleDecl);
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

// ---------------------------------------------------------------------------
// Hard line break — inserts a CommonMark hard line break (two trailing spaces + `\n`) at the
// insertion point: the end of the current selection if one exists, or the cursor if collapsed
// (`docs/plans/markdown-editing/PLAN.md` §4.1, GitHub issue #176). Point-insertion only — existing
// text is never deleted, so there is no separate empty-selection case to special-case.
// ---------------------------------------------------------------------------

export function hardBreakCommand(view: EditorView): boolean {
  const { state } = view;
  const pos = state.selection.main.to;
  const insert = "  \n";
  view.dispatch({ changes: { from: pos, to: pos, insert }, selection: { anchor: pos + insert.length } });
  view.focus();
  return true;
}

// ---------------------------------------------------------------------------
// Clear formatting — strips Markdown decoration/structure syntax (and this app's own allowlisted
// `<span style="...">` tag) from the selection, reducing it to plain text, in one transaction
// (`docs/plans/markdown-editing/PLAN.md` §4.1, GitHub issue #176). Link syntax (`[text](url)`,
// incl. `memo:`/`#slug` links) and image syntax (`![alt](url)`, `<img>`) are content/references, not
// decoration, so they are protected from every step below and restored verbatim. Naive regex-based,
// same rigor level as this file's other toggle/strip commands (e.g. `boldCommand`'s own known
// boundary-misfire limitation above) — not a full CommonMark parser.
// ---------------------------------------------------------------------------

/** Matches one `[text](url)`/`![alt](url)` link-or-image, or one self-contained `<img ...>` tag —
 * the exact set of constructs Clear Formatting must never rewrite. Images (`!...`) and links share
 * the same bracket/paren shape, told apart only by the leading `!`, so one alternation covers both;
 * `<img>` (the app's other allowlisted image form, `markdownSanitize.ts`) is a separate shape
 * entirely. */
const PROTECTED_RE = /!?\[[^\]]*\]\([^)]*\)|<img\s+[^>]*\/?>/g;

/** Placeholder-substitutes every `PROTECTED_RE` match in `text` with a token that survives the
 * regex passes below untouched (no `*`/`_`/`~`/`` ` ``/`#`/`>`/`|` characters), then returns a
 * restore function to substitute the originals back in once every strip pass has run. */
function protectLinksAndImages(text: string): { protectedText: string; restore: (s: string) => string } {
  const originals: string[] = [];
  const protectedText = text.replace(PROTECTED_RE, (match) => {
    const token = `\u0000${originals.length}\u0000`;
    originals.push(match);
    return token;
  });
  const restore = (s: string) => s.replace(/\u0000(\d+)\u0000/g, (_, i: string) => originals[Number(i)] ?? "");
  return { protectedText, restore };
}

/** Unwraps every allowlisted `<span style="...">...</span>` in `text` to its inner content —
 * the combined equivalent of the Color/Font-size buttons' own "clear" controls, applied together.
 * No other HTML tag is touched (outside the sanitization allowlist already, `markdownSanitize.ts`).
 * Exported for `tocGenerator.ts`'s heading-text/slug extraction, so a color/font-size-tagged heading
 * doesn't leak raw `<span style="...">` markup into the TOC label or the anchor slug. */
export function unwrapColorSpans(text: string): string {
  return text.replace(/<span\s+style="[^"]*"\s*>([\s\S]*?)<\/span>/gi, "$1");
}

/** Unwraps every `<mark>...</mark>` / `<mark style="...">...</mark>` in `text` to its inner content (issue #190, mirrors `unwrapColorSpans`). */
export function unwrapMarkSpans(text: string): string {
  return text.replace(/<mark(?:\s+style="[^"]*")?\s*>([\s\S]*?)<\/mark>/gi, "$1");
}

const FENCE_RE = /^(\s*)(```|~~~)/;
const HR_RE = /^\s*(?:-\s*){3,}$|^\s*(?:\*\s*){3,}$|^\s*(?:_\s*){3,}$/;
const TABLE_SEPARATOR_RE = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;
const HEADING_PREFIX_RE = /^#{1,6}\s+/;
const QUOTE_PREFIX_RE = /^(>\s?)+/;
const LIST_PREFIX_RE = /^(\s*)(?:[-*+]|\d+\.)\s+/;
const INDENTED_CODE_RE = /^(?: {4}|\t)/;

/** Strips bold/italic/strikethrough/inline-code markers, keeping the text between them. Bold is
 * processed before italic so a `**bold**` pair isn't half-consumed by the single-marker italic
 * regex first. Only ever applied to a line that is NOT fenced-code-block content — see
 * `stripLineLevelSyntax`'s call site below. */
function stripInlineSyntax(text: string): string {
  let result = text;
  result = result.replace(/~~([^~]+?)~~/g, "$1");
  result = result.replace(/\*\*([^*]+?)\*\*/g, "$1");
  result = result.replace(/__([^_]+?)__/g, "$1");
  result = result.replace(/\*([^*]+?)\*/g, "$1");
  result = result.replace(/\b_([^_]+?)_\b/g, "$1");
  result = result.replace(/`([^`]+?)`/g, "$1");
  return result;
}

/** Strips fenced/indented code-block markers, heading/blockquote/list line-prefixes,
 * horizontal-rule lines, table pipe/separator syntax, and inline bold/italic/strikethrough/code
 * markers — one line at a time, tracking fenced code-block state across the selection so a
 * fence's own content lines are copied through completely verbatim: no line-level stripping AND
 * no inline stripping (CommonMark treats fence content as literal text, not Markdown syntax).
 * Inline stripping is folded into this same per-line loop — rather than run as a separate pass
 * over the whole joined result afterward — specifically so it can be skipped for fence-content
 * lines; a separate blind pass has no record of which output lines came from inside a fence. */
function stripLineLevelSyntax(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];
  let fenceMarker: string | null = null;
  for (const line of lines) {
    if (fenceMarker !== null) {
      const closeMatch = FENCE_RE.exec(line);
      if (closeMatch && closeMatch[2] === fenceMarker) {
        fenceMarker = null; // closing fence line — drop the marker, keep no line for it
        continue;
      }
      out.push(line); // fence content — verbatim, no inline stripping either
      continue;
    }
    const openMatch = FENCE_RE.exec(line);
    if (openMatch) {
      fenceMarker = openMatch[2];
      continue; // opening fence line — drop the marker
    }
    if (HR_RE.test(line)) continue; // horizontal rule — drop the whole line
    if (TABLE_SEPARATOR_RE.test(line) && line.includes("-")) continue; // table header separator row
    let rest = line;
    if (INDENTED_CODE_RE.test(rest)) {
      out.push(rest.replace(INDENTED_CODE_RE, ""));
      continue;
    }
    rest = rest.replace(HEADING_PREFIX_RE, "");
    rest = rest.replace(QUOTE_PREFIX_RE, "");
    rest = rest.replace(LIST_PREFIX_RE, "");
    if (rest.includes("|")) rest = rest.replace(/\s*\|\s*/g, " ").trim();
    out.push(stripInlineSyntax(rest));
  }
  return out.join("\n");
}

export function clearFormattingCommand(view: EditorView): boolean {
  const { state } = view;
  const sel = state.selection.main;
  if (sel.empty) return false; // no-op on a collapsed selection — no transaction created
  const original = state.sliceDoc(sel.from, sel.to);

  const { protectedText, restore } = protectLinksAndImages(original);
  let result = unwrapColorSpans(protectedText);
  result = stripLineLevelSyntax(result); // also strips inline syntax per-line, skipping fence content
  result = restore(result);

  view.dispatch({
    changes: { from: sel.from, to: sel.to, insert: result },
    selection: { anchor: sel.from, head: sel.from + result.length },
  });
  view.focus();
  return true;
}
