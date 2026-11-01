# PLAN: Double-Click Active Tab — Jump-to-Position (v1.2.0)

> Status: planning complete, not yet implemented. New feature, clean addition, no existing gesture on any
> tab to reconcile with (confirmed by inspection, `[detailed-spec v1.2.0 §5]`). Depends on `docs/plans/
> tab-management/PLAN.md` (`TabBar.tsx`'s tab `<div>` event wiring), `docs/plans/editor-save/PLAN.md`
> (`EditorPaneHandle` imperative-handle pattern, `flushSaveNow` precedent), and `docs/plans/markdown-
> editing/PLAN.md` (CodeMirror 6 `EditorView`/`ChangeDesc` primitives). Distinct from and does not read or
> write `docs/plans/v1.1.0/tab-memo-memory/PLAN.md`'s persisted `MemoEditorSettings.scrollPosition`/
> `cursorAnchor`/`cursorHead` fields — this feature's toggle-back state is transient, in-memory-only.

Sources: `docs/spec.v1.2.0.md` §5; `docs/detailed-spec.v1.2.0.md` §5, §11.

---

## 1. Scope Recap

Double-clicking a tab that is already the active/selected tab jumps cursor and scroll position to
document start (line 1, column 1 — equivalent to Ctrl+Home). A second double-click on the still-active tab
toggles back to the position saved immediately before the jump; a third double-click (no saved value
present) jumps to document start again, capturing the then-current position anew. Double-clicking an
inactive tab is unaffected — behaves as ordinary single-click tab switching (no special-casing needed, see
§2). Discoverability: hovering the active tab shows a tooltip naming the gesture.

## 2. Trigger Wiring

- **`TabBarProps`**: new field `onActiveTabDoubleClick?: () => void`.
- **`TabBar.tsx`**, tab `<div>` (line ~348's `onClick={() => selectTab(tab.id)}` sibling):
  ```tsx
  onDoubleClick={() => { if (tab.id === activeTabId) onActiveTabDoubleClick?.(); }}
  ```
  The `tab.id === activeTabId` guard is the entire implementation of "inactive tab unaffected" — a
  `dblclick` event fires *in addition to*, not instead of, the two `click` events a native double-click
  sequence already produces, so an inactive tab's double-click still resolves as two ordinary
  single-click selections with no extra code.
- **`App.tsx` wiring**: reuses the existing `editorRef` (`useRef<EditorPaneHandle>(null)`, already used
  for `flushSaveNow`). `EditorPaneHandle` gains a new imperative method:
  ```ts
  interface EditorPaneHandle {
    flushSaveNow: () => void;
    toggleJumpToStart: () => void; // NEW
  }
  ```
  exposed via `EditorPane.tsx`'s existing `useImperativeHandle` call, extended to `{ flushSaveNow: ed.
  flushSaveNow, toggleJumpToStart }`. `App.tsx` passes `onActiveTabDoubleClick={() => editorRef.current
  ?.toggleJumpToStart()}` down to `<TabBar>` — mirrors the existing `flushSaveNow` imperative-handle
  pattern exactly, no new cross-component plumbing style introduced.
- **No-op if no memo open**: `toggleJumpToStart` early-returns if `viewRef.current` is `null` — trivially
  correct since there is no cursor/scroll position to jump from or to.

## 3. Saved-Position Shape and Position-Mapping

No existing code in this app uses `ChangeDesc`/`mapPos()` today — new, self-contained logic living inside
`EditorPane.tsx` (or a small extracted helper module, `developer-lite` discretion) alongside the view's
own lifecycle.

### 3.1 Saved Shape

```ts
const jumpToggleRef = useRef<{ anchor: number; head: number; topLinePos: number } | null>(null);
```

- `anchor`/`head` mirror `EditorSelection.main.{anchor,head}` (character offsets — the same pair shape as
  `MemoEditorSettings.cursorAnchor`/`cursorHead`, GitHub issue #28, but this ref is a **separate,
  transient, in-memory-only pair**, never reused or touched by that persisted feature).
- `topLinePos` is a **document character offset**, not a raw line number: `view.state.doc.line
  (computeTopVisibleLine(view)).from` — storing a position (which `ChangeDesc.mapPos()` operates on
  directly) avoids a second lookup step and differs deliberately from `MemoEditorSettings.
  scrollPosition`'s persisted-line-number convention, which serves a different purpose (cross-session
  anchor vs. intra-session remapping).

### 3.2 `toggleJumpToStart()` Logic

1. **`jumpToggleRef.current === null`** (first double-click, or state was cleared per §4): capture current
   position into the ref (`{anchor, head}` from `view.state.selection.main`, `topLinePos` as above), then
   dispatch a jump to document start:
   ```ts
   view.dispatch({ selection: { anchor: 0, head: 0 }, effects: EditorView.scrollIntoView(0, { y: "start" }) });
   ```
   (reuses `EditorView.scrollIntoView`, the same effect primitive `resolveAndScrollToHeadingAnchor`/
   `scrollToTopVisibleLine` already use elsewhere — not a new scroll primitive).
2. **`jumpToggleRef.current !== null`** (second double-click, toggle-back): dispatch a restore using the
   already-remapped values (§3.3):
   ```ts
   view.dispatch({
     selection: { anchor: saved.anchor, head: saved.head },
     effects: EditorView.scrollIntoView(saved.topLinePos, { y: "start" }),
   });
   ```
   then `jumpToggleRef.current = null` — a further double-click starts fresh.

### 3.3 Continuous Remapping Across Edits

New branch inside `EditorPane.tsx`'s existing `EditorView.updateListener.of((update) => {...})` (already
present, ~line 496, for char/line/cursor status-bar refresh) — extended so that whenever `update.docChanged`
is true **and** `jumpToggleRef.current` is non-`null`, all three saved values remap through `update.changes`
(a `ChangeDesc`) before being written back:

```ts
if (update.docChanged && jumpToggleRef.current) {
  jumpToggleRef.current = {
    anchor: update.changes.mapPos(jumpToggleRef.current.anchor),
    head: update.changes.mapPos(jumpToggleRef.current.head),
    topLinePos: update.changes.mapPos(jumpToggleRef.current.topLinePos),
  };
}
```

- `ChangeDesc.mapPos()`'s own built-in behavior (position after an insertion shifts forward; position
  inside a deleted range collapses to the deletion's edge) is exactly the "adjusted for lines/characters
  inserted or removed before it" guarantee the spec describes — no custom remapping math needed.
- **Multi-transaction updates**: `update.changes` already represents the *combined* `ChangeDesc` for the
  whole update regardless of how many individual changes composed it (e.g. a multi-region edit) — a single
  `mapPos()` call per saved value already handles this correctly, no per-sub-change loop needed.
- **Interaction with the unrelated persisted `cursorAnchor`/`cursorHead` feature** (GitHub issue #28): a
  document-start jump moves the real CodeMirror selection to position 0, which is visible to that
  feature's own debounced-scroll/memo-switch flush exactly as any other cursor movement would be. If the
  user jumps and then switches tabs/memos **without** toggling back, position 0 gets persisted as this
  memo's long-term `cursorAnchor`/`cursorHead`/`scrollPosition` for next app restart — this is not a
  special case introduced by this feature, it is the same outcome as manually pressing Ctrl+Home and
  leaving without moving back, which already persists identically today. No special interaction code
  needed between the two features.

## 4. Reset Conditions

- **Concrete reset point**: `EditorPane.tsx`'s existing view-(re)creation `useEffect`, gated on
  `ed.loadGeneration` (bumps on every memo switch — already resets `liveViewOverride`/`paneModeOverride`/
  `previewLockOverride` to `null` at lines ~514–516 for the identical "session-only state must not leak
  into a newly-opened memo" reason). `jumpToggleRef.current = null;` is added as a fourth reset alongside
  those three, at the same point in the same effect.
- This single reset point covers both of the spec's discard triggers ("active tab switched away from" and
  "open memo within the tab switched"), since both necessarily go through a `loadGeneration` bump —
  including a tab switch to a tab with no open memo. **`developer-lite` must confirm the reset runs before
  the effect's `!ed.content` early-return branch** (line ~448), not after, so the reset isn't skipped for
  the "switched to an empty tab" case.
- **An edit alone does NOT discard the saved value** — superseded by §3.3's position-mapping; this is the
  entire point of using `ChangeDesc.mapPos()` instead of a raw static position.
- **Switching away and back to the same memo needs no special-casing**: switching away discards the ref
  regardless of whether the user later returns to the very same memo — no exception carved out, the plain
  "any `loadGeneration` bump clears it" rule already produces the specified behavior.

## 5. Discoverability Tooltip

`TabBar.tsx`'s tab `<div>` already sets `title={displayName}` (line ~354, the tab's only tooltip today) —
becomes conditional on active state:

```tsx
title={tab.id === activeTabId ? `${displayName} (ダブルクリックで先頭へ)` : displayName}
```

No new element, no new CSS — a plain native-tooltip (`title` attribute) extension, consistent with every
other tooltip in this app being a plain `title` attribute rather than a custom tooltip component. Exact
copy folds into `docs/plans/v1.2.0/i18n/PLAN.md`'s translation maps when that plan's extraction pass
reaches `TabBar.tsx`.

## 6. Testing / Verification

- Double-click active tab with cursor mid-document: cursor and scroll jump to line 1, column 1.
- Double-click again (toggle-back): cursor/scroll restore to the exact pre-jump position.
- Triple-click sequence (jump, no toggle-back, jump again): third double-click captures the
  then-current (post-first-jump) position, not the original.
- Edit the document between jump and toggle-back (insert/delete lines before the saved position): confirm
  toggle-back still lands at the same *logical* location, not a now-stale raw offset.
- Double-click an inactive tab: behaves as plain single-click tab switch, no jump.
- Switch tabs (or switch memo within the tab) after jumping but before toggling back, then switch back:
  confirm the saved toggle-back value is gone — a further double-click acts as a fresh first double-click.
- Double-click the active tab when no memo is open in it: no-op, no crash.
- Hover the active tab: tooltip includes the double-click hint; hover an inactive tab: tooltip shows only
  the plain display name.
