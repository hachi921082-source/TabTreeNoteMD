// Left-pane memo tree (`docs/plans/memo-tree/PLAN.md` §4). Owns: rendering
// (via `TreeRow`), the right-click context menu (incl. the "Icon" submenu =
// `label.color` picker), F2 rename, the dedicated Ctrl+Delete force-delete
// shortcut, and DND (within-tree reorder/reparent + drag-onto-a-tab-bar-tab
// cross-tab move/copy). Selection/keyboard-nav *semantics* live in
// `state/useTree.ts` (backed by the pure functions in
// `shared/treeSelection.ts`) — this component only wires DOM events to them.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { TreeRow } from "./TreeRow";
import type { DropPosition } from "./TreeRow";
import { ContextMenu } from "../ContextMenu/ContextMenu";
import type { ContextMenuItem } from "../ContextMenu/ContextMenu";
import { Dialog } from "../dialogs/Dialog";
import { PromptDialog } from "../dialogs/PromptDialog";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { PasswordDialog } from "../dialogs/PasswordDialog";
import { RecoveryDialog } from "../dialogs/RecoveryDialog";
import { PermanentUnlockRecoveryDialog } from "../dialogs/PermanentUnlockRecoveryDialog";
import { ImportCollisionDialog } from "../dialogs/ImportCollisionDialog";
import { Toast } from "../Toast/Toast";
import { validateNameStrict } from "../../shared/nameSanitize";
import { errorMessageFor } from "../../shared/errorMessages";
import { formatDateTime } from "../../shared/formatDateTime";
import { findNode, locateNode, subtreeHasLocked } from "../../shared/treeOps";
import type { UseTreeResult } from "../../state/useTree";
import type { UseTabLockResult } from "../../state/useTabLock";
import { useImport } from "../../state/useImport";
import { useToast } from "../../state/useToast";
import type { AppSettings, LabelColor, MemoProperties, RecoveryStatus } from "../../shared/models";
import { comboFromEvent, resolveShortcuts } from "../../shared/shortcuts";
import "./Tree.css";

export interface TreeViewProps {
  tree: UseTreeResult;
  tabLock: UseTabLockResult;
  /** Needed for the rebindable F2-rename/Ctrl+Delete-force-delete shortcuts
   * (`docs/plans/window-operation/PLAN.md` §3). */
  appSettings: AppSettings | null;
}

type PendingDialog =
  | { kind: "createRoot" }
  | { kind: "createChild"; parent: string }
  | { kind: "deleteConfirm"; paths: string[] }
  | { kind: "trashFailedConfirm"; paths: string[] }
  | { kind: "properties"; path: string }
  | { kind: "unlock" }
  /** Cross-tab drop onto a locked (no-session) tab (`docs/plans/tab-lock/PLAN.md` §5) — shown at
   * the moment of drop, distinct from `unlock` (which unlocks the currently-displayed tab). */
  | { kind: "unlockForDrop"; targetTabId: string; paths: string[]; copy: boolean }
  /** Entry point (a) (`docs/plans/tab-lock-permanent-unlock/PLAN.md` §7.9) — permanently
   * unlocking the currently-displayed tab from within its own active unlock session. Confirming
   * always calls `tabLock.permanentUnlockTab` with no password (a session is a precondition for
   * this banner to even render), unlike entry point (b)'s own two-step confirm/password flow in
   * `TabBar.tsx`. */
  | { kind: "permanentUnlockConfirm" };

interface ContextMenuState {
  x: number;
  y: number;
  /** `null` = right-click on empty tree area (only "Create Memo" is offered). */
  path: string | null;
}

interface DragState {
  paths: string[];
  startX: number;
  startY: number;
  dragging: boolean;
}

type DropTarget = { kind: "row"; path: string; position: DropPosition } | { kind: "tab"; tabId: string } | null;

const LABEL_CHOICES: { label: string; color: LabelColor }[] = [
  { label: "🔴 赤", color: "red" },
  { label: "🔵 青", color: "blue" },
  { label: "🟢 緑", color: "green" },
  { label: "🟡 黄", color: "yellow" },
];

export function TreeView({ tree, tabLock, appSettings }: TreeViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const dragStateRef = useRef<DragState | null>(null);
  const hoveredTabElRef = useRef<HTMLElement | null>(null);
  // Mirrors `dropTarget` state so `handleMouseUp` can read the latest value
  // synchronously without performing its backend-call side effect inside a
  // `setState` functional updater (React 18 StrictMode invokes updaters
  // twice in dev, which would fire the move/copy IPC call twice — see
  // memo-tree/PLAN.md's DND section and the reviewer-lite finding that
  // motivated this).
  const dropTargetRef = useRef<DropTarget>(null);

  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [renamingPath, setRenamingPath] = useState<string | null>(null);
  const [pendingDialog, setPendingDialog] = useState<PendingDialog | null>(null);
  const [properties, setProperties] = useState<MemoProperties | null>(null);
  const [dropTarget, setDropTarget] = useState<DropTarget>(null);
  const [recoveryStatus, setRecoveryStatus] = useState<RecoveryStatus | null>(null);

  const updateDropTarget = useCallback((next: DropTarget) => {
    dropTargetRef.current = next;
    setDropTarget(next);
  }, []);

  const { nodes, loading, locked, recoveryPending, recoveryKind, selection, openMemoName, lastErrorCode, clearLastError, activeTabId } = tree;

  // OS-folder -> tree-pane drag-and-drop import (`docs/plans/v1.1.0/folder-import/PLAN.md`).
  const importToast = useToast();
  const importState = useImport(activeTabId, nodes, tree.refresh, importToast);

  // GitHub issue #156: `true` if any of `paths` (or one of their descendants — deleting a node
  // deletes its whole subtree, `[spec §2.2]`) carries the per-memo Edit Lock. Shared by the
  // "削除" context-menu entry's `disabled` and the Ctrl+Delete shortcut's own guard below, so both
  // paths agree; the backend (`memo_delete`) independently rejects the same cases too.
  const selectionHasLocked = useCallback((paths: string[]) => paths.some((p) => { const n = findNode(nodes, p); return n ? subtreeHasLocked(n) : false; }), [nodes]);

  // Crash-mid-lock recovery (`docs/plans/tab-lock/CRASH-RECOVERY.md` "Trigger Point"): the moment
  // `tree_open` reports `recoveryPending: true`/`recoveryKind: "lock"` for this tab, fetch the
  // per-entry classification so the recovery dialog can render its Finish/Restore choice. The
  // permanent-unlock direction (`recoveryKind: "permanentUnlock"`) stages this itself inside
  // `PermanentUnlockRecoveryDialog` instead (`tab-lock-permanent-unlock/PLAN.md` §7.7).
  useEffect(() => {
    setRecoveryStatus(null);
    if (!recoveryPending || recoveryKind !== "lock" || !activeTabId) return;
    let cancelled = false;
    void tabLock.fetchRecoveryStatus(activeTabId).then((status) => {
      if (!cancelled) setRecoveryStatus(status);
    });
    return () => {
      cancelled = true;
    };
  }, [recoveryPending, recoveryKind, activeTabId, tabLock]);

  // ---- selection auto-follow: scroll the open/selected row into view -----
  useEffect(() => {
    if (!openMemoName) return;
    const el = containerRef.current?.querySelector<HTMLElement>(`[data-ttn-row-path="${CSS.escape(openMemoName)}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [openMemoName]);

  // Rebindable shortcuts (`docs/plans/window-operation/PLAN.md` §3), recomputed only when the
  // user's overrides actually change.
  const shortcuts = useMemo(() => resolveShortcuts(appSettings?.shortcuts), [appSettings?.shortcuts]);

  // ---- arrow-key nav: only while the tree container has focus, must not steal focus from a
  // future editor pane (`[spec §2.2]`) --------
  const handleContainerKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (renamingPath) return; // let the inline rename input's own handler deal with it
      if (e.key === "ArrowDown") {
        e.preventDefault();
        tree.handleArrowNav("down");
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        tree.handleArrowNav("up");
      }
    },
    [renamingPath, tree],
  );

  // ---- force-delete (default Ctrl+Delete): global, regardless of current focus target --------
  const requestDelete = useCallback(
    async (paths: string[], forceDirect = false) => {
      const outcome = await tree.deleteMemo(paths, forceDirect);
      if (!outcome.ok && outcome.code === "TRASH_FAILED") {
        setPendingDialog({ kind: "trashFailedConfirm", paths });
      }
    },
    [tree],
  );

  // ---- F2 rename (default): also global, regardless of current focus target — a plain click that
  // opens a different memo auto-focuses the editor (`[spec §2.2]`, `[spec §9.1 item 3]`), so a tree
  // container-scoped F2 handler would miss an F2 pressed right after that (GitHub issue #106);
  // mirrors force-delete's own already-global pattern above rather than trying to special-case
  // "was the editor auto-focused by the click that just opened this row" -------------------------
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (comboFromEvent(e) === shortcuts["tree.forceDelete"]) {
        const selected = Array.from(selection.selected);
        if (selected.length === 0) return;
        // GitHub issue #156: matches the right-click "削除" entry's own `disabled` guard — no
        // dialog at all for a locked selection, rather than showing one only to have the backend
        // reject it afterward.
        if (selectionHasLocked(selected)) return;
        e.preventDefault();
        // Same confirmation dialog as the right-click "削除" entry (GitHub issue #142) —
        // Ctrl+Delete used to call `requestDelete` directly, skipping it entirely.
        setPendingDialog({ kind: "deleteConfirm", paths: selected });
        return;
      }
      if (comboFromEvent(e) === shortcuts["tree.rename"]) {
        if (renamingPath) return; // already renaming; let the inline input handle its own keys
        if (selection.selected.size === 1) {
          const path = Array.from(selection.selected)[0];
          // GitHub issue #156: matches the right-click "名前を変更" entry's own `disabled` guard.
          if (findNode(nodes, path)?.locked === true) return;
          e.preventDefault();
          setRenamingPath(path);
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [selection.selected, requestDelete, shortcuts, renamingPath, selectionHasLocked, nodes]);

  // ---- row interaction wiring --------------------------------------------
  const handleRowClick = useCallback(
    (memoName: string, modifiers: { ctrlKey: boolean; shiftKey: boolean }) => {
      containerRef.current?.focus();
      tree.handleRowClick(memoName, modifiers);
    },
    [tree],
  );

  const handleToggleExpand = useCallback(
    (memoName: string) => {
      const loc = locateNode(nodes, memoName);
      if (loc) void tree.setExpanded(memoName, !loc.node.expanded);
    },
    [nodes, tree],
  );

  const handleRowContextMenu = useCallback(
    (memoName: string, x: number, y: number) => {
      // A right-click on a row not already part of the selection replaces
      // the selection with just that row (standard explorer convention),
      // without also switching the open memo the way a plain left click
      // would.
      if (!selection.selected.has(memoName)) {
        tree.selectOnly(memoName);
      }
      setContextMenu({ x, y, path: memoName });
    },
    [selection.selected, tree],
  );

  const handleRenameSubmit = useCallback(
    (memoName: string, newName: string) => {
      setRenamingPath(null);
      const trimmed = newName.trim();
      if (trimmed && trimmed !== memoName) void tree.renameMemo(memoName, trimmed);
    },
    [tree],
  );

  // ---- DND: pointer-based, within-tree reorder/reparent + drag onto a
  // tab-bar tab for cross-tab move/copy (`docs/plans/memo-tree/PLAN.md` §4) --
  const handleRowMouseDown = useCallback(
    (memoName: string, e: ReactMouseEvent) => {
      if (e.button !== 0) return;
      const alreadySelected = selection.selected.has(memoName);
      const paths = alreadySelected && selection.selected.size > 0 ? Array.from(selection.selected) : [memoName];
      dragStateRef.current = { paths, startX: e.clientX, startY: e.clientY, dragging: false };
    },
    [selection.selected],
  );

  useEffect(() => {
    const clearTabHover = () => {
      hoveredTabElRef.current?.classList.remove("ttn-tab-drop-target");
      hoveredTabElRef.current = null;
    };

    // Live cursor/tooltip feedback for the Ctrl-to-copy modifier during a
    // cross-tab drag (`docs/plans/memo-tree/PLAN.md` §4: "cursor/tooltip
    // should reflect this during drag per standard OS DND conventions").
    // Toggled imperatively (not React state) since it's purely visual.
    const updateCopyCursor = (isCopy: boolean) => {
      document.body.classList.toggle("ttn-dragging-memo-copy", isCopy);
    };

    const handleMouseMove = (e: MouseEvent) => {
      const drag = dragStateRef.current;
      if (!drag) return;
      if (!drag.dragging) {
        const dx = e.clientX - drag.startX;
        const dy = e.clientY - drag.startY;
        if (Math.hypot(dx, dy) < 4) return;
        drag.dragging = true;
        document.body.classList.add("ttn-dragging-memo");
      }

      updateCopyCursor(e.ctrlKey);

      const hoveredEl = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
      const tabEl = hoveredEl?.closest<HTMLElement>("[data-ttn-tab-id]") ?? null;
      if (tabEl) {
        if (hoveredTabElRef.current !== tabEl) {
          clearTabHover();
          tabEl.classList.add("ttn-tab-drop-target");
          hoveredTabElRef.current = tabEl;
        }
        updateDropTarget({ kind: "tab", tabId: tabEl.dataset.ttnTabId! });
        return;
      }
      clearTabHover();

      const rowEl = hoveredEl?.closest<HTMLElement>("[data-ttn-row-path]") ?? null;
      if (rowEl) {
        const path = rowEl.dataset.ttnRowPath!;
        if (drag.paths.includes(path)) {
          updateDropTarget(null);
          return;
        }
        const rect = rowEl.getBoundingClientRect();
        const rel = (e.clientY - rect.top) / rect.height;
        const position: DropPosition = rel < 0.25 ? "before" : rel > 0.75 ? "after" : "into";
        updateDropTarget({ kind: "row", path, position });
      } else {
        updateDropTarget(null);
      }
    };

    // Keeps the copy-cursor feedback correct even if Ctrl is pressed/released
    // while the mouse itself is stationary mid-drag.
    const handleKeyChange = (e: KeyboardEvent) => {
      if (!dragStateRef.current?.dragging) return;
      if (e.key === "Control") updateCopyCursor(e.ctrlKey);
    };

    const handleMouseUp = (e: MouseEvent) => {
      const drag = dragStateRef.current;
      dragStateRef.current = null;
      document.body.classList.remove("ttn-dragging-memo", "ttn-dragging-memo-copy");
      clearTabHover();

      // Read the latest drop target from the ref (not from a setState
      // updater) so the IPC call below runs exactly once per drop, even
      // under React 18 StrictMode's double-invocation of updaters.
      const target = dropTargetRef.current;
      updateDropTarget(null);

      if (drag?.dragging && target) {
        if (target.kind === "tab") {
          // No-op a drop back onto the tree's own already-active tab: it's
          // not a real cross-tab move and would otherwise hit `memo_move`
          // with source === target, surfacing a meaningless NameDuplicate
          // error.
          if (target.tabId !== activeTabId) {
            const targetTabId = target.tabId;
            const paths = drag.paths;
            const copy = e.ctrlKey;
            void (async () => {
              const outcome = await tree.moveCrossTab(paths, targetTabId, null, Number.MAX_SAFE_INTEGER, copy);
              // Dropping onto a locked (no-session) tab (`docs/plans/tab-lock/PLAN.md` §5): show
              // the password dialog at the moment of drop instead of surfacing the raw
              // `TAB_LOCKED` error — the source memo is untouched (the move never ran).
              if (!outcome.ok && outcome.code === "TAB_LOCKED") {
                setPendingDialog({ kind: "unlockForDrop", targetTabId, paths, copy });
              }
            })();
          }
        } else {
          const loc = locateNode(nodes, target.path);
          if (loc) {
            if (target.position === "into") {
              void tree.moveWithinTab(drag.paths, target.path, Number.MAX_SAFE_INTEGER);
            } else {
              const index = target.position === "before" ? loc.index : loc.index + 1;
              void tree.moveWithinTab(drag.paths, loc.parentPath, index);
            }
          }
        }
      }
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    window.addEventListener("keydown", handleKeyChange);
    window.addEventListener("keyup", handleKeyChange);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
      window.removeEventListener("keydown", handleKeyChange);
      window.removeEventListener("keyup", handleKeyChange);
      clearTabHover();
    };
  }, [nodes, tree, activeTabId, updateDropTarget]);

  // ---- OS-folder -> tree-pane drag-and-drop import (`docs/plans/v1.1.0/folder-import/PLAN.md`
  // §6): Tauri's native file-drop event, scoped to this tree pane's DOM area (same event/pattern
  // `TabBar.tsx` already uses for its own OS-folder-drop-onto-tab-bar gesture, just a different
  // hit-test rect). Hover feedback reuses the exact same `dropTarget`/row-highlight machinery as
  // the internal memo-DND effect above — dropped hovering a row is treated as an "into" drop
  // (children end), matching `[spec v1.1.0 §1.4]`; dropped on empty background resolves to
  // `parent: null` (root level's end). A drop is ignored entirely while a batch is already in
  // progress (`state/useImport.ts`'s `importing`), so a second drop landing mid-collision-dialog
  // is a no-op rather than starting a second concurrent batch (`PLAN.md` §4). -------------------
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "leave") {
          updateDropTarget(null);
          return;
        }
        const rect = containerRef.current?.getBoundingClientRect();
        if (!rect) return;
        const scale = window.devicePixelRatio || 1;
        const x = event.payload.position.x / scale;
        const y = event.payload.position.y / scale;
        const within = x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
        if (!within) {
          updateDropTarget(null);
          return;
        }

        const hoveredEl = document.elementFromPoint(x, y) as HTMLElement | null;
        const rowEl = hoveredEl?.closest<HTMLElement>("[data-ttn-row-path]") ?? null;

        if (event.payload.type === "drop") {
          updateDropTarget(null);
          if (importState.importing) return;
          const paths = event.payload.paths ?? [];
          const mdPaths = paths.filter((p) => /\.md$/i.test(p));
          const nonMdSkipped = paths.length - mdPaths.length;
          importState.startImportBatch(mdPaths, rowEl?.dataset.ttnRowPath ?? null, nonMdSkipped);
          return;
        }

        // "enter" / "over": live hover feedback only.
        updateDropTarget(rowEl ? { kind: "row", path: rowEl.dataset.ttnRowPath!, position: "into" } : null);
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [importState, updateDropTarget]);

  // ---- context menu item construction ------------------------------------
  const buildContextMenuItems = (path: string): ContextMenuItem[] => {
    const singleSelection = selection.selected.size <= 1 || !selection.selected.has(path);
    const selectedPaths = selection.selected.has(path) && selection.selected.size > 0 ? Array.from(selection.selected) : [path];
    const isLocked = findNode(nodes, path)?.locked === true;
    return [
      // GitHub issue #156: an edit-locked memo must not be renamed or deleted either, not just
      // have its body protected from edits. "削除" additionally checks every selected node's
      // whole subtree, since deleting a node deletes its entire subtree (`[spec §2.2]`).
      { label: "名前を変更", disabled: !singleSelection || isLocked, onSelect: () => setRenamingPath(path) },
      { label: "削除", danger: true, disabled: selectionHasLocked(selectedPaths), onSelect: () => setPendingDialog({ kind: "deleteConfirm", paths: selectedPaths }) },
      { label: "メモを作成", onSelect: () => setPendingDialog({ kind: "createRoot" }) },
      { label: "子として作成", disabled: !singleSelection, onSelect: () => setPendingDialog({ kind: "createChild", parent: path }) },
      {
        label: "アイコン",
        submenu: [
          { label: "なし", onSelect: () => void tree.setLabel(path, null) },
          ...LABEL_CHOICES.map((c) => ({ label: c.label, onSelect: () => void tree.setLabel(path, c.color) })),
        ],
      },
      {
        label: "プロパティ",
        disabled: !singleSelection,
        onSelect: () => {
          setPendingDialog({ kind: "properties", path });
          void tree.getProperties(path).then((props) => setProperties(props));
        },
      },
      { label: "ファイルの場所を開く", disabled: !singleSelection, onSelect: () => void tree.openLocation(path) },
      isLocked
        ? { label: "編集ロック解除", disabled: !singleSelection, onSelect: () => void tree.setEditLock(path, false) }
        : { label: "編集ロック", disabled: !singleSelection, onSelect: () => void tree.setEditLock(path, true) },
    ];
  };

  const emptyAreaMenuItems: ContextMenuItem[] = [{ label: "メモを作成", onSelect: () => setPendingDialog({ kind: "createRoot" }) }];

  if (loading) {
    return <div className="ttn-tree ttn-tree-loading" ref={containerRef} />;
  }

  if (recoveryPending && recoveryKind === "permanentUnlock") {
    return (
      <div className="ttn-tree ttn-tree-message" ref={containerRef}>
        <p>前回の完全ロック解除処理が完了しませんでした。復旧処理が必要です。</p>
        {activeTabId && <PermanentUnlockRecoveryDialog tabId={activeTabId} tabLock={tabLock} />}
      </div>
    );
  }

  if (recoveryPending) {
    return (
      <div className="ttn-tree ttn-tree-message" ref={containerRef}>
        <p>前回のロック操作が完了しませんでした。復旧処理が必要です。</p>
        {recoveryStatus && (
          <RecoveryDialog
            variant="lock"
            doneCount={recoveryStatus.renamedCount}
            total={recoveryStatus.total}
            anomalous={recoveryStatus.anomalous}
            anomalousCount={recoveryStatus.anomalousCount}
            defaultAction={recoveryStatus.defaultAction}
            busy={tabLock.busy}
            errorMessage={tabLock.lastErrorCode ? errorMessageFor(tabLock.lastErrorCode) : null}
            onCancel={() => tabLock.clearLastError()}
            onFinish={(password) => {
              if (activeTabId) void tabLock.finishRecovery(activeTabId, password);
            }}
            onRestore={(password) => {
              if (activeTabId) void tabLock.restoreRecovery(activeTabId, password);
            }}
          />
        )}
      </div>
    );
  }

  // `docs/plans/tab-lock/PLAN.md` §3 `tab_unlock` step 7: `locked` intentionally stays `true`
  // even once a session is active — the real (decrypted) tree/name arrive via `nodes`/`tabName`
  // themselves (`tree_open`'s `session_tree_snapshot` branch, `commands/tab_lock.rs`), not via
  // `locked` flipping to `false`. Only show the lock placeholder while there's genuinely no
  // active session for this tab; once one exists, fall through to the normal tree render below.
  const hasActiveSession = activeTabId ? tabLock.activeSessionRealNames.has(activeTabId) : false;

  if (locked && !hasActiveSession) {
    return (
      <div className="ttn-tree ttn-tree-message" ref={containerRef}>
        <p>🔒 このタブはロックされています</p>
        <button type="button" onClick={() => setPendingDialog({ kind: "unlock" })}>
          🔓 ロック解除
        </button>
        {pendingDialog?.kind === "unlock" && (
          <PasswordDialog
            title="ロック解除"
            mode="unlock"
            busy={tabLock.busy}
            errorMessage={tabLock.lastErrorCode ? errorMessageFor(tabLock.lastErrorCode) : null}
            submitLabel="ロック解除"
            onCancel={() => {
              setPendingDialog(null);
              tabLock.clearLastError();
            }}
            onSubmit={async (password) => {
              tabLock.clearLastError();
              if (!activeTabId) return;
              const ok = await tabLock.unlockTab(activeTabId, password);
              if (ok) setPendingDialog(null);
            }}
          />
        )}
      </div>
    );
  }

  return (
    <div
      className="ttn-tree"
      ref={containerRef}
      tabIndex={0}
      onKeyDown={handleContainerKeyDown}
      onContextMenu={(e) => {
        // Only reached when the click target didn't already stop propagation
        // from a row's own handler, i.e. this is the empty-area case.
        e.preventDefault();
        setContextMenu({ x: e.clientX, y: e.clientY, path: null });
      }}
    >
      {/* Entry point (a) (`docs/plans/tab-lock-permanent-unlock/PLAN.md` §7.9) — an active unlock
          session lets the user permanently unlock this tab without re-entering its password. */}
      {locked && hasActiveSession && (
        <div className="ttn-tree-session-banner">
          <span>🔓 このタブは一時的にロック解除されています</span>
          <button type="button" onClick={() => setPendingDialog({ kind: "permanentUnlockConfirm" })}>
            完全にロック解除
          </button>
        </div>
      )}
      {pendingDialog?.kind === "permanentUnlockConfirm" && (
        <ConfirmDialog
          title="完全にロック解除"
          message={
            <>
              <p>このタブを完全にロック解除すると、暗号化前の通常のタブに戻ります。この操作は取り消せません。</p>
              <p>念のため、事前にフォルダ全体のバックアップを取っておくことをおすすめします。</p>
            </>
          }
          confirmLabel="完全にロック解除"
          onCancel={() => setPendingDialog(null)}
          onConfirm={async () => {
            if (!activeTabId) return;
            const ok = await tabLock.permanentUnlockTab(activeTabId, undefined);
            if (ok) setPendingDialog(null);
          }}
        />
      )}
      {nodes.length === 0 ? (
        <div className="ttn-tree-empty">
          <button type="button" onClick={() => setPendingDialog({ kind: "createRoot" })}>
            ＋ メモを作成
          </button>
        </div>
      ) : (
        <>
          <div className="ttn-tree-rows">
            {nodes.map((node, i) => (
              <TreeRow
                key={node.memoName}
                node={node}
                depth={0}
                ancestorGuides={[]}
                isLastChild={i === nodes.length - 1}
                selectedNames={selection.selected}
                openMemoName={openMemoName}
                renamingPath={renamingPath}
                dropTarget={dropTarget?.kind === "row" ? { path: dropTarget.path, position: dropTarget.position } : null}
                onToggleExpand={handleToggleExpand}
                onRowClick={handleRowClick}
                onRowContextMenu={handleRowContextMenu}
                onRowMouseDown={handleRowMouseDown}
                onRenameSubmit={handleRenameSubmit}
                onRenameCancel={() => setRenamingPath(null)}
              />
            ))}
          </div>
          {/* Click on empty area (below the last row) clears selection
              (`[spec §2.2]`). */}
          <div className="ttn-tree-empty-area" onClick={() => tree.handleEmptyAreaClick()} />
        </>
      )}

      {lastErrorCode && (
        <div className="ttn-tree-error" role="alert">
          <span>{errorMessageFor(lastErrorCode)}</span>
          <button type="button" onClick={clearLastError}>
            ×
          </button>
        </div>
      )}

      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          items={contextMenu.path ? buildContextMenuItems(contextMenu.path) : emptyAreaMenuItems}
        />
      )}

      {pendingDialog?.kind === "createRoot" && (
        <PromptDialog
          title="メモを作成"
          label="メモ名"
          validate={validateNameStrict}
          onCancel={() => setPendingDialog(null)}
          onSubmit={async (value) => {
            setPendingDialog(null);
            await tree.createMemo(value);
          }}
        />
      )}

      {pendingDialog?.kind === "createChild" && (
        <PromptDialog
          title="子メモを作成"
          label="メモ名"
          validate={validateNameStrict}
          onCancel={() => setPendingDialog(null)}
          onSubmit={async (value) => {
            const parent = pendingDialog.parent;
            setPendingDialog(null);
            await tree.createChildMemo(value, parent);
          }}
        />
      )}

      {pendingDialog?.kind === "deleteConfirm" && (
        <ConfirmDialog
          title="メモを削除"
          message={`選択した ${pendingDialog.paths.length} 件のメモ（子メモを含む）をごみ箱に移動します。`}
          confirmLabel="削除"
          onCancel={() => setPendingDialog(null)}
          onConfirm={async () => {
            const paths = pendingDialog.paths;
            setPendingDialog(null);
            await requestDelete(paths);
          }}
        />
      )}

      {pendingDialog?.kind === "trashFailedConfirm" && (
        <ConfirmDialog
          title="ごみ箱への移動に失敗しました"
          message="ごみ箱に移動できませんでした。このまま完全に削除しますか？この操作は取り消せません。"
          confirmLabel="完全に削除"
          onCancel={() => setPendingDialog(null)}
          onConfirm={async () => {
            const paths = pendingDialog.paths;
            setPendingDialog(null);
            await requestDelete(paths, true);
          }}
        />
      )}

      {pendingDialog?.kind === "properties" && (
        <Dialog
          title="プロパティ"
          onClose={() => {
            setPendingDialog(null);
            setProperties(null);
          }}
          footer={
            <>
              <button type="button" onClick={() => void tree.openLocation(pendingDialog.path)}>
                ファイルの場所を開く(O)
              </button>
              <button
                type="button"
                onClick={() => {
                  setPendingDialog(null);
                  setProperties(null);
                }}
              >
                閉じる
              </button>
            </>
          }
        >
          {properties ? (
            <>
              <section>
                <h3 className="ttn-properties-section-title">ファイル情報</h3>
                <dl className="ttn-properties-list">
                  <dt>ファイル名</dt>
                  <dd>{properties.fileName}</dd>
                  <dt>ファイルパス</dt>
                  <dd className="ttn-properties-file-path">{properties.filePath}</dd>
                  <dt>作成日時</dt>
                  <dd>{formatDateTime(properties.createdMs)}</dd>
                  <dt>更新日時</dt>
                  <dd>{formatDateTime(properties.modifiedMs)}</dd>
                  <dt>ファイルサイズ</dt>
                  <dd>{properties.fileSizeBytes.toLocaleString()} バイト</dd>
                </dl>
              </section>
              <section className="ttn-properties-section">
                <h3 className="ttn-properties-section-title">文字数カウント</h3>
                <dl className="ttn-properties-list">
                  <dt>総字数</dt>
                  <dd>{properties.charCount.toLocaleString()}</dd>
                  <dt>字数</dt>
                  <dd>{properties.charCountExcludingWhitespace.toLocaleString()}（改行・空白を除く）</dd>
                  <dt>行数</dt>
                  <dd>{properties.lineCount.toLocaleString()}</dd>
                </dl>
              </section>
            </>
          ) : (
            <p>読み込み中...</p>
          )}
        </Dialog>
      )}

      {pendingDialog?.kind === "unlockForDrop" && (
        <PasswordDialog
          title="ロックされたタブにドロップ"
          mode="unlock"
          busy={tabLock.busy}
          errorMessage={tabLock.lastErrorCode ? errorMessageFor(tabLock.lastErrorCode) : null}
          submitLabel="ロック解除して移動"
          onCancel={() => {
            // `docs/plans/tab-lock/PLAN.md` §5: "the drop is aborted — the source memo is
            // untouched, no partial state" — closing this dialog without submitting leaves
            // everything exactly as it was before the drop.
            setPendingDialog(null);
            tabLock.clearLastError();
          }}
          onSubmit={async (password) => {
            tabLock.clearLastError();
            const { targetTabId, paths, copy } = pendingDialog;
            const unlocked = await tabLock.unlockTab(targetTabId, password);
            if (!unlocked) return; // wrong password: dialog stays open, nothing touched
            await tree.moveCrossTab(paths, targetTabId, null, Number.MAX_SAFE_INTEGER, copy);
            setPendingDialog(null);
          }}
        />
      )}

      {importState.pendingCollision && (
        <ImportCollisionDialog
          sourceName={importState.pendingCollision.sourceName}
          renamePreview={importState.pendingCollision.renamePreview}
          overwriteDisabled={importState.pendingCollision.overwriteDisabled}
          onChoose={importState.resolveCollision}
          onCancel={importState.cancelImport}
        />
      )}
      <Toast toasts={importToast.toasts} onDismiss={importToast.dismissToast} />
    </div>
  );
}
