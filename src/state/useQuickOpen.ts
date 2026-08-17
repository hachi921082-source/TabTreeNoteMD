// Cross-Tab Quick Open domain state (`docs/plans/v1.1.0/quick-open/PLAN.md` §3). Distinct from
// `state/useCrossTabSearch.ts` (content search against `plaintext`, submit-triggered) — this hook
// drives the `search_quick_open` command (matches `plaintextName` only) with a live, per-keystroke
// re-query and shows the full candidate list the moment the overlay opens (empty query, Q3-1).
// Session-only local state, matching `project-architecture/PLAN.md` §2's plain-hooks approach.
import { useCallback, useRef, useState } from "react";
import { searchQuickOpen } from "../ipc/search";
import { memoSetFavorite } from "../ipc/tree";
import type { QuickOpenHit } from "../shared/models";
import type { ErrorCode } from "../shared/result";

export interface UseQuickOpenResult {
  isOpen: boolean;
  /** Opens the overlay, resets the query, and immediately fires `search_quick_open("")` so the
   * full, ordering-rule-sorted candidate list is shown right away (Q3-1) — not an empty result. */
  open: () => void;
  close: () => void;
  query: string;
  /** Updates the query AND fires a live re-query for it — every keystroke, per
   * `docs/plans/v1.1.0/quick-open/PLAN.md` §3's "Live re-query on every keystroke." */
  setQuery: (value: string) => void;
  hits: QuickOpenHit[];
  loading: boolean;
  lastErrorCode: ErrorCode | null;
  /** `null` = nothing highlighted yet — Enter then opens the top result instead
   * (`docs/plans/v1.1.0/quick-open/PLAN.md` §3, meaningful since the list is never empty on open). */
  highlightedIndex: number | null;
  moveHighlight: (direction: "up" | "down") => void;
  /** Resolves the hit Enter / a row-body click should open: the highlighted one, or the top result
   * if none is highlighted yet. `null` only if the result list itself is empty. */
  resolveSelected: () => QuickOpenHit | null;
  /** Star click (`docs/plans/v1.1.0/quick-open/PLAN.md` §3, Q4-3) — calls `memo_set_favorite`
   * directly for this specific row (identified by its own `tabId`/`memoName`, independent of
   * whichever tab is currently active), then updates `hits` in place. Does not select/navigate,
   * and never re-sorts (ordering never depends on `favorite`). */
  toggleFavorite: (hit: QuickOpenHit) => Promise<void>;
}

export function useQuickOpen(): UseQuickOpenResult {
  const [isOpen, setIsOpen] = useState(false);
  const [query, setQueryState] = useState("");
  const [hits, setHits] = useState<QuickOpenHit[]>([]);
  const [loading, setLoading] = useState(false);
  const [lastErrorCode, setLastErrorCode] = useState<ErrorCode | null>(null);
  const [highlightedIndex, setHighlightedIndex] = useState<number | null>(null);

  // Only the most-recently-fired query's response is ever applied — guards against a slower
  // earlier keystroke's response arriving after (and overwriting) a faster later one's.
  const requestIdRef = useRef(0);

  const runQuery = useCallback(async (q: string) => {
    const requestId = ++requestIdRef.current;
    setLoading(true);
    const result = await searchQuickOpen(q);
    if (requestIdRef.current !== requestId) return; // superseded by a newer keystroke
    if (result.ok) {
      setHits(result.value);
      setLastErrorCode(null);
    } else {
      setHits([]);
      setLastErrorCode(result.error.code);
    }
    setHighlightedIndex(null);
    setLoading(false);
  }, []);

  const open = useCallback(() => {
    setIsOpen(true);
    setQueryState("");
    setHighlightedIndex(null);
    void runQuery("");
  }, [runQuery]);

  const close = useCallback(() => {
    setIsOpen(false);
  }, []);

  const setQuery = useCallback(
    (value: string) => {
      setQueryState(value);
      void runQuery(value);
    },
    [runQuery],
  );

  const moveHighlight = useCallback(
    (direction: "up" | "down") => {
      if (hits.length === 0) return;
      setHighlightedIndex((current) => {
        if (current === null) return direction === "down" ? 0 : hits.length - 1;
        const next = direction === "down" ? current + 1 : current - 1;
        return ((next % hits.length) + hits.length) % hits.length; // wraps around both ends
      });
    },
    [hits.length],
  );

  const resolveSelected = useCallback((): QuickOpenHit | null => {
    if (hits.length === 0) return null;
    return hits[highlightedIndex ?? 0];
  }, [hits, highlightedIndex]);

  const toggleFavorite = useCallback(async (hit: QuickOpenHit) => {
    const nextFavorite = !hit.isFavorite;
    const result = await memoSetFavorite(hit.tabId, hit.memoName, nextFavorite);
    if (result.ok) {
      setHits((current) => current.map((h) => (h.tabId === hit.tabId && h.memoName === hit.memoName ? { ...h, isFavorite: nextFavorite } : h)));
    }
    // A failed toggle simply leaves the star as-is; not surfaced via `lastErrorCode` (reserved for
    // the query itself) — rare (backend I/O failure only), and the row stays actionable to retry.
  }, []);

  return {
    isOpen,
    open,
    close,
    query,
    setQuery,
    hits,
    loading,
    lastErrorCode,
    highlightedIndex,
    moveHighlight,
    resolveSelected,
    toggleFavorite,
  };
}
