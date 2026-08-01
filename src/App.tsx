// Application shell. Feature UI beyond the tab bar/tree (editor, etc.) is
// added by each feature's own PLAN.md; this component wires up
// `docs/plans/tab-management/PLAN.md`'s TabBar and `docs/plans/memo-tree/PLAN.md`'s
// TreeView (see docs/plans/project-architecture/PLAN.md).
import { useTabs } from "./state/useTabs";
import { useTree } from "./state/useTree";
import { TabBar } from "./components/TabBar/TabBar";
import { EmptyState } from "./components/TabBar/EmptyState";
import { TreeView } from "./components/Tree/TreeView";

function App() {
  const tabsState = useTabs();
  const treeState = useTree(tabsState.activeTabId);

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
            {/* Placeholder until editor-save/PLAN.md's editor pane is implemented —
                shows which memo is currently "open" per the tree's selection. */}
            <div className="app-editor-pane">{treeState.openMemoName ? `《エディタ未実装》${treeState.openMemoName}` : "メモを選択してください"}</div>
          </>
        )}
      </div>
    </div>
  );
}

export default App;
