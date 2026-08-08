// Application shell. This component wires up `docs/plans/tab-management/PLAN.md`'s TabBar,
// `docs/plans/memo-tree/PLAN.md`'s TreeView, `docs/plans/editor-save/PLAN.md`'s EditorPane, and
// `docs/plans/tab-lock/PLAN.md`'s lock/unlock session state (see
// docs/plans/project-architecture/PLAN.md).
import { useCallback, useEffect, useRef, useState } from "react";
import { useTabs } from "./state/useTabs";
import { useTree } from "./state/useTree";
import { useSettings } from "./state/useSettings";
import { useTabLock } from "./state/useTabLock";
import { useWindowGeometry } from "./state/useWindowGeometry";
import { useAppearance } from "./state/useAppearance";
import { TabBar } from "./components/TabBar/TabBar";
import { EmptyState } from "./components/TabBar/EmptyState";
import { TreeView } from "./components/Tree/TreeView";
import { EditorPane } from "./components/Editor/EditorPane";
import type { EditorPaneHandle } from "./components/Editor/EditorPane";
import { TreeSplitter } from "./components/Layout/TreeSplitter";
import { locateNode } from "./shared/treeOps";
import { editorClose } from "./ipc/editor";

const DEFAULT_TREE_WIDTH = 260;

function App() {
  const tabsState = useTabs();
  const treeState = useTree(tabsState.activeTabId);
  const settingsState = useSettings();
  const tabLockState = useTabLock(tabsState.activeTabId, tabsState.refresh, treeState.refresh);
  useWindowGeometry(settingsState.settings, settingsState.patch);
  useAppearance(settingsState.settings);

  const editorRef = useRef<EditorPaneHandle>(null);

  // `docs/plans/tab-lock/PLAN.md` §3 `tab_lock` step 2: "If any memo in this tab ... is
  // currently open with unsaved changes, the frontend must force a save-confirm (auto-save-and-
  // close) before calling this command." Only meaningful when `tabId` is the active tab with an
  // open memo — a no-op otherwise, since a background tab can't have anything open in this
  // single-pane editor.
  //
  // The explicit `editorClose` call below is deliberate, not redundant with `useEditor.ts`'s own
  // switch-away cleanup: that cleanup fires from a React effect after `closeOpenMemo`'s state
  // update is committed, which is not guaranteed to happen before this function returns and the
  // caller (`TabBar`'s lock-setup dialog) immediately invokes `tab_lock`. Awaiting it here
  // guarantees the backend's `ActiveMemoTracker` is actually cleared before that IPC call, so
  // `tab_lock`'s step-2 defensive check doesn't spuriously reject the tab the user just had open
  // — the single most common case for this feature.
  const beforeLockTab = useCallback(
    async (tabId: string) => {
      if (tabId === treeState.activeTabId && treeState.openMemoName) {
        const memoName = treeState.openMemoName;
        await editorRef.current?.flushSaveNow();
        treeState.closeOpenMemo();
        await editorClose(tabId, memoName);
      }
    },
    [treeState],
  );

  const openMemoOverride = treeState.openMemoName ? locateNode(treeState.nodes, treeState.openMemoName)?.node.editor : undefined;

  // Cross-tab search result -> "open memo" (`docs/plans/search-index/PLAN.md` §5): reuses
  // `treeState.focusOnMemo`, the same "open memo" path a tree click uses. If the hit is in a
  // different tab, switching tabs re-fetches that tab's tree asynchronously
  // (`state/useTree.ts`'s own `tabId`-change effect), so `focusOnMemo` can't be called until that
  // tree has loaded and the target memo actually appears in `flatOrder` — `pendingSearchFocus`
  // below tracks that and applies it once the newly-active tab's tree catches up. Like
  // `focusOnMemo` itself, this is a no-op if the memo never becomes visible (e.g. nested under a
  // collapsed ancestor) — a pre-existing limitation of that shared mechanism, not something this
  // feature introduces.
  const [pendingSearchFocus, setPendingSearchFocus] = useState<{ tabId: string; memoName: string } | null>(null);
  const openSearchHit = useCallback(
    (tabId: string, memoName: string) => {
      if (tabId === treeState.activeTabId) {
        treeState.focusOnMemo(memoName);
      } else {
        tabsState.selectTab(tabId);
        setPendingSearchFocus({ tabId, memoName });
      }
    },
    [tabsState, treeState],
  );
  useEffect(() => {
    if (!pendingSearchFocus || treeState.activeTabId !== pendingSearchFocus.tabId) return;
    if (!treeState.flatOrder.includes(pendingSearchFocus.memoName)) return;
    treeState.focusOnMemo(pendingSearchFocus.memoName);
    setPendingSearchFocus(null);
  }, [pendingSearchFocus, treeState.activeTabId, treeState.flatOrder, treeState.focusOnMemo]);

  // `memo:` link click -> "open memo" (`docs/plans/link/PLAN.md` §2, §5): same tab-switch +
  // `focusOnMemo` mechanism as `openSearchHit` above, kept as its own independent state machine
  // (not merged into `openSearchHit`) since a resolved link's `memoName` may be `null` for the
  // `memo:TabName/` (tab-only, no memo selected) form — a tab switch with nothing to focus.
  const [pendingLinkFocus, setPendingLinkFocus] = useState<{ tabId: string; memoName: string } | null>(null);
  const openLinkTarget = useCallback(
    (tabId: string, memoName: string | null) => {
      if (tabId === treeState.activeTabId) {
        if (memoName) treeState.focusOnMemo(memoName);
        return;
      }
      tabsState.selectTab(tabId);
      if (memoName) setPendingLinkFocus({ tabId, memoName });
    },
    [tabsState, treeState],
  );
  useEffect(() => {
    if (!pendingLinkFocus || treeState.activeTabId !== pendingLinkFocus.tabId) return;
    if (!treeState.flatOrder.includes(pendingLinkFocus.memoName)) return;
    treeState.focusOnMemo(pendingLinkFocus.memoName);
    setPendingLinkFocus(null);
  }, [pendingLinkFocus, treeState.activeTabId, treeState.flatOrder, treeState.focusOnMemo]);

  // Startup auto-open of the last-opened memo (`[spec §2.6]`: "On startup automatically open the
  // last-opened memo (`AppSettings.lastOpened`). If tab/memo is deleted, do nothing.";
  // `[detailed-spec §5.6]`). Once both `tabsState`/`settingsState` have finished their initial
  // load, switch to `lastOpened.tabId` (if it's still in `AppSettings.tabs`) so its tree fetches
  // and goes through §3.5 reconciliation, then hand off to the effect below via
  // `pendingStartupOpen` — same two-step "switch tab, wait for its tree, then `focusOnMemo`"
  // pattern already used above for cross-tab search hits / `memo:` links. Guarded by a ref so it
  // only ever runs once per app session; a stale/deleted tab or memo silently does nothing (no
  // error, no fallback memo), matching the spec text exactly.
  const startupAutoOpenAttempted = useRef(false);
  const [pendingStartupOpen, setPendingStartupOpen] = useState<{ tabId: string; memoName: string } | null>(null);
  useEffect(() => {
    if (startupAutoOpenAttempted.current) return;
    if (tabsState.loading || settingsState.loading) return;
    startupAutoOpenAttempted.current = true;
    const lastOpened = settingsState.settings?.lastOpened;
    if (!lastOpened) return;
    if (!tabsState.tabs.some((t) => t.id === lastOpened.tabId)) return;
    if (tabsState.activeTabId !== lastOpened.tabId) tabsState.selectTab(lastOpened.tabId);
    setPendingStartupOpen(lastOpened);
  }, [tabsState.loading, settingsState.loading, tabsState.tabs, tabsState.activeTabId, settingsState.settings, tabsState]);
  useEffect(() => {
    if (!pendingStartupOpen) return;
    if (treeState.activeTabId !== pendingStartupOpen.tabId) return;
    // Wait for that tab's tree fetch (and §3.5 reconciliation) to actually finish before deciding
    // — unlike the search/link-focus effects above, this must reach a definitive one-shot
    // decision (found -> open, not found -> do nothing) rather than keep waiting indefinitely.
    if (treeState.loading) return;
    if (treeState.flatOrder.includes(pendingStartupOpen.memoName)) {
      treeState.focusOnMemo(pendingStartupOpen.memoName);
    }
    setPendingStartupOpen(null);
  }, [pendingStartupOpen, treeState.activeTabId, treeState.loading, treeState.flatOrder, treeState.focusOnMemo]);

  // Persist `AppSettings.lastOpened` whenever a memo becomes the open one (`[spec §2.6]`) — covers
  // every "open memo" path (row click, arrow-nav preview, `focusOnMemo`, including the startup
  // auto-open above) since they all funnel through `treeState.openMemoName`. No debounce needed
  // here (unlike the splitter-drag/toolbar-position patches elsewhere in this file) since this
  // isn't edited continuously; guarded against redundant writes (`[detailed-spec §5.6]`'s "don't
  // write if no change") and against firing before settings have loaded. Deliberately does NOT
  // write when a memo closes (`openMemoName` -> `null`, e.g. tab switch/lock) — `lastOpened` tracks
  // the *last* opened memo, not the currently-open one, which is exactly what makes it useful for
  // the startup effect above.
  useEffect(() => {
    if (settingsState.loading || !settingsState.settings) return;
    const tabId = treeState.activeTabId;
    const memoName = treeState.openMemoName;
    if (!tabId || !memoName) return;
    const current = settingsState.settings.lastOpened;
    if (current && current.tabId === tabId && current.memoName === memoName) return;
    void settingsState.patch({ lastOpened: { tabId, memoName } });
  }, [treeState.activeTabId, treeState.openMemoName, settingsState.settings, settingsState.loading, settingsState.patch]);

  // Tree pane width/side/hide-toggle (`docs/plans/window-operation/PLAN.md` §2). `previewTreeWidth`
  // holds the live value while dragging the splitter; `settingsState.patch` is only called once, on
  // drag release — same "preview locally, persist on release" split already used elsewhere in this
  // app (`components/Editor/Toolbar/MarkdownToolbar.tsx`'s toolbar-position drag).
  const [previewTreeWidth, setPreviewTreeWidth] = useState<number | null>(null);
  const windowSettings = settingsState.settings?.window;
  const treeSide = windowSettings?.treeSide ?? "left";
  const treeCollapsed = windowSettings?.treeCollapsed ?? false;
  const treeWidth = previewTreeWidth ?? windowSettings?.treeWidth ?? DEFAULT_TREE_WIDTH;

  const handleSplitterCommit = useCallback(
    (width: number) => {
      setPreviewTreeWidth(null);
      void settingsState.patch({ window: { treeWidth: width } });
    },
    [settingsState],
  );
  const handleToggleTreeCollapsed = useCallback(() => {
    void settingsState.patch({ window: { treeCollapsed: !treeCollapsed } });
  }, [settingsState, treeCollapsed]);

  const treePane = (
    <div
      className={`app-tree-pane app-tree-pane-${treeSide}${treeCollapsed ? " app-tree-pane-collapsed" : ""}`}
      style={{ width: treeCollapsed ? 0 : treeWidth }}
    >
      <TreeView tree={treeState} tabLock={tabLockState} appSettings={settingsState.settings} />
    </div>
  );
  const splitter = (
    <TreeSplitter
      treeWidth={treeWidth}
      collapsed={treeCollapsed}
      side={treeSide}
      onPreview={setPreviewTreeWidth}
      onCommit={handleSplitterCommit}
      onToggleCollapse={handleToggleTreeCollapsed}
    />
  );

  return (
    <div className="app-shell">
      <TabBar
        tabsState={tabsState}
        tabLockState={tabLockState}
        beforeLockTab={beforeLockTab}
        settings={settingsState.settings}
        patchSettings={settingsState.patch}
        applyFetched={settingsState.applyFetched}
        onOpenSearchHit={openSearchHit}
      />
      <div className="app-content">
        {!tabsState.loading && tabsState.tabs.length === 0 ? (
          <EmptyState tabsState={tabsState} />
        ) : (
          <>
            {treeSide === "left" && treePane}
            {treeSide === "left" && splitter}
            <EditorPane
              ref={editorRef}
              tabId={treeState.activeTabId}
              memoName={treeState.openMemoName}
              focusEditorToken={treeState.focusEditorToken}
              appSettings={settingsState.settings}
              memoEditorOverride={openMemoOverride}
              setEditorOverride={treeState.setEditorOverride}
              patchSettings={settingsState.patch}
              onOpenMemoLink={openLinkTarget}
            />
            {treeSide === "right" && splitter}
            {treeSide === "right" && treePane}
          </>
        )}
      </div>
    </div>
  );
}

export default App;
