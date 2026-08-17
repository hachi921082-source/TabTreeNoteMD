// Application shell. This component wires up `docs/plans/tab-management/PLAN.md`'s TabBar,
// `docs/plans/memo-tree/PLAN.md`'s TreeView, `docs/plans/editor-save/PLAN.md`'s EditorPane, and
// `docs/plans/tab-lock/PLAN.md`'s lock/unlock session state (see
// docs/plans/project-architecture/PLAN.md).
import { useCallback, useEffect, useRef, useState } from "react";
import { useTabs } from "./state/useTabs";
import { useTree } from "./state/useTree";
import { useTabView } from "./state/useTabView";
import { useSettings } from "./state/useSettings";
import { useTabLock } from "./state/useTabLock";
import { useTabLockAutoRelock } from "./state/useTabLockAutoRelock";
import { useToast } from "./state/useToast";
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
  const tabView = useTabView(treeState);
  const settingsState = useSettings();
  const tabLockState = useTabLock(tabsState.activeTabId, tabsState.refresh, treeState.refresh);
  // Single app-wide notification queue (`state/useToast.ts`) — shared by `TreeView`'s folder-import
  // toasts and `useTabLockAutoRelock`'s inactivity warning/auto-relock toasts below, so both render
  // in one stack instead of two independently-positioned ones (`docs/plans/v1.1.0/
  // tab-lock-auto-relock/PLAN.md` §6).
  const toastState = useToast();

  // Ordinary in-session tab switch (click, `tab.switchNext`/`switchPrev` shortcuts — both funnel
  // through `TabBar`'s own `selectTab` calls) also queues a `restoreTabView` request
  // (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §5). Deliberately NOT used by `openSearchHit`/
  // `openLinkTarget` below (those call `tabsState.selectTab` directly, unwrapped) — selecting a
  // cross-tab search hit or `memo:` link target opens a *specific* memo itself, bypassing
  // `restoreTabView` for that switch (`PLAN.md` §6).
  const selectTabAndRestore = useCallback(
    (id: string) => {
      tabsState.selectTab(id);
      tabView.requestRestore(id);
    },
    [tabsState, tabView],
  );
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

  // `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md` §7: the inactivity-timeout/app-focus-loss
  // teardown triggers reuse `beforeLockTab` verbatim as their own "flush pending edit, then close"
  // step — same routine `tab_lock` already uses before locking a tab, no new teardown logic.
  useTabLockAutoRelock({
    activeTabId: tabsState.activeTabId,
    tabLockState,
    settings: settingsState.settings,
    toast: toastState,
    flushAndCloseActiveMemo: beforeLockTab,
  });

  const openMemoOverride = treeState.openMemoName ? locateNode(treeState.nodes, treeState.openMemoName)?.node.editor : undefined;
  // Live Edit Lock state for the open memo (GitHub issue #12) — read from the current tree state
  // (already kept in sync by `useTree.ts`'s `setEditLock` on every successful toggle) rather than
  // `editor_load`'s load-time snapshot, so locking/unlocking a memo while it's open takes effect
  // immediately. Mirrors the legacy app's equivalent live `findTreeNodeInScope(...).locked` check.
  const openMemoLocked = treeState.openMemoName
    ? (locateNode(treeState.nodes, treeState.openMemoName)?.node.locked ?? false)
    : false;

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

  // Quick Open result -> "open memo" (`docs/plans/v1.1.0/quick-open/PLAN.md` §3): identical
  // tab-switch + `focusOnMemo` mechanism as `openSearchHit` above (`focusOnMemo` itself already
  // calls `tree_set_last_viewed` via `useTree.ts`'s `persistLastViewed`, which is exactly this
  // feature's required `restoreTabView`-bypass — `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §6).
  // Kept as its own independent pending-focus state machine, not merged into `openSearchHit`, same
  // "each open-memo trigger source gets its own state machine" convention `openLinkTarget` below
  // already follows.
  const [pendingQuickOpenFocus, setPendingQuickOpenFocus] = useState<{ tabId: string; memoName: string } | null>(null);
  const openQuickOpenHit = useCallback(
    (tabId: string, memoName: string) => {
      if (tabId === treeState.activeTabId) {
        treeState.focusOnMemo(memoName);
      } else {
        tabsState.selectTab(tabId);
        setPendingQuickOpenFocus({ tabId, memoName });
      }
    },
    [tabsState, treeState],
  );
  useEffect(() => {
    if (!pendingQuickOpenFocus || treeState.activeTabId !== pendingQuickOpenFocus.tabId) return;
    if (!treeState.flatOrder.includes(pendingQuickOpenFocus.memoName)) return;
    treeState.focusOnMemo(pendingQuickOpenFocus.memoName);
    setPendingQuickOpenFocus(null);
  }, [pendingQuickOpenFocus, treeState.activeTabId, treeState.flatOrder, treeState.focusOnMemo]);

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

  // Startup restore (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §5 "App startup"; supersedes the
  // old app-level `lastOpened.memoName` mechanism — `AppSettings.lastOpened` is now `{ tabId } |
  // null`, `[spec §2.6]`'s "On startup automatically open the last-opened memo" now means "switch
  // to that tab, then apply the same per-tab restore an ordinary tab switch uses",
  // `[detailed-spec v1.1.0 §2.2]`). Once both `tabsState`/`settingsState` have finished their
  // initial load, switch to `lastOpened.tabId` (if it's still in `AppSettings.tabs`) and queue a
  // restore for it — `useTabView`'s own effect applies it once that tab's tree has actually
  // loaded. Guarded by a ref so this only ever runs once per app session; a stale/deleted tab
  // silently does nothing (no error, no fallback tab), matching the spec text exactly.
  const startupAutoOpenAttempted = useRef(false);
  useEffect(() => {
    if (startupAutoOpenAttempted.current) return;
    if (tabsState.loading || settingsState.loading) return;
    startupAutoOpenAttempted.current = true;
    const lastOpened = settingsState.settings?.lastOpened;
    if (!lastOpened) return;
    if (!tabsState.tabs.some((t) => t.id === lastOpened.tabId)) return;
    if (tabsState.activeTabId !== lastOpened.tabId) tabsState.selectTab(lastOpened.tabId);
    tabView.requestRestore(lastOpened.tabId);
  }, [tabsState.loading, settingsState.loading, tabsState.tabs, tabsState.activeTabId, settingsState.settings, tabsState, tabView]);

  // Persist `AppSettings.lastOpened.tabId` — "which tab was last active," narrowed from the old
  // `{ tabId, memoName }` shape (which memo was open now lives per-tab, `PLAN.md` §1, written via
  // `state/useTree.ts`'s own `persistLastViewed` at the memo-open call sites). Same trigger/dedup
  // this app used before this feature: whenever a memo becomes the open one (row click, arrow-nav
  // preview, `focusOnMemo`) — not on every plain tab switch with nothing open, a pre-existing scope
  // limit this feature does not change.
  useEffect(() => {
    if (settingsState.loading || !settingsState.settings) return;
    const tabId = treeState.activeTabId;
    const memoName = treeState.openMemoName;
    if (!tabId || !memoName) return;
    const current = settingsState.settings.lastOpened;
    if (current && current.tabId === tabId) return;
    void settingsState.patch({ lastOpened: { tabId } });
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
      <TreeView tree={treeState} tabLock={tabLockState} appSettings={settingsState.settings} toast={toastState} />
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
        tabsState={{ ...tabsState, selectTab: selectTabAndRestore }}
        tabLockState={tabLockState}
        beforeLockTab={beforeLockTab}
        settings={settingsState.settings}
        patchSettings={settingsState.patch}
        applyFetched={settingsState.applyFetched}
        onOpenSearchHit={openSearchHit}
        onOpenQuickOpenHit={openQuickOpenHit}
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
              isMemoEditLocked={openMemoLocked}
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
