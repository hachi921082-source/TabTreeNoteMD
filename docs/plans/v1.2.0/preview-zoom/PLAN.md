# PLAN: Preview-Pane Zoom (v1.2.0)

> Status: planning complete, not yet implemented. New, independent zoom mechanism scoped to the preview
> pane's rendered content — no existing zoom precedent to extend (`AppSettings.uiFontScale` is explicitly
> scoped to non-editor-body UI chrome, a different axis). Depends on `docs/plans/markdown/markdown-
> preview/PLAN.md` (`paneMode` tri-state, `PreviewPane.tsx`, toolbar-header layout, auto-minimize-to-
> header-only-in-preview-mode behavior) and `docs/plans/window-operation/PLAN.md` §3 (`resolveShortcuts`/
> `comboFromEvent` shortcut system, `AppSettings.shortcuts`/`DEFAULT_SHORTCUTS`). Extracted UI copy
> (button tooltips/`aria-label`s) folds into `docs/plans/v1.2.0/i18n/PLAN.md`'s translation maps.

Sources: `docs/spec.v1.2.0.md` §6; `docs/detailed-spec.v1.2.0.md` §6, §11; `docs/DECISIONS.md`
("Preview-Pane Zoom CSS Mechanism: `zoom` Property, Not `transform: scale()`").

---

## 1. Scope Recap

Toolbar-header ± buttons zoom the preview pane's own rendered HTML content (text, images, tables, and
rendered math if `docs/plans/v1.2.0/math-rendering/PLAN.md` ships in the same release) — strictly while
`paneMode === "preview"`, not the preview sub-pane in `split` mode. Range 70–180%, 10% steps, default
100%, session-only (resets on tab switch, memo switch, or leaving `preview` mode). Fixed ± buttons only
(no slider, no Ctrl+scroll); clicking the percentage readout resets to 100%.

## 2. State Placement and Reset Wiring

- **New state**, `EditorPane.tsx`, alongside the existing `paneModeOverride`/`liveViewOverride`
  session-scoped hooks:
  ```ts
  const [previewZoomPercent, setPreviewZoomPercent] = useState(100);
  ```
- **Reset conditions — three independent triggers**:
  1. Active tab switched, 2. open memo switched — both collapse to the exact same `loadGeneration`-gated
     reset point already used for `tab-double-click-jump/PLAN.md` §4's `jumpToggleRef` reset and the
     pre-existing `paneModeOverride`/`liveViewOverride`/`previewLockOverride` resets. `setPreviewZoomPercent
     (100)` added as a fifth reset at that same point (all five are "session-only state, cleared on fresh
     memo load").
  3. `paneMode` leaves `"preview"` — not covered by the memo-switch reset point (`paneMode` can change
     without a memo switch, via the pane-mode-cycle button/shortcut), needs its own effect:
     ```ts
     useEffect(() => { if (paneMode !== "preview") setPreviewZoomPercent(100); }, [paneMode]);
     ```
     — same shape already used elsewhere in this file for `paneMode`-driven side effects (e.g.
     `MarkdownToolbar.tsx`'s own auto-minimize-on-`"preview"` effect).
- **Threaded down** as new props to `PreviewPane.tsx` (to apply the zoom CSS, §3) and `MarkdownToolbar.tsx`
  (to render the ± buttons and drive the value, §4) — mirroring how `paneMode`/`onCyclePaneMode` are
  already threaded to both.

## 3. CSS Mechanism

**Decided** (`DECISIONS.md`): the CSS `zoom` property, not `transform: scale()` — `zoom` triggers real
layout reflow at the new size (text rewraps, images/tables resize within their containers), matching the
spec's "affects... text size, images, tables" description of a genuine content-size change.
`transform: scale()` does not reflow layout and would need extra width/height compensation math. `zoom` is
universally supported across every Chromium-based engine — this app's one and only rendering target
(Tauri+WebView2), so no cross-browser concern applies.

```tsx
// PreviewPane.tsx
<div className="ttn-editor-preview-pane-content" style={{ zoom: `${previewZoomPercent}%` }}>
  {/* rendered HTML */}
</div>
```

- **Caveat, not yet visually confirmed**: whether the existing wide-table horizontal-scroll container and
  image `max-width: 100%` rule look correct at the range's upper bound (180%) in a real running preview
  pane is a visual/subjective judgment left as a manual check for the user during/after implementation —
  not something `reviewer-lite` should adjudicate on its own.

## 4. Button Placement and Bounds-Disabled Rendering

- **Placement**: `MarkdownToolbar.tsx`'s header `<div className="ttn-md-toolbar-header">` (already houses
  the live/source toggle, pane-mode toggle, preview-lock, PDF-export, and minimize buttons in sequence,
  lines ~590–641) — the ± / percentage-readout group is a further sibling in that header row,
  **conditionally rendered only while `paneMode === "preview"`** (not always-rendered-but-disabled, since
  the control has no meaningful value/effect outside `"preview"` mode).
- **Concrete markup**, mirroring the existing preview-lock button's `disabled`/icon-swap pattern:
  ```tsx
  {paneMode === "preview" && (
    <div className="ttn-md-toolbar-zoom">
      <button type="button" disabled={previewZoomPercent <= PREVIEW_ZOOM_MIN} onClick={onZoomOut}
        title={`ズームアウト (${zoomOutCombo})`} aria-label="プレビューをズームアウト">－</button>
      <button type="button" onClick={onZoomReset} title="100%にリセット"
        aria-label="ズームを100%にリセット">{previewZoomPercent}%</button>
      <button type="button" disabled={previewZoomPercent >= PREVIEW_ZOOM_MAX} onClick={onZoomIn}
        title={`ズームイン (${zoomInCombo})`} aria-label="プレビューをズームイン">＋</button>
    </div>
  )}
  ```
- **Named constants** (near `PANE_MODE_LABEL` in `MarkdownToolbar.tsx`, or `src/shared/models.ts` if
  co-locating with other numeric-range constants is preferred — `developer-lite` discretion, this
  codebase's existing practice for such bounds is mixed):
  ```ts
  const PREVIEW_ZOOM_MIN = 70;
  const PREVIEW_ZOOM_MAX = 180;
  const PREVIEW_ZOOM_STEP = 10;
  ```
- **At-bounds disabled rendering**: `previewZoomPercent <= 70` disables the － button, `>= 180` disables
  the ＋ button — greyed out, non-interactive, rather than silently no-oping past the limit.
- **Tooltip shows live-bound shortcut**: `zoomOutCombo`/`zoomInCombo` computed via `resolveShortcuts
  (appSettings?.shortcuts)["preview.zoomOut"/"preview.zoomIn"]` — the same `resolveShortcuts` call already
  used elsewhere in this file (e.g. the 18-toolbar-action keydown handler) and in `TabBar.tsx` — so the
  tooltip stays correct if the user rebinds the primary shortcut in Settings. The fixed numpad alias
  (§5) is not separately named in the tooltip (no Settings entry to go stale against).

## 5. Keyboard Shortcut Wiring

### 5.1 New `DEFAULT_SHORTCUTS` Entries

`src/shared/shortcuts.ts`: `"preview.zoomIn": "Ctrl+="`, `"preview.zoomOut": "Ctrl+-"` — ordinary,
rebindable entries, same convention as every other action-id/combo pair there (e.g. `"markdown.pdfExport":
"Ctrl+P"`). Rebindable via Settings → Shortcuts like any other action.

### 5.2 Dispatch Handler

New, **focus-unconstrained** `useEffect` in `EditorPane.tsx`, following the exact pattern already
established for `markdown.paneModeCycle`/`markdown.pdfExport` (both separate `useEffect`s outside the
focus-scoped 18-toolbar-action dispatcher, specifically because those two "must fire regardless of focus
location within the editor pane, including while focus is inside the preview pane's own DOM subtree") —
the identical requirement applies here, since the zoom buttons must work while focus is inside the
(possibly hidden, in `paneMode === "preview"`) preview pane, not just when CodeMirror's `contentDOM`
happens to have focus.

### 5.3 Numpad Alias — Confirmed Asymmetry (Non-Obvious Finding)

`comboFromEvent`'s implementation (`src/shared/shortcuts.ts` lines ~118–127) builds a combo string from
`e.ctrlKey`/`e.altKey`/`e.shiftKey` plus `e.key` — **not** `e.code`. Standard DOM `key` values for numpad
operator keys are the literal printed symbol (`"+"` for `NumpadAdd`, `"-"` for `NumpadSubtract`):

- **Zoom OUT**: `Ctrl+NumpadSubtract`'s `e.key` is `"-"`, identical to the main-row `Ctrl+-`'s `e.key` —
  `comboFromEvent` already produces the same `"Ctrl+-"` string for **both** physical keys, with **zero
  extra code** once `"preview.zoomOut": "Ctrl+-"` is registered. No special-casing needed for this alias.
- **Zoom IN**: `Ctrl+NumpadAdd`'s `e.key` is `"+"`, but the main-row `Ctrl+=`'s `e.key` (no Shift held) is
  `"="` — these are **different** strings, `comboFromEvent` does **not** already alias them. This is the
  one case genuinely needing a separate, hardcoded, non-rebindable check inside the same
  focus-unconstrained handler, checked via `e.code` (not `e.key`, and not routed through
  `resolveShortcuts` — matching "not exposed as its own separate Settings entry"):
  ```ts
  if (e.ctrlKey && e.code === "NumpadAdd") { e.preventDefault(); onZoomIn(); }
  ```
- **Code-comment requirement**: `developer-lite` should document both as intentional aliases in code
  comments (this codebase's own dense-comment convention) so a future reader doesn't mistake the zoom-out
  case's "no extra code" for an oversight.
- **`e.preventDefault()` required for both**: `Ctrl+=`/`Ctrl+-` are also WebView2/Chromium's native
  page-zoom shortcuts (zooming the whole app window, not this app's preview content) — same reasoning as
  `markdown.pdfExport`'s own native-print-dialog conflict handling; both must be intercepted before the
  native browser-zoom behavior fires.
- **Ctrl+scroll explicitly not added** in this pass (deferred, not rejected — would additionally require
  intercepting WebView2's native Ctrl+wheel page-zoom gesture).

## 6. No Schema/Backend Change

`previewZoomPercent` is session-only, plain React state — no `AppSettings`/`MemoEditorSettings` field, no
`schemaVersion` bump, no backend IPC call. Mirrors `tab-double-click-jump/PLAN.md`'s toggle-back state in
being transient, in-memory UI state, though unlike that feature this is a plain reset (not a
restore-on-return toggle).

## 7. Testing / Verification

- In `preview` pane mode: click ＋ repeatedly, confirm 10% steps up to 180%, button visually disables at
  180%; click － repeatedly down to 70%, disables at 70%.
- Click the percentage readout: resets to 100% from any current value.
- `Ctrl+=`/`Ctrl+-` and `Ctrl+NumpadAdd`/`Ctrl+NumpadSubtract` all zoom in/out identically, with focus
  inside the preview pane's own DOM (not just CodeMirror).
- Rebind `preview.zoomIn` in Settings → Shortcuts to a different combo: confirm the new combo works, the
  tooltip updates to show it, and `Ctrl+NumpadAdd` still works unchanged (fixed alias, not rebindable).
- Switch tab, switch memo, or leave `preview` mode (via pane-mode cycle button/shortcut): confirm zoom
  resets to 100% each time.
- Switch from `preview` to `split`: confirm the preview sub-pane in `split` mode is unaffected by whatever
  zoom level was set — always renders at 100% in `split`.
- Confirm the ± button group is not rendered at all outside `paneMode === "preview"`.
- Visual check at 180% zoom: wide table and a wide image both remain within a horizontally-scrollable
  container, no layout breakage (the caveat noted in §3).
