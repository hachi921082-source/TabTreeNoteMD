// Tab Lock session state (`docs/plans/tab-lock/PLAN.md` §3). Wraps `ipc/tabLock.ts`; matches
// `project-architecture/PLAN.md` §2's plain-hooks approach (no external state library), same
// style as `state/useTabs.ts`/`state/useTree.ts`.
//
// More than one tab CAN hold an active unlock session at once — e.g. dropping a memo onto a
// locked (no-session) tab unlocks it in place without leaving the drag-source tab
// (`docs/plans/tab-lock/PLAN.md` §5), so that target tab's session lives on in the background
// even though it never became the active tab. `activeSessionRealNames` therefore tracks every
// currently-active session (tabId -> real tab name), not just one.
import { useCallback, useEffect, useRef, useState } from "react";
import {
  tabEndSession,
  tabLock,
  tabLockRecoveryFinish,
  tabLockRecoveryRestore,
  tabLockRecoveryStatus,
  tabPermanentUnlock,
  tabPermanentUnlockRecoveryFinish,
  tabPermanentUnlockRecoveryRestore,
  tabPermanentUnlockRecoveryStatus,
  tabUnlock,
} from "../ipc/tabLock";
import type { PermanentUnlockRecoveryStatus, RecoveryStatus } from "../shared/models";
import type { ErrorCode } from "../shared/result";

export interface UseTabLockResult {
  /** tabId -> real (decrypted) tab name, for every tab currently holding an active unlock
   * session — `TabBar` overlays this over the 🔒-placeholder `AppSettings`-cached name. */
  activeSessionRealNames: Map<string, string>;
  /** `true` while a lock/unlock/recovery call is in flight (scrypt is deliberately slow) — UI
   * should disable submit buttons and show a busy indicator. */
  busy: boolean;
  lastErrorCode: ErrorCode | null;
  clearLastError: () => void;
  lockTab: (tabId: string, password: string, hint?: string) => Promise<boolean>;
  unlockTab: (tabId: string, password: string) => Promise<boolean>;
  /** Ends `tabId`'s active session WITHOUT switching away from it — used by
   * `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md` §7's inactivity-timeout and app-focus-loss
   * teardown triggers, both of which end the currently-active tab's own session while it stays the
   * active tab. Unlike the tab-switch cleanup below (where a fresh `tree_open` for the newly-active
   * tab happens for free via `state/useTree.ts`'s own `[tabId]`-keyed effect), `activeTabId` never
   * changes here, so this explicitly triggers `onTreeChanged` itself — otherwise the tree pane would
   * keep showing the just-torn-down decrypted tree until some unrelated refresh happened to fire. */
  endSession: (tabId: string) => void;
  fetchRecoveryStatus: (tabId: string) => Promise<RecoveryStatus | null>;
  finishRecovery: (tabId: string, password: string) => Promise<boolean>;
  restoreRecovery: (tabId: string, password: string) => Promise<boolean>;
  /** `docs/plans/tab-lock-permanent-unlock/PLAN.md` §7.4. `password` omitted for entry point
   * (a) (an active session already supplies the key); given for entry point (b). */
  permanentUnlockTab: (tabId: string, password?: string) => Promise<boolean>;
  fetchPermanentUnlockRecoveryStatus: (tabId: string, password?: string) => Promise<PermanentUnlockRecoveryStatus | null>;
  finishPermanentUnlockRecovery: (tabId: string, password?: string) => Promise<boolean>;
  restorePermanentUnlockRecovery: (tabId: string, password?: string) => Promise<boolean>;
}

/** `onTabsChanged`/`onTreeChanged` are `state/useTabs.ts`'s `refresh` / `state/useTree.ts`'s
 * `refresh` — every operation here that changes locked state or tree content triggers a refetch
 * rather than hand-rolling an optimistic local merge. */
export function useTabLock(activeTabId: string | null, onTabsChanged: () => void, onTreeChanged: () => void): UseTabLockResult {
  const [activeSessionRealNames, setActiveSessionRealNames] = useState<Map<string, string>>(new Map());
  const [busy, setBusy] = useState(false);
  const [lastErrorCode, setLastErrorCode] = useState<ErrorCode | null>(null);

  const sessionsRef = useRef<Map<string, string>>(activeSessionRealNames);
  sessionsRef.current = activeSessionRealNames;

  /** Discards `tabId`'s active session locally: fires the backend `tab_end_session` call
   * (fire-and-forget, same convention this hook has always used for a tab-switch-triggered session
   * end — nothing else needs to await it, `tab-lock/PLAN.md` §3's own "just discards the in-memory
   * plaintext session state") and removes it from `activeSessionRealNames`. Shared by the tab-switch
   * cleanup effect below and the `endSession` method exposed to
   * `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md` §7's same-tab teardown triggers. */
  const discardSession = useCallback((tabId: string) => {
    void tabEndSession(tabId);
    setActiveSessionRealNames((prev) => {
      if (!prev.has(tabId)) return prev;
      const next = new Map(prev);
      next.delete(tabId);
      return next;
    });
  }, []);

  // Ends the tab being switched AWAY FROM's session, if it has one (`tab-lock/PLAN.md` §3:
  // "triggered by the frontend the moment the user switches away from this tab"). A session that
  // was created via a cross-tab drop-onto-a-locked-tab (`PLAN.md` §5) and whose tab never
  // actually became `activeTabId` simply isn't touched by this effect — it lives on until the
  // user does eventually select that tab and then switch away from it, or until app exit.
  useEffect(() => {
    return () => {
      const endingTabId = activeTabId;
      if (endingTabId && sessionsRef.current.has(endingTabId)) {
        discardSession(endingTabId);
      }
    };
  }, [activeTabId, discardSession]);

  const endSession = useCallback(
    (tabId: string) => {
      discardSession(tabId);
      onTreeChanged();
    },
    [discardSession, onTreeChanged],
  );

  const clearLastError = useCallback(() => setLastErrorCode(null), []);

  const lockTab = useCallback(
    async (tabId: string, password: string, hint?: string): Promise<boolean> => {
      setBusy(true);
      const result = await tabLock(tabId, password, hint);
      setBusy(false);
      if (result.ok) {
        onTabsChanged();
        onTreeChanged();
        return true;
      }
      setLastErrorCode(result.error.code);
      return false;
    },
    [onTabsChanged, onTreeChanged],
  );

  const unlockTab = useCallback(
    async (tabId: string, password: string): Promise<boolean> => {
      setBusy(true);
      const result = await tabUnlock(tabId, password);
      setBusy(false);
      if (result.ok) {
        setActiveSessionRealNames((prev) => new Map(prev).set(tabId, result.value.name));
        onTreeChanged();
        return true;
      }
      setLastErrorCode(result.error.code);
      return false;
    },
    [onTreeChanged],
  );

  const fetchRecoveryStatus = useCallback(async (tabId: string): Promise<RecoveryStatus | null> => {
    const result = await tabLockRecoveryStatus(tabId);
    if (result.ok) return result.value;
    setLastErrorCode(result.error.code);
    return null;
  }, []);

  const finishRecovery = useCallback(
    async (tabId: string, password: string): Promise<boolean> => {
      setBusy(true);
      const result = await tabLockRecoveryFinish(tabId, password);
      setBusy(false);
      if (result.ok) {
        onTabsChanged();
        onTreeChanged();
        return true;
      }
      setLastErrorCode(result.error.code);
      return false;
    },
    [onTabsChanged, onTreeChanged],
  );

  const restoreRecovery = useCallback(
    async (tabId: string, password: string): Promise<boolean> => {
      setBusy(true);
      const result = await tabLockRecoveryRestore(tabId, password);
      setBusy(false);
      if (result.ok) {
        onTabsChanged();
        onTreeChanged();
        return true;
      }
      setLastErrorCode(result.error.code);
      return false;
    },
    [onTabsChanged, onTreeChanged],
  );

  const permanentUnlockTab = useCallback(
    async (tabId: string, password?: string): Promise<boolean> => {
      setBusy(true);
      const result = await tabPermanentUnlock(tabId, password);
      setBusy(false);
      if (result.ok) {
        onTabsChanged();
        onTreeChanged();
        return true;
      }
      setLastErrorCode(result.error.code);
      return false;
    },
    [onTabsChanged, onTreeChanged],
  );

  const fetchPermanentUnlockRecoveryStatus = useCallback(async (tabId: string, password?: string): Promise<PermanentUnlockRecoveryStatus | null> => {
    const result = await tabPermanentUnlockRecoveryStatus(tabId, password);
    if (result.ok) return result.value;
    setLastErrorCode(result.error.code);
    return null;
  }, []);

  const finishPermanentUnlockRecovery = useCallback(
    async (tabId: string, password?: string): Promise<boolean> => {
      setBusy(true);
      const result = await tabPermanentUnlockRecoveryFinish(tabId, password);
      setBusy(false);
      if (result.ok) {
        onTabsChanged();
        onTreeChanged();
        return true;
      }
      setLastErrorCode(result.error.code);
      return false;
    },
    [onTabsChanged, onTreeChanged],
  );

  const restorePermanentUnlockRecovery = useCallback(
    async (tabId: string, password?: string): Promise<boolean> => {
      setBusy(true);
      const result = await tabPermanentUnlockRecoveryRestore(tabId, password);
      setBusy(false);
      if (result.ok) {
        onTabsChanged();
        onTreeChanged();
        return true;
      }
      setLastErrorCode(result.error.code);
      return false;
    },
    [onTabsChanged, onTreeChanged],
  );

  return {
    activeSessionRealNames,
    busy,
    lastErrorCode,
    clearLastError,
    lockTab,
    unlockTab,
    endSession,
    fetchRecoveryStatus,
    finishRecovery,
    restoreRecovery,
    permanentUnlockTab,
    fetchPermanentUnlockRecoveryStatus,
    finishPermanentUnlockRecovery,
    restorePermanentUnlockRecovery,
  };
}
