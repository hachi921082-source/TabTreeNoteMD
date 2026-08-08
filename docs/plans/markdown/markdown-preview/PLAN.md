# PLAN: Markdown Preview Pane

> Status: planning complete, not yet implemented. Strictly additive on top of
> [`markdown-editing/PLAN.md`](../../markdown-editing/PLAN.md) (CodeMirror 6 decoration engine, 16-slot
> toolbar, live/source toggle, sanitization allowlist) — that plan's own modules and behavior are
> **unchanged** by this one; this plan only adds new modules and a few new props/effects to
> `components/Editor/EditorPane.tsx` and `components/Editor/Toolbar/MarkdownToolbar.tsx`.
>
> For the history of how the `paneMode` tri-state entry point was resolved (it was originally a gap
> escalated to `docs/QA.md`), see `docs/DECISIONS.md`'s "Markdown Preview: Pane-Mode Reached via
> Single 3-State Cycling Toggle" entry.

**Scope note**: this plan originated as a single combined plan covering the preview pane, PDF export,
and live-view real image rendering, later split by feature. This file now covers **only the preview
pane**. The other two pieces live in:
- [`image-handling/PLAN.md`](../image-handling/PLAN.md) — the shared image-loading pipeline
  (`imageLoader.ts`, `image_read_bytes` backend command) and live-view real `<img>` rendering.
- [`pdf-export/PLAN.md`](../pdf-export/PLAN.md) — PDF export (hidden-iframe print flow), which depends
  on this plan's `renderMarkdownToHtml`.

**Dependency**: this plan assumes `image-handling/PLAN.md`'s shared image-loading pipeline
(`getOrLoadImage`/`src/editor/imageLoader.ts`) already exists — read that doc first for how preview
images get resolved (§5.5 below references `getOrLoadImage`).

Sources: `docs/MDEditor-spec.md` (all 9 sections, primary/authoritative source for this plan);
`docs/spec.md` §4.2; `docs/detailed-spec.md` §5.10; `docs/plans/markdown-editing/PLAN.md` (extended
here, unchanged in itself); `docs/plans/image-handling/PLAN.md` (shared image-loading pipeline this
plan depends on); `docs/plans/table-of-contents/PLAN.md` (`scanHeadings`/`slugifyHeading` reused for
preview heading anchors); `docs/plans/project-architecture/PLAN.md` §3/§4 (`Result<T>` convention);
`docs/DECISIONS.md`'s "Markdown Preview: Pane-Mode Reached via Single 3-State Cycling Toggle" decision
record.

---

## 1. Scope and Relationship to Existing Plans

Elaborates `docs/MDEditor-spec.md` §1. This plan covers one addition, layered on top of the existing
editable pane without touching it:

1. A second, independent, read-only rendered-HTML **preview pane**, shown alongside the editable pane.

(The other two original scope items — PDF export and real `<img>` rendering in the live-view — moved to
`pdf-export/PLAN.md` and `image-handling/PLAN.md` respectively, per the split noted above.)

**Non-goal (confirmed)**: not a WYSIWYG rewrite. `markdown-editing/PLAN.md`'s decoration engine, the
plain-text-buffer-as-source-of-truth model, and the live/source toggle's own behavior are all
untouched. The preview pane is a **separate rendering pipeline** (a full Markdown→HTML library run
against the same buffer's text on every debounced change) — it does not read CodeMirror decorations
or the `@lezer/markdown` syntax tree at all, except where explicitly noted (§5's heading-slug reuse).

## 2. Module Layout

New files (frontend):

| File | Purpose |
|---|---|
| `src/editor/preview/markdownIt.ts` | Builds the configured `markdown-it` instance (§5.1). |
| `src/editor/preview/previewRenderer.ts` | `renderMarkdownToHtml(state: EditorState, opts): { html: string }` — orchestrates token-walk (`data-source-line` + heading-id injection), the span/img allowlist substitution, and the DOMPurify backstop pass (§5.2-§5.4). Pure function of the document text; no DOM/React dependency, so this plan's `PreviewPane.tsx` and `pdf-export/PLAN.md`'s export flow both call it. |
| `src/editor/preview/scrollSync.ts` | `scrollPreviewToLine(container, targetLine)` — finds the closest `[data-source-line]` element ≤ `targetLine` and scrolls to it (§5.6). |
| `src/editor/preview/previewInteractions.ts` | Click-delegation handlers mounted once on the preview container: checkbox write-back (§5.8), link click open-trigger/confirm/resolve (§5.9), heading-anchor jump (§5.7). |
| `src/components/Editor/PreviewPane.tsx` | React component: owns the debounced re-render effect, the scroll-sync listener, mounts the container `previewInteractions.ts` delegates onto, and the context-menu suppression (§4.3). |

Modified files:

| File | Change |
|---|---|
| `src/shared/models.ts` | `AppSettings.preview: { debounceMs: number }`, `AppSettings.editorDefaults.paneMode: "edit" \| "split" \| "preview"`, `MemoEditorSettings.paneMode?: "edit" \| "split" \| "preview" \| null`, matching `AppSettingsPatch` fields (§3). |
| `src-tauri/src/model/app_settings.rs` | `PreviewSettings` struct + `AppSettings.preview` field; `EditorDefaults.pane_mode` field (§3). |
| `src-tauri/src/model/app_settings_patch.rs` | `PreviewSettingsPatch`, `EditorDefaultsPatch.pane_mode` (§3). |
| `src-tauri/src/model/tree.rs` (wherever the Rust `MemoEditorSettings` lives) | `pane_mode: Option<PaneMode>` field (§3). |
| `src/components/Settings/SettingsDialog.tsx` | New debounce-ms control + `editorDefaults.paneMode` select control (§3). |
| `src/components/Editor/EditorPane.tsx` | New `paneMode`/`onCyclePaneMode` state (mirrors existing `liveViewOverride`/`onToggleLiveView` pattern but cycles 3 states instead of flipping a boolean), DOM restructure for the two sub-panes (§4.2), new global shortcut effect for pane-mode-cycle (§6), passes `memoKey` context down. |
| `src/components/Editor/Toolbar/MarkdownToolbar.tsx` | New header button (§6), auto-minimize effect (§4.4). |
| `src/shared/shortcuts.ts` | `markdown.paneModeCycle` entry (§6). |
| `package.json` | New dependencies: `markdown-it`, a GFM task-list plugin (e.g. `markdown-it-task-lists`), `dompurify`, plus their type packages if not self-hosted (§5.1 — exact package/version choice and empirical verification of their default rule sets is implementation-time work, see that section's caveat). |

## 3. Settings Schema Additions

Elaborates `docs/MDEditor-spec.md` §3, following `EditorDefaults`/`AutosaveSettings`'s exact existing
patterns (`src-tauri/src/model/app_settings.rs`, `app_settings_patch.rs`, `src/shared/models.ts`).

**`AppSettings.preview: PreviewSettings`** (new top-level section, sibling of `autosave`):

```rust
// src-tauri/src/model/app_settings.rs — additive field, same treatment as `appearance`/`autostart`
// so a pre-existing settings.json (written before this feature) still parses.
#[serde(default)]
pub preview: PreviewSettings,

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSettings {
    pub debounce_ms: u32,
}
impl Default for PreviewSettings {
    fn default() -> Self { PreviewSettings { debounce_ms: 150 } }
}
```

```rust
// app_settings_patch.rs
#[serde(default)]
pub preview: Option<PreviewSettingsPatch>,
// ...
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSettingsPatch {
    #[serde(default)]
    pub debounce_ms: Option<u32>,
}
impl PreviewSettingsPatch {
    fn apply_to(self, current: &mut PreviewSettings) {
        if let Some(v) = self.debounce_ms { current.debounce_ms = v; }
    }
}
```

```ts
// src/shared/models.ts
export interface AppSettings {
  // ...existing fields...
  preview: { debounceMs: number };
}
export type AppSettingsPatch = {
  // ...existing fields...
  preview?: Partial<{ debounceMs: number }>;
};
```

Range **50–2000**, step `50` (tighter than autosave's 200–10000 since no disk I/O is involved), default
`150`. Two independent debounce timers — this one must never be tied to autosave's own debounce value
or completion event (§4's re-render trigger is a plain doc-changed listener, not a save-completion
event).

**`AppSettings.editorDefaults.paneMode: "edit" | "split" | "preview"`** — new field alongside
`wrap`/`tabWidth`/`showLineNumbers`/`fontSize`. Unlike `preview` above, `EditorDefaults` is an existing
(non-additive) struct, so only the *new field itself* needs `#[serde(default)]` (not the whole struct)
so an old-format `settings.json` still parses:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PaneMode {
    Edit,
    Split,
    Preview,
}
impl Default for PaneMode {
    fn default() -> Self { PaneMode::Edit }
}

// on `EditorDefaults`:
#[serde(default)]
pub pane_mode: PaneMode,
```

**Default value: `"edit"`** (today's edit-only appearance unchanged) — chosen so this additive feature
never changes an existing user's current layout without an explicit toggle.

**`MemoEditorSettings.paneMode?: "edit" | "split" | "preview" | null`** — mirrors `viewMode` exactly:

```rust
// wherever the Rust `MemoEditorSettings` struct lives (currently `model/tree.rs`)
#[serde(default, skip_serializing_if = "Option::is_none")]
pub pane_mode: Option<PaneMode>,
```
```ts
// src/shared/models.ts
export interface MemoEditorSettings {
  // ...existing fields...
  paneMode?: "edit" | "split" | "preview" | null;
}
```

**Persistence timing**: immediate on every cycle (no debounce), via the exact same per-memo-override
write path `viewMode` already uses (`EditorPane.tsx`'s `onToggleLiveView`, lines 148–173 in the current
file) — see §4.1 for the mirrored `onCyclePaneMode` implementation. Unrelated to `preview.debounceMs`
above, which only controls preview re-render timing.

**Settings-dialog UI** (`SettingsDialog.tsx`, same Editor tab as the existing autosave-delay control):

```tsx
<section className="ttn-settings-section">
  <h3>プレビュー</h3>
  <label className="ttn-settings-row">
    <span>プレビュー再描画の遅延 (ミリ秒)</span>
    <input
      type="number"
      min={50}
      max={2000}
      step={50}
      value={settings.preview.debounceMs}
      onChange={(e) => void patchSettings({ preview: { debounceMs: Number(e.target.value) } })}
    />
  </label>
</section>
```

Plus, in the existing Editor section next to `showLineNumbers`'s checkbox, a select control (not a
checkbox, since this is now a 3-way field, not a boolean):

```tsx
<label className="ttn-settings-row">
  <span>新規メモの既定のペイン表示</span>
  <select
    value={settings.editorDefaults.paneMode}
    onChange={(e) => void patchSettings({ editorDefaults: { paneMode: e.target.value as PaneMode } })}
  >
    <option value="edit">編集のみ</option>
    <option value="split">分割表示</option>
    <option value="preview">プレビューのみ</option>
  </select>
</label>
```

Both immediate-write, no separate Apply step — identical pattern to every other control in that
dialog.

`EditorPane.tsx`'s `FALLBACK_EDITOR_DEFAULTS` constant (used before `settings_get` resolves) gains
`paneMode: "edit"`.

## 4. Pane Layout, Toggles, and Auto-Reveal

Elaborates `docs/MDEditor-spec.md` §2.

### 4.1 The `paneMode` Cycling Toggle

Mirrors `EditorPane.tsx`'s existing `liveViewOverride`/`onToggleLiveView` pattern (§3 "Live View vs.
Source Toggle" in `markdown-editing/PLAN.md`) structurally, targeting `paneMode` instead of `viewMode`,
but **cycles through 3 states** (`edit → split → preview → edit → ...`) rather than flipping a
boolean:

```ts
type PaneMode = "edit" | "split" | "preview";
const PANE_MODE_CYCLE: Record<PaneMode, PaneMode> = { edit: "split", split: "preview", preview: "edit" };

const resolvedPaneMode = memoEditorOverride?.paneMode ?? editorDefaults.paneMode;
const [paneModeOverride, setPaneModeOverride] = useState<PaneMode | null>(null);
const paneMode = paneModeOverride ?? resolvedPaneMode;
const onCyclePaneMode = useCallback(() => {
  setPaneModeOverride((current) => {
    const currentMode = current !== null ? current : resolvedPaneMode;
    const next = PANE_MODE_CYCLE[currentMode];
    const name = memoNameRef.current;
    if (name) {
      const updated = { ...overrideRef.current, paneMode: next };
      void setEditorOverrideRef.current(name, updated);
      if (loadedMemoNameRef.current === name) loadedOverrideRef.current = updated;
    }
    return next;
  });
}, [resolvedPaneMode]);

/** Reveal helper used by §4.4's auto-reveal principle and by the live/source toggle's own
 * auto-reveal case (§4.4): forces pane-mode to `split` if (and only if) it is currently `preview`,
 * otherwise leaves it untouched (e.g. `edit` is never disturbed by an auto-reveal). */
const revealSplitIfPreviewOnly = useCallback(() => {
  setPaneModeOverride((current) => {
    const currentMode = current !== null ? current : resolvedPaneMode;
    if (currentMode !== "preview") return current;
    const next: PaneMode = "split";
    const name = memoNameRef.current;
    if (name) {
      const updated = { ...overrideRef.current, paneMode: next };
      void setEditorOverrideRef.current(name, updated);
      if (loadedMemoNameRef.current === name) loadedOverrideRef.current = updated;
    }
    return next;
  });
}, [resolvedPaneMode]);
```

Reset to `null` on every memo switch (same place `liveViewOverride` is reset), so a freshly-opened
memo always starts from its governed setting, never a stale session toggle from the previously-open
memo.

**Fully independent of `liveViewEnabled`** — no shared state, no merged enum. `viewMode`
(live/source) and `paneMode` (edit/split/preview) are two separate, independently-persisted fields
that can be in any combination simultaneously (`docs/MDEditor-spec.md` §2's "two independent toggles"
— the live/source axis and the pane-mode axis never interact directly), **except** for the one
specific auto-reveal interaction in §4.4 where clicking the live/source toggle while `paneMode ===
"preview"` first calls `revealSplitIfPreviewOnly()` above.

### 4.2 DOM Structure (Split Ratio, CSS-Only Hiding)

`EditorPane.tsx`'s render tree is restructured so the two sub-panes are siblings inside a new wrapper,
while the floating toolbar/status-bar/dialogs stay direct children of the existing `.ttn-editor-pane`
(unchanged mount point):

```
.ttn-editor-pane          (unchanged: position:relative, unchanged CSS identity/mount point)
  ├─ .ttn-editor-panes-row   (NEW: flex row, width/height 100%)
  │    ├─ .ttn-editor-cm-container   (existing ref/element — unchanged CodeMirror mount; `display:none`
  │    │                               when paneMode === "preview", §4.4)
  │    └─ .ttn-editor-preview-pane   (NEW, `<PreviewPane>`, only rendered/mounted when
  │                                    paneMode !== "edit", i.e. `split` or `preview`)
  ├─ <MarkdownToolbar/>       (unchanged JSX position — stays a direct child of .ttn-editor-pane)
  ├─ search panel / status bar / dialogs   (unchanged)
```

**Why the toolbar's mount point must not move**: `MarkdownToolbar.tsx`'s `clampAndApply` measures
`panel.offsetParent` at runtime (not a hardcoded ref) — since `.ttn-editor-pane` already has
`position: relative` and now spans the full combined width of both sub-panes (the new
`.ttn-editor-panes-row` is a 100%-width child of it), keeping the toolbar as `.ttn-editor-pane`'s
direct child means `clampToolbarPosition`'s existing bounds computation (`docs/MDEditor-spec.md` §7's
"widen clamp bounds to the combined pane container") is satisfied with **zero code change** to
`clampToolbarPosition` itself. The bug this guards against: if the toolbar were instead nested *inside*
`.ttn-editor-panes-row` or `.ttn-editor-cm-container` specifically, its clamp bounds would collapse to
a 0×0 box the moment the edit sub-pane is hidden (paneMode `"preview"`, §4.4) — this DOM-nesting choice
is what actually fixes the issue `docs/MDEditor-spec.md` §7 describes, not a change to the clamp math.

**CSS**:
```css
.ttn-editor-panes-row { display: flex; width: 100%; height: 100%; }
.ttn-editor-cm-container { flex: 1 1 50%; min-width: 0; }   /* flex:1 1 100% when paneMode === "edit" */
.ttn-editor-preview-pane { flex: 1 1 50%; min-width: 0; overflow-y: auto; }
```
Both sub-panes always at a fixed 50/50 flex-basis when both are rendered (`paneMode === "split"`) — no
drag-resizable divider (`docs/MDEditor-spec.md` §2, confirmed). `.ttn-editor-preview-pane` inherits
theme colors automatically via `src/theme.css`'s `data-theme` custom properties, since it's a normal
DOM node in the same document (not an iframe) — no new theme-detection code needed.

**CodeMirror `EditorView` is never unmounted for this toggle.** `paneMode` only controls (a) whether
`<PreviewPane>` is rendered at all (mounted for `split`/`preview`, unmounted for `edit`), and (b)
whether `.ttn-editor-cm-container` gets `display:none` (only for `preview`, §4.4). Neither path
touches `viewRef.current`, `ed.loadGeneration`, or any of `EditorPane.tsx`'s existing view-(re)creation
effects; undo history, cursor position, and `computeTopVisibleLine`/`scrollToTopVisibleLine` state all
survive every pane-mode transition, including while pane-mode is `preview`.

### 4.3 Context-Menu Suppression Over the Preview

`PreviewPane.tsx`'s own container unconditionally suppresses the native/default context menu:

```tsx
<div className="ttn-editor-preview-pane" onContextMenu={(e) => e.preventDefault()} ref={containerRef}>
```

`docs/MDEditor-spec.md` §2 mandates this for pane-mode `"preview"` specifically (right-click over the
preview while the edit pane is hidden must show *nothing*, not even a native browser menu — plain
Ctrl+C copy still works via native text selection). This plan applies it unconditionally, in `"split"`
mode too, as the simpler consistent behavior across both modes.

### 4.4 How Pane-Mode Is Reached: the 3-State Cycling Toggle and Auto-Reveal

Resolved via `docs/DECISIONS.md`'s "Markdown Preview: Pane-Mode Reached via Single 3-State Cycling
Toggle" (see the header note at the top of this document). There is a **single** pane-mode toggle —
not one button per state, not a separate "enter preview-only" gesture — that cycles all three states
on each activation, in the fixed order `edit → split → preview → edit → ...` (§4.1's
`onCyclePaneMode`/`PANE_MODE_CYCLE`). It is reachable two ways, both invoking the exact same
`onCyclePaneMode` callback:
- Clicking the pane-mode toggle button in the toolbar header (§6).
- The `Ctrl+Shift+P` shortcut (§6).

**Setting `display:none` on `.ttn-editor-cm-container`**: applied whenever `paneMode === "preview"`
(and only then) — `edit` and `split` both keep the edit pane visible, per §4.2's DOM structure.

**Auto-reveal principle** (`docs/MDEditor-spec.md` §2): any action that expects to focus/scroll/mutate
the CodeMirror view while `paneMode === "preview"` must first call `revealSplitIfPreviewOnly()` (§4.1)
— which switches pane-mode to `split` specifically, never `edit` — then perform the action, so the
result is visible. This applies to:
- Ctrl+F (in-memo search open)
- tree-row click auto-focus
- cross-tab search-result jump
- `memo:` link-click navigation
- every toolbar formatting button (`src/editor/decorationCommands.ts` — bold/heading/list/link/quote/
  codeBlock/table/etc.)
- **the live/source toggle button** (`viewMode`'s own toggle, unrelated schema, inside the edit pane)
  — clicking it while `paneMode === "preview"` first calls `revealSplitIfPreviewOnly()` to reveal the
  edit pane at `split`, then performs the live/source toggle as normal, so the toggle's visible effect
  is actually visible instead of being applied to a hidden pane. This is a new item not previously
  enumerated among the auto-reveal actions — the live/source toggle button's own click handler
  (`EditorPane.tsx`, wherever `onToggleLiveView` is wired to the button) gains this one extra call
  before its existing logic runs; `onToggleLiveView` itself and `MemoEditorSettings.viewMode`'s schema
  are otherwise completely unchanged by this feature.

None of the above are disabled or silently no-op while `paneMode === "preview"`; they reveal-then-act.
This is fully wired in this plan's initial implementation — there is no remaining deferred piece.

## 5. Preview Rendering

Elaborates `docs/MDEditor-spec.md` §4.

### 5.1 Renderer Setup — `src/editor/preview/markdownIt.ts`

```ts
import MarkdownIt from "markdown-it";
// exact plugin package(s) TBD at implementation time — see caveat below.

export const md = new MarkdownIt("default", { html: false, linkify: true, breaks: false })
  .use(taskListsPlugin, { enabled: true /* keep <input> non-disabled; readOnly gating is JS-side, see §5.8 */ });
```

- `html: false` — raw HTML in the source auto-escapes to literal visible text, exactly matching the
  live-view's own "undecorated HTML stays visible as text" behavior. **This is why the allowlisted
  `<span>`/`<img>` constructs cannot go through markdown-it's own HTML handling at all** — see §5.3.
- markdown-it's `"default"` preset (as opposed to the stricter `"commonmark"` preset) already includes
  GFM-equivalent table and strikethrough support with no extra plugin. `linkify: true` autolinks bare
  `http(s)://`/`www.` tokens (backed by the bundled `linkify-it`), matching the live-view's
  `@lezer/markdown` GFM autolink parity. Task-list checkboxes are not part of any markdown-it preset
  and need a small plugin (e.g. `markdown-it-task-lists`).
- **Caveat, to verify empirically during implementation** (this plan's confidence in exact library
  default behavior is based on general knowledge, not a live install/test in this repo): confirm the
  chosen markdown-it version's `"default"` preset really does enable tables/strikethrough without
  extra config, and confirm the chosen task-list plugin's rendered `<input>` shape (attributes, class
  names) before wiring §5.8's click handler against it. If either assumption is wrong, the fallback is
  a small custom `md.renderer.rules` override or an alternate plugin — a self-contained fix scoped to
  this one file. **(Open item — see bottom of this document.)**
- **Stress test**: per `docs/MDEditor-spec.md` §5's root-cause note, test this renderer against one
  pathological huge single line before shipping (this app's own image-embedding mechanism can't
  produce one, per that note, but a hand-typed or pasted-from-elsewhere huge line still could).

### 5.2 `renderMarkdownToHtml` — Token Walk

`src/editor/preview/previewRenderer.ts`:

```ts
export function renderMarkdownToHtml(state: EditorState): { html: string }
```

1. Take `text = state.doc.toString()`.
2. **Span/image allowlist substitution (before parsing — see §5.3)**: scan `text` with
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
8. **Substitute placeholders back in**, replacing each placeholder token (§5.2 step 3) with its safe,
   code-constructed HTML fragment:
   - Color span: `<span data-source-line="..." style="${filteredStyle}">${escapedInnerText}</span>`
     (inner text HTML-escaped since it wasn't run through markdown-it's own inline rendering — matches
     the live-view's own span content treatment, which doesn't support nested Markdown either).
   - Image: `<img data-relative-src="${src}" alt="${escapedAlt}" ${width ? `width="${width}"` : ""} ${height ? `height="${height}"` : ""}>` — **no `src` attribute yet** (resolved async, see §5.5); `data-relative-src` carries the path for the post-mount resolution pass.
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

### 5.3 Why Span/Img Can't Go Through markdown-it's HTML Path

With `html:false`, a literal `<span style="...">` or `<img ...>` typed in the source would otherwise be
auto-escaped to visible text — exactly matching the live-view's behavior for any *other* raw HTML tag.
The two allowlisted constructs are deliberately pulled out **before** parsing (step 2 above) and
re-inserted as already-safe fragments **after** rendering (step 8) so they bypass markdown-it's
HTML-escaping path entirely, without needing a general HTML allowlist inside the renderer itself
(`docs/MDEditor-spec.md` §4's explicit design).

### 5.4 `PreviewPane.tsx`

```tsx
export interface PreviewPaneProps {
  getView: () => EditorView | null;
  renderTick: number;          // bumped by EditorPane's existing updateListener — reused, not duplicated
  debounceMs: number;          // AppSettings.preview.debounceMs
  tabId: string | null;
  memoKey: string;
  readOnly: boolean;           // gates checkbox write-back, §5.8
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
  via `getOrLoadImage` (§5.5), and (re)binds the scroll-sync listener (§5.6).
- Container: `<div className="ttn-editor-preview-pane" ref={containerRef} onContextMenu={...} dangerouslySetInnerHTML={{ __html: html }} />` with `previewInteractions.ts`'s click delegation attached once via `useEffect` (not re-bound every render).

### 5.5 Image Resolution in the Preview

After `container.innerHTML` is set (§5.4), a `useEffect` finds every `img[data-relative-src]` and calls
`getOrLoadImage(tabId, path, memoKey)` (`image-handling/PLAN.md`'s shared image-loading pipeline),
setting `.src` on resolve (or leaving it as the fallback — a small inline "image unavailable"
glyph/background, same visual family as the live-view's placeholder) on failure. `alt`/`width`/`height`
are already set from §5.2 step 8's HTML.

### 5.6 Scroll-Sync (One-Directional: Editor → Preview)

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

### 5.7 Heading-Anchor Click Handling

In `previewInteractions.ts`'s delegated click handler: for a clicked `<a href="#slug">`,
`e.preventDefault()`, then `container.querySelector("#" + CSS.escape(slug))?.scrollIntoView({block:
"start"})`, scoped to the preview's own container (not `document.getElementById`, which could collide
with an unrelated same-id element elsewhere in the app) — matches `docs/MDEditor-spec.md` §4's
"resolved by calling `scrollIntoView` on the matching `id` element within the preview pane's own
internal scroll container."

### 5.8 Checkbox Click Write-Back

`previewInteractions.ts`, on click of a rendered `<input type="checkbox">`:

1. If `readOnly`, no-op (matches the live-view's `cm-md-checkbox-readonly` gating) — the click handler
   itself performs this check rather than relying on the plugin's own `disabled` attribute (more
   robust against plugin-specific rendering quirks, per §5.1's caveat).
2. Find the nearest ancestor `[data-source-line]` element (the enclosing `list_item_open`'s line,
   tagged in §5.2 step 5) and read its line number (0-indexed, per `token.map`).
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

### 5.9 Link Clicks

`previewInteractions.ts`'s delegated click handler, for any `e.target.closest("a")`:

- **Always `e.preventDefault()`** first, regardless of kind — letting the WebView2 default anchor
  navigation proceed would navigate the app's own window away from itself.
- `href` starting with `memo:` → `resolveAndOpenMemoLink(tabId, href, onOpenMemoLink)` (imported
  verbatim from `links.ts` — already framework-agnostic, no CodeMirror dependency, directly reusable
  as-is).
- `href` starting with `#` → §5.7 above.
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

### 5.10 Tables, Code Fences, Images — Presentation

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

## 6. Floating Toolbar Interaction and Keyboard Shortcuts

Elaborates `docs/MDEditor-spec.md` §7-§8.

### 6.1 Clamp Bounds

Resolved by §4.2's DOM-nesting choice alone — the toolbar stays a direct child of `.ttn-editor-pane`,
which already spans the full combined width of both sub-panes. No change to
`clampToolbarPosition`/`clampAndApply` themselves is needed; this section exists only to record the
requirement and point at where it's actually satisfied (§4.2), so a future refactor doesn't
accidentally reintroduce the bug by moving the toolbar's mount point inside the new
`.ttn-editor-panes-row`.

### 6.2 Auto-Minimize When Pane-Mode Is `"preview"`

`MarkdownToolbar.tsx` gains a new prop, `paneMode: "edit" | "split" | "preview"`, and a `useEffect`
watching its transition into `"preview"` specifically (from either `"edit"` or `"split"`), factored to
share logic with the existing `toggleMinimized`'s "entering minimized" branch (extract a small internal
`minimizeNow()` helper called by both `toggleMinimized(true)` and this new effect, rather than
duplicating the four state-setting/patch-settings lines):

```ts
useEffect(() => {
  if (paneMode === "preview" && !minimized) minimizeNow();
  // deliberately does NOT auto-restore when paneMode leaves "preview" — the toolbar's
  // minimize/restore is otherwise a simple one-way manual toggle everywhere else in this component;
  // forcing a restore on leaving pane-mode "preview" would be an inconsistent special case with no
  // stated requirement for it.
}, [paneMode]);
```

Not fully hiding the toolbar when `paneMode === "preview"` (keeping the header visible) is required so
the header's pane-mode toggle button remains clickable — the escape hatch back to an editable view
(`docs/MDEditor-spec.md` §7's "dead end" rationale).

### 6.3 Keyboard Shortcut and New Toolbar-Header Button

`src/shared/shortcuts.ts`'s `DEFAULT_SHORTCUTS` gains:

```ts
"markdown.paneModeCycle": "Ctrl+Shift+P",
```

Confirmed unused in the existing table. `markdown.paneModeCycle` **cycles** all three pane-mode states
(`edit → split → preview → edit → ...`, §4.1/§4.4) on every activation — it is not a binary flip and
there is no separate shortcut per state.

**Dispatch**: this shortcut is *not* part of `ToolbarActionId`/`toolbarSlots.ts`'s 16-slot registry (it
lives in the toolbar's header, not the fixed grid — `docs/MDEditor-spec.md` §8), so it can't go through
`MarkdownToolbar`'s existing `invokeAction` switch. Also, unlike the 16 grid actions' shortcut handler
(which only fires while focus is inside `.ttn-editor-cm-container`, `EditorPane.tsx`'s `containerRef`),
this one must work regardless of focus location within the editor pane — including while focus is
inside the preview pane itself, a separate DOM subtree. A **new, separate** effect in `EditorPane.tsx`,
mirroring the existing (also focus-unconstrained) `Ctrl+F` handler's pattern:

```ts
useEffect(() => {
  const handler = (e: KeyboardEvent) => {
    const shortcuts = resolveShortcuts(appSettings?.shortcuts);
    const combo = comboFromEvent(e);
    if (combo === shortcuts["markdown.paneModeCycle"]) {
      e.preventDefault();
      onCyclePaneMode();
    }
  };
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}, [appSettings?.shortcuts, onCyclePaneMode]);
```

(`pdf-export/PLAN.md` adds its own `Ctrl+P` branch to a similarly-shaped effect; whether that ends up
being the same effect instance or a sibling one is that plan's implementation-time discretion.)

**New toolbar-header button** (`MarkdownToolbar.tsx`'s header, alongside the existing live/source
toggle button and minimize button). The pane-mode button is a single button showing the *current*
state and cycling to the next one on click — not three separate buttons and not a checkbox-style
toggle:

```tsx
const PANE_MODE_LABEL: Record<PaneMode, string> = {
  edit: "編集のみ",
  split: "分割表示",
  preview: "プレビューのみ",
};

<button
  type="button"
  className="ttn-md-toolbar-pane-mode"
  onClick={onCyclePaneMode}
  aria-label={`ペイン表示モードを切り替え (現在: ${PANE_MODE_LABEL[paneMode]})`}
>
  {PANE_MODE_LABEL[paneMode]}
</button>
```

`onCyclePaneMode` is a new `MarkdownToolbarProps` field, passed down from `EditorPane.tsx` (the same
call-through pattern `liveViewEnabled`/`onToggleLiveView` already use), with `paneMode` itself also
passed down as a prop (used by both the button's label and §6.2's auto-minimize effect).

(`pdf-export/PLAN.md` adds its own PDF-export button alongside this one in the same toolbar header.)

## Open Items

1. **§5.1's caveat**: verify the chosen `markdown-it` version's default table/strikethrough support and
   the chosen task-list plugin's exact rendered `<input>` shape empirically, early in implementation,
   before wiring §5.8's checkbox click handler against assumed markup.
