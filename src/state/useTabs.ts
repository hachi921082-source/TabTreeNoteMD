// Tab domain state (`docs/plans/tab-management/PLAN.md` §4). Wraps
// `ipc/tab.ts`, keeps the active-tab selection as local React state,
// matching `project-architecture/PLAN.md` §2's plain-hooks approach (no
// external state library).
//
// The "always on top" pin toggle (`docs/spec.md` §2.1) is NOT part of this
// hook's state — it's session-only, global (not per-tab), and lives as
// local `useState` in `components/TabBar/TabBar.tsx` instead
// (`docs/DECISIONS.md` "'Pin' Button: Global Window Always-On-Top Toggle,
// Not Per-Tab").
import { useCallback, useEffect, useState } from "react";
import { tabList, tabCreate, tabRegisterExisting, tabRename, tabReorder, tabRemove, tabOpenLocation } from "../ipc/tab";
import type { TabInfo } from "../shared/models";
import type { ErrorCode } from "../shared/result";

export type TabMutationOutcome = { ok: true } | { ok: false; code: ErrorCode; message: string; path?: string };

export interface UseTabsResult {
  tabs: TabInfo[];
  activeTabId: string | null;
  loading: boolean;
  lastErrorCode: ErrorCode | null;
  clearLastError: () => void;
  selectTab: (id: string) => void;
  /** Opens the OS folder-picker. Resolves to `null` if the user cancels it
   * (silent no-op, not an error). */
  createTab: () => Promise<TabMutationOutcome | null>;
  /** No dialog — registers an already-known folder path (OS drag-and-drop
   * onto the tab bar). */
  registerExistingTab: (path: string) => Promise<TabMutationOutcome>;
  /** Retries `createTab`/`registerExistingTab` with an explicit name after a
   * `NAME_DUPLICATE` rejection, for the same folder path, without
   * re-opening the dialog. */
  retryCreateWithName: (path: string, name: string) => Promise<TabMutationOutcome>;
  renameTab: (id: string, newName: string) => Promise<TabMutationOutcome>;
  reorderTabs: (order: string[]) => Promise<void>;
  removeTab: (id: string) => Promise<TabMutationOutcome>;
  openTabLocation: (id: string) => Promise<void>;
  refresh: () => Promise<void>;
}

export function useTabs(): UseTabsResult {
  const [tabs, setTabs] = useState<TabInfo[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [lastErrorCode, setLastErrorCode] = useState<ErrorCode | null>(null);

  const refresh = useCallback(async () => {
    const result = await tabList();
    if (result.ok) {
      setTabs(result.value);
      setActiveTabId((current) => (current && result.value.some((t) => t.id === current) ? current : (result.value[0]?.id ?? null)));
    } else {
      setLastErrorCode(result.error.code);
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const selectTab = useCallback((id: string) => {
    setActiveTabId(id);
  }, []);

  const createTab = useCallback(async (): Promise<TabMutationOutcome | null> => {
    const result = await tabCreate();
    if (result.ok) {
      if (result.value) {
        await refresh();
        setActiveTabId(result.value.id);
        return { ok: true };
      }
      return null; // user cancelled the dialog
    }
    setLastErrorCode(result.error.code);
    return { ok: false, code: result.error.code, message: result.error.message, path: result.error.path };
  }, [refresh]);

  const registerExistingTab = useCallback(
    async (path: string): Promise<TabMutationOutcome> => {
      const result = await tabRegisterExisting(path);
      if (result.ok) {
        await refresh();
        setActiveTabId(result.value.id);
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message, path: result.error.path ?? path };
    },
    [refresh],
  );

  const retryCreateWithName = useCallback(
    async (path: string, name: string): Promise<TabMutationOutcome> => {
      const result = await tabCreate(path, name);
      if (result.ok && result.value) {
        await refresh();
        setActiveTabId(result.value.id);
        return { ok: true };
      }
      if (!result.ok) {
        setLastErrorCode(result.error.code);
        return { ok: false, code: result.error.code, message: result.error.message, path: result.error.path ?? path };
      }
      // result.value === null shouldn't happen when initialPath is already
      // given (no dialog is opened in that case), but handle it defensively.
      return { ok: false, code: "UNKNOWN", message: "tab creation was unexpectedly cancelled", path };
    },
    [refresh],
  );

  const renameTab = useCallback(
    async (id: string, newName: string): Promise<TabMutationOutcome> => {
      const result = await tabRename(id, newName);
      if (result.ok) {
        await refresh();
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [refresh],
  );

  const reorderTabs = useCallback(
    async (order: string[]) => {
      // Optimistic local reorder for a snappy drag; `refresh()` corrects it
      // if the backend rejects the order (stale drag state).
      setTabs((current) => {
        const byId = new Map(current.map((t) => [t.id, t]));
        const reordered = order.map((id) => byId.get(id)).filter((t): t is TabInfo => !!t);
        return reordered.length === current.length ? reordered : current;
      });
      const result = await tabReorder(order);
      if (!result.ok) {
        setLastErrorCode(result.error.code);
        await refresh();
      }
    },
    [refresh],
  );

  const removeTab = useCallback(
    async (id: string): Promise<TabMutationOutcome> => {
      const result = await tabRemove(id);
      if (result.ok) {
        await refresh();
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [refresh],
  );

  const openTabLocation = useCallback(async (id: string) => {
    const result = await tabOpenLocation(id);
    if (!result.ok) setLastErrorCode(result.error.code);
  }, []);

  const clearLastError = useCallback(() => setLastErrorCode(null), []);

  return {
    tabs,
    activeTabId,
    loading,
    lastErrorCode,
    clearLastError,
    selectTab,
    createTab,
    registerExistingTab,
    retryCreateWithName,
    renameTab,
    reorderTabs,
    removeTab,
    openTabLocation,
    refresh,
  };
}
