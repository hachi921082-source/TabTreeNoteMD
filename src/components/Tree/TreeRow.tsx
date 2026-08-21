// A single tree row, rendering itself and (when expanded) its children
// recursively (`docs/plans/memo-tree/PLAN.md` §4). Purely presentational —
// all state (selection, open memo, rename-in-progress, drag) lives in
// `TreeView`, which is also where the interaction semantics (click/Ctrl-
// click/Shift-click, DND hit-testing) are implemented as calls into
// `state/useTree.ts` and `shared/treeSelection.ts`/`shared/treeOps.ts`.
//
// Indentation is rendered as VSCode-explorer-style guide-line segments
// (`docs/plans/tree-guide-lines/PLAN.md`, GitHub issue #60) rather than a
// plain `paddingLeft`. `ancestorGuides[i]` says whether the ancestor at
// depth `i + 1` still has a following sibling below it (continuing vertical
// line) or was itself a last child (blank). Root-level rows (`depth === 0`)
// render no guide/branch segment at all — see PLAN.md §2.
import { useRef } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import type { TreeNode } from "../../shared/models";
import type { ClickModifiers } from "../../shared/treeSelection";
import { useT } from "../../i18n";
import { FileIcon } from "./FileIcon";

export type DropPosition = "before" | "after" | "into";

export interface TreeRowProps {
  node: TreeNode;
  depth: number;
  ancestorGuides: boolean[];
  isLastChild: boolean;
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
  const {
    node,
    depth,
    ancestorGuides,
    isLastChild,
    selectedNames,
    openMemoName,
    renamingPath,
    dropTarget,
    onToggleExpand,
    onRowClick,
    onRowContextMenu,
    onRowMouseDown,
    onRenameSubmit,
    onRenameCancel,
  } = props;
  const t = useT();
  const inputRef = useRef<HTMLInputElement>(null);
  const isRenaming = renamingPath === node.memoName;
  const isSelected = selectedNames.has(node.memoName);
  const isOpen = openMemoName === node.memoName;
  const hasChildren = node.children.length > 0;
  const dropClass = dropTarget && dropTarget.path === node.memoName ? ` ttn-tree-row-drop-${dropTarget.position}` : "";
  const childAncestorGuides = depth === 0 ? [] : [...ancestorGuides, !isLastChild];

  return (
    <div className="ttn-tree-row-wrapper">
      <div
        className={`ttn-tree-row${isSelected ? " ttn-tree-row-selected" : ""}${isOpen ? " ttn-tree-row-open" : ""}${dropClass}`}
        data-ttn-row-path={node.memoName}
        onMouseDown={(e) => onRowMouseDown(node.memoName, e)}
        onClick={(e) => {
          if (isRenaming) return;
          onRowClick(node.memoName, { ctrlKey: e.ctrlKey, shiftKey: e.shiftKey });
        }}
        onContextMenu={(e) => {
          if (isRenaming) {
            e.stopPropagation();
            return;
          }
          e.preventDefault();
          e.stopPropagation();
          onRowContextMenu(node.memoName, e.clientX, e.clientY);
        }}
      >
        {depth > 0 && (
          <div className="ttn-tree-guides">
            {ancestorGuides.map((hasMoreSiblings, i) => (
              <span key={i} className={`ttn-tree-guide ${hasMoreSiblings ? "ttn-tree-guide-continue" : "ttn-tree-guide-blank"}`} />
            ))}
            <span className={`ttn-tree-guide ttn-tree-guide-branch ${isLastChild ? "ttn-tree-guide-branch-last" : "ttn-tree-guide-branch-mid"}`} />
          </div>
        )}
        <div className="ttn-tree-row-content">
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
            <span className={`ttn-tree-row-name${node.locked ? " ttn-tree-row-name-locked" : ""}`} title={node.locked ? t("treeRow.editLocked") : undefined}>
              {node.memoName}
            </span>
          )}
        </div>
      </div>
      {node.expanded &&
        node.children.map((child, i) => (
          <TreeRow
            key={child.memoName}
            node={child}
            depth={depth + 1}
            ancestorGuides={childAncestorGuides}
            isLastChild={i === node.children.length - 1}
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
