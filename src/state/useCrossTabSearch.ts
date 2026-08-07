// Cross-tab full-text search domain state (`docs/plans/search-index/PLAN.md` §5). Distinct from
// `state/useSearch.ts` (in-memo find/replace against the currently-open `EditorView`, no backend
// command) — this hook drives the top-bar-button-triggered `search_query` command against every
// unlocked tab's indexed memos. Session-only, matching the same "plain hooks, no external state
// library" approach used throughout `src/state/` (`docs/plans/project-architecture/PLAN.md` §2).
import { useCallback, useState } from "react";
import { searchQuery } from "../ipc/search";
import type { CrossTabSearchOptions, SearchHit } from "../shared/models";
import type { ErrorCode } from "../shared/result";

/** "全タブ" (all tabs) vs. "現在のタブ" (current tab only) — the scope radio buttons in
 * `CrossTabSearchDialog` (GitHub issue #4). Kept separate from the boolean-only `options`/
 * `toggleOption` pair since it isn't a simple checkbox toggle. */
export type CrossTabSearchScope = "all" | "current";

export interface UseCrossTabSearchResult {
  isOpen: boolean;
  open: () => void;
  close: () => void;
  query: string;
  setQuery: (v: string) => void;
  options: CrossTabSearchOptions;
  toggleOption: (key: keyof Pick<CrossTabSearchOptions, "caseSensitive" | "regex">) => void;
  scope: CrossTabSearchScope;
  setScope: (scope: CrossTabSearchScope) => void;
  hits: SearchHit[];
  loading: boolean;
  lastErrorCode: ErrorCode | null;
  /** Runs `search_query` with the current `query`/`options`/scope. An empty `query` clears the
   * results without calling the backend (mirrors `search_query`'s own "empty query -> no hits"
   * contract, but avoids the round-trip). */
  runSearch: () => Promise<void>;
}

/**
 * `activeTabId` is the id of the currently-active tab (owned by `TabBar`/`useTabs`), read fresh
 * on every `runSearch` call so that switching tabs while "現在のタブ" stays selected scopes to
 * whichever tab is active *at search time* — not whichever tab was active when the dialog first
 * opened (`docs/plans/search-index/PLAN.md` §5 / GitHub issue #4).
 */
export function useCrossTabSearch(activeTabId: string | null): UseCrossTabSearchResult {
  const [isOpen, setIsOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [options, setOptions] = useState<CrossTabSearchOptions>({ caseSensitive: false, regex: false });
  // Default "現在のタブ" (current tab only) per the confirmed decision for GitHub issue #4 —
  // a deliberate change from this app's prior always-all-tabs behavior.
  const [scope, setScope] = useState<CrossTabSearchScope>("current");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [loading, setLoading] = useState(false);
  const [lastErrorCode, setLastErrorCode] = useState<ErrorCode | null>(null);

  const open = useCallback(() => setIsOpen(true), []);
  const close = useCallback(() => setIsOpen(false), []);

  const toggleOption = useCallback((key: keyof Pick<CrossTabSearchOptions, "caseSensitive" | "regex">) => {
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
    // If scope is "current" but no tab is currently active (e.g. all tabs closed), there is no
    // "current tab" to scope to — fall back to `null` (all tabs) rather than silently no-op'ing
    // the search, since an unscoped search is still a more useful result than none.
    const scopeTabId = scope === "current" ? activeTabId : null;
    const result = await searchQuery(query, { ...options, scopeTabId });
    if (result.ok) {
      setHits(result.value);
    } else {
      setLastErrorCode(result.error.code);
      setHits([]);
    }
    setLoading(false);
  }, [query, options, scope, activeTabId]);

  return {
    isOpen,
    open,
    close,
    query,
    setQuery,
    options,
    toggleOption,
    scope,
    setScope,
    hits,
    loading,
    lastErrorCode,
    runSearch,
  };
}
