# PLAN: Markdown Editing (Live View, Toolbar, Sanitization)

> Status: implemented (`src/editor/codeMirrorSetup.ts`, `markdownDecorations.ts`,
> `markdownSpanParser.ts`, `markdownSanitize.ts`, `checkboxClick.ts`, `decorationCommands.ts`,
> `toolbarSlots.ts`, `imagePaste.ts`, `links.ts`; `components/Editor/Toolbar/MarkdownToolbar.tsx`;
> wired into `components/Editor/EditorPane.tsx`), except the parts this plan itself defers to
> sibling plans: `link/PLAN.md` (`memo:` resolution, email/file-path open special-casing) and
> `table-of-contents/PLAN.md` (heading scan/TOC insert-replace — the toolbar's "Update" slot is a
> labeled no-op) — both not yet implemented. `image-handling/PLAN.md` landed its backend command and
> real paste-save + insertion-markup flow (`imagePaste.ts` now actually calls `image_save_pasted`
> and inserts `![alt](path)`), but `markdownDecorations.ts`'s `ImagePlaceholderWidget` still renders
> a placeholder rather than the real image — real `<img>` rendering and the resize-handle
> interaction both need a new architectural decision (asset-protocol scope vs. a base64-read IPC
> command) that spans this plan and `image-handling/PLAN.md` without either specifying it; see
> `docs/QA.md`'s "Real `<img>` Rendering + Drag-Resize-Handle in Live View" entry. Layered on top of
> `editor-save/PLAN.md`'s plain-text editing surface. `eolKeeper.ts` (listed in §2 below) was not
> created — see Implementation Notes.

Sources: `docs/spec.md` §2.8, §4.2, §8; `docs/detailed-spec.md` §4.3, §5.10; `docs/DECISIONS.md`
"Editor Engine for Markdown Editing," "Markdown Toolbar: Finalized 16-Slot Fixed Grid Layout," "New
Feature: Table of Contents (TOC)."

## Implementation Notes (resolutions of this plan's discretion points)

- **Risk-reduction strategy for §9's high-risk defect zone**: every marker/styling decoration
  (headings, emphasis/strikethrough/inline-code, list/quote markers, blockquote/code-block/table
  line styling, link label + syntax styling) is a pure `Decoration.mark`/`Decoration.line` — CSS
  overlay only, marker text is never hidden. This sidesteps the "hidden marker vs. visible text
  length mismatch" cursor-mapping bug for that whole category rather than mitigating it after the
  fact. `Decoration.replace`/widgets are used only for task checkboxes and the two allowlisted
  `<span>`/`<img>` HTML fragments, both of which reveal raw source text whenever the current
  selection touches their range. All 5 of §9's required scenarios were exercised via `tauri dev` +
  WebView2 CDP (real `EditorView`, real `Input.insertText`/`Input.dispatchMouseEvent`, not just unit
  logic) and passed; one real bug was found and fixed in the process (see below).
- **Bug found and fixed during self-verification**: a `<span style="color:...">` wrapping a
  `[label](memo:...)` link was initially not being decorated at all — a single `skipRanges` list
  was reused both for "don't treat a `<span>`/`<img>` example inside a code fence as live" and for
  "don't double-detect a bare `memo:`/file-path token that's actually inside a link's URL," and a
  color span's range always overlaps its enclosed Link node's range, so the second use wrongly
  suppressed the first. Split into `codeSkipRanges` (code-only) and a separate
  `bareTokenSkipRanges` superset (code + Link/Image ranges), used only by the bare-token scan.
  Re-verified via CDP after the fix: the color mark and link mark now both render and the link
  stays independently clickable — confirmed §9 item 5 end to end.
- **Live-view engine built on `@lezer/markdown`'s real syntax tree** (via `@codemirror/lang-markdown`
  + the `GFM` extension for tables/task-lists/strikethrough/autolinks), not a hand-rolled
  line-by-line scan — this is what makes the mark-only approach above practical (correct heading/
  emphasis/list/blockquote/code-fence/table/link boundaries come straight from the parser) and
  incidentally gives bare `http(s)://`/`www.`/email autolink detection for free (GFM tokenizes these
  as `URL` syntax nodes), so `links.ts` only needs its own regex for the two token shapes the parser
  doesn't know about: bare `memo:...` and bare file paths.
- **Tables, code blocks, and blockquotes render as `Decoration.line` styling** (monospace/background/
  border-left per line), not a full interactive grid/widget — a pragmatic simplification; visually
  distinguishes these blocks per `[spec §4.2]` without the widget-replacement risk class.
- **List/heading markers are styled, not hidden** — `- `/`1. `/`#` markers stay visible with a dimmed/
  bold mark class rather than disappearing into a bullet glyph. Same risk-reduction rationale as
  above; a live-preview editor that hides markers entirely was considered but rejected for this
  first pass specifically because it would reintroduce the widget-replacement risk class for
  every heading/list line, not just the two spots (checkboxes, allowlisted HTML) where it's
  actually required by spec.
- **Bold/italic/strikethrough toolbar buttons toggle-wrap** the selection; a known low-severity edge
  case (not spec-mandated to resolve) is a naive single-`*` italic toggle can misfire at the boundary
  of an already-bold `**text**` selection. Documented in `decorationCommands.ts`.
- **Link/email/file-path classification**: `url` (bare `http(s)://`/`www.`/`ftp://`, or a
  `[label](url)` whose URL isn't `memo:`/`#slug`) opens for real via the already-implemented
  `linkOpenExternal`. `email` and `filePath` render as clickable (this plan's scope) but their
  actual opening is a stub — `commands/link.rs`'s own doc comment explicitly excludes "email/
  file-path special-casing" (mailto: prefixing, relative-to-absolute resolution against the tab
  folder) from what's implemented so far, so calling `linkOpenExternal` with a bare email/relative
  path would silently fail rather than open anything; a labeled stub was chosen over a broken call.
- **`eolKeeper.ts` was not created** — the plan's own §2 already describes it as "cross-reference
  only, actual EOL logic is owned by `editor-save/PLAN.md`," and that plan's Rust/frontend EOL
  handling is complete with no gap for this module to fill; an empty placeholder file seemed like
  clutter rather than a meaningful module.
- **Toolbar mount scope**: the floating toolbar is absolutely positioned within `.ttn-editor-pane`
  (the editor pane), not the whole app window, since that pane is where a Markdown toolbar
  naturally belongs and is the only sensible drag boundary once the tree pane/tab bar are excluded.
  Default position `{x:24, y:24}` can overlap the first couple of lines of a very short/empty memo
  (observed during checkbox-click self-verification, where the default position happened to sit
  over the test line) — not a decoration-engine defect, just a floating-panel default; the toolbar
  is user-draggable to work around it. Not escalated (no spec statement constrains the default
  position), but worth a reviewer look if it reads as poor UX on first launch.
- **Keyboard shortcuts**: all 16 toolbar actions plus the live/source toggle got default bindings in
  `shared/shortcuts.ts` (`docs/detailed-spec.md §5.10`'s confirmed policy). Dispatch reuses
  `MarkdownToolbar`'s own action-invocation logic via a `forwardRef`/`useImperativeHandle` handle
  (`components/Editor/EditorPane.tsx`) rather than duplicating the action switch statement, so
  Color/Font-size (which open a popover) and the rearrange toggle work from the keyboard too, not
  just Undo/Redo/Bold/etc.

---

## 1. Engine & Core Constraint

**CodeMirror 6, custom-built decoration extension, confirmed and final** (`[spec §4.2]`,
`DECISIONS.md`). One CodeMirror instance per open memo; its document *is* the Markdown source text —
there is no separate parse tree serialized back to text on save (`editor-save/PLAN.md` §1 already
states this). All work here is additive decoration/interaction logic over that buffer.

**Hard constraint, non-negotiable**: switching between live view and source view must never
re-parse-and-regenerate text or convert content in any way. It is purely toggling the decoration
layer's presence. Cursor and scroll position must be preserved across the toggle via CodeMirror's own
position-mapping APis (`EditorState.changes`/`RangeSet` mapping), not recomputed from scratch — this
must hold for arbitrarily frequent toggling within one editing session (`[detailed-spec §5.10]`).

## 2. Module Layout (`src/editor/`)

- `codeMirrorSetup.ts` — base `EditorState`/`EditorView` construction, extension composition
  (language support, our decoration extension, search package, keymap).
- `markdownDecorations.ts` — the core `ViewPlugin`/`StateField` that computes a `DecorationSet` from
  the current document: headings (font-size/weight per level), lists (bullet/numbered/checklist
  markers), blockquotes, code blocks/inline code, tables, bold/italic, horizontal rules, link rendering
  (including `memo:` scheme, see `link/PLAN.md`), image widgets (see `image-handling/PLAN.md`),
  `<span style="...">` decoration rendering.
- `markdownSpanParser.ts` — parses the allowlisted `<span style="color:...;font-size:...">` and `<img
  src width height>` HTML fragments embedded in the Markdown source into a form the decoration layer
  can render (this is a small, targeted parser for exactly these two allowlisted tag shapes — **not** a
  general HTML parser, and must not be extended to accept anything outside the allowlist without a
  spec change).
- `markdownSanitize.ts` — applies the sanitization allowlist (§5 below) to whatever HTML the decoration
  layer would otherwise render, on every render pass (not just at save).
- `checkboxClick.ts` — click handler on a rendered `☐`/`☑` decoration widget: toggles the underlying
  text character(s) in place via a CodeMirror transaction (this is the "direct click" toggle path,
  `[spec §2.3]`, distinct from the right-click "Check" menu insert action already covered in
  `editor-save/PLAN.md` §4).
- `decorationCommands.ts` — toolbar-invoked commands (see §4) that produce CodeMirror transactions
  (e.g. wrap selection in `**`/`*`, insert a heading prefix, insert a table skeleton).
- `toolbarSlots.ts` — the 16-slot action registry (id, Lucide icon, handler) and rearrange-mode state
  machine (see §4).
- `tocGenerator.ts` — heading scan, slugification, marker detection, insert-or-replace transaction
  (see §9). **Not created by this pass** — §8 below assigns full ownership of TOC generation to
  `table-of-contents/PLAN.md`, so building a same-named file here first would just be overwritten;
  the toolbar's "Update" slot calls a small labeled no-op in `decorationCommands.ts`
  (`tocUpdateStub`) instead, pending that plan.
- `imagePaste.ts` — clipboard paste handler; delegates the actual file write to
  `image-handling/PLAN.md`'s `image_save_pasted` command, then inserts the resulting Markdown/HTML at
  the cursor.
- `eolKeeper.ts` — not Markdown-specific but lives alongside these for historical/legacy-parity reasons;
  cross-reference only, actual EOL logic is owned by `editor-save/PLAN.md`.
- `links.ts` — click-to-open handling for auto-links and `memo:`/`[text](memo:...)` links rendered by
  the decoration layer; delegates resolution/opening to `link/PLAN.md`'s commands.

## 3. Live View vs. Source Toggle

- Governed initially by `AppSettings.editorDefaults.viewMode` (`'live'` factory default) with an
  optional per-memo `MemoEditorSettings.viewMode` override (`[spec §3.4]`). `editor_load`
  (`editor-save/PLAN.md`) does not itself decide the view mode — the frontend reads
  `MemoEditorSettings.viewMode ?? AppSettings.editorDefaults.viewMode` when constructing the CodeMirror
  instance for a newly-opened memo.
- A runtime toggle (keyboard shortcut and/or toolbar button) is **always available** regardless of the
  above, and changes only the current session's display — it does not write back to either setting
  value (`[detailed-spec §5.10]`; persisting a toggled state as a new default is explicitly out of
  current scope).
- Implementation: the decoration `StateField`/`ViewPlugin` checks a simple boolean facet/compartment
  (`liveViewEnabled`); toggling reconfigures that compartment only — the document itself is never
  touched by the toggle.

## 4. Toolbar

**Finalized 16-slot fixed grid** (`[detailed-spec §5.10]`, `DECISIONS.md` "Markdown Toolbar: Finalized
16-Slot Fixed Grid Layout" — supersedes any earlier 12-slot/6×2 draft). Floating, freely-repositionable
panel (drag anywhere within the window, plain CSS `position: absolute` + a drag-handle), fixed
**8 columns × 2 rows = 16 slots, no blanks**:

| | Col 1 | Col 2 | Col 3 | Col 4 | Col 5 | Col 6 | Col 7 | Col 8 |
|---|---|---|---|---|---|---|---|---|
| Row 1 | Undo | Color | Heading | List | Bold | Strikethrough | Link insert | Update (TOC) |
| Row 2 | Redo | Font size | Code block | Quote | Italic | Table insert | Image insert | Rearrange toggle |

Bold and Italic are separate slots. Undo/Redo are toolbar actions here (in addition to their existing
keyboard shortcuts, `editor-save/PLAN.md` §4). Update (TOC) triggers §9 below. Each action applies to
the current selection or inserts at the cursor, per standard Markdown-editor conventions.

**Icon set**: Lucide, bundled locally (no CDN). Mapping:

| Action | Icon | | Action | Icon |
|---|---|---|---|---|
| Undo | `Undo2` | | Redo | `Redo2` |
| Color | `Palette` | | Font size | `ALargeSmall` |
| Heading | `Heading` | | List | `List` |
| Bold | `Bold` | | Italic | `Italic` |
| Strikethrough | `Strikethrough` | | Quote | `Quote` |
| Link insert | `Link` | | Code block | `Code` |
| Update (TOC) | `RefreshCw` | | Table insert | `Table` |
| — | — | | Image insert | `Image` |
| — | — | | Rearrange toggle | `GripVertical` |

**Rearrange mode** (`toolbarSlots.ts`): slot 16 (`GripVertical`) toggles it; does not open a settings
screen. The action *set* is fixed — only slot order changes (no add/remove/hide).
- **OFF** (default): click invokes the slot's action normally.
- **ON**: click does nothing; only drag reorders slots. Exit via re-clicking the toggle or Esc.
  Whole-toolbar border/background tint indicates ON state.

**Color button palette** (`<span style="color:...">` on the selection; distinct from `label.color`,
`memo-tree/PLAN.md` §3) — closed 5-swatch enum + separate "clear color" control:

| Choice | value |
|---|---|
| Red | `#e53935` |
| Blue | `#1e88e5` |
| Green | `#43a047` |
| Orange | `#fb8c00` |
| Purple | `#8e24aa` |

"Clear color" is a 6th, separate control that strips the `color` property from the selection's span
(not one of the 5 swatches).

**Font-size button palette** (`<span style="font-size:...">` on the selection) — closed 3-choice enum +
separate "clear size" control:

| Choice | value |
|---|---|
| Small | `12px` |
| Large | `20px` |
| Extra-large | `28px` |

"Clear size" is a 4th, separate control, same treatment as Color's clear control.

**Persistence** (both via `settings_get`/`settings_set`, `persistence-fs/PLAN.md` §6.2):
- Position: `AppSettings.window.toolbarPosition: { x, y }` — write on drag-end (pointerup), not per
  mousemove frame.
- Slot order: `AppSettings.window.toolbarOrder` — 16-element array of action-ID strings (exact ID
  spellings are this plan's call, e.g. `"undo" | "redo" | "color" | "fontSize" | "heading" | "list" |
  "bold" | "italic" | "strikethrough" | "quote" | "link" | "codeBlock" | "tocUpdate" | "table" |
  "image" | "rearrangeToggle"`) — write on exiting rearrange mode (not per drag-move frame). Absent/
  `null` on first launch → the default layout above.

## 5. Sanitization Allowlist

Applied by `markdownSanitize.ts` to the HTML the decoration layer produces for rendering, **on every
render pass**, not just at save (`[detailed-spec §5.10]`):

| Tag | Allowed attributes | Notes |
|---|---|---|
| `<img>` | `src` (relative path only — reject absolute paths, `file://`, any non-relative scheme, and any path containing a `..` segment), `width`, `height` | path-traversal guard per `DECISIONS.md` "Image `src` Allowlist: Reject `..` Path-Traversal Segments" |
| `<span>` | `style`, and within it **only** `color` and `font-size` properties | any other CSS property in the same `style` string is stripped from that string, not the whole `<span>` |

Everything else — `<script>`, any `on*` event-handler attribute, `javascript:`-scheme URLs in any
attribute, `<iframe>`, `<style>`, disallowed CSS properties like `position` — is stripped entirely,
every render pass, so pasted or hand-typed unsafe HTML never executes even transiently
(`[spec §4.2]`, `[detailed-spec §5.10]`). Use DOMPurify (continuity with legacy `dompurify` dependency)
configured with a strict custom allowlist matching exactly this table, rather than DOMPurify's
permissive defaults.

This sanitization applies to the **live-view rendering** path only — the underlying CodeMirror
document/buffer (and therefore the saved `.md` file) always retains the user's original typed text
verbatim, including any syntax that gets stripped from the *rendered* output. Sanitization never
mutates the buffer.

## 6. Decoration Notation

- Bold `**text**` / italic `*text*`: standard CommonMark, rendered via the engine's normal emphasis
  handling in the decoration layer (no custom notation).
- Color/size: `<span style="color:...;font-size:...">...</span>`, inserted/edited via toolbar helpers;
  manual hand-typing of the same syntax is equally valid input, parsed by `markdownSpanParser.ts`.

## 7. `memo:` Link Coexistence in Markdown

Bare `memo:TabName/MemoName` auto-link recognition and `[label](memo:TabName/MemoName)` Markdown-link
syntax both resolve through the same logic in `link/PLAN.md` — the only difference is the clickable
region (whole bare token vs. the link's visible label span), computed by `markdownDecorations.ts`
(`[detailed-spec §5.8]`). This file's decoration layer is responsible for detecting both patterns in
the buffer and rendering them as clickable; actual resolution/opening logic is not duplicated here.

## 8. Table of Contents (TOC)

Triggered by the toolbar's Update slot (§4, row 1 slot 8, `RefreshCw`). Fully specified as its own plan
in [`table-of-contents/PLAN.md`](../table-of-contents/PLAN.md) — heading scan (with code-fence
awareness), slugification, marker-block generation/insert-replace, persistence, and `#slug` link
coexistence all live there; read it alongside this file rather than duplicating it here.

## 9. High-Risk Defect Zone — Required Test Coverage

Carried forward from `[spec §8]` and `[detailed-spec §5.10]`, explicit and binding on
`developer-lite`'s implementation + self-verification (not just `reviewer-lite`'s later pass):
overlapping-range decoration edits and decoration-widget click/selection handling are flagged as the
highest-risk area of this entire feature, because a decoration and its underlying hidden syntax markers
can occupy different visual space than their underlying text length, which is exactly the kind of
mismatch that produces wrong cursor/selection placement. This class of bug does **not** corrupt the
saved file (the buffer beneath decorations is always correct plain text) but is easy to miss manually
and easy to hit in daily use. Before considering this feature done, exercise (manually or via automated
CDP-driven interaction, per `project-architecture/PLAN.md` §7):

1. Editing text immediately at a decoration boundary (e.g. typing right before/after `**` in
   `**bold**`).
2. Editing inside a decorated range (e.g. inserting a character inside `**bo|ld**`).
3. Clicking to place the cursor on/near a decoration widget (a rendered checkbox glyph, a resized
   image).
4. Rapid live/source toggling while a selection spans a decorated range — selection must survive the
   toggle without shifting.
5. A `memo:`/`[text](memo:...)` link rendered inside a `<span style="color:...">`-decorated range
   (nested/overlapping decorations) — both must render and remain independently clickable/styled
   correctly (`DECISIONS.md` "Overlapping Text-Styling Decorations: Implement as Composable Mark
   Decorations"; see §2's note below on why this is expected to be low-risk if implemented as two
   composable "mark" decorations rather than a "widget" replacement).

**Implementation guidance for item 5**: color-span styling and link click-affordance rendering should
both be implemented as CodeMirror "mark" decorations (CSS class/style applied over a text range), not
"widget" decorations (a range replaced by a rendered element). Two mark decorations compose over the
same or overlapping ranges without the cursor-mapping risk described above, which is specific to widget
replacement and to hidden-syntax-marker/visible-text length mismatches — this is a lower-risk sub-case
of the same defect zone, not a separate open design question, but is still worth the explicit
regression check in item 5 above.

## Open Items

None. Engine choice, sanitization allowlist, and toggle behavior are all confirmed in `spec.md`/
`DECISIONS.md`; this plan only operationalizes them into concrete modules.
