// A single tree row, rendering itself and (when expanded) its children
// recursively (`docs/plans/memo-tree/PLAN.md` §4). Purely presentational —
// all state (selection, open memo, rename-in-progress, drag) lives in
// `TreeView`, which is also where the interaction semantics (click/Ctrl-
// click/Shift-click, DND hit-testing) are implemented as calls into
// `state/useTree.ts` and `shared/treeSelection.ts`/`shared/treeOps.ts`.
import { useRef } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import type { TreeNode } from "../../shared/models";
import type { ClickModifiers } from "../../shared/treeSelection";
import { FileIcon } from "./FileIcon";

export type DropPosition = "before" | "after" | "into";

export interface TreeRowProps {
  node: TreeNode;
  depth: number;
  selectedNames: ReadonlySet<string>;
  openMemoName: string | null;
  renamingPath: string | null;
  dropTarget: { path: string; position: DropPosition } | null;
  onToggleExpand: (memoName: string) => void;
  onRowClick: (memoName: string, modifiers: ClickModifiers) => void;
  onRowContextMenu: (memoName: string, x: number, y: number) => void;
  onRowMouseDown: (memoName: string, e: ReactMouseEvent) => void;
  onRenameSubmit: (memoName: string, newName: string) => void;
  onRenameCancel: () => void;
}

export function TreeRow(props: TreeRowProps) {
  const { node, depth, selectedNames, openMemoName, renamingPath, dropTarget, onToggleExpand, onRowClick, onRowContextMenu, onRowMouseDown, onRenameSubmit, onRenameCancel } =
    props;
  const inputRef = useRef<HTMLInputElement>(null);
  const isRenaming = renamingPath === node.memoName;
  const isSelected = selectedNames.has(node.memoName);
  const isOpen = openMemoName === node.memoName;
  const hasChildren = node.children.length > 0;
  const dropClass = dropTarget && dropTarget.path === node.memoName ? ` ttn-tree-row-drop-${dropTarget.position}` : "";

  return (
    <div className="ttn-tree-row-wrapper">
      <div
        className={`ttn-tree-row${isSelected ? " ttn-tree-row-selected" : ""}${isOpen ? " ttn-tree-row-open" : ""}${dropClass}`}
        style={{ paddingLeft: depth * 16 + 6 }}
        data-ttn-row-path={node.memoName}
        onMouseDown={(e) => onRowMouseDown(node.memoName, e)}
        onClick={(e) => {
          if (isRenaming) return;
          onRowClick(node.memoName, { ctrlKey: e.ctrlKey, shiftKey: e.shiftKey });
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
          onRowContextMenu(node.memoName, e.clientX, e.clientY);
        }}
      >
        {hasChildren ? (
          <button
            type="button"
            className="ttn-tree-toggle"
            onClick={(e) => {
              e.stopPropagation();
              onToggleExpand(node.memoName);
            }}
          >
            {node.expanded ? "▾" : "▸"}
          </button>
        ) : (
          <span className="ttn-tree-toggle-spacer" />
        )}
        <FileIcon color={node.label?.color ?? "var(--ttn-fg-muted)"} />
        {isRenaming ? (
          <input
            ref={inputRef}
            autoFocus
            defaultValue={node.memoName}
            className="ttn-tree-rename-input"
            onClick={(e) => e.stopPropagation()}
            onMouseDown={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                onRenameSubmit(node.memoName, (e.target as HTMLInputElement).value);
              } else if (e.key === "Escape") {
                e.preventDefault();
                onRenameCancel();
              }
            }}
            onBlur={(e) => onRenameSubmit(node.memoName, e.target.value)}
          />
        ) : (
          <span className={`ttn-tree-row-name${node.locked ? " ttn-tree-row-name-locked" : ""}`} title={node.locked ? "編集ロック中" : undefined}>
            {node.memoName}
          </span>
        )}
      </div>
      {node.expanded &&
        node.children.map((child) => (
          <TreeRow
            key={child.memoName}
            node={child}
            depth={depth + 1}
            selectedNames={selectedNames}
            openMemoName={openMemoName}
            renamingPath={renamingPath}
            dropTarget={dropTarget}
            onToggleExpand={onToggleExpand}
            onRowClick={onRowClick}
            onRowContextMenu={onRowContextMenu}
            onRowMouseDown={onRowMouseDown}
            onRenameSubmit={onRenameSubmit}
            onRenameCancel={onRenameCancel}
          />
        ))}
    </div>
  );
}
