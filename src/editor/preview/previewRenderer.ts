// `renderMarkdownToHtml` — the preview pane's Markdown->HTML token walk
// (`docs/plans/markdown/markdown-preview/RENDERING.md` §2). Pure function of an `EditorState`'s
// document text; no DOM/React dependency, so `PreviewPane.tsx` calls it from a plain `setTimeout`
// callback (safe — see this file's own comments and this feature's implementation report on why
// that's a different risk class from the live-view's CodeMirror-`contentDOM`-touching widgets).
import type { EditorState } from "@codemirror/state";
import DOMPurify from "dompurify";
import { createPreviewMarkdownIt } from "./markdownIt";
import { findColorSpans, findImageTags } from "../markdownSpanParser";
import { scanHeadings } from "../tocGenerator";

// One shared instance — stateless across `parse`/`render` calls (see `markdownIt.ts`'s own doc
// comment), so re-creating it on every call would be pure waste.
const md = createPreviewMarkdownIt();

// Unicode Private-Use-Area delimiters for the span/image placeholder tokens (RENDERING.md §2 step
// 3) — chosen to survive markdown-it's inline tokenizer as inert plain text on every realistic
// input; a document containing these exact literal code points in its own prose would see
// corrupted rendering, an accepted, vanishingly unlikely residual risk (RENDERING.md's own words).
const PLACEHOLDER_START = "";
const PLACEHOLDER_END = "";
const PLACEHOLDER_RE = /(\d+)\n*/g;

interface CharRange {
  from: number;
  to: number;
}

/** Converts a markdown-it `.map` line range (`[startLine0, endLineExclusive0]`) to a document
 * character-offset range, using the SAME `EditorState.doc` the caller already has — simpler and
 * more robust than re-deriving line-start offsets by hand-walking the raw text string. */
function mapToCharRange(state: EditorState, map: [number, number]): CharRange {
  const fromLine = state.doc.line(Math.min(map[0] + 1, state.doc.lines));
  const toLineNo = Math.min(Math.max(map[1], map[0] + 1), state.doc.lines);
  const toLine = state.doc.line(toLineNo);
  return { from: fromLine.from, to: toLine.to };
}

function overlapsAny(ranges: CharRange[], from: number, to: number): boolean {
  return ranges.some((r) => from < r.to && to > r.from);
}

/** ASCII-minimal escaping for text/attribute values spliced directly into the HTML string this
 * module builds by hand (RENDERING.md §2 step 8) — reuses markdown-it's own battle-tested
 * `escapeHtml` utility rather than a bespoke implementation. */
function escapeHtml(value: string): string {
  return md.utils.escapeHtml(value);
}

/** The preview pane's full Markdown->HTML pipeline (RENDERING.md §2). Re-parses the entire
 * document text on every call — no incremental/cached parse state is kept between calls (`md`
 * above is reused only because it's a stateless configuration object, not a parse cache). */
export function renderMarkdownToHtml(state: EditorState): { html: string } {
  const text = state.doc.toString();

  // Step 2: exclude span/img matches that fall inside a fenced/indented code block — a first,
  // throwaway parse of the ORIGINAL (unsubstituted) text, read only for `fence`/`code_block`
  // tokens' `.map` line ranges (converted to character offsets via `state.doc`, not by manually
  // re-deriving line starts from the raw string).
  const probeTokens = md.parse(text, {});
  const codeSkipRanges: CharRange[] = [];
  for (const token of probeTokens) {
    if ((token.type === "fence" || token.type === "code_block") && token.map) {
      codeSkipRanges.push(mapToCharRange(state, token.map));
    }
  }

  // Step 2-3: collect accepted span/image matches (in document order), building each one's final
  // safe HTML fragment up front, then splice PUA placeholders into a fresh substituted-text string
  // in a single forward pass.
  //
  // Hardening beyond RENDERING.md's literal placeholder shape: `findColorSpans`/`findImageTags`
  // matches CAN span multiple lines (`SPAN_RE`'s inner-content group and `IMG_RE`'s attribute-list
  // group both use patterns that match embedded newlines). Collapsing a multi-line match into a
  // single-line placeholder token would shift every SUBSEQUENT line's number in the substituted
  // text relative to the original document — silently corrupting every later block's
  // `data-source-line` (scroll-sync drift) and, more seriously, `§8`'s checkbox click-to-toggle
  // line lookup (which maps a clicked checkbox's `data-source-line` back to
  // `view.state.doc.line(n)` and dispatches a document mutation at that line — a drifted line
  // number there could toggle or corrupt the WRONG line of the real document). Each placeholder
  // below therefore embeds the same number of literal `\n` characters the original match spanned,
  // keeping the substituted text's line count identical to the original's for every position after
  // the match. `PLACEHOLDER_RE` above accounts for the optional embedded newlines when matching
  // placeholders back out in step 8.
  interface Match {
    from: number;
    to: number;
    fragment: string;
  }
  const matches: Match[] = [];

  for (const span of findColorSpans(text)) {
    if (overlapsAny(codeSkipRanges, span.openFrom, span.closeTo)) continue;
    const innerText = escapeHtml(text.slice(span.contentFrom, span.contentTo));
    const lineNo0 = state.doc.lineAt(span.openFrom).number - 1;
    const styleAttr = span.style ? ` style="${escapeHtml(span.style)}"` : "";
    matches.push({
      from: span.openFrom,
      to: span.closeTo,
      fragment: `<span data-source-line="${lineNo0}"${styleAttr}>${innerText}</span>`,
    });
  }
  for (const img of findImageTags(text)) {
    if (overlapsAny(codeSkipRanges, img.from, img.to)) continue;
    const widthAttr = img.width != null ? ` width="${img.width}"` : "";
    const heightAttr = img.height != null ? ` height="${img.height}"` : "";
    // `alt` is always empty here: the allowlisted raw `<img>` HTML tag form
    // (`markdownSpanParser.ts`'s `ParsedImageTag`) does not currently surface a parsed `alt`
    // attribute even if the user typed one — same pre-existing treatment the live-view's own
    // `buildImageWidget(img.src, "", config)` call already gives this form (`markdownDecorations.ts`).
    matches.push({
      from: img.from,
      to: img.to,
      fragment: `<img data-relative-src="${escapeHtml(img.src)}" alt=""${widthAttr}${heightAttr}>`,
    });
  }
  matches.sort((a, b) => a.from - b.from);

  let substituted = "";
  let cursor = 0;
  const fragments: string[] = [];
  matches.forEach((m, i) => {
    substituted += text.slice(cursor, m.from);
    const original = text.slice(m.from, m.to);
    const newlineCount = (original.match(/\n/g) ?? []).length;
    substituted += PLACEHOLDER_START + String(i) + "\n".repeat(newlineCount) + PLACEHOLDER_END;
    fragments.push(m.fragment);
    cursor = m.to;
  });
  substituted += text.slice(cursor);

  // Step 4
  const tokens = md.parse(substituted, {});

  // Step 5: `data-source-line` on every token with a non-null `.map` (paragraph_open,
  // heading_open, list_item_open, blockquote_open, table_open/tr_open, fence/code_block, hr, etc.
  // — every one of these lives in the flat top-level `tokens` array already, not nested under an
  // "inline" token's own `.children`, since only block-level tokens carry `.map` at all).
  for (const token of tokens) {
    if (token.map) token.attrSet("data-source-line", String(token.map[0]));
  }

  // Step 6: heading-anchor ids, reusing `tocGenerator.ts`'s already-deduplicated `scanHeadings`
  // array verbatim (never a second independent slugify call) so the preview can never disagree
  // with the TOC/`#slug` resolution logic.
  const headings = scanHeadings(state);
  let headingIndex = 0;
  for (const token of tokens) {
    if (token.type === "heading_open") {
      if (headingIndex < headings.length) token.attrSet("id", headings[headingIndex].slug);
      headingIndex++;
    }
  }

  // Step 7
  let html = md.renderer.render(tokens, md.options, {});

  // Step 8: substitute placeholders back with their pre-built safe HTML fragments. Safe as a
  // string splice specifically because every substituted value was already individually validated
  // (`isSafeRelativeImageSrc`/`filterStyleDeclarations`, both from `markdownSanitize.ts`, called
  // inside `findColorSpans`/`findImageTags` themselves) or HTML-escaped above — never unescaped
  // user-text concatenation.
  html = html.replace(PLACEHOLDER_RE, (_match, indexStr: string) => fragments[Number(indexStr)] ?? "");

  // Step 9: DOMPurify backstop — explicit allowlist covering exactly what steps 5-8 can produce.
  // `html:false` upstream means this pass should rarely have anything dangerous to catch; its role
  // is defense-in-depth, not the primary control.
  //
  // Deviation from RENDERING.md §2 step 9's literal example list, corrected against this
  // markdown-it version's actual empirically-observed output (verified at implementation time,
  // `markdownIt.ts`'s own header comment): the `"default"` preset renders strikethrough as
  // `<s>...</s>`, NOT `<del>...</del>` — `'s'` is included below (alongside `'del'`, kept as a
  // harmless superset in case a future markdown-it version or plugin ever emits it) so
  // strikethrough text isn't silently unwrapped by the sanitizer.
  const clean = DOMPurify.sanitize(html, {
    ALLOWED_TAGS: [
      "p", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li", "blockquote", "pre", "code", "hr",
      "table", "thead", "tbody", "tr", "th", "td", "a", "strong", "em", "del", "s", "span", "img",
      "input", "div", "br",
    ],
    ALLOWED_ATTR: [
      "data-source-line", "id", "href", "style", "src", "alt", "width", "height",
      "data-relative-src", "type", "checked", "disabled", "class",
    ],
    // DOMPurify's own default `ALLOWED_URI_REGEXP` (installed dompurify@3.4.13, `purify.cjs.js`'s
    // `IS_ALLOWED_URI` constant) only allows the `f/httpg(s)/mailto/tel/callto/sms/cid/xmpp/matrix`
    // schemes plus scheme-less/relative values — it does NOT know about this app's own `memo:` link
    // scheme (`RENDERING.md` §9 / `previewInteractions.ts`'s `href.startsWith("memo:")` branch), so
    // without this override every `href="memo:..."` produced by `markdownIt.ts`'s link rule and its
    // bare-token autolink rule gets silently stripped here, leaving a dead `<a>` with no `href`.
    // This is the EXACT same regex as DOMPurify's default with only `|memo` added to the scheme
    // alternation — every other safe scheme and the relative/scheme-less fallback branches are
    // unchanged, so genuinely dangerous schemes (`javascript:`, `data:` outside the separate
    // `DATA_URI_TAGS` allowance, `vbscript:`, etc.) remain rejected exactly as before.
    ALLOWED_URI_REGEXP:
      /^(?:(?:(?:f|ht)tps?|mailto|tel|callto|sms|cid|xmpp|matrix|memo):|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i,
  });

  return { html: clean };
}
