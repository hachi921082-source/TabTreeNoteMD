// Application shell. This component wires up `docs/plans/tab-management/PLAN.md`'s TabBar,
// `docs/plans/memo-tree/PLAN.md`'s TreeView, `docs/plans/editor-save/PLAN.md`'s EditorPane, and
// `docs/plans/tab-lock/PLAN.md`'s lock/unlock session state (see
// docs/plans/project-architecture/PLAN.md).
import { useCallback, useRef, useState } from "react";
import { useTabs } from "./state/useTabs";
import { useTree } from "./state/useTree";
import { useSettings } from "./state/useSettings";
import { useTabLock } from "./state/useTabLock";
import { useWindowGeometry } from "./state/useWindowGeometry";
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
      <TreeView tree={treeState} tabLock={tabLockState} />
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
      />
      <div className="app-content">
        {!tabsState.loading && tabsState.tabs.length === 0 ? (
          <EmptyState onCreateTab={() => void tabsState.createTab()} />
        ) : (
          <>
            {treeSide === "left" && treePane}
            {treeSide === "left" && splitter}
            <EditorPane
              ref={editorRef}
              tabId={treeState.activeTabId}
              memoName={treeState.openMemoName}
              appSettings={settingsState.settings}
              memoEditorOverride={openMemoOverride}
              setEditorOverride={treeState.setEditorOverride}
              patchSettings={settingsState.patch}
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
