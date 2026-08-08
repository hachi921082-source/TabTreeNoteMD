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
    const src = token.attrs[srcIdx][1];
    token.attrs.splice(srcIdx, 1);
    token.attrSet("data-relative-src", src);
  }
  return self.renderToken(tokens, idx, options);
};

/** Splits every plain "text" token in `tokens` on `re`, replacing each match with a
 * `link_open`/text/`link_close` triple carrying `href="<the matched token>"`. Used for the two bare
 * token shapes `@lezer/markdown`'s GFM extension does NOT recognize (`links.ts`'s own header
 * comment) — `memo:...` and bare file paths.
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
function splitTextTokensByBareTokenRegex(children: Token[], re: RegExp, TokenCtor: TokenConstructor): Token[] {
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
    children = splitTextTokensByBareTokenRegex(children, BARE_MEMO_RE, state.Token);
    children = splitTextTokensByBareTokenRegex(children, FILE_PATH_RE, state.Token);
    blockToken.children = children;
  }
}

/** Builds a fresh configured instance. Stateless across calls to `render`/`parse` — it retains no
 * per-document data, so `previewRenderer.ts` is free to call `.parse`/`.renderer.render` on the
 * same shared instance repeatedly (once per debounced re-render). */
export function createPreviewMarkdownIt(): MarkdownItInstance {
  const md = new MarkdownIt("default", { html: false, linkify: true, breaks: false });
  md.use(taskLists, { enabled: true });
  md.linkify.set({ fuzzyLink: true, fuzzyEmail: true });
  md.renderer.rules.table_open = wrapTableOpen;
  md.renderer.rules.table_close = wrapTableClose;
  md.renderer.rules.image = renderImageWithDeferredSrc;
  md.core.ruler.push("ttn_bare_link_scan", bareLinkCoreRule);
  return md;
}
