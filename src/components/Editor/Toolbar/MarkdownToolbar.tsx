// Floating, freely-repositionable Markdown toolbar (`docs/plans/markdown-editing/PLAN.md` §4,
// `DECISIONS.md` "Markdown Toolbar: Finalized 16-Slot Fixed Grid Layout"). Fixed 8-column×2-row
// grid of `editor/toolbarSlots.ts`'s 16 actions; drag the header to reposition, toggle slot 16
// (`rearrangeToggle`) to reorder slots by drag. Position/order persist via `AppSettings.window`
// (`state/useSettings.ts`'s generic `patch`), written on drag-end/mode-exit only, never per-frame.
//
// The live/source view-mode runtime toggle (`docs/plans/markdown-editing/PLAN.md` §3) is NOT one
// of the 16 grid slots — it's a separate control in this panel's header, since the 16-slot grid's
// contents are fixed by spec and don't include it.
import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import type { EditorView } from "@codemirror/view";
import { DEFAULT_TOOLBAR_ORDER, TOOLBAR_ACTIONS, normalizeToolbarOrder } from "../../../editor/toolbarSlots";
import type { ToolbarActionId } from "../../../editor/toolbarSlots";
import {
  applyColorSpan,
  applyFontSizeSpan,
  boldCommand,
  clearColorSpan,
  clearFontSizeSpan,
  codeBlockCommand,
  headingCommand,
  italicCommand,
  linkCommand,
  listCommand,
  quoteCommand,
  strikethroughCommand,
  tableCommand,
} from "../../../editor/decorationCommands";
import { insertImageViaFilePicker } from "../../../editor/imagePaste";
import { redoCommand, undoCommand } from "../../../editor/textCommands";
import { tocUpdateCommand } from "../../../editor/tocGenerator";
import type { AppSettings, AppSettingsPatch } from "../../../shared/models";
import type { ErrorCode } from "../../../shared/result";
import "./MarkdownToolbar.css";

/** Toolbar's Color button — closed 5-choice enum, adopted verbatim from the legacy toolbar
 * (`docs/plans/markdown-editing/PLAN.md` §4). */
const COLOR_SWATCHES: { label: string; hex: string }[] = [
  { label: "赤", hex: "#e53935" },
  { label: "青", hex: "#1e88e5" },
  { label: "緑", hex: "#43a047" },
  { label: "橙", hex: "#fb8c00" },
  { label: "紫", hex: "#8e24aa" },
];

/** Toolbar's Font-size button — closed 3-choice enum, same treatment as `COLOR_SWATCHES`. */
const FONT_SIZE_CHOICES: { label: string; px: string }[] = [
  { label: "小", px: "12px" },
  { label: "大", px: "20px" },
  { label: "特大", px: "28px" },
];

const DEFAULT_POSITION = { x: 24, y: 24 };

function moveItem<T>(arr: T[], from: number, to: number): T[] {
  const copy = [...arr];
  const [item] = copy.splice(from, 1);
  copy.splice(to, 0, item);
  return copy;
}

export interface MarkdownToolbarProps {
  /** Reads the currently-mounted `EditorView` at call time (not a stored ref), so this component
   * doesn't need re-rendering when the view itself is recreated on memo switch. */
  getView: () => EditorView | null;
  appSettings: AppSettings | null;
  patchSettings: (patch: AppSettingsPatch) => Promise<void>;
  liveViewEnabled: boolean;
  onToggleLiveView: () => void;
  /** `true` while the memo is read-only (edit-locked, or a conflict dialog is open) — disables
   * every content-mutating slot except the rearrange toggle itself (`[spec §2.2]`). */
  readOnly: boolean;
  /** Tab the currently-open memo belongs to — the "Image insert" slot's file-picker save target
   * (`docs/plans/image-handling/PLAN.md` §6). `null` only when no tab is open at all. */
  tabId: string | null;
  /** Surfaces an `image_save_pasted` failure via the shared non-blocking error banner
   * (`docs/plans/image-handling/PLAN.md` §6 step 4) — wired to `state/useEditor.ts`'s
   * `reportError` by the caller. */
  onImageError: (code: ErrorCode) => void;
}

/** Exposes the 16 grid actions for external (keyboard-shortcut) invocation
 * (`docs/plans/markdown-editing/PLAN.md` §4's "every action gets a default keyboard shortcut" —
 * `components/Editor/EditorPane.tsx`'s global shortcut dispatcher calls this rather than
 * duplicating the action switch statement below), since state like which popover is open or
 * whether rearrange mode is active must stay owned here, not lifted to the parent. */
export interface MarkdownToolbarHandle {
  invokeAction: (id: ToolbarActionId) => void;
}

export const MarkdownToolbar = forwardRef<MarkdownToolbarHandle, MarkdownToolbarProps>(function MarkdownToolbar(
  { getView, appSettings, patchSettings, liveViewEnabled, onToggleLiveView, readOnly, tabId, onImageError },
  ref,
) {
  const persistedOrderKey = JSON.stringify(appSettings?.window.toolbarOrder ?? null);
  const [order, setOrder] = useState<ToolbarActionId[]>(() => normalizeToolbarOrder(appSettings?.window.toolbarOrder));
  useEffect(() => {
    setOrder(normalizeToolbarOrder(appSettings?.window.toolbarOrder ?? DEFAULT_TOOLBAR_ORDER));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [persistedOrderKey]);

  const [rearranging, setRearranging] = useState(false);
  const [openPopover, setOpenPopover] = useState<"color" | "fontSize" | null>(null);
  const dragIndexRef = useRef<number | null>(null);

  const position = appSettings?.window.toolbarPosition ?? DEFAULT_POSITION;
  const dragOriginRef = useRef<{ startX: number; startY: number; originX: number; originY: number } | null>(null);
  const [dragPosition, setDragPosition] = useState<{ x: number; y: number } | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  const exitRearrangeMode = () => {
    setRearranging(false);
    void patchSettings({ window: { toolbarOrder: order } });
  };

  // Esc exits rearrange mode (`docs/plans/markdown-editing/PLAN.md` §4).
  useEffect(() => {
    if (!rearranging) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") exitRearrangeMode();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rearranging, order]);

  // Close an open color/font-size popover on outside click.
  useEffect(() => {
    if (!openPopover) return;
    const handler = (e: MouseEvent) => {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) setOpenPopover(null);
    };
    window.addEventListener("mousedown", handler, true);
    return () => window.removeEventListener("mousedown", handler, true);
  }, [openPopover]);

  const runOnView = (fn: (view: EditorView) => void) => {
    const view = getView();
    if (view) fn(view);
  };

  const handleHeaderPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if ((e.target as HTMLElement).closest("button")) return; // buttons (the view toggle) handle their own click
    dragOriginRef.current = { startX: e.clientX, startY: e.clientY, originX: position.x, originY: position.y };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const handleHeaderPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!dragOriginRef.current) return;
    const { startX, startY, originX, originY } = dragOriginRef.current;
    setDragPosition({ x: originX + (e.clientX - startX), y: originY + (e.clientY - startY) });
  };
  const handleHeaderPointerUp = () => {
    if (!dragOriginRef.current) return;
    dragOriginRef.current = null;
    setDragPosition((current) => {
      if (current) void patchSettings({ window: { toolbarPosition: current } });
      return null;
    });
  };

  const effectivePosition = dragPosition ?? position;

  const invokeAction = (id: ToolbarActionId) => {
    if (id === "rearrangeToggle") {
      if (rearranging) exitRearrangeMode();
      else setRearranging(true);
      return;
    }
    // "ON: clicking an icon does not invoke its action — only dragging works"
    // (`docs/plans/markdown-editing/PLAN.md` §4).
    if (rearranging) return;
    // Every other slot mutates the buffer — blocked while read-only (`[spec §2.2]`), same gate the
    // grid buttons themselves already show as `disabled`, re-checked here since keyboard-shortcut
    // invocation (`MarkdownToolbarHandle.invokeAction`) doesn't go through the button element.
    if (readOnly) return;
    switch (id) {
      case "undo":
        runOnView(undoCommand);
        break;
      case "redo":
        runOnView(redoCommand);
        break;
      case "color":
        setOpenPopover((v) => (v === "color" ? null : "color"));
        break;
      case "fontSize":
        setOpenPopover((v) => (v === "fontSize" ? null : "fontSize"));
        break;
      case "heading":
        runOnView(headingCommand);
        break;
      case "list":
        runOnView(listCommand);
        break;
      case "bold":
        runOnView(boldCommand);
        break;
      case "italic":
        runOnView(italicCommand);
        break;
      case "strikethrough":
        runOnView(strikethroughCommand);
        break;
      case "quote":
        runOnView(quoteCommand);
        break;
      case "link":
        runOnView(linkCommand);
        break;
      case "codeBlock":
        runOnView(codeBlockCommand);
        break;
      case "tocUpdate":
        runOnView(tocUpdateCommand);
        break;
      case "table":
        runOnView(tableCommand);
        break;
      case "image":
        runOnView((v) => insertImageViaFilePicker(v, tabId, onImageError));
        break;
    }
  };

  useImperativeHandle(ref, () => ({ invokeAction }));

  return (
    <div
      ref={panelRef}
      className={`ttn-md-toolbar${rearranging ? " ttn-md-toolbar-rearranging" : ""}`}
      style={{ left: effectivePosition.x, top: effectivePosition.y }}
    >
      <div
        className="ttn-md-toolbar-header"
        onPointerDown={handleHeaderPointerDown}
        onPointerMove={handleHeaderPointerMove}
        onPointerUp={handleHeaderPointerUp}
      >
        <span className="ttn-md-toolbar-drag-hint" aria-hidden="true">
          ⋮⋮
        </span>
        <button type="button" className="ttn-md-toolbar-view-toggle" onClick={onToggleLiveView}>
          {liveViewEnabled ? "ライブ表示" : "ソース表示"}
        </button>
      </div>

      <div className="ttn-md-toolbar-grid">
        {order.map((id, index) => {
          const action = TOOLBAR_ACTIONS[id];
          const Icon = action.icon;
          const disabled = readOnly && id !== "rearrangeToggle";
          return (
            <button
              key={id}
              type="button"
              className="ttn-md-toolbar-btn"
              title={action.label}
              aria-label={action.label}
              draggable={rearranging}
              disabled={disabled}
              onDragStart={() => {
                dragIndexRef.current = index;
              }}
              onDragOver={(e) => {
                if (rearranging) e.preventDefault();
              }}
              onDrop={(e) => {
                e.preventDefault();
                if (!rearranging || dragIndexRef.current === null) return;
                setOrder((current) => moveItem(current, dragIndexRef.current as number, index));
                dragIndexRef.current = null;
              }}
              onClick={() => invokeAction(id)}
            >
              <Icon size={16} />
            </button>
          );
        })}
      </div>

      {openPopover === "color" && (
        <div className="ttn-md-toolbar-popover" role="menu">
          {COLOR_SWATCHES.map((c) => (
            <button
              key={c.hex}
              type="button"
              className="ttn-md-toolbar-swatch"
              style={{ background: c.hex }}
              title={c.label}
              aria-label={c.label}
              onClick={() => {
                runOnView((v) => applyColorSpan(v, c.hex));
                setOpenPopover(null);
              }}
            />
          ))}
          <button
            type="button"
            className="ttn-md-toolbar-popover-clear"
            onClick={() => {
              runOnView(clearColorSpan);
              setOpenPopover(null);
            }}
          >
            解除
          </button>
        </div>
      )}

      {openPopover === "fontSize" && (
        <div className="ttn-md-toolbar-popover" role="menu">
          {FONT_SIZE_CHOICES.map((f) => (
            <button
              key={f.px}
              type="button"
              className="ttn-md-toolbar-popover-item"
              onClick={() => {
                runOnView((v) => applyFontSizeSpan(v, f.px));
                setOpenPopover(null);
              }}
            >
              {f.label}
            </button>
          ))}
          <button
            type="button"
            className="ttn-md-toolbar-popover-clear"
            onClick={() => {
              runOnView(clearFontSizeSpan);
              setOpenPopover(null);
            }}
          >
            解除
          </button>
        </div>
      )}
    </div>
  );
});
