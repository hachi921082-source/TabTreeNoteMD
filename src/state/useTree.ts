// Tree domain state for the currently-active tab (`docs/plans/memo-tree/PLAN.md`
// §4). Wraps `ipc/tree.ts`; matches `project-architecture/PLAN.md` §2's
// plain-hooks approach (no external state library), same style as
// `state/useTabs.ts`.
//
// Only ever holds ONE tab's tree at a time (the active one) — this app has a
// single tree pane, not one per tab. A cross-tab `memo_move`'s `target`
// snapshot (the *other* tab's updated tree) is therefore intentionally not
// applied anywhere here: switching to that tab later re-fetches it fresh via
// `tree_open` regardless (`docs/shared/models.ts`'s `MemoMoveResult` doc
// comment).
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  memoCreate,
  memoCreateChild,
  memoDelete,
  memoGetProperties,
  memoMove,
  memoOpenLocation,
  memoRename,
  memoSetEditLock,
  memoSetEditorOverride,
  memoSetExpanded,
  memoSetLabel,
  treeOpen,
} from "../ipc/tree";
import type { LabelColor, MemoEditorSettings, MemoProperties, MoveTarget, TreeNode, TreeSnapshot } from "../shared/models";
import type { ErrorCode } from "../shared/result";
import type { ClickModifiers, SelectionState } from "../shared/treeSelection";
import { applyClick, clearSelection, flattenVisible, moveSelection } from "../shared/treeSelection";

export type TreeMutationOutcome = { ok: true } | { ok: false; code: ErrorCode; message: string };

export interface UseTreeResult {
  /** The tab this tree instance is bound to (`null` when no tab is active). Used by
   * `TreeView`'s cross-tab DND to no-op a drop back onto the tree's own already-active tab. */
  activeTabId: string | null;
  nodes: TreeNode[];
  tabName: string;
  locked: boolean;
  recoveryPending: boolean;
  loading: boolean;
  lastErrorCode: ErrorCode | null;
  clearLastError: () => void;

  /** Currently-visible row order (respects collapsed nodes). */
  flatOrder: string[];
  selection: SelectionState;
  /** The memo currently shown in the editor pane, or `null` if none. Tree selection highlight
   * and scroll position auto-follow this (`[spec §2.2]`). */
  openMemoName: string | null;
  /** One-shot "should the editor pane auto-focus" signal, bumped only by an explicit open (row
   * click, `focusOnMemo`) and left untouched by arrow-key navigation (`[spec §2.2]`: "After
   * clicking a row, editor auto-focuses" / "arrow key navigation does not steal editor focus").
   * `components/Editor/EditorPane.tsx` compares this against the token it last consumed when its
   * view-(re)creation effect fires, same one-shot-signal pattern as `ed.content.scrollPosition`. */
  focusEditorToken: number;

  handleRowClick: (memoName: string, modifiers: ClickModifiers) => void;
  handleEmptyAreaClick: () => void;
  selectOnly: (memoName: string) => void;
  handleArrowNav: (direction: "up" | "down") => void;
  /** Programmatically opens a memo (e.g. `memo:` link navigation, cross-tab search result) and
   * moves the tree selection/anchor to follow it. No-op if the memo isn't currently visible in the
   * tree. Bumps `focusEditorToken` — it's a deliberate navigation action equivalent to a click. */
  focusOnMemo: (memoName: string) => void;
  /** Closes the currently-open memo without opening another one (`openMemoName` -> `null`).
   * `components/Editor/EditorPane.tsx`'s load effect flushes the outgoing memo's pending save as
   * its cleanup when `memoName` changes away, so this alone is enough to persist any unsaved
   * content before e.g. `tab-lock/PLAN.md`'s `tab_lock` is called on this tab
   * (`docs/plans/tab-lock/PLAN.md` §3 step 2's "force a save-confirm (auto-save-and-close)"). */
  closeOpenMemo: () => void;

  createMemo: (name: string, parent?: string) => Promise<TreeMutationOutcome>;
  createChildMemo: (name: string, parent: string) => Promise<TreeMutationOutcome>;
  renameMemo: (path: string, newName: string) => Promise<TreeMutationOutcome>;
  deleteMemo: (paths: string[], forceDirect?: boolean) => Promise<TreeMutationOutcome>;
  moveWithinTab: (paths: string[], newParent: string | null, index: number) => Promise<TreeMutationOutcome>;
  moveCrossTab: (paths: string[], targetTabId: string, newParent: string | null, index: number, copy: boolean) => Promise<TreeMutationOutcome>;
  setExpanded: (path: string, expanded: boolean) => Promise<void>;
  setLabel: (path: string, color: LabelColor | null) => Promise<void>;
  setEditorOverride: (path: string, editor: MemoEditorSettings | null) => Promise<void>;
  setEditLock: (path: string, locked: boolean) => Promise<void>;
  getProperties: (path: string) => Promise<MemoProperties | null>;
  /** "ファイルの場所を開く" — reveals the memo's file (or the tab's folder, for locked/session
   * tabs) in Explorer. Reports failure via `lastErrorCode`, same convention as every other
   * mutation here. */
  openLocation: (path: string) => Promise<void>;
  refresh: () => Promise<void>;
}

export function useTree(tabId: string | null): UseTreeResult {
  const [nodes, setNodes] = useState<TreeNode[]>([]);
  const [tabName, setTabName] = useState("");
  const [locked, setLocked] = useState(false);
  const [recoveryPending, setRecoveryPending] = useState(false);
  const [loading, setLoading] = useState(true);
  const [lastErrorCode, setLastErrorCode] = useState<ErrorCode | null>(null);
  const [selection, setSelection] = useState<SelectionState>(clearSelection());
  const [openMemoName, setOpenMemoName] = useState<string | null>(null);
  const [focusEditorToken, setFocusEditorToken] = useState(0);

  const flatOrder = useMemo(() => flattenVisible(nodes), [nodes]);

  const applySnapshot = useCallback((snapshot: TreeSnapshot) => {
    setNodes(snapshot.nodes);
    setTabName(snapshot.tabName);
    setLocked(snapshot.locked);
    setRecoveryPending(snapshot.recoveryPending);
  }, []);

  const refresh = useCallback(async () => {
    if (!tabId) {
      setNodes([]);
      setTabName("");
      setLocked(false);
      setRecoveryPending(false);
      setLoading(false);
      return;
    }
    setLoading(true);
    const result = await treeOpen(tabId);
    if (result.ok) {
      applySnapshot(result.value);
    } else {
      setLastErrorCode(result.error.code);
    }
    setLoading(false);
  }, [tabId, applySnapshot]);

  // Reload the tree, and reset per-tab selection/open-memo state, whenever
  // the active tab changes.
  useEffect(() => {
    setSelection(clearSelection());
    setOpenMemoName(null);
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabId]);

  // If the tree changes underfoot (e.g. a future external-change
  // reconciliation event) and the open/selected memo no longer exists,
  // drop it rather than pointing at a stale id.
  useEffect(() => {
    if (openMemoName && !flatOrder.includes(openMemoName)) {
      setOpenMemoName(null);
      setSelection(clearSelection());
    }
  }, [flatOrder, openMemoName]);

  const handleRowClick = useCallback(
    (memoName: string, modifiers: ClickModifiers) => {
      setSelection((current) => applyClick(current, flatOrder, memoName, modifiers));
      // Plain click = single-select + open memo; Ctrl/Shift-click manage a
      // multi-select without changing what's open (`[spec §2.2]`).
      if (!modifiers.ctrlKey && !modifiers.shiftKey) {
        setOpenMemoName(memoName);
        // Click-open auto-focuses the editor (`[spec §2.2]`, `[spec §9.1 item 3]`).
        setFocusEditorToken((t) => t + 1);
      }
    },
    [flatOrder],
  );

  const handleEmptyAreaClick = useCallback(() => {
    setSelection(clearSelection());
  }, []);

  /** Selects `memoName` alone without changing `openMemoName` — used when a right-click lands
   * on a row that isn't already part of the selection (the context menu should act on that row,
   * but a right-click alone shouldn't switch which memo is open, unlike a plain left click). */
  const selectOnly = useCallback((memoName: string) => {
    setSelection({ selected: new Set([memoName]), anchor: memoName });
  }, []);

  const handleArrowNav = useCallback(
    (direction: "up" | "down") => {
      const next = moveSelection(flatOrder, selection.anchor, direction);
      if (next === null) return;
      setSelection({ selected: new Set([next]), anchor: next });
      // Intentionally does NOT bump `focusEditorToken` — arrow-key nav must not steal focus into
      // the editor (`[spec §2.2]`).
      setOpenMemoName(next);
    },
    [flatOrder, selection.anchor],
  );

  const focusOnMemo = useCallback(
    (memoName: string) => {
      if (!flatOrder.includes(memoName)) return;
      setSelection({ selected: new Set([memoName]), anchor: memoName });
      setOpenMemoName(memoName);
      setFocusEditorToken((t) => t + 1);
    },
    [flatOrder],
  );

  const closeOpenMemo = useCallback(() => {
    setOpenMemoName(null);
  }, []);

  const clearLastError = useCallback(() => setLastErrorCode(null), []);

  const createMemo = useCallback(
    async (name: string, parent?: string): Promise<TreeMutationOutcome> => {
      if (!tabId) return { ok: false, code: "UNKNOWN", message: "no active tab" };
      const result = await memoCreate(tabId, name, parent);
      if (result.ok) {
        applySnapshot(result.value);
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [tabId, applySnapshot],
  );

  const createChildMemo = useCallback(
    async (name: string, parent: string): Promise<TreeMutationOutcome> => {
      if (!tabId) return { ok: false, code: "UNKNOWN", message: "no active tab" };
      const result = await memoCreateChild(tabId, name, parent);
      if (result.ok) {
        applySnapshot(result.value);
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [tabId, applySnapshot],
  );

  const renameMemo = useCallback(
    async (path: string, newName: string): Promise<TreeMutationOutcome> => {
      if (!tabId) return { ok: false, code: "UNKNOWN", message: "no active tab" };
      const result = await memoRename(tabId, path, newName);
      if (result.ok) {
        applySnapshot(result.value);
        if (openMemoName === path) setOpenMemoName(newName.trim());
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [tabId, applySnapshot, openMemoName],
  );

  const deleteMemo = useCallback(
    async (paths: string[], forceDirect = false): Promise<TreeMutationOutcome> => {
      if (!tabId) return { ok: false, code: "UNKNOWN", message: "no active tab" };
      const result = await memoDelete(tabId, paths, forceDirect);
      if (result.ok) {
        applySnapshot(result.value);
        if (openMemoName && paths.includes(openMemoName)) setOpenMemoName(null);
        setSelection(clearSelection());
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [tabId, applySnapshot, openMemoName],
  );

  const moveWithinTab = useCallback(
    async (paths: string[], newParent: string | null, index: number): Promise<TreeMutationOutcome> => {
      if (!tabId) return { ok: false, code: "UNKNOWN", message: "no active tab" };
      const target: MoveTarget = { kind: "withinTab", newParent, index };
      const result = await memoMove(tabId, paths, target);
      if (result.ok) {
        applySnapshot(result.value.source);
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [tabId, applySnapshot],
  );

  const moveCrossTab = useCallback(
    async (paths: string[], targetTabId: string, newParent: string | null, index: number, copy: boolean): Promise<TreeMutationOutcome> => {
      if (!tabId) return { ok: false, code: "UNKNOWN", message: "no active tab" };
      const target: MoveTarget = { kind: "otherTab", targetTabId, newParent, index, copy };
      const result = await memoMove(tabId, paths, target);
      if (result.ok) {
        applySnapshot(result.value.source);
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [tabId, applySnapshot],
  );

  const setExpanded = useCallback(
    async (path: string, expanded: boolean) => {
      if (!tabId) return;
      const result = await memoSetExpanded(tabId, path, expanded);
      if (result.ok) applySnapshot(result.value);
      else setLastErrorCode(result.error.code);
    },
    [tabId, applySnapshot],
  );

  const setLabel = useCallback(
    async (path: string, color: LabelColor | null) => {
      if (!tabId) return;
      const result = await memoSetLabel(tabId, path, color);
      if (result.ok) applySnapshot(result.value);
      else setLastErrorCode(result.error.code);
    },
    [tabId, applySnapshot],
  );

  const setEditorOverride = useCallback(
    async (path: string, editor: MemoEditorSettings | null) => {
      if (!tabId) return;
      const result = await memoSetEditorOverride(tabId, path, editor);
      if (result.ok) applySnapshot(result.value);
      else setLastErrorCode(result.error.code);
    },
    [tabId, applySnapshot],
  );

  const setEditLock = useCallback(
    async (path: string, locked: boolean) => {
      if (!tabId) return;
      const result = await memoSetEditLock(tabId, path, locked);
      if (result.ok) applySnapshot(result.value);
      else setLastErrorCode(result.error.code);
    },
    [tabId, applySnapshot],
  );

  const getProperties = useCallback(
    async (path: string): Promise<MemoProperties | null> => {
      if (!tabId) return null;
      const result = await memoGetProperties(tabId, path);
      if (result.ok) return result.value;
      setLastErrorCode(result.error.code);
      return null;
    },
    [tabId],
  );

  const openLocation = useCallback(
    async (path: string): Promise<void> => {
      if (!tabId) return;
      const result = await memoOpenLocation(tabId, path);
      if (!result.ok) setLastErrorCode(result.error.code);
    },
    [tabId],
  );

  return {
    activeTabId: tabId,
    nodes,
    tabName,
    locked,
    recoveryPending,
    loading,
    lastErrorCode,
    clearLastError,
    flatOrder,
    selection,
    openMemoName,
    focusEditorToken,
    handleRowClick,
    handleEmptyAreaClick,
    selectOnly,
    handleArrowNav,
    focusOnMemo,
    closeOpenMemo,
    createMemo,
    createChildMemo,
    renameMemo,
    deleteMemo,
    moveWithinTab,
    moveCrossTab,
    setExpanded,
    setLabel,
    setEditorOverride,
    setEditLock,
    getProperties,
    openLocation,
    refresh,
  };
}
