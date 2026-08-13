// Floating, freely-repositionable Markdown toolbar (`docs/plans/markdown-editing/PLAN.md` §4,
// `DECISIONS.md` "Markdown Toolbar: Finalized 16-Slot Fixed Grid Layout"). Fixed 8-column×2-row
// grid of `editor/toolbarSlots.ts`'s 16 actions; drag the header to reposition, toggle slot 16
// (`rearrangeToggle`) to reorder slots by drag. Position/order persist via `AppSettings.window`
// (`state/useSettings.ts`'s generic `patch`), written on drag-end/mode-exit only, never per-frame.
//
// The live/source view-mode runtime toggle (`docs/plans/markdown-editing/PLAN.md` §3) is NOT one
// of the 16 grid slots — it's a separate control in this panel's header, since the 16-slot grid's
// contents are fixed by spec and don't include it.
import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import type { EditorView } from "@codemirror/view";
import { Maximize2, Minimize2 } from "lucide-react";
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
import type { AppSettings, AppSettingsPatch, PaneMode } from "../../../shared/models";
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

/** Pane-mode header button's current-state label (`docs/plans/markdown/markdown-preview/PLAN.md`
 * §6.3) — a single button showing the *current* state and cycling to the next one on click. */
const PANE_MODE_LABEL: Record<PaneMode, string> = {
  edit: "編集のみ",
  split: "分割表示",
  preview: "プレビューのみ",
};

const DEFAULT_POSITION = { x: 24, y: 24 };

/** Debounce for persisting the toolbar's clamped position specifically during window *resize*
 * (as opposed to the mount/read and drag-end persist points below, which are each a single
 * one-shot event and so persist immediately). `resize` fires on every tick of an interactive
 * window-drag-resize, so persisting on every tick would mean a synchronous full-settings
 * `atomic_write` per animation frame (`src-tauri/src/settings/app_settings_store.rs`'s
 * `AppSettingsStore::patch`) — the same rapid-write concern `useWindowGeometry.ts`'s own
 * `PERSIST_DEBOUNCE_MS` already guards against for window geometry itself
 * (`docs/plans/persistence-fs/PLAN.md` §6.2), so this mirrors that value for consistency. The
 * *visual* clamp (`setBasePosition`) still applies immediately on every rAF-coalesced resize tick
 * — only the settings write is debounced. */
const RESIZE_PERSIST_DEBOUNCE_MS = 500;

type Size = { width: number; height: number };
type Position = { x: number; y: number };

/** Reserved vertical space (px) below the toolbar body for the color/font-size popover
 * (`.ttn-md-toolbar-popover`, `MarkdownToolbar.css`), so a toolbar snapped flush to the pane's
 * bottom edge still leaves room for the popover to render without overflowing (GitHub issue #24).
 * Derived from the popover's own CSS box: `margin-top` 2px + top/bottom `border` 1px×2 + top/bottom
 * `padding` 6px×2 (12px) + its tallest row content (~20px — either a `.ttn-md-toolbar-swatch`'s
 * fixed 20px circle, or a `.ttn-md-toolbar-popover-item`/`-clear` button's text+padding+border at
 * the default `--ui-font-scale`) = 2 + 2 + 12 + 20 = 38px. `SettingsDialog.tsx`'s `uiFontScale`
 * slider actually goes up to 150% (not 200% as first assumed here), which can inflate the
 * text-based rows to ~26-28px, pushing the true requirement to ~43px — rounded up further to 50px
 * as a simple constant with comfortable headroom rather than a precise dynamic measurement. The
 * popover's width (`left: 4px` within the toolbar, content ~144-192px wide) stays comfortably
 * inside the toolbar's own ~246px grid width, so only the bottom bound needs this reservation — the
 * right bound doesn't. */
const POPOVER_RESERVED_HEIGHT = 50;

/** Clamps `pos` (the toolbar's top-left, `left`/`top` in `.ttn-editor-pane` content-box
 * coordinates) so the toolbar's full `toolbarSize` box — plus `POPOVER_RESERVED_HEIGHT` of extra
 * bottom margin for the popover — never leaves `paneSize` (GitHub issue #24). Per-axis independent
 * clamping: this is both the mathematically nearest in-bounds point to any given `pos`, and — for
 * the specific case of dragging the toolbar out past the pane's bottom-right corner (the common
 * drag-out gesture) — it naturally snaps to that bottom-right corner, matching the issue's stated
 * preference, without needing to special-case "always jump to the corner" for a single-axis
 * overshoot (which would make sliding along one edge during a drag feel broken). */
function clampToolbarPosition(pos: Position, toolbarSize: Size, paneSize: Size): Position {
  const maxX = Math.max(0, paneSize.width - toolbarSize.width);
  const maxY = Math.max(0, paneSize.height - toolbarSize.height - POPOVER_RESERVED_HEIGHT);
  return {
    x: Math.min(Math.max(pos.x, 0), maxX),
    y: Math.min(Math.max(pos.y, 0), maxY),
  };
}

export interface MarkdownToolbarProps {
  /** Reads the currently-mounted `EditorView` at call time (not a stored ref), so this component
   * doesn't need re-rendering when the view itself is recreated on memo switch. */
  getView: () => EditorView | null;
  appSettings: AppSettings | null;
  patchSettings: (patch: AppSettingsPatch) => Promise<void>;
  liveViewEnabled: boolean;
  onToggleLiveView: () => void;
  /** `docs/plans/markdown/markdown-preview/PLAN.md` §4/§6 — current pane-layout tri-state, and the
   * single cycling toggle that advances it (`edit -> split -> preview -> edit -> ...`). Drives both
   * this component's new header button's label and the auto-minimize effect (§6.2). */
  paneMode: PaneMode;
  onCyclePaneMode: () => void;
  /** Reveals the edit pane (switches pane-mode to `"split"`) if it's currently `"preview"`, a no-op
   * otherwise (`PLAN.md` §4.1/§4.4's auto-reveal principle) — called before every grid action that
   * mutates the CodeMirror view, so the result of clicking a formatting button while pane-mode is
   * `"preview"` is actually visible instead of being applied to a hidden pane. */
  revealSplitIfPreviewOnly: () => void;
  /** `true` while the memo is read-only (edit-locked, or a conflict dialog is open) — disables
   * every content-mutating slot except the rearrange toggle itself (`[spec §2.2]`). */
  readOnly: boolean;
  /** Tab the currently-open memo belongs to — the "Image insert" slot's file-picker save target
   * (`docs/plans/image-handling/PLAN.md` §6). `null` only when no tab is open at all. */
  tabId: string | null;
  /** Name of the currently-open memo — only used by the PDF-export button's `disabled` check
   * (`docs/plans/markdown/pdf-export/PLAN.md` §4: "only on whether a memo is open at all", not
   * `readOnly`). Always truthy in practice by the time this component mounts (`EditorPane.tsx`
   * doesn't render `<MarkdownToolbar>` at all while `memoName` is null) — kept as an explicit,
   * defensive prop-driven check rather than relying on that caller-side invariant. */
  memoName: string | null;
  /** Surfaces an `image_save_pasted` failure via the shared non-blocking error banner
   * (`docs/plans/image-handling/PLAN.md` §6 step 4) — wired to `state/useEditor.ts`'s
   * `reportError` by the caller. */
  onImageError: (code: ErrorCode) => void;
  /** PDF export trigger (`docs/plans/markdown/pdf-export/PLAN.md` §4, `docs/MDEditor-spec.md` §6) —
   * same call-through pattern as `onToggleLiveView`/`onCyclePaneMode` above. Deliberately NOT gated
   * on `readOnly` anywhere in this component (§3's "printing never mutates the document"
   * availability rule) — only this button's own `disabled={!memoName}` check below applies. */
  onPdfExport: () => void;
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
  {
    getView,
    appSettings,
    patchSettings,
    liveViewEnabled,
    onToggleLiveView,
    paneMode,
    onCyclePaneMode,
    revealSplitIfPreviewOnly,
    readOnly,
    tabId,
    onImageError,
    onPdfExport,
    memoName,
  },
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
  // Slot reorder drag state, keyed by `ToolbarActionId` rather than array index (mirrors
  // `TabBar.tsx`'s `draggingIdRef`) so a mid-drag reorder never invalidates a stale index. Uses
  // plain pointer/mouse events, not HTML5 Drag-and-Drop — Tauri's native drag-drop
  // (`tauri.conf.json`'s `dragDropEnabled`, default `true`) conflicts with the browser's own HTML5
  // DnD events on Windows (see `TabBar.tsx`'s own comment on this), so this component follows
  // `TabBar.tsx`'s tab-reorder pattern instead (GitHub issue #25).
  const draggingIdRef = useRef<ToolbarActionId | null>(null);

  // `basePosition` is the clamped resting (non-drag) position actually rendered — kept in local
  // state (rather than reading `appSettings?.window.toolbarPosition` directly, as before) because
  // it must reflect the toolbar's/pane's actual measured DOM size, which the persisted value alone
  // doesn't know about (GitHub issue #24). Synced from the persisted value below whenever that
  // value changes externally (settings load, or an update from elsewhere), same pattern as `order`/
  // `persistedOrderKey` above.
  const persistedPositionKey = JSON.stringify(appSettings?.window.toolbarPosition ?? null);
  const [basePosition, setBasePosition] = useState<Position>(() => appSettings?.window.toolbarPosition ?? DEFAULT_POSITION);
  const basePositionRef = useRef(basePosition);
  basePositionRef.current = basePosition;
  const dragOriginRef = useRef<{ startX: number; startY: number; originX: number; originY: number; toolbarSize: Size; paneSize: Size } | null>(
    null,
  );
  const [dragPosition, setDragPosition] = useState<Position | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const resizePersistTimerRef = useRef<number | null>(null);

  // Minimize-to-header toggle (GitHub issue #26) — collapses the grid and snaps the panel to the
  // editor pane's bottom-right corner, reusing `clampToolbarPosition`'s bottom-right-snapping
  // behavior (GitHub issue #24) rather than a separate positioning path.
  //
  // Unlike `order`/`basePosition` above, `minimized` is NOT mirrored into local state via a passive
  // `useEffect` — it's read directly from `appSettings` on every render. That mirroring pattern has
  // a one-render lag baked in (a prop change doesn't retroactively update state a passive effect
  // hasn't run for yet), which is harmless for `order`/`basePosition` but actively corrupts data
  // here: `appSettings` starts out `null` on mount and only becomes the loaded value on a LATER
  // render (`App.tsx` doesn't gate this component's mount on the settings load), and the mount/read
  // position-clamp effect below is a `useLayoutEffect` — it runs within THAT SAME render's commit,
  // before any passive effect gets a chance to sync a lagging local `minimized` copy. For a
  // returning user with `toolbarMinimized: true` persisted, that means the position-clamp effect
  // would measure the panel while it's still rendered as the FULL grid (stale `minimized === false`
  // from the mount render), clamp the correctly-saved header-only position against that wrong box,
  // and immediately persist the wrong result — corrupting the saved position on every single launch.
  // Reading `appSettings` directly instead makes `minimized` update in the exact same render as
  // `persistedPositionKey`, so the position-clamp effect always measures a DOM whose grid-visibility
  // already agrees with the position it's about to clamp.
  //
  // `optimisticMinimized` is a separate, purely-local override used only so clicking the
  // minimize/restore button (`toggleMinimized`) hides/shows the grid immediately, without waiting on
  // the async `patchSettings` round-trip. It's cleared as soon as `appSettings` itself catches up to
  // the same value, at which point `appSettings` becomes authoritative again (and continues to be, if
  // e.g. settings later get reloaded from elsewhere).
  const settingsMinimized = appSettings?.window.toolbarMinimized ?? false;
  const [optimisticMinimized, setOptimisticMinimized] = useState<boolean | null>(null);
  useEffect(() => {
    if (optimisticMinimized !== null && optimisticMinimized === settingsMinimized) setOptimisticMinimized(null);
  }, [settingsMinimized, optimisticMinimized]);
  const minimized = optimisticMinimized ?? settingsMinimized;
  // Set right before `setOptimisticMinimized` in `toggleMinimized` below, so the layout effect that
  // reacts to `minimized` changing can tell "the user just clicked the toggle" (snap/restore the
  // position) apart from "`minimized` changed because settings were reloaded from elsewhere" (in
  // which case the persisted `toolbarPosition` — already correct from when it was last snapped — is
  // restored by the existing mount/read effect above instead; recomputing via `clampAndApply` here
  // too would just be redundant and would trigger an extra unnecessary settings write on every
  // launch).
  const pendingMinimizeToggleRef = useRef(false);

  /** Re-clamps `raw` against the toolbar's/pane's current measured size and applies it as the new
   * `basePosition` (always immediate — the visual position must never lag). If the clamp actually
   * moved the value, also writes the clamped position back to settings so it stays put on next
   * launch rather than drifting back toward a stale pre-clamp value once the window grows again,
   * per `persist`:
   * - `"immediate"`: writes right away — for the mount/read and drag-end call sites, each a single
   *   one-shot event.
   * - `"debounced"`: writes after `RESIZE_PERSIST_DEBOUNCE_MS` of inactivity (clearing/resetting any
   *   pending write on each call) — for the resize call site, which fires on every tick of an
   *   interactive window-drag-resize and would otherwise trigger a synchronous full-settings
   *   `atomic_write` per animation frame.
   * Falls back to rendering `raw` unclamped only if the panel/pane aren't measurable yet (e.g. not
   * yet mounted) — best-effort, corrected on the very next measurable run. */
  const clampAndApply = (raw: Position, persist: "immediate" | "debounced") => {
    const panel = panelRef.current;
    const pane = panel?.offsetParent as HTMLElement | null;
    if (!panel || !pane) {
      setBasePosition(raw);
      return;
    }
    const clamped = clampToolbarPosition(raw, { width: panel.offsetWidth, height: panel.offsetHeight }, { width: pane.clientWidth, height: pane.clientHeight });
    setBasePosition(clamped);
    if (clamped.x === raw.x && clamped.y === raw.y) return;
    if (persist === "immediate") {
      void patchSettings({ window: { toolbarPosition: clamped } });
      return;
    }
    if (resizePersistTimerRef.current !== null) window.clearTimeout(resizePersistTimerRef.current);
    resizePersistTimerRef.current = window.setTimeout(() => {
      resizePersistTimerRef.current = null;
      void patchSettings({ window: { toolbarPosition: clamped } });
    }, RESIZE_PERSIST_DEBOUNCE_MS);
  };

  // Mount/read clamp (GitHub issue #24): re-clamps whenever the persisted position changes
  // externally (initial settings load, or a change from elsewhere). `useLayoutEffect` so an
  // out-of-bounds persisted value (e.g. from a smaller window in a prior session) is corrected
  // before the first paint, not visible-then-corrected. Persists the correction immediately — see
  // `clampAndApply`'s doc comment.
  useLayoutEffect(() => {
    clampAndApply(appSettings?.window.toolbarPosition ?? DEFAULT_POSITION, "immediate");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [persistedPositionKey]);

  // Window-resize clamp (GitHub issue #24): shrinking the window can leave a previously-in-bounds
  // toolbar stranded outside the (now smaller) pane; growing the window intentionally leaves the
  // toolbar's position untouched (`clampAndApply` only moves it when the clamp actually changes the
  // value). `requestAnimationFrame`-coalesced since `resize` fires on every tick of an interactive
  // window-drag-resize, not just once at the end — the visual clamp (`setBasePosition`, inside
  // `clampAndApply`) still applies on every coalesced tick, but the settings write itself is
  // additionally debounced (`"debounced"`) since even rAF-coalescing alone (~60/s) is far more
  // frequent than this app's settings-write budget tolerates (see `clampAndApply`'s doc comment).
  useEffect(() => {
    let raf = 0;
    const handleResize = () => {
      if (raf) return;
      raf = window.requestAnimationFrame(() => {
        raf = 0;
        clampAndApply(basePositionRef.current, "debounced");
      });
    };
    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
      if (raf) window.cancelAnimationFrame(raf);
      if (resizePersistTimerRef.current !== null) window.clearTimeout(resizePersistTimerRef.current);
    };
  }, []);

  // Minimize/restore snap (GitHub issue #26): re-clamps only when `minimized` changed because of
  // `toggleMinimized` itself (see `pendingMinimizeToggleRef`'s doc comment above), after the grid's
  // conditional render has already committed — so `panelRef.current.offsetHeight` correctly
  // reflects the header-only box on minimize, or the full grid box again on restore, without a
  // separate DOM measurement pass. `useLayoutEffect` so the snap/restore happens before paint, same
  // as the mount/read and resize clamps.
  useLayoutEffect(() => {
    if (!pendingMinimizeToggleRef.current) return;
    pendingMinimizeToggleRef.current = false;
    // Any point past the pane's bottom-right corner resolves, via `clampToolbarPosition`'s
    // per-axis clamp, to the true bottom-right corner for the current size — the header-only box on
    // minimize, or the full grid box on restore. Restore deliberately snaps to bottom-right rather
    // than back to wherever the panel was before minimizing: minimize is reached from anywhere (drag,
    // auto-minimize on preview pane-mode), so "restore to prior position" would sometimes just
    // reproduce a spot the panel had been dragged out of the way from.
    clampAndApply({ x: Number.POSITIVE_INFINITY, y: Number.POSITIVE_INFINITY }, "immediate");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [minimized]);

  /** The "entering minimized" branch factored out so it can be shared between `toggleMinimized`'s
   * own click path and the pane-mode auto-minimize effect below
   * (`docs/plans/markdown/markdown-preview/PLAN.md` §6.2) without duplicating these four
   * state-setting/patch-settings lines. */
  const minimizeNow = () => {
    pendingMinimizeToggleRef.current = true;
    setOptimisticMinimized(true);
    void patchSettings({ window: { toolbarMinimized: true } });
  };

  const toggleMinimized = () => {
    if (rearranging) return; // see item 5 of the GitHub issue #26 write-up: keep this simple by
    // disabling the toggle entirely while a slot reorder is in progress, rather than deciding how a
    // hidden-mid-drag grid should behave.
    if (!minimized) {
      minimizeNow();
      return;
    }
    pendingMinimizeToggleRef.current = true;
    setOptimisticMinimized(false);
    void patchSettings({ window: { toolbarMinimized: false } });
  };

  // Auto-minimize when pane-mode enters `"preview"` (`PLAN.md` §6.2) — not fully hiding the toolbar
  // (the header, and its pane-mode toggle button, stay visible) so there's always a visible escape
  // hatch back to an editable view. Deliberately does NOT auto-restore when pane-mode leaves
  // `"preview"` — the toolbar's minimize/restore is otherwise a simple one-way manual toggle
  // everywhere else in this component; forcing a restore here would be an inconsistent special
  // case with no stated requirement for it. Skipped while a slot reorder is in progress, mirroring
  // `toggleMinimized`'s own `rearranging` guard above (same "keep this simple" rationale).
  useEffect(() => {
    if (paneMode === "preview" && !minimized && !rearranging) minimizeNow();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [paneMode]);

  const exitRearrangeMode = () => {
    // Clear the drag ref here (not just via the window `mouseup` listener below) because
    // `exitRearrangeMode` can run mid-drag (e.g. Esc pressed while the mouse button is still
    // held down). In that case the `mouseup` listener effect's cleanup fires immediately as
    // `rearranging` flips to false, removing the listener *before* the button is ever released
    // — leaving `draggingIdRef.current` stuck on the stale slot id and causing a later mere
    // hover (`onMouseEnter`, no fresh `mousedown`) to be misread as an in-progress drag.
    draggingIdRef.current = null;
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

  // Ends a slot-reorder drag gesture on mouseup anywhere in the window (mirrors `TabBar.tsx`'s
  // window-level `mouseup` listener). Unlike `TabBar.tsx`, this component doesn't persist here —
  // rearrange mode has no per-drag "commit" of its own; the order is only written out once, on
  // `exitRearrangeMode` (Esc or re-toggling the slot), so mouseup just clears the drag ref and
  // leaves the already-live `order` state as-is.
  useEffect(() => {
    if (!rearranging) return;
    const handler = () => {
      draggingIdRef.current = null;
    };
    window.addEventListener("mouseup", handler);
    return () => window.removeEventListener("mouseup", handler);
  }, [rearranging]);

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
    const panel = panelRef.current;
    const pane = panel?.offsetParent as HTMLElement | null;
    dragOriginRef.current = {
      startX: e.clientX,
      startY: e.clientY,
      originX: basePosition.x,
      originY: basePosition.y,
      // Measured once at drag start (not re-measured per move) — the toolbar's own size can't
      // change mid-drag, and the pane's size changing mid-drag (a concurrent window resize while
      // dragging) is an edge case the resize listener's own re-clamp already handles separately.
      toolbarSize: panel ? { width: panel.offsetWidth, height: panel.offsetHeight } : { width: 0, height: 0 },
      paneSize: pane ? { width: pane.clientWidth, height: pane.clientHeight } : { width: 0, height: 0 },
    };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const handleHeaderPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const origin = dragOriginRef.current;
    if (!origin) return;
    const raw = { x: origin.originX + (e.clientX - origin.startX), y: origin.originY + (e.clientY - origin.startY) };
    setDragPosition(clampToolbarPosition(raw, origin.toolbarSize, origin.paneSize));
  };
  const handleHeaderPointerUp = () => {
    if (!dragOriginRef.current) return;
    dragOriginRef.current = null;
    setDragPosition((current) => {
      if (current) {
        setBasePosition(current);
        void patchSettings({ window: { toolbarPosition: current } });
      }
      return null;
    });
  };

  const effectivePosition = dragPosition ?? basePosition;

  const invokeAction = (id: ToolbarActionId) => {
    if (id === "rearrangeToggle") {
      // Rearrange mode reorders the grid's own slots by drag, so it can't be *entered* while the
      // grid is hidden (GitHub issue #26, item 5) — this can only be reached via this action's
      // keyboard shortcut while minimized, since the grid button that normally invokes it isn't
      // rendered in that state. Exiting is still allowed unconditionally (defensive; in practice
      // `rearranging` can't be true while `minimized` is, since `toggleMinimized` itself refuses to
      // minimize while rearranging).
      if (minimized && !rearranging) return;
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
    // Auto-reveal (`docs/plans/markdown/markdown-preview/PLAN.md` §4.4): every one of these 15
    // remaining slots (all but `rearrangeToggle`, handled above) ultimately acts on the CodeMirror
    // view — reveal the edit pane first if it's currently hidden (`paneMode === "preview"`), so the
    // result is actually visible instead of landing on a hidden pane.
    revealSplitIfPreviewOnly();
    switch (id) {
      case "undo":
        runOnView(undoCommand);
        break;
      case "redo":
        runOnView(redoCommand);
        break;
      case "color":
        // Their trigger buttons live in the grid, so they can't normally be reached while
        // minimized — but this keyboard-shortcut path bypasses the button entirely, so it's
        // re-checked here too, to avoid opening a popover with no visible anchor button
        // (GitHub issue #26 follow-up review).
        if (minimized) return;
        setOpenPopover((v) => (v === "color" ? null : "color"));
        break;
      case "fontSize":
        if (minimized) return;
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
      className={`ttn-md-toolbar${rearranging ? " ttn-md-toolbar-rearranging" : ""}${minimized ? " ttn-md-toolbar-minimized" : ""}`}
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
        <button
          type="button"
          className="ttn-md-toolbar-pane-mode"
          onClick={onCyclePaneMode}
          aria-label={`ペイン表示モードを切り替え (現在: ${PANE_MODE_LABEL[paneMode]})`}
        >
          {PANE_MODE_LABEL[paneMode]}
        </button>
        {/* PDF export (`docs/plans/markdown/pdf-export/PLAN.md` §4) — never gated on `readOnly`
            (§3's "printing never mutates the document" availability rule); only disabled when no
            memo is open at all. */}
        <button type="button" className="ttn-md-toolbar-pdf-export" onClick={onPdfExport} disabled={!memoName}>
          PDF出力
        </button>
        <button
          type="button"
          className="ttn-md-toolbar-minimize-btn"
          title={minimized ? "ツールバーを元に戻す" : "ツールバーを最小化"}
          aria-label={minimized ? "ツールバーを元に戻す" : "ツールバーを最小化"}
          disabled={rearranging}
          onClick={toggleMinimized}
        >
          {minimized ? <Maximize2 size={14} /> : <Minimize2 size={14} />}
        </button>
      </div>

      {!minimized && (
      <div className="ttn-md-toolbar-grid">
        {order.map((id) => {
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
              disabled={disabled}
              onMouseDown={(e: ReactMouseEvent) => {
                if (!rearranging || e.button !== 0) return;
                draggingIdRef.current = id;
              }}
              onMouseEnter={() => {
                if (!rearranging) return;
                const draggingId = draggingIdRef.current;
                if (!draggingId || draggingId === id) return;
                setOrder((current) => {
                  const from = current.indexOf(draggingId);
                  const to = current.indexOf(id);
                  if (from === -1 || to === -1) return current;
                  const next = current.slice();
                  next.splice(from, 1);
                  next.splice(to, 0, draggingId);
                  return next;
                });
              }}
              onClick={() => invokeAction(id)}
            >
              <Icon size={16} />
            </button>
          );
        })}
      </div>
      )}

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
