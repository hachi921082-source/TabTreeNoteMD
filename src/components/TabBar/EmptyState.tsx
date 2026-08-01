// Shown in the main content area when zero tabs are registered
// (`docs/plans/tab-management/PLAN.md` §4, "the app's empty-state screen if
// zero tabs are registered" — the alternative create-tab entry point besides
// the "+" button in `TabBar`).
import "./EmptyState.css";

export interface EmptyStateProps {
  onCreateTab: () => void;
}

export function EmptyState({ onCreateTab }: EmptyStateProps) {
  return (
    <div className="ttn-empty-state">
      <p>登録されているタブがありません</p>
      <button type="button" onClick={onCreateTab}>
        ＋ 新しいタブを作成
      </button>
    </div>
  );
}
