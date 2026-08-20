# PLAN: Math/TeX Rendering — Preview-Pane-Only First Pass (v1.2.0)

> Status: planning complete, not yet implemented. New feature, greenfield (no existing math-adjacent
> dependency or code in `src/`/`src-tauri/`, confirmed by `[spec v1.2.0 §2.1]`/`[detailed-spec v1.2.0
> §2]`). Depends on `docs/plans/markdown/markdown-preview/RENDERING.md` (the `markdown-it` rendering
> pipeline, DOMPurify sanitizer allowlist, `data-source-line` scroll-sync scheme, wide-table
> horizontal-scroll-container treatment), `docs/plans/markdown-editing/PLAN.md` (CodeMirror 6
> `liveViewCompartment`/decoration-widget architecture, `ImagePlaceholderWidget` precedent), `docs/plans/
> image-handling/PLAN.md` §... (`SourceImageMarkerPlugin`/`sourceImageMarkerCompartment`, GitHub issue
> #81 — this feature's own source-view marker directly mirrors it). Extracted UI copy (marker
> tooltips/labels, if any) is folded into `docs/plans/v1.2.0/i18n/PLAN.md`'s translation maps per that
> plan's §6 sequencing note.

Sources: `docs/spec.v1.2.0.md` §2; `docs/detailed-spec.v1.2.0.md` §2, §10.3, §11.

---

## 1. Scope Recap

Full math typesetting (`$...$` inline / `$$...$$` block, KaTeX) added **only** to the preview pane
(`paneMode: "preview"`/`"split"`). Live view and source view get an at-a-glance marker only, not full
rendering, as a deliberate first-pass scope boundary — not deferred work to revisit inside this plan.

## 2. Preview-Pane Rendering Pipeline

### 2.1 Renderer Construction (`src/editor/preview/markdownIt.ts`)

- Add a KaTeX rendering step to the shared `markdown-it` instance already configured here (`html: false`,
  GFM/task-lists), via a `markdown-it`-ecosystem math plugin (e.g. `markdown-it-texmath`, or an
  equivalent) configured with the `$...$`/`$$...$$` delimiter pair. Plugin package choice is
  `developer-lite` discretion (`[spec v1.2.0 §2.2]`'s own delegation) — the only observable requirement is
  "delimiter syntax in, typeset HTML out, through the same `md.render()` call," no second render pass, no
  new debounce timer.
- **False-positive heuristic (currency `$`)**: adopt the standard Pandoc/Typora-style rule already built
  into common `markdown-it` math plugins — an inline `$...$` span only matches when there is no whitespace
  immediately inside either delimiter (e.g. `It costs $5, but not $10 total` is never treated as inline
  math). This is the plugin's own default behavior if a `markdown-it-texmath`-class plugin is chosen; if a
  hand-rolled rule is written instead, it must replicate this exact heuristic so live-view/source-view
  scanning (§3/§4) and the preview renderer agree on what counts as a math span.
- **`data-source-line` attribution**: the chosen plugin's block/inline tokens must expose a `token.map`
  (the standard convention nearly every `markdown-it` block rule already follows) so the existing scroll-
  sync attribute-application code picks it up automatically — no new scroll-sync wiring.

### 2.2 KaTeX Output Mode and Sanitizer Allowlist

- Configure the KaTeX call with `output: "html"` (skip the parallel MathML tree) — avoids extending
  DOMPurify's `ALLOWED_TAGS` with MathML tag names (`<math>`, `<semantics>`, `<mrow>`, `<mo>`, etc.) and
  keeps the extra sanitizer surface area at zero. (`output: "htmlAndMathml"`/`"mathml"` is the accessible/
  copy-paste-friendly alternative if `developer-lite` later prefers that tradeoff — not spec-mandated
  either way, `[detailed-spec v1.2.0 §11]`.)
- With `output: "html"`, KaTeX's own generated markup is almost entirely `<span class="katex-..."
  style="...">` — `previewRenderer.ts`'s existing DOMPurify allowlist (`ALLOWED_TAGS`/`ALLOWED_ATTR`,
  confirmed by inspection at lines ~186–195) already includes `"span"` and both `"style"`/`"class"` (added
  for the existing `<span style>` color/font-size construct and internal preview classes) — **little to
  no new allowlist entry is expected**. Confirm this empirically once the plugin is wired (render a test
  formula, diff the DOMPurify-sanitized output against the raw KaTeX output for any stripped nodes) rather
  than assuming zero-change up front.
- Sanitizer ordering unchanged: KaTeX output still flows through the same DOMPurify pass, unchanged
  position in the pipeline (defense-in-depth backstop, not the primary control) — this is the renderer's
  own generated markup, analogous to markdown-it's own `<h1>`/`<table>` output, not user-authored HTML
  being allowlisted through.

### 2.3 Wide-Formula Horizontal Scroll

- Block math (`$$...$$`, rendered as a `.katex-display` element) reuses the exact same horizontal-scroll
  treatment already applied to wide tables: `markdownIt.ts`'s `wrapTableOpen`/`wrapTableClose` pattern
  wraps `<table>` in `<div class="ttn-preview-table-wrap">` (plain `overflow-x: auto`, `Editor.css` line
  ~118). The block-math renderer rule (or a post-processing wrap step, if the chosen plugin has no
  rule-override point) wraps its output in an analogous `<div class="ttn-preview-math-wrap">`, sharing the
  same `overflow-x: auto` CSS declaration — either a second selector on the existing rule
  (`.ttn-preview-table-wrap, .ttn-preview-math-wrap { overflow-x: auto; }`) or a shared class both
  wrappers apply. `"class"` is already DOMPurify-allowlisted (§2.2), no sanitizer change needed for the
  new wrapper class.
- **Inline math (`$...$`) is not wrapped** — reflows/overflows its containing paragraph like any other
  long inline content, mirroring how the wide-table treatment itself only applies at block level.

### 2.4 Asset Bundling (Offline Requirement)

- KaTeX's stylesheet (`katex.min.css`) and font files (`woff2`/`woff`/`ttf`) must be bundled into the Vite
  build, never CDN-loaded — a static `import "katex/dist/katex.min.css";` inside
  `src/editor/preview/markdownIt.ts` (or `PreviewPane.tsx`), which Vite already bundles for every other
  CSS import in this codebase (`EditorPane.tsx`'s own `import "./Editor.css";` is the existing precedent —
  no new build-config work implied). No existing CDN-link pattern exists anywhere in `index.html`/
  `src/main.tsx` today, so this is "don't introduce one," not "remove one." Verify at implementation time
  by disabling network access and confirming formulas still render with correct glyphs/styling.

## 3. Live-View Marker (`MathPlaceholderWidget`)

Elaborates `[detailed-spec v1.2.0 §2.3]`. New `MathPlaceholderWidget extends WidgetType` in
`src/editor/markdownDecorations.ts`, following `ImagePlaceholderWidget`'s own precedent one-for-one:

- **Glyph**: the `Sigma` icon from `lucide-react` (already a bundled-locally dependency, `[detailed-spec
  v1.2.0 §2.3]`'s own decision) — since `WidgetType.toDOM()` builds a raw CodeMirror-owned DOM node
  (React components cannot render inside it), use `lucide`'s vanilla-JS/SVG-string export rather than the
  `lucide-react` component wrapper, the same constraint any CodeMirror widget icon already has.
- **Markup**: `toDOM()` returns `<span class="cm-md-math-placeholder">{Sigma SVG} formula</span>`
  (exact inner label text `developer-lite` discretion, e.g. literal " formula" or the matched delimiter
  content truncated — not spec-mandated beyond "short, non-interactive badge/marker").
- **Non-interactive**: no click handler registered on this widget at all — unlike `ImageWidget`'s
  click-to-cursor `mousedown` handler, clicking the math badge does nothing special.
- **Wiring**: `Decoration.replace({ widget: new MathPlaceholderWidget(...) })` range, built inside the
  same `MarkdownDecorationsPlugin.build()` pass (governed by `liveViewCompartment`, active only while live
  view is on) that already produces `ImageWidget`/other live-view decorations. Scans for `$...$`/`$$...$$`
  spans using §2.1's delimiter heuristic, applying the same code-fence/inline-code exclusion
  (`codeSkipRanges`, §4 below) already applied to `findImageTags` — a literal `$...$`-looking string
  inside a fenced code block is never marked in either live-view or source-view mode.

## 4. Source-View Marker (`SourceMathMarkerPlugin`)

Elaborates `[detailed-spec v1.2.0 §2.4]`. Reuses the existing `SourceImageMarkerPlugin`/
`sourceImageMarkerCompartment` precedent (`src/editor/markdownDecorations.ts` lines ~527–605,
`src/editor/codeMirrorSetup.ts`) — active precisely while live view is off.

- **New constant**: `SOURCE_MATH_MARKER_MARK = Decoration.mark({ class: "cm-md-source-math-marker" })`
  (mirroring `SOURCE_IMAGE_MARKER_MARK` one-for-one).
- **Two implementation shapes, `developer-lite` discretion** (`[spec v1.2.0 §2.4]`'s own explicit
  delegation):
  - (a) extend the existing `SourceImageMarkerPlugin.build()`'s `syntaxTree(state).iterate` switch with a
    new case, renaming exports to something less image-specific; or
  - (b) a sibling `SourceMathMarkerPlugin` class, identical `PluginValue` shape (`decorations:
    DecorationSet`, `build(state)`, `docChanged`-gated rebuild), wired through its own new
    `sourceMathMarkerCompartment` in `codeMirrorSetup.ts`, mirroring `sourceImageMarkerCompartment`'s own
    declaration and `.of(sourceImageMarkerViewExtension(!liveViewEnabled))` wiring one-for-one.
- **Detection is a regex scan, not a lezer-node walk**: `$...$`/`$$...$$` spans are not first-class lezer
  syntax-tree nodes (unlike `Image`), so — mirroring `findImageTags`'s own regex-based approach for raw
  `<img>` tags — this is a raw-text regex scan applying the same `codeSkipRanges` exclusion (built from
  `InlineCode`/`FencedCode`/`CodeBlock` node ranges) `SourceImageMarkerPlugin.build()` already computes,
  plus §2.1's delimiter-boundary heuristic (reused identically so live-view widget locations and
  source-view marker locations always agree on what counts as "a math span").
- **CSS**: `.cm-md-source-math-marker { background: var(--ttn-bg-math-marker); border-radius: 3px; }` plus
  a `::before { content: "..." }` icon (reusing §3's Sigma glyph, encoded as a text/Unicode equivalent or
  small inline SVG for the `::before` pseudo-element, since CSS can't embed the `lucide` SVG export
  directly — a small, self-contained glyph choice consistent with the mark-decoration's own CSS-only
  nature) — added to `src/components/Editor/Editor.css` immediately after `.cm-md-source-image-marker`.
- **New CSS custom property**: `--ttn-bg-math-marker`, added to `src/theme.css` alongside the three
  existing `--ttn-bg-image-marker` definitions (once per theme variant, lines ~57/99/135) — a **new,
  distinct** color value, not a reuse of the image marker's RGBA, per the direct user request for visual
  distinguishability when both marker kinds appear in the same memo.

## 5. Testing / Verification

- Inline `$x^2 + y^2 = z^2$` and block `$$\int_0^\infty e^{-x}dx = 1$$` render as typeset formulas in
  preview pane (`edit`→`preview`, `split` mode).
- Currency false-positive: `Price: $5, not $10` does not render as math in preview.
- Live view: math span shows the Σ badge, non-interactive (click does nothing beyond normal cursor
  placement), raw delimiter text unchanged in the underlying buffer (toggle to source view confirms).
- Source view: math span shows the mark-decoration background/icon, underlying text remains fully
  selectable/editable — visually distinct color from an adjacent image marker in the same memo.
- Math span inside a fenced code block (` ```\n$x$\n``` `): no live-view badge, no source-view marker, no
  preview rendering (renders as literal code text).
- Wide block formula: horizontal scrollbar appears in preview, no font-size auto-shrink.
- Disable network access, restart app, open a memo with a formula in preview: confirm correct glyph
  rendering (no missing-font/unstyled fallback), verifying §2.4's bundling requirement.
- DOMPurify allowlist check: render a formula, inspect the sanitized output for any unexpectedly-stripped
  KaTeX-generated nodes (confirms whether §2.2's "little to no new allowlist entry" prediction holds).
