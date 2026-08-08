# Markdown Preview / PDF Export / Live-View Image Rendering: Detailed Specification

This document elaborates `docs/spec.md` §4.2 and `docs/detailed-spec.md` §5.10 to the same level of
detail `detailed-spec.md` normally provides. It was written as a standalone top-level file (rather than
appended to §5.10 in place) because it covers a large, self-contained addition to Markdown editing — a
new read-only rendered-HTML preview pane, PDF export, and real `<img>` rendering in the existing
live-view — mirroring the split-out convention already used by `docs/tab-lock-detailed-spec.md`.

**Read `docs/detailed-spec.md` §5.10 first.** It covers the pre-existing Markdown-editing material this
document builds on and does not repeat: the CodeMirror 6 decoration-engine architecture, the live/source
toggle, the 16-slot toolbar layout and icon set, TOC generation (`src/editor/tocGenerator.ts`), the
Color/Font-size palettes, and the existing narrow sanitization allowlist (`src/editor/markdownSanitize.ts`).
This document is strictly additive to that material — the decoration engine itself is unchanged.

## Index

Keep this in sync whenever a section is added, renamed, or moved below.

- **§1 Scope and Non-Goals** — what this feature adds; confirms the decoration engine is kept as-is.
- **§2 Pane Layout and Toggles** — the two independent toggles, split ratio, theming, auto-reveal
  principle, context-menu suppression.
- **§3 Settings Schema Additions** — `MemoEditorSettings.previewVisible`, `AppSettings.editorDefaults`
  fallback, `AppSettings.preview.debounceMs` and its Settings-dialog UI.
- **§4 Preview Rendering** — renderer choice, debounce trigger, sanitize allowlist, scroll-sync, heading
  anchors, checkbox click write-back, link click behavior, tables, code fences, images.
- **§5 Live-View Real Image Rendering** — backend byte-read command, Blob URL flow, security posture,
  display cap, lazy loading, Blob-URL cache, failure fallback, source-mode interaction.
- **§6 PDF Export** — hidden-iframe + `print()` mechanism, print stylesheet, filename hint, availability.
- **§7 Floating Toolbar Interaction** — clamp-bounds widening, auto-minimize in preview-only mode.
- **§8 Keyboard Shortcuts and New Toolbar-Header Buttons**.
- **§9 Deferred Items and Open Verification** — pointers to `docs/QA.md` and the next
  `docs/plans/` step (not created by this document).

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

- **Two independent toggles**, not a combined tri-state:
  1. The existing live/source toggle inside the editable pane (`MemoEditorSettings.viewMode`,
     `[detailed-spec §5.10]`) — unchanged, no schema change from this feature.
  2. A new preview show/hide toggle (§3).
- **Split ratio**: when both panes are shown, fixed **50/50**. No drag-resizable divider between them.
- **Theming**: the preview pane is a normal DOM node in the same document (not an on-screen iframe), so
  it inherits the app's theme automatically via the existing `data-theme` CSS custom properties
  (`src/theme.css`) with no new theme-detection code.
- **Editable-pane show/hide is CSS-only**: hiding the editable pane (when the user selects "preview
  only") sets `display:none` on its container. The CodeMirror `EditorView` instance in
  `src/components/Editor/EditorPane.tsx` must never be unmounted/recreated for this — undo history,
  cursor position, and the pane's existing scroll-restore logic (`computeTopVisibleLine`/
  `scrollToTopVisibleLine`, `src/editor/textCommands.ts`) must survive toggling preview visibility.
- **Auto-reveal principle**: any action that expects to focus/scroll/mutate the CodeMirror view while
  currently in "preview only" mode must first auto-switch to split view, then perform the action, so the
  result is visible. Applies to: Ctrl+F (in-memo search open), tree-row click auto-focus, cross-tab
  search-result jump, `memo:` link-click navigation, and every toolbar formatting button
  (`src/editor/decorationCommands.ts` — bold/heading/list/link/quote/codeBlock/table/etc.). None of these
  are disabled or silently no-op while preview-only; they reveal-then-act.
- **Context menu inside the preview-only area**: the opposite principle applies — right-click shows no
  context menu at all while hovering the preview pane in preview-only mode (fully suppressed, not
  auto-reveal-then-show). Plain-text copy still works via native Ctrl+C text selection in the rendered
  HTML.

## 3. Settings Schema Additions

Elaborates `[spec §3.4]` / `[detailed-spec §3.2]`.

- **`MemoEditorSettings.previewVisible?: boolean | null`** — mirrors `viewMode`'s existing pattern
  exactly: per-memo override, `null`/absent falls back to a new app-wide default.
- **`AppSettings.editorDefaults.previewVisible: boolean`** — new field alongside the existing `wrap` /
  `tabWidth` / `showLineNumbers` / `fontSize` members of `editorDefaults` (`[spec §3.4]`); the app-wide
  default a memo without its own override falls back to.
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
- **Sanitize allowlist**: a strict superset of the existing narrow allowlist in
  `src/editor/markdownSanitize.ts` (currently `<span style="color/font-size">` and relative-path
  `<img src/width/height>` only, `[detailed-spec §5.10]`). The new, broader allowlist adds headings,
  lists, tables, blockquote, code, links, etc., but must still include every entry of the existing narrow
  allowlist unchanged, so a memo's color spans and images don't visually differ between live-view and
  preview.
- **Scroll-sync**: when both panes are visible, the preview pane's scroll position is kept in sync with
  the editable pane using the same line-based approach as the reused
  `computeTopVisibleLine`/`scrollToTopVisibleLine` primitives (`src/editor/textCommands.ts`).
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
  shrink-to-fit only, never upscale, aspect ratio always preserved). This is deliberately different from
  the live-view's own image display cap (§5) — the preview represents the "final look" that PDF export
  (§6) prints verbatim, so images should use the full available width and must never bleed off a printed
  page edge.

## 5. Live-View Real Image Rendering

Elaborates `[spec §4.2]`; closes the scope/mechanism question in `docs/QA.md`'s "Real `<img>` Rendering +
Drag-Resize-Handle in Live View" entry (the image-rendering half of that question — the drag-resize-handle
half remains deferred, §9).

- **Backend command**: a new Tauri command reads an image file's bytes and returns them as a raw byte
  response via `tauri::ipc::Response` (Tauri 2.11.5 feature) — explicitly not base64-encoded, avoiding the
  ~33% size inflation and JSON-string round-trip cost of a base64 payload.
- **Frontend flow**: wraps the returned bytes in a `Blob`, calls `URL.createObjectURL()` to obtain a
  `blob:` URL, and sets it as the `<img src>`.
- **Security posture unchanged**: this does not widen the Tauri `assetProtocol` scope to arbitrary
  runtime-chosen folders — keeps the app's existing conservative posture (cf.
  `src/editor/markdownSanitize.ts`'s `isSafeRelativeImageSrc`, which already rejects absolute paths,
  traversal, and other URL schemes).
- **Replaces** `src/editor/markdownDecorations.ts`'s `ImagePlaceholderWidget` (currently a text-only
  `🖼 alt-or-src` placeholder) with a real `<img>` element.
- **Display cap**: CSS `max-width` + `max-height` with `width:auto`/`height:auto` (aspect ratio always
  preserved) to a small fixed size — a purely cosmetic cap for the editing view, keeping large photos
  from dominating the text-editing screen. Must not touch the image's stored `width`/`height` values in
  the Markdown/HTML source.
- **Lazy loading**: no dedicated new mechanism needed — CodeMirror 6 only calls a widget's `toDOM()` for
  widgets that actually enter the rendered viewport, so triggering the byte-fetch from inside `toDOM()`
  (rather than eagerly during decoration-set build) is lazy by construction.
- **Blob-URL cache**: a path-keyed `Map`, evicting old entries via `URL.revokeObjectURL()` with an upper
  bound, to avoid redundant IPC reads for the same image path appearing more than once in a document and
  to avoid a visible reload flash when a memo is closed and reopened.
- **Read-failure fallback**: on a read failure (file missing/moved), fall back to rendering the existing
  placeholder widget/text rather than a broken native `<img>` icon.
- **Source-view mode unaffected**: source mode disables the whole decoration `Compartment` (returns
  `[]`), so images — like every other construct — render as literal raw text there, unchanged.
- **Root-cause note** (context for future readers, not a requirement): the old reference-only repository
  (`TabTreeNote`) had a critical bug (its own `AGENT_LOG.md`, item `M10c-T1-01`) where pasting an image
  over 500KB froze the renderer for minutes, because that app embedded pasted images as inline base64
  data URIs directly in the Markdown text (one enormous line), which its link-detection regex then
  scanned inefficiently. TabTreeNoteMD's image mechanism only ever stores a short relative-path reference
  (`![alt](img_xxxx.png)`, bytes saved to disk separately via `image_save_pasted`), so this exact failure
  mode cannot recur structurally. The new preview's markdown-it-based renderer (§4) is new code, though,
  and should get a stress test against a pathological single huge line before shipping, as a precaution.

## 6. PDF Export

- **Mechanism**: build a hidden `<iframe>`, feed it the same generated HTML string used for the on-screen
  preview (§4) plus its own minimal, self-contained print stylesheet (independent of the rest of the
  app's CSS), then call `iframe.contentWindow.print()`. This routes through the OS's native print dialog
  (e.g. "Microsoft Print to PDF" on Windows) rather than a headless direct-to-file API.
- **Rationale for the iframe over a global `@media print` stylesheet** (load-bearing, kept for future
  readers): this app has many independent floating/absolute-positioned UI elements (the freely
  repositionable `MarkdownToolbar`, the tree pane, tab bar, dialogs, context menu) that a global print
  rule would have to remember to hide one-by-one, forever, as new UI is added. The iframe is immune to
  this by construction, since it is never fed any of that markup in the first place.
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
  the outer container that holds both the edit and preview sub-panes combined — otherwise, once
  "preview only" mode sets the edit sub-pane to `display:none` (a 0×0 box), the toolbar's position math
  breaks. By design, this also means the toolbar can be dragged so it visually floats over the preview
  pane — its buttons act on the CodeMirror view via `getView()` regardless of where the toolbar sits on
  screen, so this is a pure bounds/visual change with no functional effect.
- **Auto-minimize in preview-only mode**: the toolbar is not fully hidden while in "preview only" mode
  (that would also hide its header, where the new preview show/hide toggle button lives, §8 — creating a
  dead end with no way back to split view). Instead, entering "preview only" mode automatically triggers
  the toolbar's already-existing "minimized" state (the header-only, grid-collapsed, corner-snapped mode
  from GitHub issue #26) — reusing existing code; only the clamp-bounds reference above needs fixing.

## 8. Keyboard Shortcuts and New Toolbar-Header Buttons

- **New default shortcuts**, added to `src/shared/shortcuts.ts`'s `DEFAULT_SHORTCUTS` table (both combos
  verified unused in the existing table):
  - Preview pane show/hide toggle: `Ctrl+Shift+P`
  - PDF export: `Ctrl+P`
- **New toolbar-header buttons**: the preview show/hide toggle and the PDF export trigger are not among
  the existing fixed 16 toolbar grid slots (`src/editor/toolbarSlots.ts` — spec-closed, no add/remove/
  hide, `[detailed-spec §5.10]`). Both new buttons live in the toolbar's header area instead, the same
  place the existing live/source view-mode toggle button already lives (outside the 16-slot grid).

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

An eventual `docs/plans/<feature-name>/PLAN.md` implementation plan, created by `developer-lite`, is the
next step after this document.
