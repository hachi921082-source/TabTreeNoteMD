# PLAN: Cross-Tab/Cross-Memo Heading Anchor Links, `memo:TabName/MemoName#slug` (v1.2.0)

> Status: planning complete, not yet implemented. Extends `docs/plans/link/PLAN.md` (`memo:` link
> detection/resolution, `resolveAndOpenMemoLink`, `link_resolve` backend command) and `docs/plans/
> table-of-contents/PLAN.md` (`scanHeadings`/`slugifyHeading`, the same-memo `#slug` baseline this
> feature extends into a cross-memo form). Also depends on `docs/plans/v1.1.0/tab-memo-memory/PLAN.md`
> (the `pendingXxxFocus` cross-tab-navigation state-machine convention this feature's own
> `pendingLinkFocus.anchorSlug` extension follows) and `docs/plans/tab-lock/PLAN.md` §... (session-aware
> `find_tab_id_by_display_name`, unmodified by this feature but load-bearing for §5 below).
>
> **This plan defines the shared cross-cutting primitive from `[detailed-spec v1.2.0 §10.1]`** (the
> "preview-render-readiness gate," needed by both this feature and `docs/plans/v1.2.0/preview-scroll-
> fix/PLAN.md`) — see §4 below. `preview-scroll-fix/PLAN.md` cross-references this section rather than
> repeating it, per `docs/plans/README.md`'s "referenced by name from the others rather than repeated"
> convention for foundational/shared pieces.
>
> **Prerequisite**: this feature's `preview`/`split`-mode behavior depends on `docs/plans/v1.2.0/
> preview-link-encoding-fix/PLAN.md` being implemented first (or in the same pass) — without that fix, a
> non-ASCII tab/memo name's `memo:` half fails to resolve at all when clicked from the preview pane,
> independent of whether an anchor half is present. This feature's own code (§2 below) requires no
> decode step of its own; it inherits already-decoded input once the encoding-fix plan lands (see §2's own
> note and `preview-link-encoding-fix/PLAN.md` §3).

Sources: `docs/spec.v1.2.0.md` §4; `docs/detailed-spec.v1.2.0.md` §4, §10.1, §10.3.

---

## 1. Scope Recap

Extends the existing `memo:` link syntax with an optional `#slug` anchor suffix:
`memo:TabName/MemoName#slug` and the symmetric same-tab form `memo:MemoName#slug`. Not applicable to the
tab-only form (`memo:TabName/#slug` is treated as any other unresolved/malformed link). Pure additive
syntax extension — every `memo:` link written today without `#` continues to resolve exactly as before.

Resolution is two independent steps: **step 1** resolves the memo target exactly as today (unchanged
`link_resolve` matching); **step 2**, if `#slug` is present, resolves the anchor within the now-identified
target memo, once it is actually open, reusing the existing same-memo `scanHeadings`/slugify mechanism —
no new heading-scan or slugify implementation.

## 2. Target-String Parsing — `resolveAndOpenMemoLink` Signature Change

No backend change: `BARE_MEMO_RE` (`src/editor/links.ts`) already matches a full
`memo:TabName/MemoName#slug` token as one unit (`#` is not in its excluded-character set), so live-view's
bare-token scan and the `[text](memo:...)` markdown-link case need no detection change — only
**resolution** changes.

The `#slug` suffix is split off **client-side, before** calling `linkResolve` (`src/ipc/link.ts`), inside
`src/editor/links.ts`'s `resolveAndOpenMemoLink` — the single function both live-view and preview-pane
`memo:` click handling already funnel through:

```ts
export async function resolveAndOpenMemoLink(
  currentTabId: string | null,
  rawTarget: string,
  onOpenMemo: (tabId: string, memoName: string | null, anchorSlug: string | null) => void, // +1 param
): Promise<void> {
  if (!currentTabId) return;
  const stripped = rawTarget.startsWith("memo:") ? rawTarget.slice("memo:".length) : rawTarget;
  const hashIndex = stripped.indexOf("#");
  const target = hashIndex === -1 ? stripped : stripped.slice(0, hashIndex);
  const anchorSlug = hashIndex === -1 ? null : stripped.slice(hashIndex + 1);
  const result = await linkResolve(currentTabId, target);
  if (!result.ok || result.value.kind !== "memo") return;
  onOpenMemo(result.value.tabId, result.value.memoName, anchorSlug);
}
```

- **No decode step here**: `rawTarget` reaching this function is already clean of `normalizeLink`
  encoding by the time it arrives — `preview-link-encoding-fix/PLAN.md` §3 decodes the `href` *upstream*,
  at the preview pane's own click-handler call site, before `resolveAndOpenMemoLink` is ever invoked; a
  live-view-sourced `rawTarget` was never encoded to begin with (read straight from the raw CodeMirror
  buffer). This function stays agnostic to which pane called it.
- **`memo:TabName/#slug` (tab-only, stray anchor)**: the split logic still runs and extracts an anchor,
  but `target` is `"TabName/"` (trailing slash, empty memo-name portion); `link_resolve`'s existing
  behavior for that shape already returns `memoName: null`. §3's step-2 anchor resolution is gated on a
  specific memo actually being opened, so a `null` `memoName` naturally means step 2 never runs — no extra
  special-casing needed.
- **Rust/`src-tauri/` unchanged**: `link_resolve` keeps matching exactly the memo-name portion it already
  does today, unaware `#slug` ever existed.

## 3. Cross-Tab Navigation State Machine — `pendingLinkFocus.anchorSlug`

Extends the existing `pendingLinkFocus` state machine (`src/App.tsx`) — one of three `pending<Source>Focus`
machines already following the identical shape: set a pending `{tabId, memoName}` record when a same-tick
tab switch is needed, then a `useEffect` watching `treeState.activeTabId`/`treeState.flatOrder` calls
`treeState.focusOnMemo(memoName)` once the target tab's tree has finished loading — consumed exactly once,
silently dropped if the memo is never found.

- **Type extension**: `pendingLinkFocus: { tabId: string; memoName: string; anchorSlug: string | null }`.
  `openLinkTarget` (`App.tsx`, passed as `EditorPane`'s `onOpenMemoLink` prop and
  `previewInteractions.ts`'s `config.onOpenMemoLink`) receives the extra `anchorSlug` param from §2's
  updated signature and threads it into `setPendingLinkFocus`.
- **Both branches of `openLinkTarget` need the deferred-anchor treatment** — including the existing
  early-return "same-tab, no tab switch needed" branch (`tabId === treeState.activeTabId`). Opening a
  *different* memo within the same tab is already async (`treeState.focusOnMemo(memoName)` triggers
  `EditorPane.tsx`'s memo-load effect via an IPC `editor_load` round-trip), so the anchor cannot be applied
  synchronously even in the same-tab case — this branch routes through the same pending-anchor mechanism
  as the cross-tab case, not a separate code path.
- **Where the anchor is consumed**: NOT inside `App.tsx`'s `pendingLinkFocus`-consuming `useEffect` (it
  only has `treeState` — no access to the mounted `EditorView`/preview-DOM). Instead, that effect, once it
  calls `treeState.focusOnMemo(memoName)`, additionally stashes `anchorSlug` into a new ref threaded down
  into `EditorPane.tsx` as a prop, e.g. `pendingAnchorSlug: string | null` (alongside the existing
  `focusEditorToken`/`onOpenMemoLink` props) — consumed inside `EditorPane.tsx`'s own view-(re)creation
  effect, at the point the newly-loaded memo's `EditorView` has just been created (line ~501–525, right
  after `viewRef.current = view;` and the existing `focusEditorTokenRef` consumption). Mirrors the exact
  same "one-shot signal, consumed via a ref comparison, at the moment a fresh view exists for the
  just-navigated-to memo" pattern `focusEditorToken`/`consumedFocusTokenRef` already establishes.
- A sibling `consumedAnchorSlugRef` (or a single generation-token approach covering both auto-focus and
  anchor-jump) avoids re-applying a stale anchor jump on a later, unrelated re-render — `developer-lite`
  discretion on exact shape.

## 4. Anchor-Jump Execution, Pane-Mode-Dependent

Split by the `paneMode` of the memo just opened (`MemoEditorSettings.paneMode ?? AppSettings.
editorDefaults.paneMode`) — required because `[MDEditor-spec §2]`'s "editable-pane show/hide is CSS-only"
design (`display:none`, not unmounted) makes the `edit`/`split` execution path unreliable once
`paneMode === "preview"`.

### 4.1 `edit`/`split` Pane Mode

Reuse `src/editor/links.ts`'s existing `resolveAndScrollToHeadingAnchor(view, slug)` **unchanged** — the
exact function the same-memo case already calls (dispatches `{selection: {anchor: match.from}, effects:
EditorView.scrollIntoView(...)}`). Works correctly for `split` mode with no preview-specific code, because
`PreviewPane.tsx`'s own scroll-sync listener (`view.scrollDOM.addEventListener("scroll", ...)`) fires from
the real DOM `scroll` event this dispatch produces, driving `scrollPreviewToLine` automatically —
`split` mode's preview half is a side effect, not a separate code path.

### 4.2 `preview` Pane Mode

The `edit`/`split` path is unreliable here (a zero-height `scrollDOM` cannot compute a meaningful scroll
offset — the same underlying mechanism `preview-scroll-fix/PLAN.md`'s bug independently establishes for
the reverse/save direction). Instead:

- Act **directly on the preview container**, reusing `preview-link-encoding-fix/PLAN.md`'s now-fixed
  `handleAnchorClick` `#`-branch logic (`previewInteractions.ts`) verbatim:
  `container.querySelector('#' + CSS.escape(slug))?.scrollIntoView({block: "start"})` — triggered
  programmatically from §3's ref-consumption point instead of a real click event.
- **No `decodeURIComponent` needed on this programmatic path** — the slug already came from §2's clean
  string-splitting by the time it reaches here, regardless of whether the original click was on a
  preview-pane `<a href="memo:...">` (decoded upstream by `preview-link-encoding-fix/PLAN.md`) or a
  live-view bare token/link (never encoded to begin with).

## 4.3 Shared Primitive — Preview-Render-Readiness Gate (owned by this plan)

Both this feature's §4.2 and `preview-scroll-fix/PLAN.md`'s restore path independently need the same
condition: "wait until `PreviewPane`'s rendered `html` actually reflects the target memo before reading/
querying its DOM." `PreviewPane.tsx`'s `html` state does **not** update synchronously on a memo switch — it
is recomputed after `AppSettings.preview.debounceMs` (default 150ms) from the doc-change event the new
`EditorView`'s creation triggers; until that timer fires, `html` still holds the previous memo's rendered
markup (`PreviewPane` is never unmounted across a memo switch while `paneMode !== "edit"`).

**Mechanism, defined once here, reused by `preview-scroll-fix/PLAN.md`**:

- `PreviewPane.tsx`'s existing debounced-render effect (`setTimeout(() => { if (view) setHtml
  (renderMarkdownToHtml(view.state).html); }, debounceMs)`) already has access to `memoKey`
  (`${tabId}:${memoName}`, already computed in `EditorPane.tsx` and passed down). Track which `memoKey` the
  **current** `html` was actually rendered for: a new `renderedForMemoKeyRef` ref, set at the same moment
  `setHtml(...)` is called inside that timeout callback.
- **Consumption**: a small exported helper/hook, e.g. `waitForPreviewRender(previewHandle, targetMemoKey,
  { timeoutMs })`, that polls (or subscribes via a `useEffect` watching `html`/a render-tick counter)
  until `renderedForMemoKeyRef.current === targetMemoKey`, then resolves. A bounded timeout (a few
  seconds, mirroring PDF export's existing per-image wait precedent, `[MDEditor-spec §6]`) guards against
  waiting forever if the memo is closed again before its preview ever renders — falling back to simply not
  performing the jump/restore, consistent with this app's "silent no-op on edge case" convention.
- **Exposure shape**: `developer-lite` discretion — either an imperative handle method
  (`PreviewPaneHandle.waitForRender(memoKey)`) mirroring `EditorPaneHandle`'s own existing pattern
  (`flushSaveNow`), or a callback prop `PreviewPane` invokes once per completed render. Implemented inside
  `PreviewPane.tsx` itself; no new component/module required.
- §4.2 above and `preview-scroll-fix/PLAN.md`'s restore path both call this same primitive before their
  respective `querySelector`/`scrollPreviewToLine` call, instead of each inventing its own ad hoc
  poll/timeout.

## 5. Tab Lock and Unresolved Cases

All three cases fall out of §2/§4's existing mechanisms with **zero additional code** (confirmed by direct
inspection of `src-tauri/src/commands/link.rs`'s `find_tab_id_by_display_name`):

- **Locked target tab, no active session**: `find_tab_id_by_display_name` returns `None` (locked, no
  session), so `link_resolve` returns `Unresolved` before ever reaching memo-tree/heading logic. §2's `if
  (!result.ok || result.value.kind !== "memo") return;` short-circuits before `anchorSlug` is ever read or
  `onOpenMemo` is called — the entire link (memo and anchor) is a full no-op by construction.
- **Locked target tab, active unlock session**: `find_tab_id_by_display_name` matches against
  `sessions.with_session(&entry.id, |s| s.real_tab_name.clone())` instead of the placeholder name, so
  `link_resolve` resolves normally. §2/§4 have no lock-awareness of their own — an active-session target
  flows through both steps identically to an unlocked tab.
- **Memo resolves, no heading matches `slug`**: `edit`/`split`'s `resolveAndScrollToHeadingAnchor` already
  no-ops on no match (`if (!match) return;`, never touching the already-completed memo switch); `preview`
  mode's `container.querySelector(...)?.scrollIntoView(...)` already no-ops via optional chaining on a
  `null` query result. Both paths leave the target memo open and visible, only the scroll/cursor jump is
  skipped.

**Test coverage requirement** (no new production code, but explicit test cases needed since these are easy
to silently break with an unrelated future change to `link_resolve` or the anchor-jump functions): a
locked-no-session `memo:...#slug` link; a locked-with-session `memo:...#slug` link; a `memo:...#slug` link
whose memo resolves but whose slug matches no heading.

## 6. Testing / Verification

- `memo:OtherTab/SomeMemo#heading-slug` from live view, target tab not currently active: tab switches,
  memo opens, cursor/scroll lands on the matching heading, in `edit`, `split`, and `preview` pane modes.
- Same-tab form `memo:SomeMemo#heading-slug` where a *different* memo in the same tab is currently open:
  memo switches within the tab, anchor jump still applies (exercises §3's "both branches need deferred
  treatment" point).
- `memo:TabName/#slug` (tab-only, stray anchor): tab opens, no specific memo selected, no anchor applied,
  no error.
- Target memo resolves in `preview` mode but its preview hasn't finished rendering yet at the moment of
  navigation (simulate with an artificially long `debounceMs` or a large memo): confirm the jump still
  lands correctly once rendering catches up (exercises §4.3's readiness gate), rather than silently
  failing or jumping against stale content.
- The three Tab Lock/unresolved-case scenarios from §5.
- A `memo:` link with a non-ASCII tab/memo name and a non-ASCII heading slug, clicked from the preview
  pane: confirms this feature composes correctly with `preview-link-encoding-fix/PLAN.md`'s fix (both
  halves of the link resolve).
