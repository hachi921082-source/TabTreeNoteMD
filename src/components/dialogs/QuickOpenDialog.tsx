// Cross-Tab Quick Open overlay (`docs/plans/v1.1.0/quick-open/PLAN.md` §3, `[spec v1.1.0 §3]`).
// A keyboard-driven "jump to memo by name" surface distinct from `components/Search/
// CrossTabSearchDialog.tsx` (content search) — matches memo display names only, across all tabs.
import { useEffect, useRef } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import type { UseQuickOpenResult } from "../../state/useQuickOpen";
import type { QuickOpenHit } from "../../shared/models";
import { Dialog } from "./Dialog";
import "./QuickOpenDialog.css";

export interface QuickOpenDialogProps {
  quickOpen: UseQuickOpenResult;
  onClose: () => void;
  /** Selecting a result (row-body click or Enter) — switches to the result's owning tab if needed
   * and opens that specific memo (`docs/plans/v1.1.0/quick-open/PLAN.md` §3). Wired from `App.tsx`,
   * which owns both tab and tree state, same as `CrossTabSearchDialog`'s `onOpenHit`. */
  onOpenHit: (tabId: string, memoName: string) => void;
}

function hitKey(hit: QuickOpenHit): string {
  return `${hit.tabId}/${hit.memoName}`;
}

export function QuickOpenDialog({ quickOpen, onClose, onOpenHit }: QuickOpenDialogProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleSelect = (hit: QuickOpenHit) => {
    onOpenHit(hit.tabId, hit.memoName);
  };

  const handleKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      quickOpen.moveHighlight("down");
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      quickOpen.moveHighlight("up");
    } else if (e.key === "Enter") {
      e.preventDefault();
      const selected = quickOpen.resolveSelected();
      if (selected) handleSelect(selected);
    }
    // Esc is handled by `Dialog`'s own built-in Esc-closes handling (`onClose` below) — dismissal
    // without acting, a star toggle already committed before Esc stays committed.
  };

  // Distinct click target from the row body (`docs/plans/v1.1.0/quick-open/PLAN.md` §3, Q4-3) —
  // must not also select/navigate, so every event handler here stops propagation before acting.
  const handleStarClick = (hit: QuickOpenHit) => {
    void quickOpen.toggleFavorite(hit);
  };

  return (
    <Dialog title="クイックオープン" onClose={onClose} className="ttn-quick-open-dialog">
      <input
        ref={inputRef}
        type="text"
        className="ttn-quick-open-input"
        placeholder="メモ名で検索..."
        value={quickOpen.query}
        onChange={(e) => quickOpen.setQuery(e.target.value)}
        onKeyDown={handleKeyDown}
      />

      <div className="ttn-quick-open-results">
        {!quickOpen.loading && quickOpen.hits.length === 0 && <p className="ttn-quick-open-status">一致する結果はありません</p>}
        {quickOpen.hits.map((hit, index) => (
          // A `<div>` row (not `<button>`) since it hosts the star as its own nested `<button>` —
          // interactive elements must not nest inside a `<button>`.
          <div
            key={hitKey(hit)}
            className={`ttn-quick-open-row${index === quickOpen.highlightedIndex ? " ttn-quick-open-row-highlighted" : ""}`}
            onClick={() => handleSelect(hit)}
          >
            <span className="ttn-quick-open-row-memo">{hit.memoName}</span>
            <span className="ttn-quick-open-row-tab">{hit.tabName}</span>
            <button
              type="button"
              className={`ttn-quick-open-star${hit.isFavorite ? " ttn-quick-open-star-active" : ""}`}
              title={hit.isFavorite ? "お気に入りから外す" : "お気に入りに追加"}
              aria-label={hit.isFavorite ? "お気に入りから外す" : "お気に入りに追加"}
              onClick={(e) => {
                e.stopPropagation();
                handleStarClick(hit);
              }}
            >
              {hit.isFavorite ? "★" : "☆"}
            </button>
          </div>
        ))}
      </div>
    </Dialog>
  );
}
