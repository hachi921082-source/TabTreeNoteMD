# PLAN: Markdown Preview Pane — Preview Rendering (Markdown→HTML Pipeline)

> Status: planning complete, not yet implemented. Companion to [`PLAN.md`](PLAN.md) (read that file
> first — this file only covers one sub-algorithm of that plan). Covers the preview pane's
> Markdown→HTML rendering pipeline specifically: the `markdown-it` renderer setup, the
> `renderMarkdownToHtml` token walk, the span/img allowlist mechanism, `PreviewPane.tsx` itself, image
> resolution, scroll-sync, heading-anchor/link/checkbox click interaction, and code-fence/table/image
> presentation. Split out into its own file because it was the single largest, most self-contained
> section of `PLAN.md` — large enough on its own to warrant separate reading, the same reasoning
> `docs/plans/tab-lock/PLAN.md` used when it split its own crash-recovery design into
> [`docs/plans/tab-lock/CRASH-RECOVERY.md`](../../tab-lock/CRASH-RECOVERY.md).

Elaborates `docs/MDEditor-spec.md` §4. Sources otherwise unchanged from [`PLAN.md`](PLAN.md)'s own
"Sources" list at the top of that file.

---

## 1. Renderer Setup — `src/editor/preview/markdownIt.ts`

```ts
import MarkdownIt from "markdown-it";
// exact plugin package(s) TBD at implementation time — see caveat below.

export const md = new MarkdownIt("default", { html: false, linkify: true, breaks: false })
  .use(taskListsPlugin, { enabled: true /* keep <input> non-disabled; readOnly gating is JS-side, see §8 */ });
```

- `html: false` — raw HTML in the source auto-escapes to literal visible text, exactly matching the
  live-view's own "undecorated HTML stays visible as text" behavior. **This is why the allowlisted
  `<span>`/`<img>` constructs cannot go through markdown-it's own HTML handling at all** — see §3.
- markdown-it's `"default"` preset (as opposed to the stricter `"commonmark"` preset) already includes
  GFM-equivalent table and strikethrough support with no extra plugin. `linkify: true` autolinks bare
  `http(s)://`/`www.` tokens (backed by the bundled `linkify-it`), matching the live-view's
  `@lezer/markdown` GFM autolink parity. Task-list checkboxes are not part of any markdown-it preset
  and need a small plugin (e.g. `markdown-it-task-lists`).
- **Caveat, to verify empirically during implementation** (this plan's confidence in exact library
  default behavior is based on general knowledge, not a live install/test in this repo): confirm the
  chosen markdown-it version's `"default"` preset really does enable tables/strikethrough without
  extra config, and confirm the chosen task-list plugin's rendered `<input>` shape (attributes, class
  names) before wiring §8's click handler against it. If either assumption is wrong, the fallback is
  a small custom `md.renderer.rules` override or an alternate plugin — a self-contained fix scoped to
  this one file. **(Open item — see bottom of this document.)**
- **Stress test**: per `docs/MDEditor-spec.md` §5's root-cause note, test this renderer against one
  pathological huge single line before shipping (this app's own image-embedding mechanism can't
  produce one, per that note, but a hand-typed or pasted-from-elsewhere huge line still could).

## 2. `renderMarkdownToHtml` — Token Walk

`src/editor/preview/previewRenderer.ts`:

```ts
export function renderMarkdownToHtml(state: EditorState): { html: string }
```

1. Take `text = state.doc.toString()`.
2. **Span/image allowlist substitution (before parsing — see §3)**: scan `text` with
   `findColorSpans`/`findImageTags` (imported verbatim from `markdownSpanParser.ts` — same functions
   the live-view uses, not reimplemented). Exclude any match overlapping a fenced code block: first
   run `md.parse(text, {})` once to get `fence`/`code_block` tokens' `.map` line ranges, convert those
   to character offsets via the document's line-start offsets, and skip any span/img match overlapping
   one of those ranges — a parallel implementation of `markdownDecorations.ts`'s `codeSkipRanges`
   concept, sourced from markdown-it's own parse output instead of the `@lezer/markdown` tree (the
   preview pipeline has no CodeMirror syntax tree available).
3. Replace each accepted match in `text` with a short placeholder token built from two Unicode
   Private-Use-Area code points as delimiters around the match's numeric index (delimiter code
   points U+E000 and U+E001, i.e. a placeholder shaped like `<PUA-start><index><PUA-end>`),
   chosen to survive markdown-it's inline tokenizer as inert plain text. (Residual, accepted
   risk: a document containing these exact literal code points in its real prose would see
   corrupted rendering — vanishingly unlikely, not worth defending against further for v1.)
4. `const tokens = md.parse(substitutedText, {});`
5. **`data-source-line` injection**: for every token with a non-null `.map` (covers `paragraph_open`,
   `heading_open`, `list_item_open`, `blockquote_open`, `table_open`/`tr_open`, `fence`/`code_block`,
   `hr`, etc. — not filtered to strictly top-level only, which is harmless extra granularity and
   simpler than tracking nesting depth), call `token.attrSet("data-source-line", String(token.map[0]))`.
6. **Heading-anchor IDs**: call `scanHeadings(state)` (imported directly from `tocGenerator.ts` — the
   *same already-computed, already-deduplicated array* the TOC feature uses, not a second independent
   call to `slugifyHeading`). Walk `tokens` in order, and for the *i*-th `heading_open` token
   encountered, `token.attrSet("id", headings[i].slug)`. Reusing the literal array (not just the
   slugify function) guarantees the preview can never disagree with the TOC/`#slug` resolution logic
   on heading-text-extraction edge cases (ATX closing-sequence stripping, etc.), since both read from
   the exact same scan.
7. `const html = md.renderer.render(tokens, md.options, {});`
8. **Substitute placeholders back in**, replacing each placeholder token (§2 step 3) with its safe,
   code-constructed HTML fragment:
   - Color span: `<span data-source-line="..." style="${filteredStyle}">${escapedInnerText}</span>`
     (inner text HTML-escaped since it wasn't run through markdown-it's own inline rendering — matches
     the live-view's own span content treatment, which doesn't support nested Markdown either).
   - Image: `<img data-relative-src="${src}" alt="${escapedAlt}" ${width ? `width="${width}"` : ""} ${height ? `height="${height}"` : ""}>` — **no `src` attribute yet** (resolved async, see §5); `data-relative-src` carries the path for the post-mount resolution pass.
   This string-splice is safe specifically because every substituted value was already individually
   validated (`isSafeRelativeImageSrc`/`filterStyleDeclarations`, both from `markdownSanitize.ts`) —
   not unescaped user-text concatenation.
9. **DOMPurify backstop**: `DOMPurify.sanitize(html, { ALLOWED_TAGS: [...], ALLOWED_ATTR: [...] })`
   with an explicit allowlist covering exactly what steps 5-8 can produce: block tags
   (`p,h1..h6,ul,ol,li,blockquote,pre,code,hr,table,thead,tbody,tr,th,td,a,strong,em,del,span,img,
   input,div,br`), and attributes `data-source-line,id,href,style,src,alt,width,height,
   data-relative-src,type,checked,disabled,class`. `html:false` upstream means this pass should rarely
   have anything dangerous to catch — its role is a defense-in-depth backstop, not the primary control
   (`docs/MDEditor-spec.md` §4).
10. Return `{ html }`.

## 3. Why Span/Img Can't Go Through markdown-it's HTML Path

With `html:false`, a literal `<span style="...">` or `<img ...>` typed in the source would otherwise be
auto-escaped to visible text — exactly matching the live-view's behavior for any *other* raw HTML tag.
The two allowlisted constructs are deliberately pulled out **before** parsing (step 2 of §2 above) and
re-inserted as already-safe fragments **after** rendering (step 8 of §2) so they bypass markdown-it's
HTML-escaping path entirely, without needing a general HTML allowlist inside the renderer itself
(`docs/MDEditor-spec.md` §4's explicit design).

## 4. `PreviewPane.tsx`

```tsx
export interface PreviewPaneProps {
  getView: () => EditorView | null;
  renderTick: number;          // bumped by EditorPane's existing updateListener — reused, not duplicated
  debounceMs: number;          // AppSettings.preview.debounceMs
  tabId: string | null;
  memoKey: string;
  readOnly: boolean;           // gates checkbox write-back, §8
  openTrigger: AppSettings["links"]["openTrigger"];
  confirmBeforeOpen: boolean;
  requestExternalConfirm: (kind: ExternalLinkKind, value: string) => Promise<boolean>;
  onOpenMemoLink: (tabId: string, memoName: string | null) => void;
}
```

- Debounced re-render: `useEffect(() => { const t = window.setTimeout(() => { const view = getView(); if (view) setHtml(renderMarkdownToHtml(view.state).html); }, debounceMs); return () => window.clearTimeout(t); }, [renderTick, debounceMs])`
  — reuses `EditorPane.tsx`'s existing `renderTick` state (already bumped on every doc change via the
  CodeMirror `updateListener`) rather than adding a second listener.
- After `setHtml` commits, a `useEffect` resolves every `img[data-relative-src]` inside the container
  via `getOrLoadImage` (§5), and (re)binds the scroll-sync listener (§6).
- Container: `<div className="ttn-editor-preview-pane" ref={containerRef} onContextMenu={...} dangerouslySetInnerHTML={{ __html: html }} />` with `previewInteractions.ts`'s click delegation attached once via `useEffect` (not re-bound every render).

## 5. Image Resolution in the Preview

After `container.innerHTML` is set (§4), a `useEffect` finds every `img[data-relative-src]` and calls
`getOrLoadImage(tabId, path, memoKey)` (`image-handling/PLAN.md`'s shared image-loading pipeline),
setting `.src` on resolve (or leaving it as the fallback — a small inline "image unavailable"
glyph/background, same visual family as the live-view's placeholder) on failure. `alt`/`width`/`height`
are already set from §2 step 8's HTML.

## 6. Scroll-Sync (One-Directional: Editor → Preview)

`src/editor/preview/scrollSync.ts`:

```ts
export function scrollPreviewToLine(container: HTMLElement, targetLine: number): void {
  const blocks = container.querySelectorAll<HTMLElement>("[data-source-line]");
  let best: HTMLElement | null = null;
  for (const el of blocks) {
    const line = Number(el.dataset.sourceLine);
    if (line <= targetLine) best = el;   // blocks are in document order, so the last match ≤ target wins
    else break;
  }
  best?.scrollIntoView({ block: "start" });
}
```

Driven by `PreviewPane`'s own listener on `getView()?.scrollDOM`'s `scroll` event (same
`requestAnimationFrame`-coalescing pattern `MarkdownToolbar.tsx`'s resize handler already uses, to
avoid layout thrash on continuous scroll), calling `computeTopVisibleLine(view)` (reused from
`textCommands.ts`) then `scrollPreviewToLine`. **One-directional only** (editor scroll drives preview
scroll, never the reverse) — this is exactly what `docs/MDEditor-spec.md` §4 describes; no
preview-to-editor sync is implied or implemented.

## 7. Heading-Anchor Click Handling

In `previewInteractions.ts`'s delegated click handler: for a clicked `<a href="#slug">`,
`e.preventDefault()`, then `container.querySelector("#" + CSS.escape(slug))?.scrollIntoView({block:
"start"})`, scoped to the preview's own container (not `document.getElementById`, which could collide
with an unrelated same-id element elsewhere in the app) — matches `docs/MDEditor-spec.md` §4's
"resolved by calling `scrollIntoView` on the matching `id` element within the preview pane's own
internal scroll container."

## 8. Checkbox Click Write-Back

`previewInteractions.ts`, on click of a rendered `<input type="checkbox">`:

1. If `readOnly`, no-op (matches the live-view's `cm-md-checkbox-readonly` gating) — the click handler
   itself performs this check rather than relying on the plugin's own `disabled` attribute (more
   robust against plugin-specific rendering quirks, per §1's caveat).
2. Find the nearest ancestor `[data-source-line]` element (the enclosing `list_item_open`'s line,
   tagged in §2 step 5) and read its line number (0-indexed, per `token.map`).
3. `const line = view.state.doc.line(lineNumber + 1);` (CodeMirror lines are 1-indexed).
4. `const match = /^(\s*(?:[-*+]|\d+[.)])\s+)\[([ xX])\]/.exec(line.text);` — locates the task marker
   immediately after the list-item's own bullet/number prefix (GFM's own syntax requirement, so this
   is not a loose scan of the whole line).
5. If matched: `const from = line.from + match[1].length; const to = from + 3;` then
   `toggleCheckboxRange(view, from, to, /[xX]/.test(match[2]))` — **reuses** `checkboxClick.ts`'s
   exported `toggleCheckboxRange` directly for the actual transaction dispatch (the position-mapping
   *approach* above is a new, distinct implementation per `docs/MDEditor-spec.md` §4's requirement;
   reusing the existing dispatch primitive once the range is known is ordinary DRY, not a duplicate of
   the thing that needed to be distinct).

## 9. Link Clicks

`previewInteractions.ts`'s delegated click handler, for any `e.target.closest("a")`:

- **Always `e.preventDefault()`** first, regardless of kind — letting the WebView2 default anchor
  navigation proceed would navigate the app's own window away from itself.
- `href` starting with `memo:` → `resolveAndOpenMemoLink(tabId, href, onOpenMemoLink)` (imported
  verbatim from `links.ts` — already framework-agnostic, no CodeMirror dependency, directly reusable
  as-is).
- `href` starting with `#` → §7 above.
- Otherwise (`http(s)://`, `mailto:`, or a bare relative-file-path token rendered as a link by the two
  custom inline rules below) → classify by scheme (`mailto:` prefix → `email`, value minus prefix;
  otherwise → `url`, unless it matches the bare-file-path shape → `filePath`), then apply
  `linkClickMatchesTrigger` (reused from `links.ts`, already generic over a raw `MouseEvent`) +
  `confirmBeforeOpen` gating (via `requestExternalConfirm`) + `linkOpenExternal` (reused ipc call) —
  same user-facing behavior as the live-view, new call site (`docs/MDEditor-spec.md` §4).

**Bare `memo:...` and bare file-path tokens** (the two token shapes `@lezer/markdown`'s GFM extension
doesn't recognize, and so `linkify:true` won't autolink either): two small custom markdown-it inline
rules (registered via `md.inline.ruler.push`) that scan plain-text inline content for
`BARE_MEMO_RE`/`FILE_PATH_RE` (imported verbatim from `links.ts`, reused, not reimplemented) and emit
an inline link token — this mirrors *detection* with the live view (same regexes, same module) while
necessarily using a different *mechanism* (a markdown-it inline rule vs. a CodeMirror decoration range,
since the preview has no CodeMirror ranges to decorate).

## 10. Tables, Code Fences, Images — Presentation

- **Wide tables**: `md.renderer.rules.table_open`/`table_close` overridden to wrap output in
  `<div class="ttn-preview-table-wrap">...</div>` (`overflow-x: auto` — standard markdown-it renderer
  customization technique). Never auto-shrinks font size.
- **Code fences**: plain monospace text with a background color only (`.ttn-preview-code { font-family:
  monospace; background: var(--...); }`), matching the live-view's `cm-md-codeblock` treatment — no
  syntax-highlighting library added.
- **Images**: `.ttn-editor-preview-pane img { max-width: 100%; height: auto; }` — shrink-to-fit only,
  never upscale, aspect ratio preserved. Deliberately different from the live-view's small fixed cosmetic
  cap (`image-handling/PLAN.md`'s live-view rendering section) — the preview represents the "final look"
  PDF export (`pdf-export/PLAN.md`) prints verbatim, so it uses the full available column width and must
  never bleed off a printed page edge.

## Open Items

1. **§1's caveat**: verify the chosen `markdown-it` version's default table/strikethrough support and
   the chosen task-list plugin's exact rendered `<input>` shape empirically, early in implementation,
   before wiring §8's checkbox click handler against assumed markup.
