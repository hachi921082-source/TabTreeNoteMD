// Right-pane editor surface (`docs/plans/editor-save/PLAN.md` for the plain-text layer;
// `docs/plans/markdown-editing/PLAN.md` for the live-view decoration/toolbar layer on top of it).
// Mounts one CodeMirror 6 `EditorView` per open memo — a fresh `EditorState`/history is created on
// every memo switch (`editor-save/PLAN.md` §1: "no cross-memo undo"), while settings/read-only/
// whitespace-visibility/live-view-toggle changes reconfigure the existing view's compartments in
// place. Owns: the CodeMirror mount, the status bar, the right-click context menu
// (Convert/Check/Search-with-Google/Properties, shared component per `[spec §9.1 item 2]`), the
// in-memo search panel, the conflict-resolution dialog, the floating Markdown toolbar
// (`components/Editor/Toolbar/MarkdownToolbar.tsx`), and the live/source runtime toggle.
import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { useEditor } from "../../state/useEditor";
import { useSearch } from "../../state/useSearch";
import { memoGetProperties, memoOpenLocation } from "../../ipc/tree";
import type { AppSettings, AppSettingsPatch, EditorDefaults, ExternalLinkKind, MemoEditorSettings, MemoProperties } from "../../shared/models";
import { errorMessageFor } from "../../shared/errorMessages";
import { formatDateTime } from "../../shared/formatDateTime";
import { ContextMenu } from "../ContextMenu/ContextMenu";
import type { ContextMenuItem } from "../ContextMenu/ContextMenu";
import { Dialog } from "../dialogs/Dialog";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { SearchPanel } from "../Search/SearchPanel";
import { ConflictDialog } from "./ConflictDialog";
import { MarkdownToolbar } from "./Toolbar/MarkdownToolbar";
import type { MarkdownToolbarHandle } from "./Toolbar/MarkdownToolbar";
import type { ToolbarActionId } from "../../editor/toolbarSlots";
import { buildMarkdownExtensions, liveViewCompartment, liveViewExtension } from "../../editor/codeMirrorSetup";
import type { MarkdownDecorationConfig } from "../../editor/markdownDecorations";
import { imagePasteExtension } from "../../editor/imagePaste";
import { comboFromEvent, resolveShortcuts } from "../../shared/shortcuts";
import {
  buildPlainTextExtensions,
  buildSettingsExtension,
  computeTopVisibleLine,
  convertLowercase,
  convertSpacesToTabs,
  convertTabsToSpaces,
  convertToFullWidth,
  convertToHalfWidth,
  convertToHiragana,
  convertToKatakana,
  convertUppercase,
  copyCommand,
  cutCommand,
  cycleCheckbox,
  deleteLineCommand,
  duplicateLineCommand,
  getEditorStatus,
  moveLineDownCommand,
  moveLineUpCommand,
  pasteCommand,
  readOnlyCompartment,
  readOnlyExtension,
  redoCommand,
  resolveEditorSettings,
  scrollToTopVisibleLine,
  searchSelectionWithGoogle,
  selectAllCommand,
  settingsCompartment,
  undoCommand,
  whitespaceCompartment,
  whitespaceExtension,
} from "../../editor/textCommands";
import "./Editor.css";

/** Matches `AppSettings::default().editorDefaults` (`model/app_settings.rs`) — used until
 * `appSettings` has loaded from `settings_get`. */
const FALLBACK_EDITOR_DEFAULTS: EditorDefaults = { wrap: "window", tabWidth: 4, softTab: true, showLineNumbers: true, fontSize: 14, viewMode: "live" };

/** `docs/plans/editor-save/PLAN.md` §2 "Scroll Position Write Path": debounced so a continuous
 * scroll doesn't fire a metadata write per frame. */
const SCROLL_PERSIST_DEBOUNCE_MS = 500;

export interface EditorPaneProps {
  tabId: string | null;
  memoName: string | null;
  /** `state/useTree.ts`'s one-shot "should the editor auto-focus" signal — bumped only by an
   * explicit open (row click, `focusOnMemo`), left untouched by arrow-key nav (`[spec §2.2]`).
   * Compared against the last-consumed token inside the view-(re)creation effect below, same
   * one-shot-signal pattern as `ed.content.scrollPosition`. */
  focusEditorToken: number;
  appSettings: AppSettings | null;
  memoEditorOverride: MemoEditorSettings | undefined;
  setEditorOverride: (path: string, editor: MemoEditorSettings | null) => Promise<void>;
  /** `state/useSettings.ts`'s generic patch — used by `MarkdownToolbar` to persist
   * `AppSettings.window.toolbarPosition`/`toolbarOrder` (`docs/plans/markdown-editing/PLAN.md` §4). */
  patchSettings: (patch: AppSettingsPatch) => Promise<void>;
  /** `memo:` link click target (`docs/plans/link/PLAN.md` §5) — reuses `App.tsx`'s shared "open
   * memo" path (same one `TabBar`'s cross-tab search-result click uses). `memoName: null` for the
   * `memo:TabName/` (tab-only) resolved form. */
  onOpenMemoLink: (tabId: string, memoName: string | null) => void;
}

/** Imperative handle so `App.tsx` can force-flush the currently-open memo's pending save and
 * await its completion before an operation that must not race with it — currently only
 * `tab-lock/PLAN.md` §3 `tab_lock` step 2's "force a save-confirm (auto-save-and-close) before
 * calling this command", when the tab being locked is the active tab with an open memo. Distinct
 * from `useEditor`'s own switch-away flush (fire-and-forget, not awaitable from outside). */
export interface EditorPaneHandle {
  flushSaveNow: () => Promise<void>;
}

export const EditorPane = forwardRef<EditorPaneHandle, EditorPaneProps>(function EditorPane(
  { tabId, memoName, focusEditorToken, appSettings, memoEditorOverride, setEditorOverride, patchSettings, onOpenMemoLink },
  ref,
) {
  const ed = useEditor(tabId, memoName, appSettings);
  useImperativeHandle(ref, () => ({ flushSaveNow: ed.flushSaveNow }));
  const search = useSearch();

  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  // Bumped after view (re)creation and on every CodeMirror update (doc change or cursor/selection
  // move), so the status bar's char/line/cursor-position derived state stays live.
  const [, setRenderTick] = useState(0);

  const [showWhitespace, setShowWhitespace] = useState(false);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const [propertiesOpen, setPropertiesOpen] = useState(false);
  const [properties, setProperties] = useState<MemoProperties | null>(null);
  const toolbarRef = useRef<MarkdownToolbarHandle>(null);

  const editorDefaults = appSettings?.editorDefaults ?? FALLBACK_EDITOR_DEFAULTS;
  const resolvedSettings = useMemo(() => resolveEditorSettings(memoEditorOverride, editorDefaults), [memoEditorOverride, editorDefaults]);
  const readOnly = Boolean(ed.content?.editLocked) || ed.conflict !== null;

  // Live/source view mode (`docs/plans/markdown-editing/PLAN.md` §3; persistence added for
  // GitHub issue #31 "Persist the Markdown toolbar's source/live view toggle per memo").
  // `resolvedViewMode` is the *governed* setting a memo opens with (per-memo override, falling
  // back to the global default); `liveViewOverride` is the current session's instant-feedback
  // local state — `null` until the user toggles at least once for the currently-open memo, reset
  // to `null` on every memo switch (a fresh load always starts from the governed setting, which by
  // now reflects any previously-persisted toggle). Toggling both flips this local state (so the UI
  // updates immediately, without waiting on the async `setEditorOverride` IPC round-trip) and
  // persists the new value into the memo's per-memo override — same per-memo-only write pattern as
  // `persistEditorViewState` below (never the global `editorDefaults.viewMode`), so the choice
  // survives reopening the memo or restarting the app (`docs/detailed-spec.md` §5.10).
  const resolvedViewMode = memoEditorOverride?.viewMode ?? editorDefaults.viewMode;
  const [liveViewOverride, setLiveViewOverride] = useState<"live" | "source" | null>(null);
  const liveViewEnabled = (liveViewOverride ?? resolvedViewMode) === "live";
  const onToggleLiveView = useCallback(() => {
    setLiveViewOverride((current) => {
      const currentlyLive = current !== null ? current === "live" : resolvedViewMode === "live";
      const next: "live" | "source" = currentlyLive ? "source" : "live";
      const name = memoNameRef.current;
      if (name) {
        void setEditorOverrideRef.current(name, { ...overrideRef.current, viewMode: next });
      }
      return next;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedViewMode]);

  // External-link confirm dialog (`docs/plans/link/PLAN.md` §2, §4): gated by
  // `AppSettings.links.confirmBeforeOpen`, shown only when a URL/email/file-path link is clicked
  // while that setting is on. The click happens deep inside a CodeMirror `ViewPlugin`
  // (`markdownDecorations.ts`), not a React event handler, so `requestExternalConfirm` bridges
  // that with a Promise whose resolver is stashed until the dialog closes.
  const [pendingExternalLink, setPendingExternalLink] = useState<{ kind: ExternalLinkKind; value: string } | null>(null);
  const externalConfirmResolveRef = useRef<((confirmed: boolean) => void) | null>(null);
  const requestExternalConfirm = useCallback((kind: ExternalLinkKind, value: string): Promise<boolean> => {
    return new Promise<boolean>((resolve) => {
      externalConfirmResolveRef.current = resolve;
      setPendingExternalLink({ kind, value });
    });
  }, []);
  const settleExternalConfirm = (confirmed: boolean) => {
    externalConfirmResolveRef.current?.(confirmed);
    externalConfirmResolveRef.current = null;
    setPendingExternalLink(null);
  };

  const markdownConfig = useMemo<MarkdownDecorationConfig>(
    () => ({
      openTrigger: appSettings?.links.openTrigger ?? "singleClick",
      readOnly,
      currentTabId: tabId,
      confirmBeforeOpen: appSettings?.links.confirmBeforeOpen ?? false,
      onOpenMemo: onOpenMemoLink,
      requestExternalConfirm,
    }),
    [appSettings, readOnly, tabId, onOpenMemoLink, requestExternalConfirm],
  );

  // Refs so the debounced scroll listener / view-teardown flush always see the latest identity
  // without stale closures (same pattern as `state/useEditor.ts`).
  const memoNameRef = useRef(memoName);
  memoNameRef.current = memoName;
  const overrideRef = useRef(memoEditorOverride);
  overrideRef.current = memoEditorOverride;
  // One-shot auto-focus signal (`useTree.ts`'s `focusEditorToken`): `focusEditorTokenRef` always
  // holds the latest value; `consumedFocusTokenRef` records which value the view-(re)creation
  // effect below has already acted on, so a click-open focuses exactly once per open and an
  // arrow-nav-open (which never changes the token) never does.
  const focusEditorTokenRef = useRef(focusEditorToken);
  focusEditorTokenRef.current = focusEditorToken;
  const consumedFocusTokenRef = useRef<number | null>(null);
  const setEditorOverrideRef = useRef(setEditorOverride);
  setEditorOverrideRef.current = setEditorOverride;
  const scrollDebounceRef = useRef<number | null>(null);
  // Which memo `viewRef.current` actually belongs to (and that memo's override snapshot at the
  // time its view was created) — set ONLY inside the view-(re)creation effect below, once a view
  // has actually been (re)created for a given `ed.loadGeneration`. Unlike `memoNameRef`/
  // `overrideRef` above (which track the live `memoName`/`memoEditorOverride` props and can flip
  // to the NEXT memo before this effect's next run, since `useTree.ts`'s `setOpenMemoName` is
  // synchronous while the load itself is async), these two stay pinned to the OUTGOING memo until
  // the next effect run overwrites them for the memo it just loaded. This is what lets the
  // outgoing-flush at the top of that next run target the correct (previous) memo instead of
  // whatever the props have already drifted to (GitHub issue #28 follow-up review finding).
  const loadedMemoNameRef = useRef<string | null>(null);
  const loadedOverrideRef = useRef<MemoEditorSettings | undefined>(undefined);

  // Persists both the scroll position and the cursor/selection for `name` — grabbed together from
  // `view` since both are read at the same trigger points (switching memos, debounced scroll,
  // unmount), so one combined write avoids a second `memo_set_editor_override` round-trip per
  // trigger. `name`/`override` are passed explicitly (rather than read from the live
  // `memoNameRef`/`overrideRef` internally) because the outgoing-view-flush call site needs the
  // PREVIOUS memo's identity, which the live refs no longer hold by the time this fires.
  const persistEditorViewState = (view: EditorView, name: string | null, override: MemoEditorSettings | undefined) => {
    if (!name) return;
    const { anchor, head } = view.state.selection.main;
    void setEditorOverrideRef.current(name, {
      ...override,
      scrollPosition: computeTopVisibleLine(view),
      cursorAnchor: anchor,
      cursorHead: head,
    });
  };

  // Fresh EditorState/history per memo (`editor-save/PLAN.md` §1: "no cross-memo undo") —
  // recreated whenever `loadGeneration` bumps (initial load, memo switch, or a conflict-
  // resolution "reload from disk"), not merely re-synced via a doc-replace on a persistent view.
  useEffect(() => {
    if (!containerRef.current) return;

    // Flush this memo's scroll position immediately when switching away, same trigger point as
    // the autosave flush (`editor-save/PLAN.md` §2 "Scroll Position Write Path"). Targets
    // `loadedMemoNameRef`/`loadedOverrideRef` (the memo `outgoingView` actually belongs to), NOT
    // the live `memoName`/`memoEditorOverride` props — those already point at the memo being
    // switched TO by the time this effect runs (see the refs' declaration comment above).
    const outgoingView = viewRef.current;
    if (outgoingView) {
      if (scrollDebounceRef.current !== null) window.clearTimeout(scrollDebounceRef.current);
      persistEditorViewState(outgoingView, loadedMemoNameRef.current, loadedOverrideRef.current);
      outgoingView.destroy();
      viewRef.current = null;
    }

    if (!ed.content) {
      loadedMemoNameRef.current = null;
      loadedOverrideRef.current = undefined;
      setRenderTick((n) => n + 1);
      return;
    }
    // From here on `memoName`/`memoEditorOverride` (the props captured by this effect's closure,
    // from the same render that bumped `ed.loadGeneration`) correctly identify the memo this view
    // is being built for — snapshot them now so the NEXT effect run's outgoing-flush (above) reads
    // the right memo once the props have moved on again.
    loadedMemoNameRef.current = memoName;
    loadedOverrideRef.current = memoEditorOverride;

    // A fresh load always starts at the *governed* view mode (per-memo override falling back to
    // the global default) — any prior session's runtime toggle never carries over to a newly
    // opened memo (`docs/plans/markdown-editing/PLAN.md` §3). Computed directly from
    // `resolvedViewMode` here (not `liveViewEnabled`) so this initial build can't see a stale
    // `liveViewOverride` left over from the memo just switched away from.
    const initialLiveView = resolvedViewMode === "live";

    // Restored cursor/selection (GitHub issue #28) — set at state-creation time (not a later
    // dispatch, unlike scroll position below which needs the view's first layout/measure pass)
    // since an initial `EditorState.selection` avoids an extra transaction/history entry.
    // Clamped to the just-loaded document's length in case the file was edited externally between
    // saves and the persisted offsets are now stale/out of range.
    const docLength = ed.content.text.length;
    const clamp = (value: number) => Math.min(Math.max(value, 0), docLength);
    const savedAnchor = ed.content.cursorAnchor;
    const savedHead = ed.content.cursorHead;
    const initialSelection =
      savedAnchor != null && savedHead != null ? EditorSelection.single(clamp(savedAnchor), clamp(savedHead)) : undefined;

    const state = EditorState.create({
      doc: ed.content.text,
      selection: initialSelection,
      extensions: [
        buildPlainTextExtensions({
          settings: resolvedSettings,
          readOnly,
          showWhitespace,
          onDocChanged: (text) => ed.notifyTextChanged(text),
        }),
        ...buildMarkdownExtensions(initialLiveView, markdownConfig),
        imagePasteExtension({ tabId, onError: ed.reportError }),
        // Keeps the status bar's char/line/cursor-position derived state live on every doc
        // change or cursor/selection move, not just after (re)creating the view.
        EditorView.updateListener.of((update) => {
          if (update.docChanged || update.selectionSet) setRenderTick((n) => n + 1);
        }),
      ],
    });
    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;
    setRenderTick((n) => n + 1);
    // Click-open (and equivalent explicit-navigation opens like `memo:` link clicks) auto-focuses
    // the editor; arrow-key tree nav does not (`[spec §2.2]`, `[spec §9.1 item 3]`). The token only
    // changes for the former, so this fires at most once per distinct open.
    if (focusEditorTokenRef.current !== consumedFocusTokenRef.current) {
      consumedFocusTokenRef.current = focusEditorTokenRef.current;
      view.focus();
    }
    // Reset the session-only toggle for the newly-opened memo (harmless if `liveViewEnabled`
    // already equalled `initialLiveView` — the reconfigure effect below just re-dispatches the
    // same extension content in that case).
    setLiveViewOverride(null);

    const savedScrollLine = ed.content.scrollPosition;
    let raf = 0;
    if (savedScrollLine) {
      raf = requestAnimationFrame(() => scrollToTopVisibleLine(view, savedScrollLine));
    }
    const scrollHandler = () => {
      if (scrollDebounceRef.current !== null) window.clearTimeout(scrollDebounceRef.current);
      // Safe to read the live `memoNameRef`/`overrideRef` here (not the `loaded*` snapshots):
      // if the user switches memos before this fires, the next effect run's outgoing-flush
      // clears this very timeout (the `window.clearTimeout` at the top of the effect, before
      // `outgoingView.destroy()`) before it ever gets a chance to run, so by construction this
      // callback only ever fires while `view` is still the live, currently-open view.
      scrollDebounceRef.current = window.setTimeout(() => {
        persistEditorViewState(view, memoNameRef.current, overrideRef.current);
      }, SCROLL_PERSIST_DEBOUNCE_MS);
    };
    view.scrollDOM.addEventListener("scroll", scrollHandler);

    return () => {
      if (raf) cancelAnimationFrame(raf);
      view.scrollDOM.removeEventListener("scroll", scrollHandler);
    };
    // Intentionally keyed only on `loadGeneration`: settings/readOnly/whitespace changes on an
    // already-mounted view are handled by the separate compartment-reconfigure effects below,
    // not by recreating the whole view.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ed.loadGeneration]);

  // Reconfigure (not recreate) on settings / read-only / whitespace changes.
  useEffect(() => {
    viewRef.current?.dispatch({ effects: settingsCompartment.reconfigure(buildSettingsExtension(resolvedSettings)) });
  }, [resolvedSettings]);

  useEffect(() => {
    viewRef.current?.dispatch({ effects: readOnlyCompartment.reconfigure(readOnlyExtension(readOnly)) });
  }, [readOnly]);

  useEffect(() => {
    viewRef.current?.dispatch({ effects: whitespaceCompartment.reconfigure(whitespaceExtension(showWhitespace)) });
  }, [showWhitespace]);

  // Live/source toggle (`docs/plans/markdown-editing/PLAN.md` §3) — reconfigures only the
  // decoration compartment, never the document, so cursor/scroll survive the toggle. Also
  // reconfigured here whenever `markdownConfig` changes (e.g. read-only flips while a memo stays
  // open) without needing a separate effect.
  useEffect(() => {
    viewRef.current?.dispatch({ effects: liveViewCompartment.reconfigure(liveViewExtension(liveViewEnabled, markdownConfig)) });
  }, [liveViewEnabled, markdownConfig]);

  // Destroy the view (and flush its scroll position) on unmount.
  useEffect(() => {
    return () => {
      const view = viewRef.current;
      if (view) {
        if (scrollDebounceRef.current !== null) window.clearTimeout(scrollDebounceRef.current);
        // Whole-component unmount: only one memo is ever open at this point, so the live
        // `memoNameRef`/`overrideRef` necessarily still refer to it.
        persistEditorViewState(view, memoNameRef.current, overrideRef.current);
        view.destroy();
        viewRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Default Ctrl+F opens the in-memo search panel (`editor-save/PLAN.md` §5;
  // `docs/plans/window-operation/PLAN.md` §3's rebindable-shortcut system, action id
  // "editor.findReplace").
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const shortcuts = resolveShortcuts(appSettings?.shortcuts);
      if (comboFromEvent(e) === shortcuts["editor.findReplace"]) {
        e.preventDefault();
        search.open();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [search, appSettings?.shortcuts]);

  // Default keyboard shortcuts for the 16 Markdown toolbar actions plus the live/source toggle
  // (`docs/detailed-spec.md` §5.10: "every one of the 16 toolbar actions gets a default keyboard
  // shortcut, registered as an entry in the existing generic shortcut system"). Dispatches through
  // `MarkdownToolbar`'s imperative handle rather than duplicating its action switch statement here.
  // Scoped to when focus is within this editor's own CodeMirror content, so these common
  // combinations (Ctrl+B, Ctrl+I, etc.) don't fire while typing elsewhere in the app (e.g. a
  // rename input or a dialog).
  useEffect(() => {
    const shortcuts = resolveShortcuts(appSettings?.shortcuts);
    const reverse = new Map<string, string>();
    for (const [actionId, combo] of Object.entries(shortcuts)) {
      if (actionId.startsWith("markdown.")) reverse.set(combo, actionId);
    }
    const handler = (e: KeyboardEvent) => {
      if (e.defaultPrevented || !viewRef.current) return;
      if (!containerRef.current?.contains(document.activeElement)) return;
      const actionId = reverse.get(comboFromEvent(e));
      if (!actionId) return;
      if (actionId === "markdown.toggleLiveView") {
        e.preventDefault();
        onToggleLiveView();
        return;
      }
      e.preventDefault();
      toolbarRef.current?.invokeAction(actionId.slice("markdown.".length) as ToolbarActionId);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [appSettings?.shortcuts, onToggleLiveView]);

  const runOnView = (fn: (view: EditorView) => void) => {
    const view = viewRef.current;
    if (view) fn(view);
  };

  const buildMenuItems = (): ContextMenuItem[] => [
    { label: "元に戻す", onSelect: () => runOnView(undoCommand), disabled: readOnly },
    { label: "やり直す", onSelect: () => runOnView(redoCommand), disabled: readOnly },
    { label: "切り取り", onSelect: () => runOnView(cutCommand), disabled: readOnly },
    { label: "コピー", onSelect: () => runOnView(copyCommand) },
    { label: "貼り付け", onSelect: () => runOnView(pasteCommand), disabled: readOnly },
    { label: "すべて選択", onSelect: () => runOnView(selectAllCommand) },
    {
      label: "行操作",
      submenu: [
        { label: "行を上に移動", onSelect: () => runOnView(moveLineUpCommand), disabled: readOnly },
        { label: "行を下に移動", onSelect: () => runOnView(moveLineDownCommand), disabled: readOnly },
        { label: "行を複製", onSelect: () => runOnView(duplicateLineCommand), disabled: readOnly },
        { label: "行を削除", onSelect: () => runOnView(deleteLineCommand), disabled: readOnly },
      ],
    },
    {
      label: "変換",
      submenu: [
        { label: "大文字に変換", onSelect: () => runOnView(convertUppercase), disabled: readOnly },
        { label: "小文字に変換", onSelect: () => runOnView(convertLowercase), disabled: readOnly },
        { label: "ひらがなに変換", onSelect: () => runOnView(convertToHiragana), disabled: readOnly },
        { label: "カタカナに変換", onSelect: () => runOnView(convertToKatakana), disabled: readOnly },
        { label: "半角に変換", onSelect: () => runOnView(convertToHalfWidth), disabled: readOnly },
        { label: "全角に変換", onSelect: () => runOnView(convertToFullWidth), disabled: readOnly },
        { label: "タブをスペースに", onSelect: () => runOnView(convertTabsToSpaces(resolvedSettings.tabWidth)), disabled: readOnly },
        { label: "スペースをタブに", onSelect: () => runOnView(convertSpacesToTabs(resolvedSettings.tabWidth)), disabled: readOnly },
      ],
    },
    { label: "チェック（☐/☑）", onSelect: () => runOnView(cycleCheckbox), disabled: readOnly },
    { label: "Googleで検索", onSelect: () => runOnView(searchSelectionWithGoogle) },
    { label: "検索と置換", onSelect: () => search.open() },
    { label: showWhitespace ? "空白/改行の表示を消す" : "空白/改行を表示", onSelect: () => setShowWhitespace((v) => !v) },
    {
      label: "プロパティ",
      onSelect: () => {
        if (!tabId || !memoName) return;
        setPropertiesOpen(true);
        void memoGetProperties(tabId, memoName).then((result) => {
          if (result.ok) setProperties(result.value);
        });
      },
    },
  ];

  if (!memoName) {
    return <div className="ttn-editor-pane ttn-editor-empty">メモを選択してください</div>;
  }

  const status = viewRef.current ? getEditorStatus(viewRef.current.state) : null;

  return (
    <div
      className="ttn-editor-pane"
      onContextMenu={(e) => {
        e.preventDefault();
        setContextMenu({ x: e.clientX, y: e.clientY });
      }}
    >
      {ed.loading && <div className="ttn-editor-loading">読み込み中...</div>}

      <div className="ttn-editor-cm-container" ref={containerRef} />

      <MarkdownToolbar
        ref={toolbarRef}
        getView={() => viewRef.current}
        appSettings={appSettings}
        patchSettings={patchSettings}
        liveViewEnabled={liveViewEnabled}
        onToggleLiveView={onToggleLiveView}
        readOnly={readOnly}
        tabId={tabId}
        onImageError={ed.reportError}
      />

      {search.isOpen && <SearchPanel search={search} getView={() => viewRef.current} />}

      {/* Right-click suppressed here (`[spec §2.3]`, existing behavior to preserve) — the status
          bar area does not open the shared context menu. */}
      <div className="ttn-editor-status-bar" onContextMenu={(e) => e.stopPropagation()}>
        <span className="ttn-editor-status-name">{memoName}</span>
        {ed.content && (
          <>
            <span>{ed.content.encoding === "utf-8" ? "UTF-8" : "Shift_JIS"}</span>
            <span>{ed.content.eol.toUpperCase()}</span>
          </>
        )}
        {status && (
          <>
            <span>{status.charCount.toLocaleString()} 文字</span>
            <span>{status.lineCount} 行</span>
            <span>
              {status.line} 行, {status.column} 列
            </span>
          </>
        )}
        {readOnly && <span className="ttn-editor-status-locked">🔒 編集ロック中</span>}
      </div>

      {ed.lastErrorCode && (
        <div className="ttn-editor-error" role="alert">
          <span>{errorMessageFor(ed.lastErrorCode)}</span>
          <button type="button" onClick={ed.clearLastError}>
            ×
          </button>
        </div>
      )}

      {contextMenu && <ContextMenu x={contextMenu.x} y={contextMenu.y} items={buildMenuItems()} onClose={() => setContextMenu(null)} />}

      {ed.conflict && (
        <ConflictDialog
          kind={ed.conflict}
          onKeepMine={() => void ed.resolveConflictKeepMine()}
          onReloadFromDisk={() => void ed.resolveConflictReloadFromDisk()}
        />
      )}

      {pendingExternalLink && (
        <ConfirmDialog
          title="外部リンクを開く"
          message={`次のリンクを開きます。よろしいですか？ — ${pendingExternalLink.value}`}
          confirmLabel="開く"
          onConfirm={() => settleExternalConfirm(true)}
          onCancel={() => settleExternalConfirm(false)}
        />
      )}

      {propertiesOpen && (
        <Dialog
          title="プロパティ"
          onClose={() => {
            setPropertiesOpen(false);
            setProperties(null);
          }}
          footer={
            <>
              <button
                type="button"
                onClick={() => {
                  if (!tabId || !memoName) return;
                  void memoOpenLocation(tabId, memoName).then((result) => {
                    if (!result.ok) ed.reportError(result.error.code);
                  });
                }}
              >
                ファイルの場所を開く(O)
              </button>
              <button
                type="button"
                onClick={() => {
                  setPropertiesOpen(false);
                  setProperties(null);
                }}
              >
                閉じる
              </button>
            </>
          }
        >
          {properties ? (
            <>
              <section>
                <h3 className="ttn-properties-section-title">ファイル情報</h3>
                <dl className="ttn-properties-list">
                  <dt>ファイル名</dt>
                  <dd>{properties.fileName}</dd>
                  <dt>ファイルパス</dt>
                  <dd className="ttn-properties-file-path">{properties.filePath}</dd>
                  <dt>作成日時</dt>
                  <dd>{formatDateTime(properties.createdMs)}</dd>
                  <dt>更新日時</dt>
                  <dd>{formatDateTime(properties.modifiedMs)}</dd>
                  <dt>ファイルサイズ</dt>
                  <dd>{properties.fileSizeBytes.toLocaleString()} バイト</dd>
                </dl>
              </section>
              <section className="ttn-properties-section">
                <h3 className="ttn-properties-section-title">文字数カウント</h3>
                <dl className="ttn-properties-list">
                  <dt>総字数</dt>
                  <dd>{properties.charCount.toLocaleString()}</dd>
                  <dt>字数</dt>
                  <dd>{properties.charCountExcludingWhitespace.toLocaleString()}（改行・空白を除く）</dd>
                  <dt>行数</dt>
                  <dd>{properties.lineCount.toLocaleString()}</dd>
                </dl>
              </section>
            </>
          ) : (
            <p>読み込み中...</p>
          )}
        </Dialog>
      )}
    </div>
  );
});
