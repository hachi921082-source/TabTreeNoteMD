// `restoreTabView` — single restore function shared by app startup and ordinary mid-session tab
// switches (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §5): composes `state/useTree.ts`'s
// tree-loading (`activeTabId`/`loading`/`lastViewedMemo`/`flatOrder`) and its `focusOnMemo`
// "open like a click" action.
//
// Deliberately NOT a blanket "watch every `activeTabId`/`loading` change" effect — that would also
// fire for a Tab-Lock session start (`tab_unlock` succeeding never changes `activeTabId`, since the
// user was already viewing that locked tab, but it DOES call `treeState.refresh()`, toggling
// `loading`) and would re-apply on every unrelated refresh. Instead, `requestRestore(tabId)` must
// be called explicitly at the two call sites `PLAN.md` §5 names (app startup, ordinary tab switch);
// a pending request is consumed at most once, only while it still names the tab currently active,
// and is silently dropped (never re-applied) once consumed or once `activeTabId` moves on to
// something else — see `PLAN.md` §6 for the excluded transitions this design relies on never
// observing a spurious match.
import { useCallback, useEffect, useRef } from "react";
import type { UseTreeResult } from "./useTree";

export interface UseTabViewResult {
  /** Queues a restore attempt for `tabId` — applied by this hook's own effect once that tab's
   * `tree_open` snapshot has actually loaded (`treeState.loading` becomes `false`). A no-op if
   * `tabId` never becomes `treeState.activeTabId` (e.g. the caller's own subsequent tab-existence
   * check failed) since nothing here fires without that match. */
  requestRestore: (tabId: string) => void;
}

export function useTabView(treeState: UseTreeResult): UseTabViewResult {
  // The tab a restore is still owed for, or `null` once consumed/never requested.
  const pendingTabIdRef = useRef<string | null>(null);

  const requestRestore = useCallback((tabId: string) => {
    pendingTabIdRef.current = tabId;
  }, []);

  useEffect(() => {
    if (pendingTabIdRef.current === null) return;
    if (pendingTabIdRef.current !== treeState.activeTabId) return;
    // Wait for this tab's `tree_open` fetch (and §3.5 reconciliation) to finish before deciding —
    // must reach a definitive one-shot decision (found -> open, not found -> do nothing) rather
    // than keep waiting indefinitely.
    if (treeState.loading) return;

    pendingTabIdRef.current = null; // consume — at most one restore attempt per request
    const memoName = treeState.lastViewedMemo;
    if (memoName && treeState.flatOrder.includes(memoName)) {
      // `[detailed-spec v1.1.0 §2.3]`: opens exactly like a manual row click (selection + tree
      // scroll-into-view + editor auto-focus), restoring `scrollPosition` via the same
      // `editor_load` path an ordinary memo-switch already uses.
      treeState.focusOnMemo(memoName);
    }
    // If `memoName` is null/absent, or no longer present among the reconciled `flatOrder`
    // (deleted, renamed away, or dropped by reconciliation): do nothing — no error, no fallback
    // memo auto-selected (`PLAN.md` §5 step 2).
  }, [treeState.activeTabId, treeState.loading, treeState.lastViewedMemo, treeState.flatOrder, treeState.focusOnMemo]);

  return { requestRestore };
}
