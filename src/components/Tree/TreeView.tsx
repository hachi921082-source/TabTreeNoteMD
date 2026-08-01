// Left-pane memo tree (`docs/plans/memo-tree/PLAN.md` §4). Owns: rendering
// (via `TreeRow`), the right-click context menu (incl. the "Icon" submenu =
// `label.color` picker), F2 rename, the dedicated Ctrl+Delete force-delete
// shortcut, and DND (within-tree reorder/reparent + drag-onto-a-tab-bar-tab
// cross-tab move/copy). Selection/keyboard-nav *semantics* live in
// `state/useTree.ts` (backed by the pure functions in
// `shared/treeSelection.ts`) — this component only wires DOM events to them.
import { useCallback, useEffect, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { TreeRow } from "./TreeRow";
import type { DropPosition } from "./TreeRow";
import { ContextMenu } from "../ContextMenu/ContextMenu";
import type { ContextMenuItem } from "../ContextMenu/ContextMenu";
import { Dialog } from "../dialogs/Dialog";
import { PromptDialog } from "../dialogs/PromptDialog";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { validateNameStrict } from "../../shared/nameSanitize";
import { errorMessageFor } from "../../shared/errorMessages";
import { locateNode } from "../../shared/treeOps";
import type { UseTreeResult } from "../../state/useTree";
import type { LabelColor, MemoProperties } from "../../shared/models";
import "./Tree.css";

export interface TreeViewProps {
  tree: UseTreeResult;
}

type PendingDialog =
  | { kind: "createRoot" }
  | { kind: "createChild"; parent: string }
  | { kind: "deleteConfirm"; paths: string[] }
  | { kind: "trashFailedConfirm"; paths: string[] }
  | { kind: "properties"; path: string };

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

export function TreeView({ tree }: TreeViewProps) {
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

  const updateDropTarget = useCallback((next: DropTarget) => {
    dropTargetRef.current = next;
    setDropTarget(next);
  }, []);

  const { nodes, loading, locked, recoveryPending, selection, openMemoName, lastErrorCode, clearLastError, activeTabId } = tree;

  // ---- selection auto-follow: scroll the open/selected row into view -----
  useEffect(() => {
    if (!openMemoName) return;
    const el = containerRef.current?.querySelector<HTMLElement>(`[data-ttn-row-path="${CSS.escape(openMemoName)}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [openMemoName]);

  // ---- F2 rename / arrow-key nav: only while the tree container has focus,
  // must not steal focus from a future editor pane (`[spec §2.2]`) --------
  const handleContainerKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (renamingPath) return; // let the inline rename input's own handler deal with it
      if (e.key === "F2") {
        if (selection.selected.size === 1) {
          e.preventDefault();
          setRenamingPath(Array.from(selection.selected)[0]);
        }
        return;
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        tree.handleArrowNav("down");
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        tree.handleArrowNav("up");
      }
    },
    [renamingPath, selection.selected, tree],
  );

  // ---- Ctrl+Delete: global, regardless of current focus target ----------
  const requestDelete = useCallback(
    async (paths: string[], forceDirect = false) => {
      const outcome = await tree.deleteMemo(paths, forceDirect);
      if (!outcome.ok && outcome.code === "TRASH_FAILED") {
        setPendingDialog({ kind: "trashFailedConfirm", paths });
      }
    },
    [tree],
  );

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === "Delete") {
        const selected = Array.from(selection.selected);
        if (selected.length === 0) return;
        e.preventDefault();
        void requestDelete(selected);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [selection.selected, requestDelete]);

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
            void tree.moveCrossTab(drag.paths, target.tabId, null, Number.MAX_SAFE_INTEGER, e.ctrlKey);
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

  // ---- context menu item construction ------------------------------------
  const buildContextMenuItems = (path: string): ContextMenuItem[] => {
    const singleSelection = selection.selected.size <= 1 || !selection.selected.has(path);
    const selectedPaths = selection.selected.has(path) && selection.selected.size > 0 ? Array.from(selection.selected) : [path];
    return [
      { label: "名前を変更", disabled: !singleSelection, onSelect: () => setRenamingPath(path) },
      { label: "削除", danger: true, onSelect: () => setPendingDialog({ kind: "deleteConfirm", paths: selectedPaths }) },
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
    ];
  };

  const emptyAreaMenuItems: ContextMenuItem[] = [{ label: "メモを作成", onSelect: () => setPendingDialog({ kind: "createRoot" }) }];

  if (loading) {
    return <div className="ttn-tree ttn-tree-loading" ref={containerRef} />;
  }

  if (recoveryPending) {
    return (
      <div className="ttn-tree ttn-tree-message" ref={containerRef}>
        <p>前回のロック操作が完了しませんでした。復旧処理が必要です。</p>
      </div>
    );
  }

  if (locked) {
    return (
      <div className="ttn-tree ttn-tree-message" ref={containerRef}>
        <p>🔒 このタブはロックされています</p>
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
      {nodes.length === 0 ? (
        <div className="ttn-tree-empty">
          <button type="button" onClick={() => setPendingDialog({ kind: "createRoot" })}>
            ＋ メモを作成
          </button>
        </div>
      ) : (
        <>
          <div className="ttn-tree-rows">
            {nodes.map((node) => (
              <TreeRow
                key={node.memoName}
                node={node}
                depth={0}
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
            <button
              type="button"
              onClick={() => {
                setPendingDialog(null);
                setProperties(null);
              }}
            >
              閉じる
            </button>
          }
        >
          {properties ? (
            <dl className="ttn-properties-list">
              <dt>ファイルサイズ</dt>
              <dd>{properties.fileSizeBytes.toLocaleString()} バイト</dd>
              <dt>作成日時</dt>
              <dd>{properties.created}</dd>
              <dt>更新日時</dt>
              <dd>{properties.modified}</dd>
              <dt>文字数</dt>
              <dd>{properties.charCount.toLocaleString()}</dd>
            </dl>
          ) : (
            <p>読み込み中...</p>
          )}
        </Dialog>
      )}
    </div>
  );
}
