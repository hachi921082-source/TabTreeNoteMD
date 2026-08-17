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
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  treeSetLastViewed,
} from "../ipc/tree";
import type { LabelColor, MemoEditorSettings, MemoProperties, MoveTarget, RecoveryKind, TreeNode, TreeSnapshot } from "../shared/models";
import type { ErrorCode } from "../shared/result";
import type { ClickModifiers, SelectionState } from "../shared/treeSelection";
import { applyClick, clearSelection, flattenVisible, moveSelection } from "../shared/treeSelection";

export type TreeMutationOutcome = { ok: true } | { ok: false; code: ErrorCode; message: string };

/** The just-created memo is always appended as the last child of its parent (or the last
 * root-level node, when `parent` is omitted) — `insert_node_at_end` (`commands/tree.rs`). Walking
 * the FRESH post-create snapshot for this, rather than trusting the caller-supplied `name`
 * directly, is what makes this correct under GitHub issue #97's creation-time auto-numbering
 * (`name` -> `name(1)` on a collision) — `memo_create_impl`'s response doesn't otherwise surface
 * the resolved name explicitly (GitHub issue #143). `null` only if `parent` doesn't resolve to any
 * node in `nodes` (defensive; shouldn't happen for a create that just succeeded against this exact
 * snapshot). */
function lastChildName(nodes: TreeNode[], parent: string | undefined): string | null {
  if (parent === undefined) {
    return nodes.length > 0 ? nodes[nodes.length - 1].memoName : null;
  }
  const stack: TreeNode[] = [...nodes];
  while (stack.length > 0) {
    const node = stack.pop() as TreeNode;
    if (node.memoName === parent) {
      return node.children.length > 0 ? node.children[node.children.length - 1].memoName : null;
    }
    stack.push(...node.children);
  }
  return null;
}

/** Everything about the currently-active tab that must change ATOMICALLY together, bundled into a
 * single `useState` rather than one `useState` per field. This is load-bearing, not just tidiness:
 * with N separate `useState` calls, React can commit an intermediate render where some of them
 * (e.g. a just-updated `loadedTabId`) reflect the new tab while others (e.g. `openMemoName`) still
 * hold the previous tab's value — observed live via a reviewer-lite CDP repro of
 * `docs/plans/v1.1.0/tab-memo-memory` finding #1's fix: even with the tab-switch reset issuing all
 * of `setNodes`/`setOpenMemoName`/`setLoadedTabId`/etc. together during render, an in-between
 * `useEditor.ts` effect run was observed to fire with `{ tabId: <new tab>, memoName: <previous
 * tab's memo> }`, re-creating the exact same "new tabId + stale memo name" bug the fields-in-one-
 * object refactor eliminates for good: a consumer reading `perTab` only ever sees either the
 * complete previous-tab snapshot or the complete new-tab one, never a mix, because React guarantees
 * a single state value is swapped atomically with no intermediate reads. */
interface PerTabState {
  /** The tab whose `tree_open` snapshot the REST of this object actually reflects, or `null`
   * while nothing has loaded yet for the current `activeTabId`. See `UseTreeResult.loadedTabId`'s
   * doc comment for why consumers must gate on this instead of `!loading`. */
  loadedTabId: string | null;
  nodes: TreeNode[];
  tabName: string;
  locked: boolean;
  recoveryPending: boolean;
  recoveryKind: RecoveryKind | undefined;
  lastViewedMemo: string | null;
  openMemoName: string | null;
  selection: SelectionState;
}

function makeEmptyPerTab(): PerTabState {
  return {
    loadedTabId: null,
    nodes: [],
    tabName: "",
    locked: false,
    recoveryPending: false,
    recoveryKind: undefined,
    lastViewedMemo: null,
    openMemoName: null,
    selection: clearSelection(),
  };
}

export interface UseTreeResult {
  /** The tab this tree instance is bound to (`null` when no tab is active). Used by
   * `TreeView`'s cross-tab DND to no-op a drop back onto the tree's own already-active tab. */
  activeTabId: string | null;
  /** The tab whose `tree_open` snapshot `nodes`/`lastViewedMemo` (and the rest of this tab's
   * loaded state) actually reflects right now, or `null` while nothing has loaded yet for the
   * current `activeTabId`. Distinct from `activeTabId` itself: `activeTabId` is a same-render
   * pass-through of this hook's `tabId` argument, but the snapshot for a newly-active tab can only
   * become available after its `tree_open` fetch resolves. Consumers that must not act on a stale
   * snapshot (`state/useTabView.ts`'s restore effect) should gate on `loadedTabId ===
   * activeTabId`, not on `!loading` alone — `!loading` was found to spuriously pass during the
   * one-render window where `activeTabId` already named the new tab but the previous tab's
   * `loading === false` from before the switch hadn't been overwritten yet
   * (`docs/plans/v1.1.0/tab-memo-memory` reviewer finding #1). */
  loadedTabId: string | null;
  nodes: TreeNode[];
  tabName: string;
  locked: boolean;
  recoveryPending: boolean;
  /** Which recovery dialog to show when `recoveryPending` is `true`
   * (`docs/plans/tab-lock-permanent-unlock/CRASH-RECOVERY.md` §3). `undefined` otherwise. */
  recoveryKind: RecoveryKind | undefined;
  loading: boolean;
  lastErrorCode: ErrorCode | null;
  clearLastError: () => void;

  /** Currently-visible row order (respects collapsed nodes). */
  flatOrder: string[];
  selection: SelectionState;
  /** The memo currently shown in the editor pane, or `null` if none. Tree selection highlight
   * and scroll position auto-follow this (`[spec §2.2]`). */
  openMemoName: string | null;
  /** `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §2 — this tab's raw persisted `lastViewedMemo`
   * from the most recent `tree_open` snapshot, as-is (not reconciled against `flatOrder`).
   * `state/useTabView.ts`'s `restoreTabView` is the only consumer. */
  lastViewedMemo: string | null;
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
  const [perTab, setPerTab] = useState<PerTabState>(makeEmptyPerTab);
  const [loading, setLoading] = useState(true);
  const [lastErrorCode, setLastErrorCode] = useState<ErrorCode | null>(null);
  const [focusEditorToken, setFocusEditorToken] = useState(0);

  const { loadedTabId, nodes, tabName, locked, recoveryPending, recoveryKind, lastViewedMemo, openMemoName, selection } = perTab;

  const flatOrder = useMemo(() => flattenVisible(nodes), [nodes]);

  // Always holds the latest `tabId` argument, updated synchronously during render (not via an
  // effect) so `refresh`'s in-flight fetch can detect, after its `await`, whether the active tab
  // has since moved on and its result is now stale (e.g. a fast switch to another tab and back).
  const currentTabIdRef = useRef<string | null>(tabId);
  currentTabIdRef.current = tabId;

  // Reset ALL per-tab state atomically (single `setPerTab`, see `PerTabState`'s doc comment)
  // synchronously during render, the moment `tabId` changes, instead of waiting for a post-commit
  // `useEffect`. React re-renders immediately when state is set during render (see "adjusting
  // state when a prop changes" in the React docs), so this closes the one-render window where
  // `activeTabId` (a same-render pass-through of `tabId` below) already named the new tab while
  // `nodes`/`lastViewedMemo`/`flatOrder` still held the previous tab's stale values — the root
  // cause of reviewer finding #1 (`docs/plans/v1.1.0/tab-memo-memory` review).
  const prevTabIdRef = useRef<string | null>(tabId);
  if (prevTabIdRef.current !== tabId) {
    prevTabIdRef.current = tabId;
    setPerTab(makeEmptyPerTab());
    setLoading(tabId !== null);
  }

  /** Applies a `perTab` update only if `tabId` (the specific tab THIS closure was created for) is
   * still the active tab. Bundling all per-tab fields into one atomic `perTab` object (above) is
   * necessary but not sufficient on its own: a functional `setPerTab((prev) => ({ ...prev, ... }))`
   * update still MERGES onto whatever `perTab` happens to be at the moment it's actually applied —
   * which, for a mutator whose own promise/effect resolves asynchronously, can be well AFTER the
   * active tab has already moved on and `perTab` has already been reset for the NEW tab. Live-
   * reproduced via CDP for `focusOnMemo`: `useTabView`'s restore effect for tab A can be flushed by
   * React only after a subsequent synchronous switch to tab B has already reset+re-rendered
   * `perTab` for B, so `focusOnMemo`'s belated `setPerTab` call — still closed over A's own
   * `memoName` — smeared A's `openMemoName` onto B's now-otherwise-correctly-reset state
   * (`docs/plans/v1.1.0/tab-memo-memory` reviewer finding #1). Every mutator that resolves after an
   * `await` (`applySnapshot`, called by every tree-mutating command) or fires from a deferred
   * effect (`focusOnMemo`) is exposed to this same hazard, so they all route their `perTab` writes
   * through here rather than each reasoning individually about whether a delay is possible. */
  const applyIfCurrent = useCallback(
    (updater: (prev: PerTabState) => PerTabState) => {
      if (currentTabIdRef.current !== tabId) return;
      setPerTab(updater);
    },
    [tabId],
  );

  const applySnapshot = useCallback(
    (snapshot: TreeSnapshot) => {
      applyIfCurrent((prev) => ({
        ...prev,
        loadedTabId: tabId,
        nodes: snapshot.nodes,
        tabName: snapshot.tabName,
        locked: snapshot.locked,
        recoveryPending: snapshot.recoveryPending,
        recoveryKind: snapshot.recoveryKind,
        lastViewedMemo: snapshot.lastViewedMemo,
      }));
    },
    [tabId, applyIfCurrent],
  );

  /** Fire-and-forget: rides along with `openMemoName` becoming a new, non-null memo (memo click,
   * arrow-key nav, `focusOnMemo` — `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §4's "same trigger
   * points... memo click, arrow-key nav, memo: link navigation, tab switch"). Never called with
   * `null` (closing a memo does not erase the tab's memory of what was last viewed — that would
   * defeat the whole point of restoring it next time). Errors are not surfaced via
   * `lastErrorCode`: this is auxiliary bookkeeping, not a user-visible mutation. */
  const persistLastViewed = useCallback(
    (name: string) => {
      if (!tabId) return;
      void treeSetLastViewed(tabId, name);
    },
    [tabId],
  );

  const refresh = useCallback(async () => {
    if (!tabId) {
      setPerTab(makeEmptyPerTab());
      setLoading(false);
      return;
    }
    setLoading(true);
    const result = await treeOpen(tabId);
    // Bail if the active tab has since moved on (e.g. a fast switch to another tab and back) —
    // applying this response now would overwrite the newer tab's already-loading/loaded state
    // with this stale one.
    if (currentTabIdRef.current !== tabId) return;
    if (result.ok) {
      applySnapshot(result.value);
    } else {
      setLastErrorCode(result.error.code);
    }
    setLoading(false);
  }, [tabId, applySnapshot]);

  // Reload the tree whenever the active tab changes. Per-tab selection/open-memo/loaded-snapshot
  // state is already reset synchronously above, during render — this effect only needs to kick
  // off the actual fetch.
  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabId]);

  // If the tree changes underfoot (e.g. a future external-change
  // reconciliation event) and the open/selected memo no longer exists,
  // drop it rather than pointing at a stale id.
  useEffect(() => {
    if (openMemoName && !flatOrder.includes(openMemoName)) {
      setPerTab((prev) => ({ ...prev, openMemoName: null, selection: clearSelection() }));
    }
  }, [flatOrder, openMemoName]);

  const handleRowClick = useCallback(
    (memoName: string, modifiers: ClickModifiers) => {
      // Plain click = single-select + open memo; Ctrl/Shift-click manage a
      // multi-select without changing what's open (`[spec §2.2]`).
      if (!modifiers.ctrlKey && !modifiers.shiftKey) {
        setPerTab((prev) => ({ ...prev, selection: applyClick(prev.selection, flatOrder, memoName, modifiers), openMemoName: memoName }));
        // Click-open auto-focuses the editor (`[spec §2.2]`, `[spec §9.1 item 3]`).
        setFocusEditorToken((t) => t + 1);
        persistLastViewed(memoName);
      } else {
        setPerTab((prev) => ({ ...prev, selection: applyClick(prev.selection, flatOrder, memoName, modifiers) }));
      }
    },
    [flatOrder, persistLastViewed],
  );

  const handleEmptyAreaClick = useCallback(() => {
    setPerTab((prev) => ({ ...prev, selection: clearSelection() }));
  }, []);

  /** Selects `memoName` alone without changing `openMemoName` — used when a right-click lands
   * on a row that isn't already part of the selection (the context menu should act on that row,
   * but a right-click alone shouldn't switch which memo is open, unlike a plain left click). */
  const selectOnly = useCallback((memoName: string) => {
    setPerTab((prev) => ({ ...prev, selection: { selected: new Set([memoName]), anchor: memoName } }));
  }, []);

  const handleArrowNav = useCallback(
    (direction: "up" | "down") => {
      const next = moveSelection(flatOrder, selection.anchor, direction);
      if (next === null) return;
      // Intentionally does NOT bump `focusEditorToken` — arrow-key nav must not steal focus into
      // the editor (`[spec §2.2]`).
      setPerTab((prev) => ({ ...prev, selection: { selected: new Set([next]), anchor: next }, openMemoName: next }));
      persistLastViewed(next);
    },
    [flatOrder, selection.anchor, persistLastViewed],
  );

  /** Programmatic "open like a click" — also used by `state/useTabView.ts`'s `restoreTabView` to
   * reopen a tab's `lastViewedMemo`, per `[detailed-spec v1.1.0 §2.3]`'s "same end visual state as
   * if the user had just clicked that row manually" (selection + auto-focus, not just
   * `openMemoName`). */
  const focusOnMemo = useCallback(
    (memoName: string) => {
      // Guards against exactly the delayed-effect hazard `applyIfCurrent` documents — this is the
      // one mutator here most exposed to it, since `state/useTabView.ts`'s restore effect (and
      // `App.tsx`'s cross-tab search/link-focus effects) can call it well after the tab it was
      // queued for is no longer active.
      if (currentTabIdRef.current !== tabId) return;
      if (!flatOrder.includes(memoName)) return;
      setPerTab((prev) => ({ ...prev, selection: { selected: new Set([memoName]), anchor: memoName }, openMemoName: memoName }));
      setFocusEditorToken((t) => t + 1);
      persistLastViewed(memoName);
    },
    [tabId, flatOrder, persistLastViewed],
  );

  const closeOpenMemo = useCallback(() => {
    setPerTab((prev) => ({ ...prev, openMemoName: null }));
  }, []);

  const clearLastError = useCallback(() => setLastErrorCode(null), []);

  /** Selects, opens, and auto-focuses the newly-created memo — same effect as clicking its row
   * right after creation (GitHub issue #143). Operates on the FRESH snapshot's `nodes` directly
   * (via `lastChildName`) rather than reading `flatOrder`/`nodes` state, which wouldn't reflect
   * this creation yet within the same synchronous callback (state updates from `applySnapshot`
   * above haven't flushed). Also persists `lastViewedMemo`, same as every other call site that
   * changes which memo is open (`handleRowClick`/`handleArrowNav`/`focusOnMemo`) — creation isn't
   * in `[docs/plans/v1.1.0/tab-memo-memory/PLAN.md]` §4's "not called by" exclusion list, and
   * `[docs/detailed-spec.v1.1.0.md]` §2.2 states this is folded into whatever operation already
   * changes the open memo (reviewer finding #2). */
  const focusJustCreated = useCallback(
    (nodes: TreeNode[], parent: string | undefined) => {
      // Same delayed-continuation hazard `applyIfCurrent` documents — `createMemo`/`createChildMemo`
      // call this right after their own `await`, so the active tab may have already moved on.
      if (currentTabIdRef.current !== tabId) return;
      const createdName = lastChildName(nodes, parent);
      if (!createdName) return;
      setPerTab((prev) => ({ ...prev, selection: { selected: new Set([createdName]), anchor: createdName }, openMemoName: createdName }));
      setFocusEditorToken((t) => t + 1);
      persistLastViewed(createdName);
    },
    [tabId, persistLastViewed],
  );

  const createMemo = useCallback(
    async (name: string, parent?: string): Promise<TreeMutationOutcome> => {
      if (!tabId) return { ok: false, code: "UNKNOWN", message: "no active tab" };
      const result = await memoCreate(tabId, name, parent);
      if (result.ok) {
        applySnapshot(result.value);
        focusJustCreated(result.value.nodes, parent);
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [tabId, applySnapshot, focusJustCreated],
  );

  const createChildMemo = useCallback(
    async (name: string, parent: string): Promise<TreeMutationOutcome> => {
      if (!tabId) return { ok: false, code: "UNKNOWN", message: "no active tab" };
      const result = await memoCreateChild(tabId, name, parent);
      if (result.ok) {
        applySnapshot(result.value);
        focusJustCreated(result.value.nodes, parent);
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [tabId, applySnapshot, focusJustCreated],
  );

  const renameMemo = useCallback(
    async (path: string, newName: string): Promise<TreeMutationOutcome> => {
      if (!tabId) return { ok: false, code: "UNKNOWN", message: "no active tab" };
      const result = await memoRename(tabId, path, newName);
      if (result.ok) {
        applySnapshot(result.value);
        const trimmed = newName.trim();
        applyIfCurrent((prev) => (prev.openMemoName === path ? { ...prev, openMemoName: trimmed } : prev));
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [tabId, applySnapshot, applyIfCurrent],
  );

  const deleteMemo = useCallback(
    async (paths: string[], forceDirect = false): Promise<TreeMutationOutcome> => {
      if (!tabId) return { ok: false, code: "UNKNOWN", message: "no active tab" };
      const result = await memoDelete(tabId, paths, forceDirect);
      if (result.ok) {
        applySnapshot(result.value);
        applyIfCurrent((prev) => ({
          ...prev,
          openMemoName: prev.openMemoName && paths.includes(prev.openMemoName) ? null : prev.openMemoName,
          selection: clearSelection(),
        }));
        return { ok: true };
      }
      setLastErrorCode(result.error.code);
      return { ok: false, code: result.error.code, message: result.error.message };
    },
    [tabId, applySnapshot, applyIfCurrent],
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
    loadedTabId,
    nodes,
    tabName,
    locked,
    recoveryPending,
    recoveryKind,
    loading,
    lastErrorCode,
    clearLastError,
    flatOrder,
    selection,
    openMemoName,
    lastViewedMemo,
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
