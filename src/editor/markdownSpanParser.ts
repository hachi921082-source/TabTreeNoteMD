// Small, targeted parser for exactly the allowlisted inline HTML fragments the Markdown
// decoration layer supports (`docs/plans/markdown-editing/PLAN.md` §2/§6):
//   <span style="color:...;font-size:...">...</span>
//   <mark>...</mark> / <mark style="color:...;font-size:...">...</mark>  (GitHub issue #112)
//   <img src="relative/path.png" width=".." height="..">
// This is intentionally NOT a general HTML parser and must not be extended to accept anything
// outside this allowlist without a spec change (`[spec §4.2]`). The Lezer Markdown parser
// (`@lezer/markdown`, via `@codemirror/lang-markdown`) tokenizes inline HTML as opaque `HTMLTag`
// nodes without attribute-level structure, so `markdownDecorations.ts` re-scans the raw document
// text directly with the regexes below rather than trying to pull attributes out of the syntax
// tree.
import { filterStyleDeclarations, isSafeRelativeImageSrc } from "./markdownSanitize";

export interface ParsedColorSpan {
  /** Offset of the `<span` opening tag's `<`. */
  openFrom: number;
  /** Offset just past the opening tag's `>`. */
  openTo: number;
  /** Offset of the inner text's start (`=== openTo`, kept as a distinct name for readability at
   * call sites). */
  contentFrom: number;
  contentTo: number;
  /** Offset of the `</span>` closing tag's `<`. */
  closeFrom: number;
  /** Offset just past the closing tag's `>`. */
  closeTo: number;
  /** Already filtered to only `color`/`font-size` declarations (`markdownSanitize.ts`). Empty
   * string if the tag had a `style` attribute but nothing allowlisted survived. */
  style: string;
}

export interface ParsedMarkSpan {
  /** Offset of the `<mark` opening tag's `<`. */
  openFrom: number;
  /** Offset just past the opening tag's `>`. */
  openTo: number;
  contentFrom: number;
  contentTo: number;
  /** Offset of the `</mark>` closing tag's `<`. */
  closeFrom: number;
  /** Offset just past the closing tag's `>`. */
  closeTo: number;
  /** Already filtered to only `color`/`font-size` declarations (`markdownSanitize.ts`), same
   * allowlist `<span style="...">` reuses. Empty string for a plain `<mark>` (no `style`
   * attribute) or one whose `style` had nothing allowlisted survive. */
  style: string;
}

export interface ParsedMathSpan {
  /** Offset of the opening `$`/`$$` delimiter's first `$`. */
  from: number;
  /** Offset just past the closing `$`/`$$` delimiter's last `$`. */
  to: number;
  /** `true` for `$$...$$` (KaTeX display mode), `false` for `$...$` (inline mode) — mirrors
   * `markdownIt.ts`'s own `math_inline_double`/`math_inline` token distinction. Current callers
   * (`markdownDecorations.ts`'s live-view badge and source-view marker, `docs/plans/v1.2.0/
   * math-rendering/PLAN.md` §3/§4) don't visually distinguish the two — kept for completeness /
   * future use, mirroring `ParsedImageTag`'s own optional `width`/`height` fields above. */
  display: boolean;
}

export interface ParsedImageTag {
  /** Offset of the `<img` tag's `<`. */
  from: number;
  /** Offset just past the tag's `>`. */
  to: number;
  src: string;
  width?: number;
  height?: number;
}

/** Matches one `<span style="...">...</span>` pair, non-greedy on the inner content so adjacent
 * spans on the same line don't get merged into one match. Does not support nested `<span>`s (not
 * part of the allowlisted shape). */
const SPAN_RE = /<span\s+style="([^"]*)"\s*>([\s\S]*?)<\/span>/gi;

/** Matches one `<mark>...</mark>` or `<mark style="...">...</mark>` pair — `style` is optional,
 * unlike `SPAN_RE`, since a plain unstyled `<mark>` (relying on its default highlight styling) is
 * itself a valid allowlisted use (GitHub issue #112). */
const MARK_RE = /<mark(?:\s+style="([^"]*)")?\s*>([\s\S]*?)<\/mark>/gi;

/** Matches a single, self-contained `<img ...>` tag (no closing tag expected). Attribute order is
 * not assumed. */
const IMG_RE = /<img\s+([^>]*?)\/?>/gi;
const ATTR_RE = /(\w+)\s*=\s*"([^"]*)"/g;

/** Matches `$$...$$` (KaTeX display mode) — content must be non-empty and must not itself contain a
 * bare `$` (mirrors `markdown-it-texmath`'s own `"dollars"` preset regex, `markdownIt.ts`'s §2.1
 * plugin choice, verbatim: `/\${2}([^$]*?[^\\])\${2}/gy`). Checked before `MATH_INLINE_RE` below so
 * a `$$...$$` span is never misread as two adjacent `$...$` spans. */
const MATH_DISPLAY_RE = /\$\$([^$]*?[^\\])\$\$/g;

/** Matches `$...$` (KaTeX inline mode) — same character classes as `markdown-it-texmath`'s
 * `"dollars"` preset inline rule (`/\$((?:[^\s\\])|(?:\S.*?[^\s\\]))\$/gy`, verbatim). Combined with
 * `mathPreOk`/`mathPostOk` below (mirroring that same rule's `pre`/`post` guards, `texmath.$_pre`/
 * `texmath.$_post`), this reproduces the exact same Pandoc/Typora-style "no whitespace immediately
 * inside either delimiter, no digit immediately outside either delimiter" currency-false-positive
 * heuristic the preview pane's `markdownIt.ts` gets for free from the plugin itself — required so
 * live-view/source-view scanning and the preview renderer always agree on what counts as a math
 * span (`docs/plans/v1.2.0/math-rendering/PLAN.md` §3/§4's own explicit requirement). */
const MATH_INLINE_RE = /\$((?:[^\s\\])|(?:\S.*?[^\s\\]))\$/g;

/** `texmath.$_pre`'s own guard, reproduced here: the character immediately before the opening `$`
 * must not be a backslash (an escaped `\$`) or a decimal digit (guards `$5,000 to $10,000`-style
 * runs where an EARLIER `$...$`-shaped match's boundary would otherwise land mid-number). */
function mathPreOk(text: string, dollarStart: number): boolean {
  if (dollarStart === 0) return true;
  const prev = text.charCodeAt(dollarStart - 1);
  return prev !== 0x5c && (prev < 0x30 || prev > 0x39);
}

/** `texmath.$_post`'s own guard: the character immediately after the closing `$` must not be a
 * decimal digit (the actual currency false-positive guard — `Price: $5, not $10 total` fails this
 * check at the `$5`/`$10` boundaries and is never treated as math). */
function mathPostOk(text: string, afterDollarEnd: number): boolean {
  if (afterDollarEnd >= text.length) return true;
  const next = text.charCodeAt(afterDollarEnd);
  return next < 0x30 || next > 0x39;
}

/** Scans `text` for every `$...$`/`$$...$$` math span, applying the same currency-false-positive
 * heuristic `markdownIt.ts`'s preview pipeline gets from `markdown-it-texmath` itself. Callers are
 * responsible for excluding matches that fall inside fenced/inline code (same code-fence-awareness
 * contract `findColorSpans`/`findImageTags` above already document) — a literal `$x$`-looking string
 * inside a fenced code block is never a real math span. */
export function findMathSpans(text: string): ParsedMathSpan[] {
  const results: ParsedMathSpan[] = [];
  const consumed: { from: number; to: number }[] = [];

  MATH_DISPLAY_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = MATH_DISPLAY_RE.exec(text))) {
    const from = m.index;
    const to = from + m[0].length;
    if (mathPreOk(text, from) && mathPostOk(text, to)) {
      results.push({ from, to, display: true });
      consumed.push({ from, to });
    }
  }

  MATH_INLINE_RE.lastIndex = 0;
  while ((m = MATH_INLINE_RE.exec(text))) {
    const from = m.index;
    const to = from + m[0].length;
    if (consumed.some((r) => from < r.to && to > r.from)) continue; // already claimed by a $$ match
    if (!mathPreOk(text, from) || !mathPostOk(text, to)) continue;
    results.push({ from, to, display: false });
  }

  results.sort((a, b) => a.from - b.from);
  return results;
}

function parseAttrs(attrString: string): Record<string, string> {
  const attrs: Record<string, string> = {};
  let m: RegExpExecArray | null;
  ATTR_RE.lastIndex = 0;
  while ((m = ATTR_RE.exec(attrString))) {
    attrs[m[1].toLowerCase()] = m[2];
  }
  return attrs;
}

/** Scans `text` for every allowlisted `<span style="...">...</span>` pair. Callers (typically
 * `markdownDecorations.ts`) are responsible for excluding matches that fall inside fenced code
 * blocks (via the syntax tree) before treating them as live decorations, the same code-fence
 * awareness already required of the TOC heading scanner (`DECISIONS.md` "Code-Fence Awareness in
 * Markdown Scanning"). */
export function findColorSpans(text: string): ParsedColorSpan[] {
  const results: ParsedColorSpan[] = [];
  let m: RegExpExecArray | null;
  SPAN_RE.lastIndex = 0;
  while ((m = SPAN_RE.exec(text))) {
    const [full, styleRaw, inner] = m;
    const openTagMatch = /^<span\s+style="[^"]*"\s*>/i.exec(full);
    const openLen = openTagMatch ? openTagMatch[0].length : 0;
    const openFrom = m.index;
    const openTo = openFrom + openLen;
    const closeTo = openFrom + full.length;
    const closeFrom = closeTo - "</span>".length;
    results.push({
      openFrom,
      openTo,
      contentFrom: openTo,
      contentTo: closeFrom,
      closeFrom,
      closeTo,
      style: filterStyleDeclarations(styleRaw),
    });
    void inner;
  }
  return results;
}

/** Scans `text` for every allowlisted `<mark>`/`<mark style="...">` pair. Same code-fence-exclusion
 * contract as `findColorSpans` above. */
export function findMarkSpans(text: string): ParsedMarkSpan[] {
  const results: ParsedMarkSpan[] = [];
  let m: RegExpExecArray | null;
  MARK_RE.lastIndex = 0;
  while ((m = MARK_RE.exec(text))) {
    const [full, styleRaw, inner] = m;
    const openTagMatch = /^<mark(?:\s+style="[^"]*")?\s*>/i.exec(full);
    const openLen = openTagMatch ? openTagMatch[0].length : 0;
    const openFrom = m.index;
    const openTo = openFrom + openLen;
    const closeTo = openFrom + full.length;
    const closeFrom = closeTo - "</mark>".length;
    results.push({
      openFrom,
      openTo,
      contentFrom: openTo,
      contentTo: closeFrom,
      closeFrom,
      closeTo,
      style: styleRaw ? filterStyleDeclarations(styleRaw) : "",
    });
    void inner;
  }
  return results;
}

/** Scans `text` for every allowlisted `<img ...>` tag. Tags whose `src` fails the safe-relative-
 * path check (`markdownSanitize.ts`) are omitted entirely rather than returned with a bad `src` —
 * callers should fall back to rendering the raw text for those, same as any other sanitizer
 * rejection. */
export function findImageTags(text: string): ParsedImageTag[] {
  const results: ParsedImageTag[] = [];
  let m: RegExpExecArray | null;
  IMG_RE.lastIndex = 0;
  while ((m = IMG_RE.exec(text))) {
    const attrs = parseAttrs(m[1]);
    if (!attrs.src || !isSafeRelativeImageSrc(attrs.src)) continue;
    const width = attrs.width ? Number.parseInt(attrs.width, 10) : undefined;
    const height = attrs.height ? Number.parseInt(attrs.height, 10) : undefined;
    results.push({
      from: m.index,
      to: m.index + m[0].length,
      src: attrs.src,
      width: Number.isFinite(width) ? width : undefined,
      height: Number.isFinite(height) ? height : undefined,
    });
  }
  return results;
}
