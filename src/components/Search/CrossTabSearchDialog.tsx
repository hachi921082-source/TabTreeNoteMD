// Cross-tab full-text search dialog (`docs/plans/search-index/PLAN.md` §5). Triggered from the
// top-bar search button (adjacent to the 📌 pin button, `[spec §2.1]`'s layout note). Distinct
// from `components/Search/SearchPanel.tsx` (in-memo find/replace against the open editor).
import { useEffect, useRef } from "react";
import type { FormEvent } from "react";
import type { UseCrossTabSearchResult } from "../../state/useCrossTabSearch";
import type { MatchLocation } from "../../shared/models";
import { Dialog } from "../dialogs/Dialog";
import { errorMessageFor } from "../../shared/errorMessages";
import "./Search.css";

export interface CrossTabSearchDialogProps {
  search: UseCrossTabSearchResult;
  onClose: () => void;
  /** Opens `memoName` in `tabId` (switching tabs and following tree selection if needed) and
   * closes this dialog — reuses the same "open memo" path as a tree click / future `memo:` link
   * navigation (`docs/plans/memo-tree/PLAN.md` §4's "tree selection auto-follows" behavior
   * applies here too). Wired from `App.tsx`, which owns both tab and tree state. */
  onOpenHit: (tabId: string, memoName: string) => void;
}

const MATCH_LABELS: Record<MatchLocation, string> = {
  name: "名前",
  body: "本文",
  both: "名前・本文",
};

export function CrossTabSearchDialog({ search, onClose, onOpenHit }: CrossTabSearchDialogProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    void search.runSearch();
  };

  return (
    <Dialog title="検索" onClose={onClose} className="ttn-crosstab-search-dialog">
      <form className="ttn-crosstab-search-form" onSubmit={handleSubmit}>
        <div className="ttn-crosstab-search-row">
          <input
            ref={inputRef}
            type="text"
            placeholder={search.scope === "current" ? "現在のタブから検索" : "すべてのタブから検索"}
            value={search.query}
            onChange={(e) => search.setQuery(e.target.value)}
          />
          <button type="submit" disabled={search.loading}>
            検索
          </button>
        </div>
        <div className="ttn-crosstab-search-scope">
          <label>
            <input type="radio" name="ttn-crosstab-search-scope" checked={search.scope === "all"} onChange={() => search.setScope("all")} />
            全タブ
          </label>
          <label>
            <input type="radio" name="ttn-crosstab-search-scope" checked={search.scope === "current"} onChange={() => search.setScope("current")} />
            現在のタブ
          </label>
        </div>
        <div className="ttn-crosstab-search-options">
          <label>
            <input type="checkbox" checked={search.options.caseSensitive} onChange={() => search.toggleOption("caseSensitive")} />
            大文字/小文字を区別
          </label>
          <label>
            <input type="checkbox" checked={search.options.regex} onChange={() => search.toggleOption("regex")} />
            正規表現
          </label>
        </div>
      </form>

      {search.lastErrorCode && <p className="ttn-crosstab-search-error">{errorMessageFor(search.lastErrorCode)}</p>}

      <div className="ttn-crosstab-search-results">
        {search.loading && <p className="ttn-crosstab-search-status">検索中...</p>}
        {!search.loading && search.query && search.hits.length === 0 && !search.lastErrorCode && <p className="ttn-crosstab-search-status">一致する結果はありません</p>}
        {!search.loading &&
          search.hits.map((hit) => (
            <button
              key={`${hit.tabId}/${hit.memoName}`}
              type="button"
              className="ttn-crosstab-search-hit"
              onClick={() => onOpenHit(hit.tabId, hit.memoName)}
            >
              <span className="ttn-crosstab-search-hit-memo">{hit.memoName}</span>
              <span className="ttn-crosstab-search-hit-tab">{hit.tabName}</span>
              <span className="ttn-crosstab-search-hit-match">{MATCH_LABELS[hit.matchedIn]}</span>
            </button>
          ))}
      </div>
    </Dialog>
  );
}
