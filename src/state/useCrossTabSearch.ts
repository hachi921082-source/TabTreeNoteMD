// Cross-tab full-text search domain state (`docs/plans/search-index/PLAN.md` §5). Distinct from
// `state/useSearch.ts` (in-memo find/replace against the currently-open `EditorView`, no backend
// command) — this hook drives the top-bar-button-triggered `search_query` command against every
// unlocked tab's indexed memos. Session-only, matching the same "plain hooks, no external state
// library" approach used throughout `src/state/` (`docs/plans/project-architecture/PLAN.md` §2).
import { useCallback, useState } from "react";
import { searchQuery } from "../ipc/search";
import type { CrossTabSearchOptions, SearchHit } from "../shared/models";
import type { ErrorCode } from "../shared/result";

export interface UseCrossTabSearchResult {
  isOpen: boolean;
  open: () => void;
  close: () => void;
  query: string;
  setQuery: (v: string) => void;
  options: CrossTabSearchOptions;
  toggleOption: (key: keyof CrossTabSearchOptions) => void;
  hits: SearchHit[];
  loading: boolean;
  lastErrorCode: ErrorCode | null;
  /** Runs `search_query` with the current `query`/`options`. An empty `query` clears the results
   * without calling the backend (mirrors `search_query`'s own "empty query -> no hits" contract,
   * but avoids the round-trip). */
  runSearch: () => Promise<void>;
}

export function useCrossTabSearch(): UseCrossTabSearchResult {
  const [isOpen, setIsOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [options, setOptions] = useState<CrossTabSearchOptions>({ caseSensitive: false, regex: false });
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [loading, setLoading] = useState(false);
  const [lastErrorCode, setLastErrorCode] = useState<ErrorCode | null>(null);

  const open = useCallback(() => setIsOpen(true), []);
  const close = useCallback(() => setIsOpen(false), []);

  const toggleOption = useCallback((key: keyof CrossTabSearchOptions) => {
    setOptions((current) => ({ ...current, [key]: !current[key] }));
  }, []);

  const runSearch = useCallback(async () => {
    if (!query) {
      setHits([]);
      setLastErrorCode(null);
      return;
    }
    setLoading(true);
    setLastErrorCode(null);
    const result = await searchQuery(query, options);
    if (result.ok) {
      setHits(result.value);
    } else {
      setLastErrorCode(result.error.code);
      setHits([]);
    }
    setLoading(false);
  }, [query, options]);

  return {
    isOpen,
    open,
    close,
    query,
    setQuery,
    options,
    toggleOption,
    hits,
    loading,
    lastErrorCode,
    runSearch,
  };
}
