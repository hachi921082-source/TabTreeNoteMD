# Markdown Preview / PDF Export / Live-View Image Rendering: Detailed Specification

This document elaborates `docs/spec.md` §4.2 and `docs/detailed-spec.md` §5.10 to the same level of
detail `detailed-spec.md` normally provides, covering the new read-only rendered-HTML preview pane, PDF
export, and real `<img>` rendering in the existing live-view. Kept as a standalone top-level file,
matching `docs/tab-lock-detailed-spec.md`'s convention.

**Read `docs/detailed-spec.md` §5.10 first.** It covers the pre-existing Markdown-editing material this
document builds on and does not repeat: the CodeMirror 6 decoration-engine architecture, the live/source
toggle, the 16-slot toolbar layout and icon set, TOC generation (`src/editor/tocGenerator.ts`), the
Color/Font-size palettes, and the existing narrow sanitization allowlist (`src/editor/markdownSanitize.ts`).
This document is strictly additive to that material — the decoration engine itself is unchanged.

## Index

Keep this in sync whenever a section is added, renamed, or moved below.

- **§1 Scope and Non-Goals** — what this feature adds; confirms the decoration engine is kept as-is.
- **§2 Pane Layout and Toggles** — the two independent toggles (live/source, and the 3-state
  edit/split/preview pane-mode cycle), split ratio, theming, auto-reveal principle, context-menu
  suppression.
- **§3 Settings Schema Additions** — `MemoEditorSettings.paneMode`, `AppSettings.editorDefaults`
  fallback, `AppSettings.preview.debounceMs` and its Settings-dialog UI.
- **§4 Preview Rendering** — renderer choice, debounce trigger, `html:false` + regex-preprocessed
  span/image allowlist + DOMPurify backstop (no broad HTML allowlist), shared image pipeline, concrete
  `data-source-line` scroll-sync, heading anchors, anchor-jump click handling, checkbox click write-back,
  link click behavior, tables, code fences, images. Implementation plan:
  `docs/plans/markdown/markdown-preview/PLAN.md`.
- **§5 Live-View Real Image Rendering** — backend byte-read command with its own server-side path
  validation, Blob URL flow, security posture, display cap, lazy loading, Blob-URL cache with concrete
  eviction triggers, failure fallback, source-mode interaction. Implementation plan:
  `docs/plans/markdown/image-handling/PLAN.md`.
- **§6 PDF Export** — hidden-iframe + `print()` mechanism, shared image pipeline, wait-for-images-loaded
  before printing, print stylesheet, filename hint, availability. Implementation plan:
  `docs/plans/markdown/pdf-export/PLAN.md`.
- **§7 Floating Toolbar Interaction** — clamp-bounds widening, auto-minimize when pane-mode is
  `"preview"`. Implementation plan: `docs/plans/markdown/markdown-preview/PLAN.md`.
- **§8 Keyboard Shortcuts and New Toolbar-Header Buttons** — including the `Ctrl+Shift+P` 3-state
  pane-mode cycle and the `Ctrl+P` native-print conflict fix.
- **§9 Deferred Items and Open Verification** — pointers to `docs/QA.md` and to the three
  `docs/plans/markdown/` implementation plans that elaborate this document.

---

## 1. Scope and Non-Goals

Elaborates `[spec §4.2]`. This feature adds, on top of the existing editable pane:

1. A second, independent, read-only rendered-HTML **preview pane**, shown alongside the editable pane.
2. A **PDF export** capability that prints the same rendered HTML the preview pane shows.
3. **Real `<img>` rendering** in the existing live-view, replacing today's text-only
   `ImagePlaceholderWidget` (`src/editor/markdownDecorations.ts`).

**Non-goal, explicitly confirmed**: this is not a WYSIWYG rewrite. `src/editor/markdownDecorations.ts`
and the rest of the CodeMirror 6 decoration engine (`[detailed-spec §5.10]`) are kept unchanged — the
plain-text buffer remains the sole source of truth, and the live/source toggle's behavior is untouched.

## 2. Pane Layout and Toggles

- **Two independent toggles**: this independence is about keeping the *live/source* toggle orthogonal
  from the *pane-mode* toggle described below — it does not mean either toggle is itself binary.
  1. The existing live/source toggle inside the editable pane (`MemoEditorSettings.viewMode`,
     `[detailed-spec §5.10]`) — unchanged, no schema change from this feature.
  2. A new **pane-mode toggle** (§3): a single button that cycles through three states in a fixed order,
     `edit → split → preview → edit → ...`:
     - `edit` — today's default: only the editable pane, no preview (equivalent to the old "off").
     - `split` — both panes shown, 50/50 (see "Split ratio" below).
     - `preview` — the editable pane hidden, only the preview pane shown ("preview only").
- **Split ratio**: when pane-mode is `split`, fixed **50/50**. No drag-resizable divider between them.
- **Theming**: the preview pane is a normal DOM node in the same document (not an on-screen iframe), so
  it inherits the app's theme automatically via the existing `data-theme` CSS custom properties
  (`src/theme.css`) with no new theme-detection code.
- **Editable-pane show/hide is CSS-only**: hiding the editable pane (pane-mode `preview`) sets
  `display:none` on its container. The CodeMirror `EditorView` instance in
  `src/components/Editor/EditorPane.tsx` must never be unmounted/recreated for this — undo history,
  cursor position, and the pane's existing scroll-restore logic (`computeTopVisibleLine`/
  `scrollToTopVisibleLine`, `src/editor/textCommands.ts`) must survive pane-mode changes. Because the
  `EditorView` is never unmounted, the live/source toggle's own state is likewise preserved unaffected
  by pane-mode changes, including while pane-mode is `preview`.
- **Auto-reveal principle**: any action that expects to focus/scroll/mutate the CodeMirror view while
  pane-mode is currently `preview` must first auto-switch pane-mode to `split`, then perform the action,
  so the result is visible. Applies to: Ctrl+F (in-memo search open), tree-row click auto-focus,
  cross-tab search-result jump, `memo:` link-click navigation, every toolbar formatting button
  (`src/editor/decorationCommands.ts` — bold/heading/list/link/quote/codeBlock/table/etc.), and the
  **live/source toggle button** — clicking it while pane-mode is `preview` first auto-switches pane-mode
  to `split` (not any other state, since the edit pane must become visible to show the toggle's effect),
  then performs the live/source toggle. None of these are disabled or silently no-op while pane-mode is
  `preview`; they reveal-then-act.
- **Context menu inside the preview-only area**: the opposite principle applies — right-click shows no
  context menu at all while hovering the preview pane when pane-mode is `preview` (fully suppressed, not
  auto-reveal-then-show). Plain-text copy still works via native Ctrl+C text selection in the rendered
  HTML.

## 3. Settings Schema Additions

Elaborates `[spec §3.4]` / `[detailed-spec §3.2]`.

- **`MemoEditorSettings.paneMode?: "edit" | "split" | "preview" | null`** — mirrors `viewMode`'s existing
  pattern exactly: per-memo override, `null`/absent falls back to a new app-wide default. **Persistence
  timing**: immediate on toggle (no debounce), via the exact same per-memo-override write path
  `viewMode` already uses today — a direct call to `setEditorOverride` on every toggle
  (`src/components/Editor/EditorPane.tsx` lines 148–173), not a debounced write. This is unrelated
  to the separate `AppSettings.preview.debounceMs` below, which only controls preview RE-RENDER
  timing, not settings persistence.
- **`AppSettings.editorDefaults.paneMode: "edit" | "split" | "preview"`** — new field alongside the
  existing `wrap` / `tabWidth` / `showLineNumbers` / `fontSize` members of `editorDefaults`
  (`[spec §3.4]`); default `"edit"`; the app-wide default a memo without its own override falls back to.
- **`AppSettings.preview.debounceMs: number`** — new top-level settings section (sibling of `autosave`),
  matching `autosave.plainDebounceMs`'s schema shape:

  ```jsonc
  "preview": { "debounceMs": 150 }
  ```

  - Default `150`, range **50–2000**, step `50` (tighter than autosave's 200–10000 range since this
    involves no disk I/O).
  - `AppSettingsPatch` gains the matching partial shape, merged the same way every other settings patch
    is (`[detailed-spec §3.2]`).
  - **Settings-dialog UI**: same Editor tab as the existing autosave-delay control in
    `src/components/Settings/SettingsDialog.tsx`, same `<input type="number" min={50} max={2000}
    step={50}>` bound directly to `patchSettings({ preview: { debounceMs: Number(e.target.value) } })`,
    immediate write, no separate Apply step — identical pattern to the existing
    `autosave.plainDebounceMs` control in that file.
- These are two independent debounce timers for two independent concerns: preview re-render debounce
  must never be tied to autosave's completion event or its debounce value.

## 4. Preview Rendering

Elaborates `[spec §4.2]`.

- **Renderer**: a full Markdown→HTML library (e.g. markdown-it), covering CommonMark+GFM equivalent to
  what `@lezer/markdown`'s GFM extension already parses for the live-view (tables, task lists,
  strikethrough, autolinks) — kept at parity with the live-view's supported syntax range.
- **Re-render trigger**: a short, user-configurable debounce (`AppSettings.preview.debounceMs`, §3) after
  the document changes, hooked to the same live doc-change event the live-view decorations already use
  (CodeMirror's `updateListener`/doc-changed signal) — never the autosave-completion event.
- **Raw-HTML/sanitizer boundary**:
  - The renderer (e.g. markdown-it) is configured with `html: false` — raw inline/block HTML typed
    literally in the user's Markdown source is auto-escaped to literal visible text, matching the
    live-view's existing behavior (the decoration engine never executes or strips arbitrary HTML).
  - The two allowlisted constructs — `<span style="color/font-size">...</span>` and
    `<img src width height>` — are NOT handled via markdown-it's own HTML parsing. They are
    pre-processed using the exact same regex-based extraction the live-view already uses
    (`src/editor/markdownSpanParser.ts`'s `findColorSpans`/`findImageTags`, with
    `filterStyleDeclarations`/`isSafeRelativeImageSrc` from `src/editor/markdownSanitize.ts`), and
    converted directly into safe, code-constructed DOM/HTML elements — never round-tripped through a
    general HTML allowlist. No broad, hand-maintained HTML allowlist is needed as a result.
  - DOMPurify still wraps the final combined HTML output as a defense-in-depth backstop, not the
    primary control.
- **Shared image-loading pipeline**: the preview pane's `<img>` elements are loaded through the exact
  same pipeline as the live-view's real image rendering (§5) — the same backend byte-read Tauri command
  and the same frontend Blob-URL cache (keyed by relative path). This is a single shared image-loading
  module used by all three rendering contexts: the live-view CodeMirror widget, the preview pane, and
  the PDF export iframe (§6) — not three separate implementations.
- **Scroll-sync**: the renderer's tokens carry a `token.map` giving each block's `[startLine, endLine)`
  source-line range; each top-level rendered block element (paragraph/heading/list-item/table/etc.)
  gets a `data-source-line` attribute set to `token.map[0]`. Scroll-sync finds the rendered block whose
  `data-source-line` is the closest value ≤ the editable pane's current top-visible line (from
  `computeTopVisibleLine`, `src/editor/textCommands.ts`) and scrolls the preview container to that
  element (`scrollIntoView` or setting `scrollTop` to the element's `offsetTop`).
- **Anchor-jump handling**: clicking a `#slug` heading-anchor link inside the preview pane is
  intercepted (`event.preventDefault()`) and resolved by calling `scrollIntoView({block:"start"})` on
  the matching `id` element within the preview pane's own internal scroll container.
- **Heading anchor IDs**: the new renderer's generated heading `id` attributes (for `#slug` anchor-link
  support, `[detailed-spec §5.10]` "Slug generation") must use the exact same slugify algorithm as
  `src/editor/tocGenerator.ts`'s `scanHeadings` — share the literal function rather than reimplementing
  it, so `#slug` links resolve identically in both panes.
- **Checkbox click write-back**: clicking a rendered GFM task-list checkbox in the preview toggles
  `[ ]`⇄`[x]` at the correct character offset in the CodeMirror source. This needs its own
  DOM-node-to-source-range position mapping, analogous to (but a distinct implementation from) the
  existing `src/editor/checkboxClick.ts` used by the live-view.
- **Link clicks**: clicks on the preview's real `<a>` tags (both `memo:` and external links) re-implement
  the same open-trigger behavior (`AppSettings.links.openTrigger`: singleClick/doubleClick/ctrlClick) and
  external-link confirmation-dialog logic that `src/editor/links.ts` already provides for the live-view —
  same user-facing behavior, new call site.
- **Wide tables**: wrap in a horizontal-scroll container (`overflow-x: auto`); never auto-shrink font
  size to fit.
- **Code fences**: render as plain monospace text with a background color only, matching the live-view's
  current `cm-md-codeblock` treatment — no syntax-highlighting library added in this pass.
- **Images in the preview**: `max-width: 100%; height: auto;` (relative to the pane/column width —
  shrink-to-fit only, never upscale, aspect ratio always preserved). Deliberately different from the
  live-view's own image display cap (§5); must never bleed off a printed page edge (§6).

## 5. Live-View Real Image Rendering

Elaborates `[spec §4.2]`; closes the scope/mechanism question in `docs/QA.md`'s "Real `<img>` Rendering +
Drag-Resize-Handle in Live View" entry (the image-rendering half of that question — the drag-resize-handle
half remains deferred, §9).

- **Backend command**: a new Tauri command reads an image file's bytes and returns them as a raw byte
  response via `tauri::ipc::Response` (Tauri 2.11.5 feature), not base64-encoded.
- **Frontend flow**: wraps the returned bytes in a `Blob`, calls `URL.createObjectURL()` to obtain a
  `blob:` URL, and sets it as the `<img src>`.
- **Security posture unchanged**: this does not widen the Tauri `assetProtocol` scope to arbitrary
  runtime-chosen folders — keeps the app's existing conservative posture (cf.
  `src/editor/markdownSanitize.ts`'s `isSafeRelativeImageSrc`, which already rejects absolute paths,
  traversal, and other URL schemes).
- **Backend path validation (required, independent of the frontend check)**: the new Tauri command must
  perform its own server-side path validation, independent of and not relying on
  `isSafeRelativeImageSrc` — an IPC command can be invoked directly, bypassing any frontend gate.
  Reject absolute paths, `..` traversal segments, and any path that resolves outside the memo's own
  attachment/image directory. This project already has established conventions for this: reuse the
  existing path-safety pattern used by other file-reading commands rather than writing a new one from
  scratch — see `src-tauri/src/commands/link.rs`'s `resolve_file_path`/`has_parent_dir_segment` (which
  already rejects `..` segments and rooted-but-not-absolute paths for the same class of problem). The
  implementer should search `src-tauri/` for this and any other existing path-safety helper before
  writing new validation logic.
- **Replaces** `src/editor/markdownDecorations.ts`'s `ImagePlaceholderWidget` (currently a text-only
  `🖼 alt-or-src` placeholder) with a real `<img>` element.
- **Display cap**: CSS `max-width` + `max-height` with `width:auto`/`height:auto` (aspect ratio always
  preserved) to a small fixed size — a purely cosmetic cap for the editing view, keeping large photos
  from dominating the text-editing screen. Must not touch the image's stored `width`/`height` values in
  the Markdown/HTML source.
- **Lazy loading**: no dedicated new mechanism needed — CodeMirror 6 only calls a widget's `toDOM()` for
  widgets that actually enter the rendered viewport, so triggering the byte-fetch from inside `toDOM()`
  (rather than eagerly during decoration-set build) is lazy by construction.
- **Blob-URL cache**: a path-keyed `Map`, shared across the live-view, preview pane, and PDF export
  iframe (§4, §6), to avoid redundant IPC reads for the same image path appearing more than once in a
  document and to avoid a visible reload flash when a memo is closed and reopened. Eviction (via
  `URL.revokeObjectURL()`) triggers on: (a) the owning memo being closed — revoke all of that memo's
  cached entries; and (b) the shared cache exceeding a fixed size bound — evict oldest/least-recently-
  used entries, LRU-style. No special handling is needed at app shutdown; the OS reclaims the underlying
  memory automatically.
- **Read-failure fallback**: on a read failure (file missing/moved), fall back to rendering the existing
  placeholder widget/text rather than a broken native `<img>` icon.
- **Source-view mode unaffected**: source mode disables the whole decoration `Compartment` (returns
  `[]`), so images — like every other construct — render as literal raw text there, unchanged.

## 6. PDF Export

- **Mechanism**: build a hidden `<iframe>`, feed it the same generated HTML string used for the on-screen
  preview (§4) plus its own minimal, self-contained print stylesheet (independent of the rest of the
  app's CSS), then call `iframe.contentWindow.print()`. This routes through the OS's native print dialog
  (e.g. "Microsoft Print to PDF" on Windows) rather than a headless direct-to-file API.
- **Image pipeline reused, not reimplemented**: the iframe's `<img>` elements are loaded through the
  same shared image-loading pipeline described in §4/§5 (same backend byte-read command, same Blob-URL
  cache) — not a third, separate image-loading implementation.
- **Wait for images before printing**: before calling `iframe.contentWindow.print()`, the export flow
  must wait for every `<img>` element inside the iframe's document to finish loading (e.g. `Promise.all`
  over each image's load event or `.decode()`), with a reasonable per-image timeout (e.g. ~3 seconds) so
  one broken or slow image cannot hang PDF export indefinitely and the printed PDF doesn't end up missing
  images.
- **Print CSS**: sets margins only; does not set an explicit `@page` size — paper size (A4/Letter/etc.)
  is left to the OS/printer's own default, respecting the user's locale/printer setup.
- **Filename hint**: before calling `print()`, set the hidden iframe's own `document.title` to the memo's
  name, so the OS "Save as PDF" file-save dialog suggests a sensible default filename instead of the
  app's generic window title.
- **Availability**: PDF export is available unconditionally regardless of the memo's edit-lock/read-only/
  conflict-dialog state, since printing never mutates the document.

## 7. Floating Toolbar Interaction

Elaborates `src/components/Editor/Toolbar/MarkdownToolbar.tsx`'s interaction with the new preview pane.

- **Clamp-bounds widening**: `clampToolbarPosition`'s drag-clamp bounds are currently measured against
  `panelRef.current.offsetParent`, i.e. the edit sub-pane alone. This must be widened to clamp against
  the outer container that holds both the edit and preview sub-panes combined — otherwise, once pane-mode
  `preview` sets the edit sub-pane to `display:none` (a 0×0 box), the toolbar's position math breaks. By
  design, this also means the toolbar can be dragged so it visually floats over the preview pane — its
  buttons act on the CodeMirror view via `getView()` regardless of where the toolbar sits on screen, so
  this is a pure bounds/visual change with no functional effect.
- **Auto-minimize when pane-mode is `"preview"`**: the toolbar must not be fully hidden in this mode (its
  header holds the pane-mode toggle, §8). Instead, pane-mode becoming `"preview"` automatically triggers
  the toolbar's existing "minimized" state (header-only, grid-collapsed, corner-snapped, from GitHub
  issue #26) — reusing existing code; only the clamp-bounds reference above needs fixing.

## 8. Keyboard Shortcuts and New Toolbar-Header Buttons

- **New default shortcuts**, added to `src/shared/shortcuts.ts`'s `DEFAULT_SHORTCUTS` table (both combos
  verified unused in the existing table):
  - Pane-mode toggle (cycles `edit → split → preview → edit`, same action as clicking the header toggle
    button, §2): `Ctrl+Shift+P`
  - PDF export: `Ctrl+P`
- **New toolbar-header buttons**: the pane-mode toggle and the PDF export trigger are not among the
  existing fixed 16 toolbar grid slots (`src/editor/toolbarSlots.ts` — spec-closed, no add/remove/hide,
  `[detailed-spec §5.10]`). Both new buttons live in the toolbar's header area instead, the same place the
  existing live/source view-mode toggle button already lives (outside the 16-slot grid).
- **`Ctrl+P` native-print conflict**: `Ctrl+P` is also WebView2/Chromium's built-in browser-print
  shortcut. The PDF-export keydown handler must call `event.preventDefault()` before triggering the
  app's own hidden-iframe export flow (§6), to prevent the WebView's native print dialog from appearing
  instead of (or alongside) the app's PDF export.

## 9. Deferred Items and Open Verification

The following are explicitly deferred (tracked as open, separate future decisions — not rejected, not
bundled into this feature's implementation). They should be recorded in `docs/QA.md` alongside its
existing "Real `<img>` Rendering + Drag-Resize-Handle in Live View" entry:

- Image drag-to-resize handle (would rewrite `![alt](path)` → `<img src width height>` on drag).
- Click-to-enlarge lightbox preview for images.
- Downscaling/re-encoding images at paste time to save disk space.

**Open verification item** (not yet tested): confirm that `blob:` object URLs created in the main window
(§5) actually resolve correctly when the same HTML is re-inserted into the separate hidden iframe's
document used for PDF export (§6) — this can be browser/WebView2-version-dependent and should be verified
once implemented, with a fallback (re-fetch bytes fresh for the print-specific HTML) if it doesn't.

Implementation planning for this document is complete, split across three `developer-lite`-authored
plans, each elaborating the sections noted:

- `docs/plans/markdown/image-handling/PLAN.md` — shared image-loading pipeline + live-view real `<img>`
  rendering (§5).
- `docs/plans/markdown/markdown-preview/PLAN.md` — the preview pane itself (§1–§4, §7).
- `docs/plans/markdown/pdf-export/PLAN.md` — PDF export (§6).

Actual implementation is the next step after this document.
