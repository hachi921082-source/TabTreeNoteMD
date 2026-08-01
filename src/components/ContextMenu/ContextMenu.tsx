// Shared custom right-click context menu (`docs/spec.md` §9.1 item 2 —
// "Unified right-click menu: Adopt. Unify tree, editor, tab bar to single
// custom context menu"). First consumer was the tab bar; `memo-tree/PLAN.md`'s
// tree context menu is the first consumer of `submenu` (its "Icon" entry —
// the `label.color` picker, `memo-tree/PLAN.md` §3/§4) — `editor-save/PLAN.md`'s
// editor context menu should reuse this same component too rather than
// building its own.
import { useEffect, useRef, useState } from "react";
import "./ContextMenu.css";

export interface ContextMenuItem {
  label: string;
  /** Omitted for an item that only exists to hold a `submenu` (no direct action of its own). */
  onSelect?: () => void;
  disabled?: boolean;
  danger?: boolean;
  /**
   * Sub-items shown as an indented accordion section beneath this item when clicked (not a
   * hover flyout — simpler, and still keeps the whole menu open until a leaf item is chosen).
   */
  submenu?: ContextMenuItem[];
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
  const [expandedIndex, setExpandedIndex] = useState<number | null>(null);

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

  const expandedSubmenuLength = expandedIndex !== null ? (items[expandedIndex]?.submenu?.length ?? 0) : 0;
  const estimatedHeight = (items.length + expandedSubmenuLength) * ITEM_HEIGHT_PX + MENU_MARGIN_PX;
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
        <div key={i}>
          <button
            type="button"
            role="menuitem"
            aria-haspopup={item.submenu ? "true" : undefined}
            aria-expanded={item.submenu ? expandedIndex === i : undefined}
            className={`ttn-context-menu-item${item.danger ? " ttn-context-menu-item-danger" : ""}${item.submenu ? " ttn-context-menu-item-parent" : ""}`}
            disabled={item.disabled}
            onClick={() => {
              if (item.submenu) {
                setExpandedIndex((current) => (current === i ? null : i));
                return;
              }
              onClose();
              item.onSelect?.();
            }}
          >
            {item.label}
            {item.submenu && <span className="ttn-context-menu-caret">{expandedIndex === i ? "▾" : "▸"}</span>}
          </button>
          {item.submenu && expandedIndex === i && (
            <div className="ttn-context-menu-submenu" role="menu">
              {item.submenu.map((sub, j) => (
                <button
                  key={j}
                  type="button"
                  role="menuitem"
                  className={`ttn-context-menu-item${sub.danger ? " ttn-context-menu-item-danger" : ""}`}
                  disabled={sub.disabled}
                  onClick={() => {
                    onClose();
                    sub.onSelect?.();
                  }}
                >
                  {sub.label}
                </button>
              ))}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
