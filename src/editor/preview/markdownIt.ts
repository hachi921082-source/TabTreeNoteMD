// Configured `markdown-it` instance for the read-only preview pane
// (`docs/plans/markdown/markdown-preview/RENDERING.md` §1). A full independent Markdown->HTML
// pipeline — deliberately does not share state with the live-view's CodeMirror/`@lezer/markdown`
// syntax tree (`PLAN.md` §1's "separate rendering pipeline" non-goal note).
//
// Empirical verification performed at implementation time (RENDERING.md §1's flagged caveat —
// "confirm the chosen markdown-it version's `default` preset really does enable
// tables/strikethrough without extra config, and confirm the chosen task-list plugin's rendered
// `<input>` shape"), against markdown-it 15.0.0 / markdown-it-task-lists 2.1.1:
//   - The `"default"` preset DOES enable GFM-equivalent tables and strikethrough with no extra
//     plugin (strikethrough renders as `<s>...</s>`, NOT `<del>...</del>` — see
//     `previewRenderer.ts`'s DOMPurify allowlist, which had to be corrected from RENDERING.md §2
//     step 9's literal example accordingly).
//   - `markdown-it-task-lists` with `{ enabled: true }` renders
//     `<li class="task-list-item enabled"><input class="task-list-item-checkbox" type="checkbox"> …`
//     (and `checked=""` when checked) — no `disabled` attribute (matches this app's own JS-side
//     readOnly gating, RENDERING.md §8 item 1), and critically `previewInteractions.ts`'s click
//     handler matches on `input[type="checkbox"]` generically rather than the plugin's own class
//     name, so it isn't coupled to this exact markup shape.
//   - `linkify: true`'s bundled `linkify-it` defaults to `fuzzyLink: false`, meaning a bare
//     `www.example.com` token (no `http(s)://` prefix) would NOT be auto-linked out of the box —
//     unlike the live-view's `@lezer/markdown` GFM `Autolink` extension, which DOES recognize bare
//     `www.` tokens per the GFM autolinks-extension spec. `fuzzyLink: true` is set explicitly below
//     to match that live-view parity, which is this module's own stated intent (RENDERING.md §1).
import MarkdownIt from "markdown-it";
import type { MarkdownIt as MarkdownItInstance, RendererRule, StateCore, Token } from "markdown-it";
import taskLists from "markdown-it-task-lists";
import texmath from "markdown-it-texmath";
import katex from "katex";
// Bundled by Vite exactly like every other CSS import in this codebase (`EditorPane.tsx`'s own
// `import "./Editor.css";` precedent) — never CDN-loaded (`docs/plans/v1.2.0/math-rendering/
// PLAN.md` §2.4's offline requirement). The stylesheet's own `url(fonts/KaTeX_*.woff2)` references
// are relative to itself inside `node_modules/katex/dist/`, so Vite resolves and bundles the font
// files as build assets automatically, the same mechanism it already uses for every other
// CSS-referenced asset in this project — no extra Vite config needed (verified empirically at
// implementation time by inspecting `katex.min.css`'s font URLs; PLAN.md's own flagged
// verification step, "disable network access, confirm formulas still render with correct
// glyphs/styling," is reviewer-lite's live-test pass, not repeated here).
import "katex/dist/katex.min.css";
import { BARE_MEMO_RE, FILE_PATH_RE } from "../links";

/** `Token`'s own class is exported as a type-only named export by markdown-it's bundled `.d.mts`
 * (`export { ..., type Token, ... }`) — there is no runtime value to `import` and take `typeof` of.
 * `StateCore.Token` (the constructor markdown-it itself passes to core rules, see `bareLinkCoreRule`
 * below) is the actual runtime constructor; this alias just names its call signature so
 * `splitTextTokensByBareTokenRegex` doesn't need `typeof Token`. */
type TokenConstructor = new (type: string, tag: string, nesting: -1 | 0 | 1) => Token;

/** Wraps rendered tables in a horizontally-scrollable container (RENDERING.md §10 "Wide tables") —
 * standard markdown-it renderer-customization technique (override `table_open`/`table_close`,
 * delegate to the default token renderer for the table tag itself). */
const wrapTableOpen: RendererRule = (tokens, idx, options, _env, self) => {
  return `<div class="ttn-preview-table-wrap">${self.renderToken(tokens, idx, options)}`;
};
const wrapTableClose: RendererRule = (tokens, idx, options, _env, self) => {
  return `${self.renderToken(tokens, idx, options)}</div>`;
};

/** Minimal HTML escaping for the KaTeX-parse-error fallback text below — mirrors `markdown-it-
 * texmath`'s own internal (unexported) `escapeHTML` helper rather than importing it, since this
 * app's renderer-rule overrides bypass that package's own rendering path entirely (see
 * `markdown-it-texmath.d.ts`'s header comment). */
function escapeHtmlText(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/** Renders one `$...$`/`$$...$$` formula body to KaTeX HTML (`PLAN.md` §2.1/§2.2) — `output: "html"`
 * only (no parallel MathML tree, keeping the DOMPurify allowlist surface area to exactly what
 * `output: "html"` can produce, verified empirically against `previewRenderer.ts`'s allowlist at
 * implementation time: only `<span>`/`<svg>`/`<path>`, no `<math>`/MathML tag names). `throwOnError:
 * false` is KaTeX's own graceful-degradation setting — invalid LaTeX renders as inline error text
 * (`.katex-error`) instead of throwing, so one malformed formula can't take down the whole
 * `md.render()` call; the `try`/`catch` below is a second, defense-in-depth layer for a genuinely
 * unexpected engine-level failure, mirroring `markdown-it-texmath`'s own `texmath.render` try/catch
 * pattern (not reused directly — see `markdown-it-texmath.d.ts`'s header comment on why this app's
 * renderer rules bypass that helper). */
function renderMathToHtml(tex: string, displayMode: boolean): string {
  try {
    return katex.renderToString(tex, { output: "html", displayMode, throwOnError: false });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return `<span class="katex-error" title="${escapeHtmlText(message)}">${escapeHtmlText(tex)}</span>`;
  }
}

/** Wraps a rendered `$$...$$` block formula in the exact same horizontal-scroll container the wide-
 * table rule above uses (`PLAN.md` §2.3) — `Editor.css` pairs `.ttn-preview-math-wrap` with
 * `.ttn-preview-table-wrap` on one shared `overflow-x: auto` declaration rather than duplicating it.
 * `data-source-line` is read back off the token rather than recomputed: `previewRenderer.ts`'s own
 * pre-render pass already calls `token.attrSet("data-source-line", ...)` for every top-level token
 * with a non-null `.map` (block-math tokens included — `markdown-it-texmath`'s block rule sets
 * `token.map` the same way every other markdown-it block rule does, `PLAN.md` §2.1's own
 * requirement), so this rule only needs to read it back, exactly like `wrapTableOpen`/`wrapTableClose`
 * above implicitly inherit it via `self.renderToken`.
 *
 * Deliberately identical for both the plain `$$...$$` block form and the `dollars` preset's
 * `math_block_eqno` variant (`$$...$$ (1)`-shaped trailing-equation-number annotation, an
 * out-of-scope Pandoc-style shorthand `PLAN.md` never mentions) — the parenthesized annotation text
 * is simply not echoed into the rendered formula in that edge case rather than adding a new
 * "equation numbering" UI affordance not called for by the spec; the raw Markdown source itself is
 * untouched either way. */
function wrapBlockMath(token: Token): string {
  const sourceLine = token.attrGet("data-source-line");
  const lineAttr = sourceLine != null ? ` data-source-line="${sourceLine}"` : "";
  return `<div class="ttn-preview-math-wrap"${lineAttr}>${renderMathToHtml(token.content, true)}</div>`;
}

/** markdown-it's CommonMark-mandated link/image URL normalization (via its bundled `mdurl`
 * encoder) percent-encodes any non-ASCII byte in `![alt](url)`'s `url` — e.g. `![](きい/001.png)`
 * renders `src="%E3%81%8D%E3%81%84/001.png"`, verified empirically against markdown-it 15.0.0
 * (GitHub issue #248). Every downstream reader of `data-relative-src`
 * (`PreviewPane.tsx`'s image-resolution effect, `pdfExport.ts`'s PDF-export image pass) treats it
 * as a literal relative filesystem path, not a URL, so that encoding must be undone once, here at
 * the single point markdown-it's output is first consumed — not duplicated at every read site.
 * `decodeURIComponent` throws on a malformed escape (e.g. a stray `%` that was never meant as one);
 * `mdurl`'s own encoder only ever emits well-formed `%XX` escapes for what IT encoded, so decoding
 * markdown-it's own output back is safe in the normal case, but a user could still directly type a
 * `%XX`-shaped literal substring into `![](literal%20path)` source — falls back to the raw
 * (already-correct, in that case) string rather than letting a decode failure crash image
 * rendering. */
function decodeMarkdownItImageSrc(src: string): string {
  try {
    return decodeURIComponent(src);
  } catch {
    return src;
  }
}

/** Mirrors markdown-it's own built-in `image` render rule (which flattens `alt` to plain text per
 * CommonMark's special alt-text handling) but additionally renames `src` to `data-relative-src` —
 * standard Markdown `![alt](path)` syntax is this app's actual primary image-insertion mechanism
 * (`editor/imagePaste.ts`'s `saveAndInsertImage` always emits `![](relativePath)`, never the
 * allowlisted raw `<img>` HTML tag form), so it must go through the exact same async
 * `getOrLoadImage` Blob-URL resolution (`previewRenderer.ts` §5) as that allowlisted form — a plain
 * `src="relativePath"` handed straight to the DOM would try (and fail) to resolve as a URL against
 * the webview's own origin. No `isSafeRelativeImageSrc` filtering is applied here (unlike the
 * allowlisted raw-`<img>`-tag path in `previewRenderer.ts`, which reuses that check): standard
 * `![]()` syntax is ordinary CommonMark, not part of the narrow raw-HTML allowlist
 * `markdownSanitize.ts` governs, and the live-view's own equivalent `case "Image":` handling in
 * `markdownDecorations.ts` likewise passes the URL straight to `getOrLoadImage`/`image_read_bytes`
 * unfiltered — the actual security boundary for path-traversal/absolute-path rejection is enforced
 * server-side by `image_read_bytes`'s own Rust path validation, consistently for both forms. */
const renderImageWithDeferredSrc: RendererRule = (tokens, idx, options, env, self) => {
  const token = tokens[idx];
  const altIdx = token.attrIndex("alt");
  if (altIdx >= 0 && token.attrs) token.attrs[altIdx][1] = self.renderInlineAsText(token.children ?? [], options, env);
  const srcIdx = token.attrIndex("src");
  if (srcIdx >= 0 && token.attrs) {
    const src = String(token.attrs[srcIdx][1]);
    token.attrs.splice(srcIdx, 1);
    token.attrSet("data-relative-src", decodeMarkdownItImageSrc(src));
  }
  return self.renderToken(tokens, idx, options);
};

/** Splits every plain "text" token in `tokens` on `re`, replacing each match with a
 * `link_open`/text/`link_close` triple carrying `href="<the matched token>"`. Used for the two bare
 * token shapes `@lezer/markdown`'s GFM extension does NOT recognize (`links.ts`'s own header
 * comment) — `memo:...` and bare file paths.
 *
 * `extendedSearchTextAttr` (issue #235, file-path matches only — `bareLinkCoreRule` passes `true`
 * only for its `FILE_PATH_RE` call): also sets `data-extended-search-text` to the rest of `tok.
 * content` from the match's start onward. markdown-it splits a multi-line paragraph's inline
 * content into one "text" token per source line (a "softbreak" token sits between them), so this
 * is exactly "rest of the source line, from the match's start to end-of-line" — no separate line
 * lookup needed. `previewInteractions.ts`'s click handler reads it back to widen the file-open
 * search past the regex match's own truncated extent, the same way `links.ts`'s live-view
 * click handler does from `EditorView.state.doc`.
 *
 * Implemented as a `core.ruler` post-processing pass over already-tokenized "text" tokens
 * (mirroring markdown-it's own built-in `linkify` core rule — see `lib/rules_core/linkify.js`),
 * NOT as an `inline.ruler.push`/`inline.ruler.before("text", ...)` character-position rule as
 * RENDERING.md §9's literal wording suggested ("registered via `md.inline.ruler.push`"). Verified
 * empirically at implementation time that the inline-ruler-push approach does not work here: the
 * built-in `text` inline rule greedily consumes an entire run of non-terminator characters (and
 * `:` IS a terminator, precisely to let `linkify` react to it, but a *preceding* space or letter is
 * not) in one jump starting from wherever the tokenizer currently sits, so a rule registered later
 * in the ruler (or even via `.before("text", ...)`) is only ever tried at the START of such a run —
 * never at an arbitrary position further inside it, such as where `memo:` begins mid-sentence (e.g.
 * "see memo:Foo" — `text` claims "see memo" as one run before any bare-token rule ever gets a
 * chance at the "memo:" starting position). This is exactly the same reason markdown-it's own
 * bare-URL autolinking (`linkify: true`) is *not* implemented as an inline-position rule either. */
function splitTextTokensByBareTokenRegex(children: Token[], re: RegExp, TokenCtor: TokenConstructor, extendedSearchTextAttr: boolean): Token[] {
  let sawMatch = false;
  const out: Token[] = [];
  let linkDepth = 0;
  for (const tok of children) {
    if (tok.type === "link_open") linkDepth++;
    if (tok.type === "link_close") linkDepth = Math.max(0, linkDepth - 1);
    // Never re-linkify text that is already inside an existing `[label](url)` link's label — GFM
    // links cannot nest, and doing so would emit invalid nested `<a>` markup.
    if (tok.type !== "text" || linkDepth > 0) {
      out.push(tok);
      continue;
    }
    re.lastIndex = 0;
    let m: RegExpExecArray | null;
    let last = 0;
    const pieces: Token[] = [];
    while ((m = re.exec(tok.content))) {
      sawMatch = true;
      if (m.index > last) {
        const plain = new TokenCtor("text", "", 0);
        plain.content = tok.content.slice(last, m.index);
        pieces.push(plain);
      }
      const open = new TokenCtor("link_open", "a", 1);
      open.attrSet("href", m[0]);
      if (extendedSearchTextAttr) open.attrSet("data-extended-search-text", tok.content.slice(m.index));
      pieces.push(open);
      const inner = new TokenCtor("text", "", 0);
      inner.content = m[0];
      pieces.push(inner);
      pieces.push(new TokenCtor("link_close", "a", -1));
      last = m.index + m[0].length;
    }
    if (pieces.length === 0) {
      out.push(tok);
      continue;
    }
    if (last < tok.content.length) {
      const rest = new TokenCtor("text", "", 0);
      rest.content = tok.content.slice(last);
      pieces.push(rest);
    }
    out.push(...pieces);
  }
  return sawMatch ? out : children;
}

function bareLinkCoreRule(state: StateCore): void {
  for (const blockToken of state.tokens) {
    if (blockToken.type !== "inline" || !blockToken.children) continue;
    let children = blockToken.children;
    children = splitTextTokensByBareTokenRegex(children, BARE_MEMO_RE, state.Token, false);
    children = splitTextTokensByBareTokenRegex(children, FILE_PATH_RE, state.Token, true);
    blockToken.children = children;
  }
}

/** `linkify-it`'s `fuzzyLink: true` (set below for bare `www.`-token parity with the live view,
 * per this module's own header comment) also fuzzy-matches any bare `word.tld` shape where `tld`
 * is a *recognized* top-level domain — and every 2-letter country-code TLD is recognized by
 * default, `.md` (Moldova) included. In a Markdown notes app, bare mentions of `*.md` filenames in
 * prose are extremely common, so this false-positives constantly (GitHub issue #155). Scoped to
 * exactly the fuzzy (schema-less, `match.schema === ""`) case — an explicit `[text](url)`
 * link, a `<http://...>` autolink, or a genuine scheme-ful bare URL are untouched. */
function isUnwantedMdFilenameFuzzyMatch(match: { schema: string; raw: string }): boolean {
  return match.schema === "" && /^[^\s:/]+\.md$/i.test(match.raw) && !/^www\./i.test(match.raw);
}

/** Builds a fresh configured instance. Stateless across calls to `render`/`parse` — it retains no
 * per-document data, so `previewRenderer.ts` is free to call `.parse`/`.renderer.render` on the
 * same shared instance repeatedly (once per debounced re-render). */
export function createPreviewMarkdownIt(): MarkdownItInstance {
  const md = new MarkdownIt("default", { html: false, linkify: true, breaks: false });
  md.use(taskLists, { enabled: true });
  md.linkify.set({ fuzzyLink: true, fuzzyEmail: true });
  // GitHub issue #155: filter out the "*.md" fuzzy-link false positive (see
  // `isUnwantedMdFilenameFuzzyMatch` above) at the source — `md.linkify.match` is what
  // markdown-it's built-in `linkify` core rule calls to find fuzzy-link candidates, so this stays
  // in effect no matter which core rule consumes the result.
  // Must return `[]`, never `null`, when every match got filtered out: markdown-it's own
  // `linkify` core rule first calls `.test()` (untouched, still `true`) to decide whether to call
  // `.match()` at all, then indexes straight into `.match()`'s result assuming a non-null array
  // whenever `.test()` said yes — an empty array satisfies that same assumption (the `links.length
  // > 0` check downstream just no-ops), but `null` crashes it.
  const originalMatch = md.linkify.match.bind(md.linkify);
  md.linkify.match = (text: string) => {
    const matches = originalMatch(text);
    if (!matches) return matches;
    return matches.filter((m) => !isUnwantedMdFilenameFuzzyMatch(m));
  };
  md.renderer.rules.table_open = wrapTableOpen;
  md.renderer.rules.table_close = wrapTableClose;
  md.renderer.rules.image = renderImageWithDeferredSrc;
  md.core.ruler.push("ttn_bare_link_scan", bareLinkCoreRule);

  // Math rendering (`PLAN.md` §2.1): `"dollars"` preset registers `$...$` (`math_inline`) and
  // `$$...$$` (`math_block` when it starts its own line, `math_inline_double` — still KaTeX
  // `displayMode: true` — when `$$...$$` appears mid-paragraph) plus the Pandoc-style "no whitespace
  // immediately inside either delimiter" currency-false-positive heuristic built into its own
  // `$_pre`/`$_post` guards (verified empirically at implementation time: `Price: $5, not $10 total`
  // does not tokenize as math). `engine: katex` is required by the plugin's own setup path (see
  // `markdown-it-texmath.d.ts`) even though every renderer rule it installs is immediately
  // overridden below with `renderMathToHtml`'s own `katex.renderToString` call (no `katexOptions`
  // passed here — it would never be read) — its own `<eq>`/`<section>` wrapper tags are not in
  // `previewRenderer.ts`'s DOMPurify allowlist and are never meant to be.
  md.use(texmath, { engine: katex, delimiters: "dollars" });
  md.renderer.rules.math_inline = (tokens, idx) => renderMathToHtml(tokens[idx].content, false);
  md.renderer.rules.math_inline_double = (tokens, idx) => renderMathToHtml(tokens[idx].content, true);
  md.renderer.rules.math_block = (tokens, idx) => wrapBlockMath(tokens[idx]);
  md.renderer.rules.math_block_eqno = (tokens, idx) => wrapBlockMath(tokens[idx]);

  return md;
}
