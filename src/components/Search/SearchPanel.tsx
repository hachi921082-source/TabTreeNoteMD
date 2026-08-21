// In-memo search & replace panel (`docs/plans/editor-save/PLAN.md` §5). A custom floating panel
// over the editor pane — not `@codemirror/search`'s built-in DOM panel — driven by
// `state/useSearch.ts` against the currently-mounted `EditorView`.
import { useEffect, useRef } from "react";
import type { EditorView } from "@codemirror/view";
import type { UseSearchResult } from "../../state/useSearch";
import { useT } from "../../i18n";
import "./Search.css";

export interface SearchPanelProps {
  search: UseSearchResult;
  /** `null` while no `EditorView` is mounted yet (e.g. no memo open). */
  getView: () => EditorView | null;
}

export function SearchPanel({ search, getView }: SearchPanelProps) {
  const t = useT();
  const searchInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    searchInputRef.current?.focus();
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        search.close();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [search]);

  const withView = (fn: (view: EditorView) => void) => {
    const view = getView();
    if (view) fn(view);
  };

  return (
    <div className="ttn-search-panel" role="dialog" aria-label={t("searchPanel.ariaLabel")}>
      <div className="ttn-search-row">
        <input
          ref={searchInputRef}
          type="text"
          placeholder={t("common.search")}
          list="ttn-search-history"
          value={search.searchText}
          onChange={(e) => search.setSearchText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              withView((v) => (e.shiftKey ? search.findPreviousInView(v) : search.findNextInView(v)));
            }
          }}
        />
        <datalist id="ttn-search-history">
          {search.history.map((h) => (
            <option key={h} value={h} />
          ))}
        </datalist>
        <button type="button" onClick={() => withView(search.findPreviousInView)} title={t("searchPanel.prev")}>
          ▲
        </button>
        <button type="button" onClick={() => withView(search.findNextInView)} title={t("searchPanel.next")}>
          ▼
        </button>
        <button type="button" onClick={() => withView(search.selectAllMatchesInView)} title={t("common.selectAll")}>
          {t("common.selectAll")}
        </button>
        <button type="button" className="ttn-search-close" onClick={search.close} title={t("searchPanel.closeTitle")}>
          ×
        </button>
      </div>

      <div className="ttn-search-row">
        <input
          type="text"
          placeholder={t("searchPanel.replacePlaceholder")}
          value={search.replaceText}
          onChange={(e) => search.setReplaceText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              withView(search.replaceOneInView);
            }
          }}
        />
        <button type="button" onClick={() => withView(search.replaceOneInView)}>
          {t("searchPanel.replaceOne")}
        </button>
        <button type="button" onClick={() => withView(search.replaceAllInView)}>
          {t("searchPanel.replaceAll")}
        </button>
      </div>

      <div className="ttn-search-options">
        <label>
          <input type="checkbox" checked={search.options.caseSensitive} onChange={() => search.toggleOption("caseSensitive")} />
          {t("searchPanel.caseSensitive")}
        </label>
        <label>
          <input type="checkbox" checked={search.options.wholeWord} onChange={() => search.toggleOption("wholeWord")} />
          {t("searchPanel.wholeWord")}
        </label>
        <label>
          <input type="checkbox" checked={search.options.regexp} onChange={() => search.toggleOption("regexp")} />
          {t("search.regexp")}
        </label>
      </div>
    </div>
  );
}
