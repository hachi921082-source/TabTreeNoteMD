# PLAN: Preview-Pane Link-Encoding Fix — `#slug` Anchor Navigation and `memo:` Link Navigation (v1.2.0, Bug Fix)

> Status: planning complete, not yet implemented. Bug fix isolated to `src/editor/preview/
> previewInteractions.ts`'s `handleAnchorClick`. Depends on `docs/plans/table-of-contents/PLAN.md`
> (`scanHeadings`/slug generation, unchanged by this fix) and `docs/plans/link/PLAN.md`
> (`resolveAndOpenMemoLink`, unchanged internally by this fix — only its call site's input is now
> pre-decoded). **This fix is a correctness prerequisite for `docs/plans/v1.2.0/cross-tab-anchor-links/
> PLAN.md`** working at all on non-ASCII (i.e. almost any Japanese) tab/memo/heading names in `preview`/
> `split` pane mode — that plan should not be considered functional until this one lands, ideally
> implemented first or in the same pass.

Sources: `docs/spec.v1.2.0.md` §9; `docs/detailed-spec.v1.2.0.md` §9, §10.3; `docs/DECISIONS.md`
("Preview-Pane `memo:` Link Non-ASCII Name Encoding Bug: Bundled into v1.2.0's §9 Fix, Not Deferred").

---

## 1. Root Cause (Confirmed by Inspection)

Same underlying defect, two branches of the same function.

markdown-it's link-parsing pipeline runs every parsed `href` — including a same-document `#slug` fragment
and a `memo:`-prefixed target — through its `normalizeLink()` rule, which calls `mdurl.encode(mdurl.format
(mdurl.parse(url)))`, percent-encoding non-ASCII characters.

- **`#slug` branch**: a heading anchor link written as `[spec.mdの作成](#specmdの作成)` renders into the
  preview pane's HTML with `href="#specmd%E3%81%AE%E4%BD%9C%E6%88%90"` (percent-encoded). Meanwhile, the
  corresponding heading's `id` attribute is set directly from `scanHeadings()`'s raw slug value
  (`previewRenderer.ts`, `token.attrSet("id", headings[headingIndex].slug)`) — bypassing `normalizeLink`
  entirely, so it stays **unencoded** (`id="specmdの作成"`). `handleAnchorClick`'s `querySelector('#' +
  CSS.escape(href.slice(1)))` lookup then compares the still-encoded fragment against the raw `id`, which
  can never match — the click silently does nothing.
- **`memo:` branch (bundled follow-up, resolved `Q-v1.2.0-1`)**: identical defect on `memo:TabName/
  MemoName` hrefs whenever the tab or memo name contains a non-ASCII character — empirically,
  `mdurl.encode(mdurl.format(mdurl.parse('memo:Work/日記#見出し')))` returns
  `memo:Work/%E6%97%A5%E8%A8%98#%E8%A6%8B%E5%87%BA%E3%81%97`, corrupting the memo-name portion of the href
  just as thoroughly as the anchor portion. Pre-existing bug in today's baseline `memo:` cross-tab links
  (`[spec §2.8]`), not introduced by v1.2.0's cross-tab-anchor-links feature — but load-bearing for it.
- **Live view is unaffected in both cases**: its own click-resolution paths (`src/editor/links.ts`'s
  `resolveAndScrollToHeadingAnchor` for `#slug`; the bare-`memo:`-token/parenthetical-link handling for
  `memo:`) read raw CodeMirror source text directly — never passing through markdown-it's HTML-rendering
  pipeline, so no encoding is ever introduced there.

## 2. Fix — `#slug` Branch

`src/editor/preview/previewInteractions.ts`, `handleAnchorClick`'s `#`-prefixed branch (confirmed at lines
~97–104):

```diff
     if (href.startsWith("#")) {
-      const slug = href.slice(1);
+      const slug = decodeURIComponent(href.slice(1));
       ...
     }
```

- **No change to `tocGenerator.ts`/`previewRenderer.ts`**: the heading `id` attribute must **stay**
  unencoded (set from the raw slug) — this is an isolated decode fix on the read/lookup side only.
- **Failure mode**: a malformed percent-sequence in a hand-typed link would make `decodeURIComponent`
  throw (`URIError`) rather than silently no-op. Wrap in `try { ... } catch { return; }` (or equivalent)
  so a malformed link degrades to a silent no-op, matching this app's established "unresolved link = do
  nothing, never an error surfaced to the user" convention.

## 3. Fix — `memo:` Branch (Bundled Follow-Up)

Same file, sibling branch of the same function (confirmed at lines ~92–96):

```diff
     if (href.startsWith("memo:")) {
-      void resolveAndOpenMemoLink(config.tabId, href, config.onOpenMemoLink);
+      void resolveAndOpenMemoLink(config.tabId, decodeURIComponent(href), config.onOpenMemoLink);
       return;
     }
```

- **Decode the whole `href`**, not just a slice past the `"memo:"` prefix — the literal ASCII substring
  `"memo:"` contains no characters `decodeURIComponent` would ever transform, so decoding the full string
  first and then letting `cross-tab-anchor-links/PLAN.md` §2's existing scheme-stripping logic
  (`rawTarget.startsWith("memo:") ? rawTarget.slice("memo:".length) : rawTarget`) run against the
  now-decoded string produces the same result as decoding only the post-`memo:` remainder — simpler, no
  behavioral difference.
- **Decode point stays upstream of `resolveAndOpenMemoLink`, not inside it** — deliberately, to avoid
  double-decoding or an incorrect decode on the live-view call path. `resolveAndOpenMemoLink` is shared by
  **both** the preview pane's `handleAnchorClick` and live view's own bare-`memo:`-token/parenthetical-link
  click handling; live view's `rawTarget` is read straight from the raw CodeMirror source buffer and was
  **never** percent-encoded to begin with. Adding a decode step inside `resolveAndOpenMemoLink` itself
  would incorrectly attempt to decode an already-plain live-view target — harmless for most real names, but
  actively wrong for the rare case of a real tab/memo name containing a literal `%` character followed by
  two hex digits (e.g. `"50% Done"` — decoding a name that was never encoded could corrupt it). Fixing the
  problem at its one actual source (this preview-pane click-handler branch, exactly where §2 above already
  fixed the sibling `#`-branch) keeps the shared `resolveAndOpenMemoLink` function agnostic to which pane
  called it.
- **Interaction with `cross-tab-anchor-links/PLAN.md`'s `#slug` splitting**: because the decode happens
  *before* `resolveAndOpenMemoLink` ever runs, that plan's `hashIndex`/`target`/`anchorSlug`-splitting
  logic operates on an already-decoded string for both the memo-name portion and any `#slug` suffix — this
  fix, applied once at the `handleAnchorClick` call site, transparently also fixes decoding for
  `memo:TabName/MemoName#slug` cross-tab links' memo-name half, with no separate code needed in that
  plan's own §2/§3/§4 beyond what it already specifies. That plan's preview-mode programmatic anchor-jump
  path (the `querySelector('#' + CSS.escape(slug))` lookup) correctly needs no decode of its own, for the
  same reason established in §2 above — the slug it receives comes from this same already-decoded split,
  never re-encoded.
- **Same `try`/`catch` treatment as §2**: a malformed percent-sequence anywhere in the href — not just
  after `#` — would make this `decodeURIComponent(href)` call throw too; the same defensive wrapping (or a
  single shared wrapping around both branches' decode calls) applies here for the identical reason.

## 4. No Change to Backend or Data Model

Both fixes are isolated, frontend-only, single-file (`previewInteractions.ts`) click-handler changes — no
`ErrorCode`, no `Result<T>` shape change, no schema field, no Rust change.

## 5. Testing / Verification

- Preview pane: click a `[text](#見出しスラッグ)`-style heading-anchor link with a non-ASCII heading text —
  confirm it now scrolls to the matching heading (currently silently does nothing).
- Preview pane: click a `memo:Work/日記` or `memo:TabName/MemoName#slug`-style link with non-ASCII tab/memo
  names — confirm it now navigates correctly (currently silently does nothing for either branch).
- Live view: identical links, both branches — confirm still-correct behavior, unaffected before and after
  this fix (regression check on the branch that was never broken).
- A hand-typed link with a malformed percent-sequence in the `#slug` or `memo:` portion (e.g. a stray `%`
  not followed by two hex digits): confirm silent no-op, no uncaught `URIError`, no crash.
- A real tab/memo name containing a literal `%` followed by two hex digits (e.g. `"50% Done"`), clicked
  from live view: confirm the name resolves correctly and is *not* incorrectly decoded (regression check
  for the "decode point stays upstream" design point in §3).
- Once `docs/plans/v1.2.0/cross-tab-anchor-links/PLAN.md` is also implemented: a `memo:TabName/
  MemoName#slug` link with non-ASCII names in every component, clicked from preview/split mode — confirm
  end-to-end resolution (both the memo switch and the anchor jump).
