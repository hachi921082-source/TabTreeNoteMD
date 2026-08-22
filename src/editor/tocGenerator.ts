// Table of Contents generation (`docs/plans/table-of-contents/PLAN.md`), triggered only by the
// Markdown toolbar's "Update" slot (`docs/plans/markdown-editing/PLAN.md` §4, row 1 slot 8,
// `RefreshCw`) — never automatically/in the background. Also supplies `scanHeadings`/`slugifyHeading`
// to `links.ts`'s `#slug` heading-anchor click resolution (`table-of-contents/PLAN.md` §4), so
// generation and resolution always agree on the exact same slug for the exact same heading.
import type { EditorState } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { syntaxTree } from "@codemirror/language";
import { unwrapColorSpans, unwrapMarkSpans } from "./decorationCommands";

export interface HeadingInfo {
  /** 1-6, from the ATX `#`..`######` marker count. */
  level: number;
  /** Heading text with `#` marker(s) and any `<span style="...">`/`<mark>` wrapping tag stripped (issue #190). */
  text: string;
  /** GitHub-style slug, already de-duplicated against every earlier heading in the same scan
   * (`-1`, `-2`, ... suffix on the second/third/... occurrence of the same base slug). */
  slug: string;
  /** Document offset of the heading line's start — the scroll/jump target for `#slug` resolution. */
  from: number;
}

/** GitHub-style slugification (`docs/plans/table-of-contents/PLAN.md` §2): lowercase, strip
 * punctuation that isn't part of a word (Unicode-aware via `\p{L}`/`\p{N}` so Japanese/other
 * non-Latin heading text slugifies sensibly rather than being stripped to nothing), collapse
 * whitespace runs to a single hyphen. Exact byte-for-byte parity with GitHub's own algorithm isn't
 * required — this scheme only needs to be internally consistent between TOC generation and `#slug`
 * click resolution, which it is since both call this same function. */
export function slugifyHeading(text: string): string {
  const stripped = text
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, "")
    .trim()
    .replace(/\s+/g, "-");
  // A heading with no word/number characters at all (e.g. "### ---") would otherwise slugify to
  // an empty string, which isn't a usable `#` anchor target — fall back to a fixed placeholder
  // rather than emitting an empty/ambiguous link (implementation discretion, not spec-mandated;
  // an edge case the spec doesn't address).
  return stripped || "section";
}

/** Scans `state`'s current document for ATX headings (`#` through `######`) using the same
 * `@lezer/markdown` CommonMark+GFM syntax tree `markdownDecorations.ts` reads for rendering
 * (`codeMirrorSetup.ts`'s always-on language extension), NOT an independent line-by-line `#`-regex
 * (`DECISIONS.md` "Code-Fence Awareness in Markdown Scanning") — a `#` inside a fenced code block is
 * tokenized as plain code text, never an `ATXHeadingN` node, so it's excluded automatically here with
 * no special-casing needed. Returns headings in document order with de-duplicated slugs (first
 * occurrence of a given base slug keeps the bare slug; later occurrences get `-1`, `-2`, ...,
 * `docs/plans/table-of-contents/PLAN.md` §2). Used both by `tocUpdateCommand` below (generation) and
 * `links.ts`'s `resolveAndScrollToHeadingAnchor` (resolution), so the two never disagree. */
export function scanHeadings(state: EditorState): HeadingInfo[] {
  const raw: { level: number; text: string; from: number }[] = [];
  syntaxTree(state).iterate({
    enter: (node) => {
      const match = /^ATXHeading([1-6])$/.exec(node.name);
      if (!match) return;
      const level = Number(match[1]);
      const lineFrom = state.doc.lineAt(node.from).from;
      const rawText = state.sliceDoc(node.from, node.to);
      const text = unwrapMarkSpans(
        unwrapColorSpans(
          rawText
            .replace(/^#{1,6}\s*/, "")
            .replace(/\s+#+\s*$/, "")
            .trim(),
        ),
      ).trim();
      raw.push({ level, text, from: lineFrom });
    },
  });
  const slugCounts = new Map<string, number>();
  return raw.map((h) => {
    const base = slugifyHeading(h.text);
    const count = slugCounts.get(base) ?? 0;
    slugCounts.set(base, count + 1);
    const slug = count === 0 ? base : `${base}-${count}`;
    return { ...h, slug };
  });
}

// ---------------------------------------------------------------------------
// TOC block generation (`docs/plans/table-of-contents/PLAN.md` §2) — nested Markdown list of
// `[heading text](#slug)`, nesting depth relative to the document's own shallowest heading level
// (GitHub-style), not absolute heading level — a document whose shallowest heading is `##` gets that
// `##` heading flat at the TOC's top level, with deeper headings indenting under it. A document that
// already starts at `#`/H1 is unaffected (`minLevel` is 1, so the formula reduces to the old absolute
// behavior). See `DECISIONS.md` "Table of Contents: Relative Heading-Nesting Indentation".
// ---------------------------------------------------------------------------

export function generateTocList(headings: HeadingInfo[]): string {
  const minLevel = headings.length ? Math.min(...headings.map((h) => h.level)) : 1;
  return headings.map((h) => `${"  ".repeat(h.level - minLevel)}- [${h.text}](#${h.slug})`).join("\n");
}

/** Exact marker text is this plan's own call (`docs/plans/table-of-contents/PLAN.md` §2) — any HTML
 * comment works since it renders inert under this app's CommonMark-equivalent support with no new
 * sanitization-allowlist entry needed. */
export const TOC_MARKER_START = "<!-- TOC start -->";
export const TOC_MARKER_END = "<!-- TOC end -->";

/** Finds an existing marker-delimited TOC block in `docText`. Returns `null` if no matching pair is
 * present (a start marker with no following end marker is treated as "not found," same as no start
 * marker at all — inserting a second block rather than guessing at a malformed one is the safer
 * default). */
function findExistingBlock(docText: string): { from: number; to: number } | null {
  const startIdx = docText.indexOf(TOC_MARKER_START);
  if (startIdx === -1) return null;
  const endIdx = docText.indexOf(TOC_MARKER_END, startIdx + TOC_MARKER_START.length);
  if (endIdx === -1) return null;
  return { from: startIdx, to: endIdx + TOC_MARKER_END.length };
}

/** `true` if inserting at `pos` needs a leading/trailing newline of its own to stay on its own
 * line(s) — i.e. `pos` doesn't already sit right after/before a line break or a document edge.
 * Without this guard, inserting the block at, say, the very start of the document glues its closing
 * `<!-- TOC end -->` marker onto the same line as whatever text used to start the document (e.g.
 * `<!-- TOC end --># Title`), which silently breaks that following line's own Markdown syntax (a
 * heading right after the marker would no longer be parsed as a heading at all). Caught during this
 * feature's own self-verification, not spec-mandated wording — plain insertion hygiene. */
function needsNewlineBefore(state: EditorState, pos: number): boolean {
  return pos !== 0 && state.sliceDoc(pos - 1, pos) !== "\n";
}
function needsNewlineAfter(state: EditorState, pos: number): boolean {
  return pos !== state.doc.length && state.sliceDoc(pos, pos + 1) !== "\n";
}

/** Toolbar "Update" action (`docs/plans/markdown-editing/PLAN.md` §4 row 1 slot 8): scans the
 * current buffer's headings, generates the nested link list, and inserts a new marker-delimited
 * block at the cursor or replaces an existing one in place — one CodeMirror transaction (one undo
 * step), consistent with every other toolbar action (`[detailed-spec §5.10]`). Manual-only: never
 * called except from this direct user click (or its keyboard-shortcut equivalent). */
export function tocUpdateCommand(view: EditorView): boolean {
  const { state } = view;
  const headings = scanHeadings(state);
  const block = `${TOC_MARKER_START}\n${generateTocList(headings)}\n${TOC_MARKER_END}`;
  const docText = state.doc.toString();
  const existing = findExistingBlock(docText);
  if (existing) {
    view.dispatch({ changes: { from: existing.from, to: existing.to, insert: block }, selection: { anchor: existing.from + block.length } });
  } else {
    const pos = state.selection.main.from;
    const prefix = needsNewlineBefore(state, pos) ? "\n" : "";
    const suffix = needsNewlineAfter(state, pos) ? "\n" : "";
    view.dispatch({ changes: { from: pos, insert: `${prefix}${block}${suffix}` }, selection: { anchor: pos + prefix.length + block.length } });
  }
  view.focus();
  return true;
}
