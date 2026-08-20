# PLAN: Preview-Only-Mode Scroll-Position Tracking Fix (v1.2.0, Bug Fix)

> Status: planning complete, not yet implemented. Bug fix to `docs/plans/v1.1.0/tab-memo-memory/PLAN.md`'s
> existing per-memo `scrollPosition`/`lastViewedMemo` restore feature — extends its save/restore touch
> points in `EditorPane.tsx`, no new schema field. Depends on `docs/plans/markdown/markdown-preview/
> RENDERING.md` §6 (the existing one-directional editor→preview scroll-sync design, `scrollPreviewToLine`,
> `data-source-line` scheme) and `docs/plans/v1.2.0/cross-tab-anchor-links/PLAN.md` §4.3 (this plan
> **reuses that plan's preview-render-readiness gate primitive** rather than defining its own — see §3
> below).

Sources: `docs/spec.v1.2.0.md` §8; `docs/detailed-spec.v1.2.0.md` §8, §10.1, §10.3.

---

## 1. Bug Recap

`[spec v1.1.0 §2]`'s per-memo `scrollPosition`/`lastViewedMemo` restore feature reads/writes
`scrollPosition` exclusively via `computeTopVisibleLine(view)` — the CodeMirror editor's own scroll state
(`EditorPane.tsx`). The editor↔preview scroll sync is explicitly one-directional by design ("editor scroll
drives preview scroll, never the reverse," `markdown/markdown-preview/RENDERING.md` §6). While `paneMode
=== "preview"`, the edit sub-pane is `display:none` and the user scrolls the preview pane directly — an
action that never touches the editor's `scrollDOM`, so `computeTopVisibleLine(view)` stays frozen at
wherever the editor was last scrolled before pane-mode became `"preview"`. **Consequence**: reading
through a memo in preview-only mode never updates the persisted `scrollPosition`; on next open, the memo
restores to a stale pre-preview-mode position instead of where the user actually left off reading. Not an
Edit Lock issue — affects every preview-only-mode memo, locked or not.

## 2. New Function: `computeTopVisiblePreviewLine`

New sibling export in `src/editor/preview/scrollSync.ts` (the same file already housing the
one-directional `scrollPreviewToLine(container, targetLine)`):

```ts
export function computeTopVisiblePreviewLine(container: HTMLElement): number | null {
  const blocks = container.querySelectorAll<HTMLElement>("[data-source-line]");
  let best: HTMLElement | null = null;
  for (const el of blocks) {
    if (el.offsetTop <= container.scrollTop) best = el; // last-match-wins scan, same shape as scrollPreviewToLine
    else break;
  }
  if (!best) return null; // no rendered block yet — see §4 (readiness gate)
  return Number(best.dataset.sourceLine) + 1; // +1: data-source-line is 0-indexed, scrollPosition is 1-indexed
}
```

- Returns a **1-indexed document line number**, matching `computeTopVisibleLine`'s own convention exactly,
  so both feed `MemoEditorSettings.scrollPosition` (a 1-indexed document line number) identically
  regardless of which pane mode produced the value.
- The `+ 1` conversion reuses the exact same 0-indexed-to-1-indexed convention `previewInteractions.ts`'s
  existing `handleCheckboxClick` already performs for an unrelated purpose (`lineIndex0 + 1` — CodeMirror
  lines are 1-indexed, markdown-it's `.map` is 0-indexed) — a code comment cross-referencing that existing
  site is worth adding so a future reader sees the two are deliberately consistent.
- **Scan basis differs from `scrollPreviewToLine`'s own**: `scrollPreviewToLine` compares `data-source-
  line` values against a target *line number* (an approximate "closest value ≤" match). This function
  instead compares each block's `offsetTop` (pixel position) against `container.scrollTop` (also pixel) —
  the correct basis for "which block is currently topmost in the viewport," mirroring
  `computeTopVisibleLine`'s own `view.lineBlockAtHeight(view.scrollDOM.scrollTop)` pixel-based lookup on
  the CodeMirror side, not a line-number-based one.

## 3. Save-Path and Restore-Path Touch Points

- **Save path**: `EditorPane.tsx`'s `persistEditorViewState`/`flushOutgoingEditorViewState` (lines
  ~389–424, both currently call `computeTopVisibleLine(view)` unconditionally) each need a
  `paneMode === "preview"` branch:
  ```ts
  const savedLine = paneMode === "preview"
    ? computeTopVisiblePreviewLine(previewContainerRef.current)
    : computeTopVisibleLine(view);
  if (savedLine !== null) { /* write scrollPosition = savedLine */ }
  // if null (no rendered blocks yet), skip the write — don't overwrite a previously-saved value with a wrong/zero one
  ```
  Requires `EditorPane.tsx` to hold a ref to the mounted preview container — either threaded up from
  `PreviewPane.tsx` via a new imperative handle/callback ref (mirroring how `EditorPane` already exposes
  its own handle to `App.tsx`), or `PreviewPane.tsx` exposes a `getTopVisibleLine()` method for
  `EditorPane.tsx` to call at flush time. `developer-lite` discretion on exact shape (same class of choice
  as `cross-tab-anchor-links/PLAN.md` §4.3's readiness-gate exposure).
  - **Skip-write-on-`null` is a `developer-lite`-level edge-case call**, not explicitly spelled out in the
    spec but consistent with this app's general "don't write if no change" persistence philosophy — a
    `null` result (flushing before the debounced preview render ever completed for this memo) should never
    persist a wrong/zero `scrollPosition`.
- **Restore path**: the `savedScrollLine`-driven `requestAnimationFrame(() => scrollToTopVisibleLine(view,
  savedScrollLine))` block (`EditorPane.tsx` lines ~527–531) needs an analogous `paneMode === "preview"`
  branch:
  ```ts
  scrollPreviewToLine(previewContainer, savedScrollLine - 1); // inverse -1, undoing §2's +1
  ```
  (note the inverse `- 1` conversion: `scrollPreviewToLine`'s own `targetLine` parameter is compared
  directly against the 0-indexed `data-source-line` attribute). Gated on the preview container actually
  reflecting the **target memo** first — see §4.
- **`edit`/`split` mode unaffected**: both branches' existing, non-`"preview"` code path is untouched —
  the new logic is purely an added `if (paneMode === "preview") { ... } else { /* existing code,
  unchanged */ }` branch at each touch point, not a rewrite.

## 4. Timing Gate — Reuses `cross-tab-anchor-links/PLAN.md`'s Shared Primitive

The restore path above needs the exact same "wait until the preview container's rendered content actually
belongs to the just-opened memo" condition `docs/plans/v1.2.0/cross-tab-anchor-links/PLAN.md` §4.3 already
defines (`PreviewPane.tsx`'s `html` state does not update synchronously on a memo switch — it lags by up
to `AppSettings.preview.debounceMs`, and the stale previous memo's HTML remains in the DOM until the
debounced re-render replaces it).

**This plan does not define its own version of that mechanism** — it calls the same
`waitForPreviewRender(previewHandle, targetMemoKey, { timeoutMs })` primitive (or whichever exact exposure
shape `cross-tab-anchor-links/PLAN.md` §4.3 settles on at implementation time) before invoking
`scrollPreviewToLine` in the restore path. If `cross-tab-anchor-links` is implemented after this plan, the
restore-path call site here is a placeholder pending that primitive landing — `developer-lite` should
coordinate ordering (or implement the shared primitive once, from whichever of the two plans is built
first) rather than each plan writing its own ad hoc poll/timeout, per `docs/plans/README.md`'s
foundational-piece convention.

## 5. Scope Boundary — Not Live Bidirectional Sync

Explicitly excluded from this fix: live bidirectional syncing between preview and editor scroll at the
moment the user switches pane modes mid-session (e.g. jumping the editor to match wherever preview last
was, the instant the user switches from `preview` back to `split`/`edit`). This fix is scoped strictly to
the persist-on-leave/restore-on-open snapshot mechanism `[spec v1.1.0 §2]` already established, not a new
live-sync feature.

- **Confirmed honored by construction**: both touch points (§3) are gated on `ed.loadGeneration` (a fresh
  memo load/switch) and on unmount/outgoing-memo flush — **not** on `paneMode` itself changing while the
  same memo stays open. `EditorPane.tsx`'s existing debounced-scroll `scrollHandler` is bound to the
  editor's own `scrollDOM`, which the user never touches while `paneMode === "preview"` (edit sub-pane
  `display:none`) — so toggling `paneMode` back and forth on an already-open memo triggers neither the
  save-path branch nor any new live-sync behavior; only the pre-existing scroll-debounce/switch-away/
  unmount trigger points (from `[spec v1.1.0 §2]`) ever run this fix's code.

## 6. Testing / Verification

- Open a long memo in `preview` pane mode, scroll down partway through, switch to a different memo, switch
  back: confirm the memo restores to (approximately) the same reading position, not the pre-preview-mode
  editor scroll position.
- Same test in `edit` and `split` pane modes: confirm unchanged, unaffected behavior (existing baseline).
- Scroll in preview mode, then immediately switch tabs before the debounced preview render for the *next*
  memo has completed: confirm no crash and no wrong `scrollPosition` write (exercises the `null`-return
  skip-write branch).
- Toggle `paneMode` back and forth (`preview → split → preview`) on the same open memo without switching
  memos: confirm no unexpected scroll jump or persisted-value change from the toggle alone (§5's boundary).
- Reopen the app entirely (restart) after leaving a memo scrolled partway through in preview-only mode:
  confirm the restored position matches on-disk-persisted `scrollPosition`, across the actual app restart,
  not just an in-session tab switch.
