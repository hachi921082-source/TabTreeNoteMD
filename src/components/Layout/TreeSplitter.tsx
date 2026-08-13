// Drag handle between the tree pane and the editor pane (`docs/plans/window-operation/PLAN.md`
// §2): drag to resize (`AppSettings.window.treeWidth`), double-click to hide the tree pane
// entirely. Persists only on drag-end/toggle, never per-frame — same "preview locally while
// dragging, patch once on release" pattern already used by
// `components/Editor/Toolbar/MarkdownToolbar.tsx`'s toolbar-position drag.
import { useCallback, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import "./TreeSplitter.css";

const MIN_TREE_WIDTH = 120;
const MAX_TREE_WIDTH = 600;

export interface TreeSplitterProps {
  treeWidth: number;
  collapsed: boolean;
  side: "left" | "right";
  /** Called continuously while dragging, for live visual feedback — the caller owns applying this
   * to the tree pane's width, not this component. */
  onPreview: (width: number) => void;
  /** Called once, on drag release, with the final width to persist. */
  onCommit: (width: number) => void;
  onToggleCollapse: () => void;
}

export function TreeSplitter({ treeWidth, collapsed, side, onPreview, onCommit, onToggleCollapse }: TreeSplitterProps) {
  const [dragging, setDragging] = useState(false);
  const dragOriginRef = useRef<{ startX: number; originWidth: number; lastWidth: number } | null>(null);

  const handlePointerDown = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      if (collapsed || e.button !== 0) return;
      dragOriginRef.current = { startX: e.clientX, originWidth: treeWidth, lastWidth: treeWidth };
      setDragging(true);
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    },
    [collapsed, treeWidth],
  );

  const handlePointerMove = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      const origin = dragOriginRef.current;
      if (!origin) return;
      const delta = e.clientX - origin.startX;
      // The tree pane sits on the "start" side of the splitter; dragging away from it grows it
      // when the tree is on the left, shrinks it when the tree is on the right.
      const signedDelta = side === "left" ? delta : -delta;
      // Rounded to an integer: `WindowSettings.tree_width` is a Rust `u32` (`app_settings.rs`), so
      // a fractional width committed on pointer-up fails `settings_set`'s IPC deserialization
      // silently (GitHub issue #108) — only the always-integer `MIN`/`MAX_TREE_WIDTH` bounds
      // happened to mask this when the drag ended exactly at either clamp.
      const next = Math.round(Math.min(MAX_TREE_WIDTH, Math.max(MIN_TREE_WIDTH, origin.originWidth + signedDelta)));
      origin.lastWidth = next;
      onPreview(next);
    },
    [side, onPreview],
  );

  const handlePointerUp = useCallback(() => {
    const origin = dragOriginRef.current;
    dragOriginRef.current = null;
    setDragging(false);
    if (origin) onCommit(origin.lastWidth);
  }, [onCommit]);

  if (collapsed) {
    return (
      <button
        type="button"
        className="ttn-tree-splitter ttn-tree-splitter-collapsed"
        title="ツリーを表示"
        aria-label="ツリーを表示"
        onClick={onToggleCollapse}
      >
        {side === "left" ? "›" : "‹"}
      </button>
    );
  }

  return (
    <div
      className={`ttn-tree-splitter${dragging ? " ttn-tree-splitter-dragging" : ""}`}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onDoubleClick={onToggleCollapse}
      title="ドラッグでツリー幅を変更、ダブルクリックで非表示"
    />
  );
}
