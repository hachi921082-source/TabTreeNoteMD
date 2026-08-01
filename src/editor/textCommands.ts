// Plain-text CodeMirror 6 layer (`docs/plans/editor-save/PLAN.md` §4): extension composition,
// undo/redo/cut/copy/paste/select-all/line-wise ops, the right-click "Convert"/"Check" text
// transforms, status-bar derived state, scroll-position helpers, and "Search with Google".
// Everything here feeds the same undo history as manual typing (single CodeMirror transactions
// per action). The Markdown-specific decoration/live-view layer is `markdown-editing/PLAN.md`'s
// separate, not-yet-implemented concern, built on top of the extensions this module produces.
import { Compartment, EditorState } from "@codemirror/state";
import type { Extension } from "@codemirror/state";
import { EditorView, drawSelection, highlightActiveLine, highlightActiveLineGutter, highlightWhitespace, keymap, lineNumbers } from "@codemirror/view";
import type { Command } from "@codemirror/view";
import { copyLineDown, defaultKeymap, deleteLine, history, historyKeymap, insertNewlineKeepIndent, moveLineDown, moveLineUp, redo, selectAll, undo } from "@codemirror/commands";
import { search } from "@codemirror/search";
import jaconv from "jaconv";
import type { EditorDefaults, MemoEditorSettings } from "../shared/models";
import { linkOpenExternal } from "../ipc/link";

// ---------------------------------------------------------------------------
// Settings resolution (`[spec §3.4]`: per-memo override falls back to `AppSettings.editorDefaults`)
// ---------------------------------------------------------------------------

export interface ResolvedEditorSettings {
  wrap: "none" | "chars" | "window";
  tabWidth: number;
  softTab: boolean;
  showLineNumbers: boolean;
  fontSize: number;
}

export function resolveEditorSettings(overrides: MemoEditorSettings | undefined, defaults: EditorDefaults): ResolvedEditorSettings {
  return {
    wrap: overrides?.wrap ?? defaults.wrap,
    tabWidth: overrides?.tabWidth ?? defaults.tabWidth,
    softTab: overrides?.softTab ?? defaults.softTab,
    showLineNumbers: overrides?.showLineNumbers ?? defaults.showLineNumbers,
    fontSize: overrides?.fontSize ?? defaults.fontSize,
  };
}

/** Fixed wrap column for `wrap: "chars"` mode. Word wrap has no accompanying numeric column
 * field anywhere in `AppSettings`/`MemoEditorSettings`'s confirmed schema
 * (`[detailed-spec §3.2, §3.3]`'s example objects have none) — this is an
 * implementation-discretion constant (`[detailed-spec §9]`), not a spec ambiguity, chosen to
 * match the conventional 80-column text width. */
export const CHARS_WRAP_COLUMN = 80;

// ---------------------------------------------------------------------------
// Extension composition — reconfigurable via `Compartment.reconfigure` so settings / read-only /
// whitespace-visibility changes don't require tearing down (and losing the undo history of) the
// whole `EditorView` mid-session.
// ---------------------------------------------------------------------------

export const settingsCompartment = new Compartment();
export const whitespaceCompartment = new Compartment();
export const readOnlyCompartment = new Compartment();

export interface BuildExtensionsOptions {
  settings: ResolvedEditorSettings;
  /** `true` while the memo's per-memo Edit Lock is set, or while a conflict-resolution dialog is
   * open (`[spec §5.8]` "auto-save/input lock during resolution dialog"). */
  readOnly: boolean;
  /** Session-only, not persisted anywhere (no `MemoEditorSettings`/`EditorDefaults` field exists
   * for this — see `buildSettingsExtension`'s doc comment on the same discretion). */
  showWhitespace: boolean;
  onDocChanged: (text: string) => void;
}

/** Also used standalone by `EditorPane` to `Compartment.reconfigure` the read-only state on an
 * already-mounted view (e.g. when a conflict dialog opens/closes) without tearing down the
 * `EditorView`/losing undo history. */
export function readOnlyExtension(readOnly: boolean): Extension {
  return EditorView.editable.of(!readOnly);
}

/** Also used standalone by `EditorPane` to `Compartment.reconfigure` whitespace visibility on an
 * already-mounted view. */
export function whitespaceExtension(show: boolean): Extension {
  return show ? highlightWhitespace() : [];
}

function makeTabCommand(tabWidth: number, softTab: boolean): Command {
  return (view) => {
    // A real (non-empty) selection falls through to `defaultKeymap`'s ordinary Tab handling
    // (indent-selection) rather than this plain insert-at-cursor behavior.
    if (!view.state.selection.ranges.every((r) => r.empty)) return false;
    const insert = softTab ? " ".repeat(tabWidth) : "\t";
    view.dispatch(view.state.replaceSelection(insert), { scrollIntoView: true });
    return true;
  };
}

/** Word wrap, tab width, soft tabs, line numbers, and font size are all
 * `MemoEditorSettings`/`EditorDefaults`-backed (`[spec §3.4]`). Auto-indent-on-Enter and
 * cursor-line highlight are always-on, fixed editor behaviors instead — despite
 * `editor-save/PLAN.md` §4 grouping them alongside the settings-backed list above, neither has a
 * corresponding field in the confirmed `AppSettings`/`MemoEditorSettings` schema
 * (`[detailed-spec §3.2, §3.3]`), so they are not user-toggleable today; implementation
 * discretion (`[detailed-spec §9]`), not a spec ambiguity requiring escalation. */
/** Exported so `EditorPane` can also `Compartment.reconfigure(settingsCompartment, ...)` an
 * already-mounted view when the resolved settings change (e.g. a per-memo override write),
 * without tearing down the view / losing undo history. */
export function buildSettingsExtension(settings: ResolvedEditorSettings): Extension[] {
  const exts: Extension[] = [EditorState.tabSize.of(settings.tabWidth)];
  if (settings.showLineNumbers) exts.push(lineNumbers());
  if (settings.wrap === "window") {
    exts.push(EditorView.lineWrapping);
  } else if (settings.wrap === "chars") {
    exts.push(EditorView.lineWrapping);
    exts.push(EditorView.theme({ ".cm-content": { maxWidth: `${CHARS_WRAP_COLUMN}ch` } }));
  }
  exts.push(EditorView.theme({ "&": { fontSize: `${settings.fontSize}px` } }));
  exts.push(keymap.of([{ key: "Tab", run: makeTabCommand(settings.tabWidth, settings.softTab) }]));
  return exts;
}

export function buildPlainTextExtensions(opts: BuildExtensionsOptions): Extension[] {
  return [
    history(),
    highlightActiveLine(),
    highlightActiveLineGutter(),
    drawSelection(),
    // Registers `@codemirror/search`'s state field only — its default DOM panel is never shown
    // since nothing here calls `openSearchPanel`; `components/Search/SearchPanel.tsx` and
    // `state/useSearch.ts` drive `setSearchQuery`/`findNext`/etc. directly instead
    // (`docs/plans/editor-save/PLAN.md` §5).
    search({ top: true }),
    settingsCompartment.of(buildSettingsExtension(opts.settings)),
    whitespaceCompartment.of(whitespaceExtension(opts.showWhitespace)),
    readOnlyCompartment.of(readOnlyExtension(opts.readOnly)),
    keymap.of([{ key: "Enter", run: insertNewlineKeepIndent }, ...defaultKeymap, ...historyKeymap]),
    EditorView.updateListener.of((update) => {
      if (update.docChanged) opts.onDocChanged(update.state.doc.toString());
    }),
  ];
}

// ---------------------------------------------------------------------------
// Undo/redo, select-all, line-wise operations (`[spec §2.3]`) — thin named re-exports of
// `@codemirror/commands` so callers (context menu, future toolbar) import everything from this
// one module rather than reaching into `@codemirror/commands` directly.
// ---------------------------------------------------------------------------

export const undoCommand: Command = undo;
export const redoCommand: Command = redo;
export const selectAllCommand: Command = selectAll;
export const moveLineUpCommand: Command = moveLineUp;
export const moveLineDownCommand: Command = moveLineDown;
export const duplicateLineCommand: Command = copyLineDown;
export const deleteLineCommand: Command = deleteLine;

function currentSelectionOrLineText(view: EditorView): { from: number; to: number; text: string } {
  const { state } = view;
  const range = state.selection.main;
  if (!range.empty) return { from: range.from, to: range.to, text: state.sliceDoc(range.from, range.to) };
  const line = state.doc.lineAt(range.head);
  return { from: line.from, to: line.to, text: line.text };
}

// ---------------------------------------------------------------------------
// Cut / copy / paste — right-click menu entries; Ctrl+X/C/V already work natively via the
// browser's own clipboard event handling without any of these (kept for the custom context
// menu, which replaces the suppressed native one, `[spec §2.3]`).
// ---------------------------------------------------------------------------

export function copyCommand(view: EditorView): boolean {
  const { text } = currentSelectionOrLineText(view);
  void navigator.clipboard.writeText(text).catch((err) => console.warn("copy failed", err));
  return true;
}

export function cutCommand(view: EditorView): boolean {
  const { from, to, text } = currentSelectionOrLineText(view);
  void navigator.clipboard
    .writeText(text)
    .then(() => view.dispatch({ changes: { from, to, insert: "" } }))
    .catch((err) => console.warn("cut failed", err));
  return true;
}

export function pasteCommand(view: EditorView): boolean {
  void navigator.clipboard
    .readText()
    .then((text) => view.dispatch(view.state.replaceSelection(text)))
    .catch((err) => console.warn("paste failed", err));
  return true;
}

// ---------------------------------------------------------------------------
// Right-click "Convert" — uppercase/lowercase/hiragana/katakana/half-width/full-width, tab<->space
// (`docs/plans/editor-save/PLAN.md` §4; `[detailed-spec §5.3]`). Applies to the current
// selection, or the current line if there is no selection. Hiragana/katakana and
// half-width/full-width use `jaconv` (a well-tested Japanese text conversion library) rather than
// a hand-rolled Unicode-range map, per the plan's own guidance.
// ---------------------------------------------------------------------------

function applyTextTransform(view: EditorView, transform: (s: string) => string): boolean {
  const { state } = view;
  const hasSelection = state.selection.ranges.some((r) => !r.empty);
  const changes = state.selection.ranges.map((range) => {
    let from = range.from;
    let to = range.to;
    if (!hasSelection) {
      const line = state.doc.lineAt(range.head);
      from = line.from;
      to = line.to;
    } else if (range.empty) {
      // Mixed selection set (some ranges empty, some not): leave an individually-empty range
      // untouched rather than guessing which line it "belongs" to.
      return { from: range.from, to: range.to, insert: state.sliceDoc(range.from, range.to) };
    }
    return { from, to, insert: transform(state.sliceDoc(from, to)) };
  });
  view.dispatch({ changes, scrollIntoView: true });
  view.focus();
  return true;
}

export function convertUppercase(view: EditorView): boolean {
  return applyTextTransform(view, (s) => s.toUpperCase());
}

export function convertLowercase(view: EditorView): boolean {
  return applyTextTransform(view, (s) => s.toLowerCase());
}

export function convertToHiragana(view: EditorView): boolean {
  return applyTextTransform(view, (s) => jaconv.toHiragana(s));
}

export function convertToKatakana(view: EditorView): boolean {
  return applyTextTransform(view, (s) => jaconv.toKatakana(s));
}

export function convertToHalfWidth(view: EditorView): boolean {
  return applyTextTransform(view, (s) => jaconv.toHanAscii(s));
}

export function convertToFullWidth(view: EditorView): boolean {
  return applyTextTransform(view, (s) => jaconv.toZenAscii(s));
}

/** Replaces every run of exactly `tabWidth` consecutive spaces with a single tab character.
 * Exact run-length matching policy is implementation discretion (`[detailed-spec §9]`) — the
 * spec names the "tab<->space" convert operation without specifying matching semantics for
 * partial-width runs. */
export function convertSpacesToTabs(tabWidth: number): (view: EditorView) => boolean {
  const pattern = new RegExp(" ".repeat(Math.max(1, tabWidth)), "g");
  return (view) => applyTextTransform(view, (s) => s.replace(pattern, "\t"));
}

export function convertTabsToSpaces(tabWidth: number): (view: EditorView) => boolean {
  const spaces = " ".repeat(Math.max(1, tabWidth));
  return (view) => applyTextTransform(view, (s) => s.replaceAll("\t", spaces));
}

// ---------------------------------------------------------------------------
// Checkbox cycle (right-click "Check" menu action) — `☐` -> `☑` -> `☐`, inserted/cycled at the
// cursor (`[spec §2.3]`, `docs/plans/editor-save/PLAN.md` §4). Distinct from the direct-click
// toggle path on an already-rendered glyph, which is `markdown-editing/PLAN.md`'s
// `checkboxClick.ts` (decoration-widget-specific, not implemented here).
// ---------------------------------------------------------------------------

const CHECKBOX_UNCHECKED = "☐";
const CHECKBOX_CHECKED = "☑";

/** Cycles the first `☐`/`☑` glyph found on the cursor's current line; if neither is present on
 * that line, inserts a fresh unchecked box at the cursor. Exact "which glyph on the line" and
 * "insert vs. cycle" decision policy is implementation discretion — the plan specifies the cycle
 * sequence and "at cursor" but not the precise glyph-lookup mechanics. */
export function cycleCheckbox(view: EditorView): boolean {
  const { state } = view;
  const pos = state.selection.main.head;
  const line = state.doc.lineAt(pos);
  const uncheckedIndex = line.text.indexOf(CHECKBOX_UNCHECKED);
  const checkedIndex = line.text.indexOf(CHECKBOX_CHECKED);
  if (uncheckedIndex !== -1) {
    const from = line.from + uncheckedIndex;
    view.dispatch({ changes: { from, to: from + 1, insert: CHECKBOX_CHECKED } });
    return true;
  }
  if (checkedIndex !== -1) {
    const from = line.from + checkedIndex;
    view.dispatch({ changes: { from, to: from + 1, insert: CHECKBOX_UNCHECKED } });
    return true;
  }
  view.dispatch(state.replaceSelection(`${CHECKBOX_UNCHECKED} `));
  return true;
}

// ---------------------------------------------------------------------------
// "Search with Google" (`[spec §2.3]`) — opens the default browser via the OS-standard
// external-open primitive (`src/ipc/link.ts`; see that module's Rust counterpart,
// `commands/link.rs`, for the exact scope of what's implemented so far).
// ---------------------------------------------------------------------------

export function searchSelectionWithGoogle(view: EditorView): void {
  const { text } = currentSelectionOrLineText(view);
  const trimmed = text.trim();
  if (!trimmed) return;
  const url = `https://www.google.com/search?q=${encodeURIComponent(trimmed)}`;
  void linkOpenExternal(url);
}

// ---------------------------------------------------------------------------
// Status bar — pure derived state from `EditorState`, no backend call (`[spec §2.3]`).
// ---------------------------------------------------------------------------

export interface EditorStatus {
  charCount: number;
  lineCount: number;
  /** 1-indexed. */
  line: number;
  /** 1-indexed. */
  column: number;
}

export function getEditorStatus(state: EditorState): EditorStatus {
  const pos = state.selection.main.head;
  const line = state.doc.lineAt(pos);
  return {
    charCount: state.doc.length,
    lineCount: state.doc.lines,
    line: line.number,
    column: pos - line.from + 1,
  };
}

// ---------------------------------------------------------------------------
// Scroll position (`docs/plans/editor-save/PLAN.md` §2 "Scroll Position Write Path") —
// document-position-based, not pixel-based (`DECISIONS.md` "Scroll Position: Document-Position-
// Based Anchor, Not Raw Pixel Offset").
// ---------------------------------------------------------------------------

/** 1-indexed document line number currently at the top of the viewport. */
export function computeTopVisibleLine(view: EditorView): number {
  const block = view.lineBlockAtHeight(view.scrollDOM.scrollTop);
  return view.state.doc.lineAt(block.from).number;
}

/** Scrolls so `line` (1-indexed, clamped to the current document's line count) sits at the top
 * of the viewport. Callers defer this to after the view's first layout/measure pass (e.g.
 * `requestAnimationFrame`), since accurate line-height measurement isn't available at the
 * instant of mount. */
export function scrollToTopVisibleLine(view: EditorView, line: number): void {
  const clamped = Math.min(Math.max(1, line), view.state.doc.lines);
  const pos = view.state.doc.line(clamped).from;
  view.dispatch({ effects: EditorView.scrollIntoView(pos, { y: "start" }) });
}
