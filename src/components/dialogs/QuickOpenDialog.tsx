// Cross-Tab Quick Open overlay (`docs/plans/v1.1.0/quick-open/PLAN.md` §3, `[spec v1.1.0 §3]`).
// A keyboard-driven "jump to memo by name" surface distinct from `components/Search/
// CrossTabSearchDialog.tsx` (content search) — matches memo display names only, across all tabs.
//
// Also renders the Cross-Tab Favorites list (`docs/plans/v1.1.0/favorites/PLAN.md` §5) when
// `quickOpen.mode === "favorites"` — reused rather than split into a separate component, per that
// plan's own chosen approach. Differences in that mode: the query input is not rendered at all (no
// live re-query — the hook fetches once on open), and the star is a read-only, non-interactive
// indicator (removal is only available via the tree's right-click menu).
import { useCallback, useEffect, useRef } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import type { UseQuickOpenResult } from "../../state/useQuickOpen";
import type { QuickOpenHit } from "../../shared/models";
import { Dialog } from "./Dialog";
import "./QuickOpenDialog.css";

export interface QuickOpenDialogProps {
  quickOpen: UseQuickOpenResult;
  onClose: () => void;
  /** Selecting a result (row-body click or Enter) — switches to the result's owning tab if needed
   * and opens that specific memo (`docs/plans/v1.1.0/quick-open/PLAN.md` §3, reused verbatim by
   * "favorites" mode per `docs/plans/v1.1.0/favorites/PLAN.md` §5). Wired from `App.tsx`, which
   * owns both tab and tree state, same as `CrossTabSearchDialog`'s `onOpenHit`. */
  onOpenHit: (tabId: string, memoName: string) => void;
}

function hitKey(hit: QuickOpenHit): string {
  return `${hit.tabId}/${hit.memoName}`;
}

/** Minimal shape shared by native `KeyboardEvent` and React's `KeyboardEvent<HTMLInputElement>` —
 * lets `handleKeyDown` below serve both the "quickOpen" mode's input `onKeyDown` and the
 * "favorites" mode's window-level listener (no input to attach `onKeyDown` to in that mode). */
interface KeyDownLike {
  key: string;
  preventDefault: () => void;
}

export function QuickOpenDialog({ quickOpen, onClose, onOpenHit }: QuickOpenDialogProps) {
  const isFavorites = quickOpen.mode === "favorites";
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!isFavorites) inputRef.current?.focus();
  }, [isFavorites]);

  const handleSelect = (hit: QuickOpenHit) => {
    onOpenHit(hit.tabId, hit.memoName);
  };

  // Calls `onOpenHit` directly (not the `handleSelect` wrapper above) so this `useCallback`'s deps
  // stay accurate — `handleSelect` itself is a plain non-memoized closure recreated every render.
  const handleKeyDown = useCallback(
    (e: KeyDownLike) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        quickOpen.moveHighlight("down");
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        quickOpen.moveHighlight("up");
      } else if (e.key === "Enter") {
        e.preventDefault();
        const selected = quickOpen.resolveSelected();
        if (selected) onOpenHit(selected.tabId, selected.memoName);
      }
      // Esc is handled by `Dialog`'s own built-in Esc-closes handling (`onClose` below) — dismissal
      // without acting, a star toggle already committed before Esc stays committed.
    },
    [quickOpen, onOpenHit],
  );

  // "favorites" mode renders no input (nothing to attach `onKeyDown` to), so arrow/Enter
  // navigation is wired at the window level instead while this dialog is open. "quickOpen" mode
  // keeps its existing input-focused `onKeyDown` below, unchanged.
  useEffect(() => {
    if (!isFavorites) return;
    const handler = (e: KeyboardEvent) => handleKeyDown(e);
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [isFavorites, handleKeyDown]);

  // Distinct click target from the row body (`docs/plans/v1.1.0/quick-open/PLAN.md` §3, Q4-3) —
  // must not also select/navigate, so every event handler here stops propagation before acting.
  // Not used in "favorites" mode (star is read-only there, rendered as a plain `<span>` below).
  const handleStarClick = (hit: QuickOpenHit) => {
    void quickOpen.toggleFavorite(hit);
  };

  return (
    <Dialog title={isFavorites ? "お気に入り" : "クイックオープン"} onClose={onClose} className="ttn-quick-open-dialog">
      {!isFavorites && (
        <input
          ref={inputRef}
          type="text"
          className="ttn-quick-open-input"
          placeholder="メモ名で検索..."
          value={quickOpen.query}
          onChange={(e) => quickOpen.setQuery(e.target.value)}
          onKeyDown={(e: ReactKeyboardEvent<HTMLInputElement>) => handleKeyDown(e)}
        />
      )}

      <div className="ttn-quick-open-results">
        {!quickOpen.loading && quickOpen.hits.length === 0 && (
          <p className="ttn-quick-open-status">{isFavorites ? "お気に入りはありません" : "一致する結果はありません"}</p>
        )}
        {quickOpen.hits.map((hit, index) => (
          // A `<div>` row (not `<button>`) since it hosts the star as its own nested `<button>` in
          // "quickOpen" mode — interactive elements must not nest inside a `<button>`.
          <div
            key={hitKey(hit)}
            className={`ttn-quick-open-row${index === quickOpen.highlightedIndex ? " ttn-quick-open-row-highlighted" : ""}`}
            onClick={() => handleSelect(hit)}
          >
            <span className="ttn-quick-open-row-memo">{hit.memoName}</span>
            <span className="ttn-quick-open-row-tab">{hit.tabName}</span>
            {isFavorites ? (
              // Read-only indicator (`docs/plans/v1.1.0/favorites/PLAN.md` §5) — not a click
              // target; removing a favorite is only available via the tree's right-click menu.
              <span className="ttn-quick-open-star ttn-quick-open-star-active" aria-hidden="true">
                ★
              </span>
            ) : (
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
            )}
          </div>
        ))}
      </div>
    </Dialog>
  );
}
