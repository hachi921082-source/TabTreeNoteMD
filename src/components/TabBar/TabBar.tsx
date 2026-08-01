// Top tab bar (`docs/plans/tab-management/PLAN.md` §4). Renders `TabInfo[]`
// in order, click-to-switch, pointer-based drag-to-reorder, right-click
// shared context menu, keyboard-shortcut tab switching, OS folder
// drag-and-drop registration, and the "+" create-tab entry point.
//
// NOT implemented here (deliberately out of scope for this pass):
// - Mouse-wheel tab switching — removed from v1 scope entirely
//   (`docs/DECISIONS.md` "Mouse-Wheel Tab Switching: Removed from v1 Scope").
// - "Lock tab" context-menu action — belongs to `tab-lock/PLAN.md`
//   (out of scope for `tab-management/PLAN.md`), which owns adding it once
//   `tab_lock`/`tab_unlock` exist.
//
// The 📌 "always-on-top" toggle button IS implemented here: a single global
// button (not per-tab), leftmost in the tab bar's toolbar area (order:
// pin, search, theme toggle — the search/theme buttons themselves belong to
// `search-index/PLAN.md`/`appearance-theme/PLAN.md` and are added later to
// this same toolbar group). Session-only React state, never persisted
// (`docs/DECISIONS.md` "'Pin' Button: Global Window Always-On-Top Toggle,
// Not Per-Tab", `docs/plans/tab-management/PLAN.md` §4).
import { useCallback, useEffect, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { TabInfo } from "../../shared/models";
import type { UseTabsResult } from "../../state/useTabs";
import { ContextMenu } from "../ContextMenu/ContextMenu";
import { PromptDialog } from "../dialogs/PromptDialog";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { validateNameStrict, suggestSanitized } from "../../shared/nameSanitize";
import { comboFromEvent, resolveShortcuts } from "../../shared/shortcuts";
import { errorMessageFor } from "../../shared/errorMessages";
import { windowSetAlwaysOnTop } from "../../ipc/window";
import "./TabBar.css";

export interface TabBarProps {
  tabsState: UseTabsResult;
}

type PendingDialog =
  | { kind: "rename"; tab: TabInfo }
  | { kind: "remove"; tab: TabInfo }
  | { kind: "duplicateName"; path: string; suggestedName: string };

interface ContextMenuState {
  tabId: string;
  x: number;
  y: number;
}

function baseNameOf(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

export function TabBar({ tabsState }: TabBarProps) {
  const { tabs, activeTabId, selectTab, createTab, registerExistingTab, retryCreateWithName, renameTab, reorderTabs, removeTab, openTabLocation, lastErrorCode, clearLastError } =
    tabsState;

  const [orderIds, setOrderIds] = useState<string[]>([]);
  const draggingIdRef = useRef<string | null>(null);
  const dragStartOrderRef = useRef<string[]>([]);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [pendingDialog, setPendingDialog] = useState<PendingDialog | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  // Session-only; deliberately not read from or written to `AppSettings`.
  const [alwaysOnTop, setAlwaysOnTop] = useState(false);

  useEffect(() => {
    if (draggingIdRef.current) return; // don't clobber an in-progress drag's local order
    setOrderIds(tabs.map((t) => t.id));
  }, [tabs]);

  // Keyboard-shortcut tab switching (`shared/shortcuts.ts`). Mouse-wheel
  // switching intentionally not implemented.
  useEffect(() => {
    const shortcuts = resolveShortcuts(undefined);
    const handleKeyDown = (e: KeyboardEvent) => {
      if (orderIds.length === 0) return;
      const combo = comboFromEvent(e);
      const currentIndex = activeTabId ? orderIds.indexOf(activeTabId) : -1;
      if (combo === shortcuts["tab.switchNext"]) {
        e.preventDefault();
        selectTab(orderIds[(currentIndex + 1 + orderIds.length) % orderIds.length]);
      } else if (combo === shortcuts["tab.switchPrev"]) {
        e.preventDefault();
        selectTab(orderIds[(currentIndex - 1 + orderIds.length) % orderIds.length]);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [orderIds, activeTabId, selectTab]);

  const handleRegisterExisting = useCallback(
    async (path: string) => {
      const result = await registerExistingTab(path);
      if (!result.ok && result.code === "NAME_DUPLICATE") {
        const target = result.path ?? path;
        setPendingDialog({ kind: "duplicateName", path: target, suggestedName: suggestSanitized(baseNameOf(target)) });
      }
    },
    [registerExistingTab],
  );

  // Toggles OS-level "always on top" for the whole app window
  // (`window_set_always_on_top`). Optimistic local toggle, reverted if the
  // OS call fails; the button's own pressed state is the sole source of
  // truth for the current session (nothing is read back from the backend).
  const handleToggleAlwaysOnTop = useCallback(async () => {
    const next = !alwaysOnTop;
    setAlwaysOnTop(next);
    const result = await windowSetAlwaysOnTop(next);
    if (!result.ok) setAlwaysOnTop(!next);
  }, [alwaysOnTop]);

  const handleCreateTab = useCallback(async () => {
    const result = await createTab();
    if (result && !result.ok && result.code === "NAME_DUPLICATE" && result.path) {
      setPendingDialog({ kind: "duplicateName", path: result.path, suggestedName: suggestSanitized(baseNameOf(result.path)) });
    }
  }, [createTab]);

  // OS folder drag-and-drop onto the tab bar (`tab_register_existing`).
  // Uses Tauri's native drag-drop event (real filesystem paths) rather than
  // the HTML5 File API (which doesn't expose paths) — this is also why tab
  // reordering below uses pointer events instead of HTML5
  // draggable/dragstart/drop: native drag-drop must stay enabled
  // (`tauri.conf.json`'s `dragDropEnabled`, default `true`) for this to
  // receive real paths, but that setting is documented to conflict with the
  // browser's own HTML5 DnD API on Windows.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type !== "drop") return;
        const rect = containerRef.current?.getBoundingClientRect();
        if (!rect) return;
        const scale = window.devicePixelRatio || 1;
        const dropX = event.payload.position.x / scale;
        const dropY = event.payload.position.y / scale;
        const withinBar = dropX >= rect.left && dropX <= rect.right && dropY >= rect.top && dropY <= rect.bottom;
        if (!withinBar) return;
        const [firstPath] = event.payload.paths;
        if (firstPath) void handleRegisterExisting(firstPath);
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [handleRegisterExisting]);

  const handleMouseDownOnTab = (tabId: string) => (e: ReactMouseEvent) => {
    if (e.button !== 0) return;
    draggingIdRef.current = tabId;
    dragStartOrderRef.current = orderIds;
  };

  const handleMouseEnterTab = (tabId: string) => () => {
    const draggingId = draggingIdRef.current;
    if (!draggingId || draggingId === tabId) return;
    setOrderIds((current) => {
      const from = current.indexOf(draggingId);
      const to = current.indexOf(tabId);
      if (from === -1 || to === -1) return current;
      const next = current.slice();
      next.splice(from, 1);
      next.splice(to, 0, draggingId);
      return next;
    });
  };

  useEffect(() => {
    const handleMouseUp = () => {
      const draggingId = draggingIdRef.current;
      draggingIdRef.current = null;
      if (!draggingId) return;
      setOrderIds((current) => {
        const changed = current.some((id, i) => id !== dragStartOrderRef.current[i]);
        if (changed) void reorderTabs(current);
        return current;
      });
    };
    window.addEventListener("mouseup", handleMouseUp);
    return () => window.removeEventListener("mouseup", handleMouseUp);
  }, [reorderTabs]);

  const orderedTabs = orderIds.map((id) => tabs.find((t) => t.id === id)).filter((t): t is TabInfo => !!t);
  const contextTab = contextMenu ? tabs.find((t) => t.id === contextMenu.tabId) : undefined;

  return (
    <div className="ttn-tab-bar-wrapper">
      <div
        className="ttn-tab-bar"
        ref={containerRef}
        data-tauri-drag-region
        // Suppresses the native context menu on the empty tab-bar area
        // entirely — no custom menu either (`[spec §2.7]`, already-
        // established "suppress right-click outside registered tabs").
        // Each tab's own onContextMenu stops propagation before this fires.
        onContextMenu={(e) => e.preventDefault()}
      >
        {orderedTabs.map((tab) => (
          <div
            key={tab.id}
            className={`ttn-tab${tab.id === activeTabId ? " ttn-tab-active" : ""}${tab.locked ? " ttn-tab-locked" : ""}`}
            // Cross-tab memo DND drop-target identification
            // (`docs/plans/memo-tree/PLAN.md` §4) — `components/Tree/TreeView.tsx`
            // hit-tests for this attribute via `elementFromPoint`/`closest`
            // rather than this component needing to know anything about the
            // tree/DND itself.
            data-ttn-tab-id={tab.id}
            onMouseDown={handleMouseDownOnTab(tab.id)}
            onMouseEnter={handleMouseEnterTab(tab.id)}
            onClick={() => selectTab(tab.id)}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              setContextMenu({ tabId: tab.id, x: e.clientX, y: e.clientY });
            }}
            title={tab.name}
          >
            <span className="ttn-tab-name">{tab.name}</span>
          </div>
        ))}
        <button type="button" className="ttn-tab-create" title="新しいタブを作成" onClick={() => void handleCreateTab()}>
          ＋
        </button>

        {/* Toolbar group, left-to-right order: pin, search, theme toggle
            (`docs/plans/tab-management/PLAN.md` §4). Search/theme buttons
            are added here by `search-index/PLAN.md`/`appearance-theme/PLAN.md`
            when those features are implemented. */}
        <div className="ttn-tab-bar-toolbar">
          <button
            type="button"
            className={`ttn-toolbar-btn${alwaysOnTop ? " ttn-toolbar-btn-active" : ""}`}
            title={alwaysOnTop ? "最前面固定を解除" : "最前面に固定"}
            aria-pressed={alwaysOnTop}
            onClick={() => void handleToggleAlwaysOnTop()}
          >
            📌
          </button>
        </div>
      </div>

      {lastErrorCode && (
        <div className="ttn-tab-bar-error" role="alert">
          <span>{errorMessageFor(lastErrorCode)}</span>
          <button type="button" onClick={clearLastError}>
            ×
          </button>
        </div>
      )}

      {contextMenu && contextTab && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          items={[
            { label: "名前を変更", disabled: contextTab.locked, onSelect: () => setPendingDialog({ kind: "rename", tab: contextTab }) },
            { label: "タブの場所を開く", onSelect: () => void openTabLocation(contextTab.id) },
            { label: "タブを削除", disabled: contextTab.locked, danger: true, onSelect: () => setPendingDialog({ kind: "remove", tab: contextTab }) },
          ]}
        />
      )}

      {pendingDialog?.kind === "rename" && (
        <PromptDialog
          title="タブの名前を変更"
          label="新しい名前"
          initialValue={pendingDialog.tab.name}
          validate={validateNameStrict}
          onCancel={() => setPendingDialog(null)}
          onSubmit={async (value) => {
            const result = await renameTab(pendingDialog.tab.id, value);
            if (result.ok) setPendingDialog(null);
          }}
        />
      )}

      {pendingDialog?.kind === "remove" && (
        <ConfirmDialog
          title="タブを削除"
          message={`「${pendingDialog.tab.name}」をタブ一覧から削除します。フォルダやファイルは削除されません。`}
          confirmLabel="削除"
          onCancel={() => setPendingDialog(null)}
          onConfirm={async () => {
            await removeTab(pendingDialog.tab.id);
            setPendingDialog(null);
          }}
        />
      )}

      {pendingDialog?.kind === "duplicateName" && (
        <PromptDialog
          title="タブ名の重複"
          label="別の名前を入力してください"
          initialValue={pendingDialog.suggestedName}
          validate={validateNameStrict}
          onCancel={() => setPendingDialog(null)}
          onSubmit={async (value) => {
            const result = await retryCreateWithName(pendingDialog.path, value);
            if (result.ok) setPendingDialog(null);
            else if (result.code === "NAME_DUPLICATE") {
              setPendingDialog({ kind: "duplicateName", path: result.path ?? pendingDialog.path, suggestedName: value });
            }
          }}
        />
      )}
    </div>
  );
}
