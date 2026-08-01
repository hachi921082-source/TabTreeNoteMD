// Application shell. This component wires up `docs/plans/tab-management/PLAN.md`'s TabBar,
// `docs/plans/memo-tree/PLAN.md`'s TreeView, and `docs/plans/editor-save/PLAN.md`'s EditorPane
// (see docs/plans/project-architecture/PLAN.md).
import { useTabs } from "./state/useTabs";
import { useTree } from "./state/useTree";
import { useSettings } from "./state/useSettings";
import { TabBar } from "./components/TabBar/TabBar";
import { EmptyState } from "./components/TabBar/EmptyState";
import { TreeView } from "./components/Tree/TreeView";
import { EditorPane } from "./components/Editor/EditorPane";
import { locateNode } from "./shared/treeOps";

function App() {
  const tabsState = useTabs();
  const treeState = useTree(tabsState.activeTabId);
  const settingsState = useSettings();

  const openMemoOverride = treeState.openMemoName ? locateNode(treeState.nodes, treeState.openMemoName)?.node.editor : undefined;

  return (
    <div className="app-shell">
      <TabBar tabsState={tabsState} />
      <div className="app-content">
        {!tabsState.loading && tabsState.tabs.length === 0 ? (
          <EmptyState onCreateTab={() => void tabsState.createTab()} />
        ) : (
          <>
            <div className="app-tree-pane">
              <TreeView tree={treeState} />
            </div>
            <EditorPane
              tabId={treeState.activeTabId}
              memoName={treeState.openMemoName}
              appSettings={settingsState.settings}
              memoEditorOverride={openMemoOverride}
              setEditorOverride={treeState.setEditorOverride}
            />
          </>
        )}
      </div>
    </div>
  );
}

export default App;
