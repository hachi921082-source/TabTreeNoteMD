// Application shell. Feature UI beyond the tab bar (tree, editor, etc.) is
// added by each feature's own PLAN.md; this component wires up
// `docs/plans/tab-management/PLAN.md`'s TabBar (see
// docs/plans/project-architecture/PLAN.md).
import { useTabs } from "./state/useTabs";
import { TabBar } from "./components/TabBar/TabBar";
import { EmptyState } from "./components/TabBar/EmptyState";

function App() {
  const tabsState = useTabs();

  return (
    <div className="app-shell">
      <TabBar tabsState={tabsState} />
      <div className="app-content">
        {!tabsState.loading && tabsState.tabs.length === 0 ? (
          <EmptyState onCreateTab={() => void tabsState.createTab()} />
        ) : (
          <p>TabTreeNoteMD</p>
        )}
      </div>
    </div>
  );
}

export default App;
