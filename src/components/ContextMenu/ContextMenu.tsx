// Shared custom right-click context menu (`docs/spec.md` §9.1 item 2 —
// "Unified right-click menu: Adopt. Unify tree, editor, tab bar to single
// custom context menu"). This is the first consumer (tab bar); the tree and
// editor context menus (`memo-tree/PLAN.md`, `editor-save/PLAN.md`) should
// reuse this same component rather than building their own.
import { useEffect, useRef } from "react";
import "./ContextMenu.css";

export interface ContextMenuItem {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  danger?: boolean;
}

export interface ContextMenuProps {
  /** Viewport coordinates (CSS pixels) of the triggering right-click. */
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}

const ITEM_HEIGHT_PX = 32;
const MENU_MARGIN_PX = 16;

/** Positions at `(x, y)`; closes on outside click or Esc; flips upward if it
 * would overflow the bottom of the viewport (`[spec §2.2]`, "right-click
 * menu displays upward when at screen bottom edge"). */
export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handlePointerDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    // Capture phase so this runs before the click that reopens a menu
    // elsewhere would otherwise race with this one's own outside-click check.
    window.addEventListener("mousedown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("mousedown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [onClose]);

  const estimatedHeight = items.length * ITEM_HEIGHT_PX + MENU_MARGIN_PX;
  const flipUp = y + estimatedHeight > window.innerHeight;

  return (
    <div
      className="ttn-context-menu"
      ref={ref}
      role="menu"
      style={{
        left: x,
        top: flipUp ? undefined : y,
        bottom: flipUp ? window.innerHeight - y : undefined,
      }}
    >
      {items.map((item, i) => (
        <button
          key={i}
          type="button"
          role="menuitem"
          className={`ttn-context-menu-item${item.danger ? " ttn-context-menu-item-danger" : ""}`}
          disabled={item.disabled}
          onClick={() => {
            onClose();
            item.onSelect();
          }}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
