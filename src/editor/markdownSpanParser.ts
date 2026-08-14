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
