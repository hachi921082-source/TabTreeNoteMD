// Top tab bar (`docs/plans/tab-management/PLAN.md` §4). Renders `TabInfo[]`
// in order, click-to-switch, pointer-based drag-to-reorder, right-click
// shared context menu, keyboard-shortcut tab switching, OS folder
// drag-and-drop registration, and the "+" create-tab entry point.
//
// "Lock tab" / "Unlock tab" ARE implemented here (`docs/plans/tab-lock/PLAN.md` §3) — the
// context-menu entry point `tab-management/PLAN.md` itself deferred to this plan.
//
// NOT implemented here (deliberately out of scope for this pass):
// - Mouse-wheel tab switching — removed from v1 scope entirely
//   (`docs/DECISIONS.md` "Mouse-Wheel Tab Switching: Removed from v1 Scope").
//
// The 📌 "always-on-top" toggle button IS implemented here: a single global
// button (not per-tab), leftmost in the tab bar's toolbar area (order:
// pin, search, theme toggle — the theme-toggle button itself belongs to
// `appearance-theme/PLAN.md` and is added later to this same toolbar group).
// Session-only React state, never persisted (`docs/DECISIONS.md` "'Pin'
// Button: Global Window Always-On-Top Toggle, Not Per-Tab",
// `docs/plans/tab-management/PLAN.md` §4).
//
// The 🔍 cross-tab search button (`docs/plans/search-index/PLAN.md` §5) IS also implemented
// here, immediately to the right of the pin button per `[spec §2.1]`'s layout note ("pin sits to
// its left") — opens `CrossTabSearchDialog`, whose "open a hit" callback is threaded down from
// `App.tsx` since it needs both tab-switch and tree-focus state this component doesn't own.
import { useCallback, useEffect, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { AppSettings, AppSettingsPatch, TabInfo } from "../../shared/models";
import type { UseTabsResult } from "../../state/useTabs";
import type { UseTabLockResult } from "../../state/useTabLock";
import { ContextMenu } from "../ContextMenu/ContextMenu";
import { PromptDialog } from "../dialogs/PromptDialog";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { PasswordDialog } from "../dialogs/PasswordDialog";
import { validateNameStrict, suggestSanitized } from "../../shared/nameSanitize";
import { comboFromEvent, resolveShortcuts } from "../../shared/shortcuts";
import { errorMessageFor } from "../../shared/errorMessages";
import { windowSetAlwaysOnTop } from "../../ipc/window";
import { TitleBarControls } from "./TitleBarControls";
import { SettingsDialog } from "../Settings/SettingsDialog";
import { CrossTabSearchDialog } from "../Search/CrossTabSearchDialog";
import { useCrossTabSearch } from "../../state/useCrossTabSearch";
import "./TabBar.css";

export interface TabBarProps {
  tabsState: UseTabsResult;
  tabLockState: UseTabLockResult;
  /** Called before locking `tabId`, only when it's the currently-active tab with an open memo —
   * flushes and closes that memo first (`docs/plans/tab-lock/PLAN.md` §3 `tab_lock` step 2). A
   * no-op for any other tab. Wired from `App.tsx` since it needs `EditorPane`'s imperative
   * `flushSaveNow` handle. */
  beforeLockTab: (tabId: string) => Promise<void>;
  /** Needed for the tree-side flip button (`docs/plans/window-operation/PLAN.md` §2) and the ⚙
   * Settings dialog (§7). */
  settings: AppSettings | null;
  patchSettings: (patch: AppSettingsPatch) => Promise<void>;
  applyFetched: (settings: AppSettings) => void;
  /** Opens a cross-tab search hit (`docs/plans/search-index/PLAN.md` §5) — switches tabs if
   * needed and follows tree selection. Wired from `App.tsx`, which owns both tab and tree state. */
  onOpenSearchHit: (tabId: string, memoName: string) => void;
}

type PendingDialog =
  | { kind: "rename"; tab: TabInfo }
  | { kind: "remove"; tab: TabInfo }
  | { kind: "duplicateName"; path: string; suggestedName: string }
  | { kind: "createNew"; suggestedName?: string }
  | { kind: "lockSetup"; tab: TabInfo }
  | { kind: "unlock"; tab: TabInfo }
  /** Entry point (b) (`docs/plans/tab-lock-permanent-unlock/PLAN.md` §7.8) — permanently
   * unlocking a locked tab from the tab bar's context menu, regardless of active-session state
   * (both entry points route through this same confirmation; only the password stage below is
   * skipped when a session already exists). */
  | { kind: "permanentUnlockConfirm"; tab: TabInfo }
  /** Shown only when `tab` has no active unlock session at confirm time. */
  | { kind: "permanentUnlockPassword"; tab: TabInfo };

interface ContextMenuState {
  tabId: string;
  x: number;
  y: number;
}

function baseNameOf(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

export function TabBar({ tabsState, tabLockState, beforeLockTab, settings, patchSettings, applyFetched, onOpenSearchHit }: TabBarProps) {
  const {
    tabs,
    activeTabId,
    selectTab,
    createTab,
    registerExistingTab,
    createNewTab,
    retryCreateWithName,
    renameTab,
    reorderTabs,
    removeTab,
    openTabLocation,
    lastErrorCode,
    clearLastError,
  } = tabsState;

  const [orderIds, setOrderIds] = useState<string[]>([]);
  const draggingIdRef = useRef<string | null>(null);
  const dragStartOrderRef = useRef<string[]>([]);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [pendingDialog, setPendingDialog] = useState<PendingDialog | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  // Session-only; deliberately not read from or written to `AppSettings`.
  const [alwaysOnTop, setAlwaysOnTop] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const crossTabSearch = useCrossTabSearch(activeTabId);

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

  const handleOpenCreateNew = useCallback(() => {
    setPendingDialog({ kind: "createNew" });
  }, []);

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
        {/* GitHub issue #153 — this app's own UI never actually rendered its name anywhere; what
            looked like one in a screenshot was the OS window title, not this component.
            GitHub issue #173 — the divider rule after the word is what visually closes off the
            "label zone" before the first real tab starts. */}
        <span className="ttn-tab-brand">
          <span className="ttn-tab-brand-word">TabTreeNoteMD</span>
          <span className="ttn-tab-brand-rule" aria-hidden="true" />
        </span>
        {orderedTabs.map((tab) => {
          // While a `tab-lock/PLAN.md` unlock session is active for this tab, show its real name
          // in place of the 🔒-placeholder cached in `AppSettings.tabs[i].name`
          // (`docs/DECISIONS.md` "Tab-Lock Placeholder Display Name", `tab-lock/PLAN.md` §3
          // `tab_unlock` step 7).
          const sessionRealName = tabLockState.activeSessionRealNames.get(tab.id);
          const displayName = sessionRealName ?? tab.name;
          return (
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
              title={displayName}
            >
              <span className="ttn-tab-name">{displayName}</span>
            </div>
          );
        })}
        <button type="button" className="ttn-tab-create" title="既存フォルダを登録" onClick={() => void handleCreateTab()}>
          📂
        </button>
        <button type="button" className="ttn-tab-create" title="新しいタブを作成" onClick={handleOpenCreateNew}>
          ＋
        </button>

        {/* Toolbar group, left-to-right order: pin, search, theme toggle
            (`docs/plans/tab-management/PLAN.md` §4). The theme-toggle button
            is added here by `appearance-theme/PLAN.md` when implemented. */}
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
          {/* Cross-tab full-text search (`docs/plans/search-index/PLAN.md` §5) — immediately to
              the right of the pin button per `[spec §2.1]`'s layout note. */}
          <button type="button" className="ttn-toolbar-btn" title="検索" aria-label="検索" onClick={crossTabSearch.open}>
            🔍
          </button>
          {/* Flips which side the tree pane renders on (`docs/plans/window-operation/PLAN.md` §2).
              The hide/show toggle itself lives on the splitter between the two panes, not here. */}
          <button
            type="button"
            className="ttn-toolbar-btn"
            title="ツリーの位置を切り替え"
            aria-label="ツリーの位置を切り替え"
            onClick={() => {
              const next = (settings?.window.treeSide ?? "left") === "left" ? "right" : "left";
              void patchSettings({ window: { treeSide: next } });
            }}
          >
            {/* Trailing U+FE0E (VS15) forces text presentation, avoiding Windows' color-emoji-font
                fallback which ignores the CSS `color` property. */}
            ⇄︎
          </button>
          {/* Settings screen shell (`docs/plans/window-operation/PLAN.md` §7). */}
          <button type="button" className="ttn-toolbar-btn" title="設定" aria-label="設定" onClick={() => setSettingsOpen(true)}>
            ⚙︎
          </button>
        </div>

        {/* Custom minimize/maximize/close controls, replacing the native title bar removed by
            `decorations: false` (`docs/plans/window-operation/PLAN.md` §1). Always the rightmost
            group. */}
        <TitleBarControls />
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
            contextTab.locked
              ? {
                  label: "ロック解除",
                  disabled: tabLockState.activeSessionRealNames.has(contextTab.id),
                  onSelect: () => setPendingDialog({ kind: "unlock", tab: contextTab }),
                }
              : { label: "タブをロック", onSelect: () => setPendingDialog({ kind: "lockSetup", tab: contextTab }) },
            ...(contextTab.locked
              ? [
                  {
                    label: "完全にロック解除",
                    onSelect: () => setPendingDialog({ kind: "permanentUnlockConfirm", tab: contextTab }),
                  },
                ]
              : []),
            {
              label: "タブを削除",
              // `docs/plans/tab-lock/PLAN.md` §4: locked-with-no-session rejects; an active
              // session lifts the restriction (the backend re-checks this regardless; disabling
              // it here is just the matching UI affordance).
              disabled: contextTab.locked && !tabLockState.activeSessionRealNames.has(contextTab.id),
              danger: true,
              onSelect: () => setPendingDialog({ kind: "remove", tab: contextTab }),
            },
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

      {pendingDialog?.kind === "createNew" && (
        <PromptDialog
          title="新しいタブを作成"
          label="タブ（フォルダ）名"
          initialValue={pendingDialog.suggestedName}
          submitLabel="作成"
          validate={validateNameStrict}
          onCancel={() => setPendingDialog(null)}
          onSubmit={async (value) => {
            const result = await createNewTab(value);
            if (result.ok) setPendingDialog(null);
            else if (result.code === "NAME_DUPLICATE") {
              setPendingDialog({ kind: "createNew", suggestedName: suggestSanitized(value) });
            }
          }}
        />
      )}

      {pendingDialog?.kind === "lockSetup" && (
        <PasswordDialog
          title={`「${pendingDialog.tab.name}」をロック`}
          mode="setup"
          busy={tabLockState.busy}
          errorMessage={tabLockState.lastErrorCode ? errorMessageFor(tabLockState.lastErrorCode) : null}
          submitLabel="ロック"
          onCancel={() => {
            setPendingDialog(null);
            tabLockState.clearLastError();
          }}
          onSubmit={async (password, hint) => {
            tabLockState.clearLastError();
            const tabId = pendingDialog.tab.id;
            // `docs/plans/tab-lock/PLAN.md` §3 `tab_lock` step 2: force-save-and-close this tab's
            // open memo first, if it's the active tab. No-op for any other tab.
            await beforeLockTab(tabId);
            const ok = await tabLockState.lockTab(tabId, password, hint);
            if (ok) setPendingDialog(null);
          }}
        />
      )}

      {pendingDialog?.kind === "unlock" && (
        <PasswordDialog
          title={`「${pendingDialog.tab.name}」のロック解除`}
          mode="unlock"
          busy={tabLockState.busy}
          errorMessage={tabLockState.lastErrorCode ? errorMessageFor(tabLockState.lastErrorCode) : null}
          submitLabel="ロック解除"
          onCancel={() => {
            setPendingDialog(null);
            tabLockState.clearLastError();
          }}
          onSubmit={async (password) => {
            tabLockState.clearLastError();
            const ok = await tabLockState.unlockTab(pendingDialog.tab.id, password);
            if (ok) setPendingDialog(null);
          }}
        />
      )}

      {pendingDialog?.kind === "permanentUnlockConfirm" && (
        <ConfirmDialog
          title="完全にロック解除"
          message={
            <>
              <p>「{pendingDialog.tab.name}」を完全にロック解除すると、暗号化前の通常のタブに戻ります。この操作は取り消せません。</p>
              <p>念のため、事前にフォルダ全体のバックアップを取っておくことをおすすめします。</p>
            </>
          }
          confirmLabel="完全にロック解除"
          onCancel={() => setPendingDialog(null)}
          onConfirm={async () => {
            const tab = pendingDialog.tab;
            if (tabLockState.activeSessionRealNames.has(tab.id)) {
              const ok = await tabLockState.permanentUnlockTab(tab.id, undefined);
              if (ok) setPendingDialog(null);
            } else {
              setPendingDialog({ kind: "permanentUnlockPassword", tab });
            }
          }}
        />
      )}

      {pendingDialog?.kind === "permanentUnlockPassword" && (
        <PasswordDialog
          title={`「${pendingDialog.tab.name}」の完全ロック解除`}
          mode="unlock"
          busy={tabLockState.busy}
          errorMessage={tabLockState.lastErrorCode ? errorMessageFor(tabLockState.lastErrorCode) : null}
          submitLabel="完全にロック解除"
          onCancel={() => {
            setPendingDialog(null);
            tabLockState.clearLastError();
          }}
          onSubmit={async (password) => {
            tabLockState.clearLastError();
            const ok = await tabLockState.permanentUnlockTab(pendingDialog.tab.id, password);
            if (ok) setPendingDialog(null);
          }}
        />
      )}

      {settingsOpen && settings && (
        <SettingsDialog settings={settings} patchSettings={patchSettings} applyFetched={applyFetched} onClose={() => setSettingsOpen(false)} />
      )}

      {crossTabSearch.isOpen && (
        <CrossTabSearchDialog
          search={crossTabSearch}
          onClose={crossTabSearch.close}
          onOpenHit={(tabId, memoName) => {
            onOpenSearchHit(tabId, memoName);
            crossTabSearch.close();
          }}
        />
      )}
    </div>
  );
}
