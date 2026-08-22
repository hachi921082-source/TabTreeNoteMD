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
import { memoGetProperties, memoOpenLocation, memoSetEditorOverride } from "../../ipc/tree";
import type { AppSettings, AppSettingsPatch, EditorDefaults, ExternalLinkKind, MemoEditorSettings, MemoProperties, PaneMode } from "../../shared/models";
import { errorMessageFor } from "../../shared/errorMessages";
import { formatDateTime } from "../../shared/formatDateTime";
import { ContextMenu } from "../ContextMenu/ContextMenu";
import type { ContextMenuEntry } from "../ContextMenu/ContextMenu";
import { Dialog } from "../dialogs/Dialog";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { SearchPanel } from "../Search/SearchPanel";
import { ConflictDialog } from "./ConflictDialog";
import { MarkdownToolbar, PREVIEW_ZOOM_MAX, PREVIEW_ZOOM_MIN, PREVIEW_ZOOM_STEP } from "./Toolbar/MarkdownToolbar";
import type { MarkdownToolbarHandle } from "./Toolbar/MarkdownToolbar";
import { PreviewPane } from "./PreviewPane";
import type { PreviewPaneHandle } from "./PreviewPane";
import type { ToolbarActionId } from "../../editor/toolbarSlots";
import {
  buildMarkdownExtensions,
  liveViewCompartment,
  liveViewExtension,
  sourceImageMarkerCompartment,
  sourceImageMarkerViewExtension,
  sourceMathMarkerCompartment,
  sourceMathMarkerViewExtension,
} from "../../editor/codeMirrorSetup";
import type { MarkdownDecorationConfig } from "../../editor/markdownDecorations";
import { resolveAndScrollToHeadingAnchor } from "../../editor/links";
import { computeTopVisiblePreviewLine, scrollPreviewToLine } from "../../editor/preview/scrollSync";
import { imagePasteExtension } from "../../editor/imagePaste";
import { releaseMemoImages } from "../../editor/imageLoader";
import { exportMemoToPdf } from "../../editor/pdfExport";
import { comboFromEvent, resolveShortcuts } from "../../shared/shortcuts";
import { useT } from "../../i18n";
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
const FALLBACK_EDITOR_DEFAULTS: EditorDefaults = { wrap: "window", tabWidth: 4, softTab: true, showLineNumbers: true, fontSize: 14, paneMode: "edit" };

/** `docs/plans/markdown/markdown-preview/PLAN.md` §4.1 — the 3-state pane-mode cycling toggle's
 * fixed order, `edit -> split -> preview -> edit -> ...`. */
const PANE_MODE_CYCLE: Record<PaneMode, PaneMode> = { edit: "split", split: "preview", preview: "edit" };

/** Matches `AppSettings::default().preview.debounceMs` (`model/app_settings.rs`) — used until
 * `appSettings` has loaded from `settings_get`, same treatment as `FALLBACK_EDITOR_DEFAULTS`. */
const FALLBACK_PREVIEW_DEBOUNCE_MS = 150;

/** Fixed global default Markdown view mode a memo opens in — hardcoded, not user-configurable
 * (GitHub issue #61 removed the Settings → Editor "display mode" selector). The per-memo
 * `MemoEditorSettings.viewMode` override below still exists and falls back to this constant. */
const DEFAULT_VIEW_MODE: "live" | "source" = "live";

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
  /** Live Edit Lock state for the open memo, from the current tree state (GitHub issue #12) —
   * distinct from `ed.content.editLocked`, which is only a snapshot taken at `editor_load` time. */
  isMemoEditLocked: boolean;
  setEditorOverride: (path: string, editor: MemoEditorSettings | null) => Promise<void>;
  /** `state/useSettings.ts`'s generic patch — used by `MarkdownToolbar` to persist
   * `AppSettings.window.toolbarPosition`/`toolbarOrder` (`docs/plans/markdown-editing/PLAN.md` §4). */
  patchSettings: (patch: AppSettingsPatch) => Promise<void>;
  /** `memo:` link click target (`docs/plans/link/PLAN.md` §5) — reuses `App.tsx`'s shared "open
   * memo" path (same one `TabBar`'s cross-tab search-result click uses). `memoName: null` for the
   * `memo:TabName/` (tab-only) resolved form. `anchorSlug` is the parsed `#slug` half of a
   * `memo:...#slug` link (`docs/plans/v1.2.0/cross-tab-anchor-links/PLAN.md` §2). */
  onOpenMemoLink: (tabId: string, memoName: string | null, anchorSlug: string | null) => void;
  /** `App.tsx`'s deferred anchor-jump signal for a link-navigation-triggered `focusOnMemo`
   * (`docs/plans/v1.2.0/cross-tab-anchor-links/PLAN.md` §3) — `pendingAnchorSlug` is the `#slug` to
   * jump to (`null` if this open's link carried no anchor, or this open wasn't link-triggered at
   * all). `pendingAnchorToken` is a DEDICATED monotonic counter (deliberately NOT the shared
   * `focusEditorToken` above) bumped only by `App.tsx`'s link-focus effect, so the view-(re)creation
   * effect below can tell "this load is the one a `#slug` link asked for" apart from any later,
   * unrelated memo load (ordinary tree-row click/arrow-nav) that would otherwise incorrectly replay
   * a stale slug if it merely reused `focusEditorToken`. */
  pendingAnchorSlug: string | null;
  pendingAnchorToken: number;
}

/** Imperative handle so `App.tsx` can force-flush the currently-open memo's pending save and
 * await its completion before an operation that must not race with it — currently only
 * `tab-lock/PLAN.md` §3 `tab_lock` step 2's "force a save-confirm (auto-save-and-close) before
 * calling this command", when the tab being locked is the active tab with an open memo. Distinct
 * from `useEditor`'s own switch-away flush (fire-and-forget, not awaitable from outside). */
export interface EditorPaneHandle {
  flushSaveNow: () => Promise<void>;
  /** Double-click-active-tab jump-to-start toggle (`docs/plans/v1.2.0/tab-double-click-jump/
   * PLAN.md` §2) — invoked from `App.tsx` when `TabBar.tsx` reports a double-click on the
   * already-active tab. No-op if no memo is currently open. */
  toggleJumpToStart: () => void;
  /** #191: jump to a same-tab/same-memo anchor immediately (bypasses the deferred load pipeline, which never re-fires here). No-op if no memo open. */
  jumpToAnchor: (slug: string) => void;
}

export const EditorPane = forwardRef<EditorPaneHandle, EditorPaneProps>(function EditorPane(
  {
    tabId,
    memoName,
    focusEditorToken,
    appSettings,
    memoEditorOverride,
    isMemoEditLocked,
    setEditorOverride,
    patchSettings,
    onOpenMemoLink,
    pendingAnchorSlug,
    pendingAnchorToken,
  },
  ref,
) {
  const t = useT();
  const ed = useEditor(tabId, memoName, appSettings, isMemoEditLocked);
  useImperativeHandle(ref, () => ({ flushSaveNow: ed.flushSaveNow, toggleJumpToStart, jumpToAnchor }));
  const search = useSearch();

  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  // `docs/plans/v1.2.0/cross-tab-anchor-links/PLAN.md` §4.2/§4.3 — lets this component call
  // `PreviewPane`'s readiness gate/container accessor for a `preview`-pane-mode `#slug` anchor jump.
  const previewRef = useRef<PreviewPaneHandle | null>(null);
  // `<PreviewPane ref={...}>`が実際に(再)マウントされるたびに、下の`setPreviewRef`がこれをbumpする
  // — 下の「プレビュー側アンカー/スクロール消費用の遅延effect」の依存配列に追加してあり、
  // `previewRef.current`が実際に入った時点でそのeffectがもう一度走る機会を得られるようにしている。
  // これがないと、そのセッションで一度も`PreviewPane`を描画していないタブへのクロスタブジャンプ
  // (遷移先タブの`PreviewPane`が初めてマウントされるケース)では、そのeffectの唯一の実行タイミングで
  // `previewRef.current`がまだ`null`のままになり(`[paneMode, memoKey]`はrefのマウント完了だけでは
  // 変化しない)、`pendingPreviewAnchorRef`/`pendingPreviewScrollRef`が永遠にキューされたままになって
  // しまう — 実機クリックで再現した「そのセッションで遷移先タブを既に開いていれば動くが、未開封だと
  // ジャンプしない」という不具合そのものだった。
  const [previewMountTick, setPreviewMountTick] = useState(0);
  const setPreviewRef = useCallback((handle: PreviewPaneHandle | null) => {
    previewRef.current = handle;
    if (handle) setPreviewMountTick((n) => n + 1);
  }, []);
  // Bumped after view (re)creation and on every CodeMirror update (doc change or cursor/selection
  // move), so the status bar's char/line/cursor-position derived state stays live. Also reused,
  // unmodified, as `PreviewPane.tsx`'s own re-render trigger (`docs/plans/markdown/markdown-
  // preview/PLAN.md` §2 — "reused, not duplicated") rather than adding a second doc-change
  // listener there.
  const [renderTick, setRenderTick] = useState(0);

  const [showWhitespace, setShowWhitespace] = useState(false);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const [propertiesOpen, setPropertiesOpen] = useState(false);
  const [properties, setProperties] = useState<MemoProperties | null>(null);
  const toolbarRef = useRef<MarkdownToolbarHandle>(null);

  const editorDefaults = appSettings?.editorDefaults ?? FALLBACK_EDITOR_DEFAULTS;
  const resolvedSettings = useMemo(() => resolveEditorSettings(memoEditorOverride, editorDefaults), [memoEditorOverride, editorDefaults]);
  // `isMemoEditLocked` is the live, always-current lock state (issue #12 — `App.tsx` derives it from
  // `treeState.nodes`, refreshed on every `setEditLock` toggle). `ed.content?.editLocked` used to be
  // OR'd in too, but it's only a one-time snapshot from `editor_load` time, never refreshed after —
  // unlocking an already-open memo flipped the live term to `false` while the stale term stayed
  // `true`, keeping the editor read-only until a reload (memo switch) happened to refresh it
  // (GitHub issue #109; locking was unaffected since stale-`false` OR live-`true` still resolved
  // correctly). `isMemoEditLocked` alone is authoritative, so the stale term is dropped entirely.
  const readOnly = isMemoEditLocked || ed.conflict !== null;

  // Live/source view mode (`docs/plans/markdown-editing/PLAN.md` §3; persistence added for
  // GitHub issue #31 "Persist the Markdown toolbar's source/live view toggle per memo").
  // `resolvedViewMode` is the *governed* setting a memo opens with (per-memo override, falling
  // back to the global default); `liveViewOverride` is the current session's instant-feedback
  // local state — `null` until the user toggles at least once for the currently-open memo, reset
  // to `null` on every memo switch (a fresh load always starts from the governed setting, which by
  // now reflects any previously-persisted toggle). Toggling both flips this local state (so the UI
  // updates immediately, without waiting on the async `setEditorOverride` IPC round-trip) and
  // persists the new value into the memo's per-memo override — same per-memo-only write pattern as
  // `persistEditorViewState` below (never the fixed `DEFAULT_VIEW_MODE` constant, which has no
  // settings-backed store to write to), so the choice survives reopening the memo or restarting
  // the app (`docs/detailed-spec.md` §5.10).
  const resolvedViewMode = memoEditorOverride?.viewMode ?? DEFAULT_VIEW_MODE;
  const [liveViewOverride, setLiveViewOverride] = useState<"live" | "source" | null>(null);
  const liveViewEnabled = (liveViewOverride ?? resolvedViewMode) === "live";
  const onToggleLiveView = useCallback(() => {
    setLiveViewOverride((current) => {
      const currentlyLive = current !== null ? current === "live" : resolvedViewMode === "live";
      const next: "live" | "source" = currentlyLive ? "source" : "live";
      const name = memoNameRef.current;
      if (name) {
        const updated = { ...overrideRef.current, viewMode: next };
        void setEditorOverrideRef.current(name, updated);
        // Keep loadedOverrideRef in sync with this mid-session toggle so a later memo-switch's
        // outgoing flush (persistEditorViewState, which reads loadedOverrideRef -- not the live
        // overrideRef -- specifically so it targets the OUTGOING memo, see that ref's declaration
        // comment) doesn't clobber viewMode back to its pre-toggle value (GitHub issue #66).
        // Guarded on the name still matching the loaded memo so this can never write into the
        // wrong memo's slot during the brief async-load window after a switch (same class of race
        // issue #28's loadedMemoNameRef/loadedOverrideRef split was introduced to prevent).
        if (loadedMemoNameRef.current === name) {
          loadedOverrideRef.current = updated;
        }
      }
      return next;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedViewMode]);

  // Pane-mode 3-state cycling toggle (`docs/plans/markdown/markdown-preview/PLAN.md` §4.1) —
  // structurally mirrors `liveViewOverride`/`onToggleLiveView` above (same per-memo-override write
  // path, same reset-to-null-on-memo-switch treatment below), but cycles through 3 states
  // (`PANE_MODE_CYCLE`) instead of flipping a boolean. Fully independent of `liveViewEnabled` — no
  // shared state, no merged enum — except for `revealSplitIfPreviewOnly`'s one specific auto-reveal
  // interaction with the live/source toggle (wired at this component's `<MarkdownToolbar>` call
  // site below, not inside `onToggleLiveView` itself).
  const resolvedPaneMode = memoEditorOverride?.paneMode ?? editorDefaults.paneMode;
  const [paneModeOverride, setPaneModeOverride] = useState<PaneMode | null>(null);

  // Preview-lock (`GitHub issue #149`): pins the memo at effectively `"preview"` regardless of the
  // underlying `paneModeOverride`/`resolvedPaneMode` — overriding every auto-reveal call site and
  // the manual cycle toggle below too — same per-memo-override write path and
  // reset-to-null-on-memo-switch treatment as `paneModeOverride`/`liveViewOverride` above, but a
  // plain boolean rather than a 3-state cycle.
  const resolvedPreviewLocked = memoEditorOverride?.previewLocked ?? false;
  const [previewLockOverride, setPreviewLockOverride] = useState<boolean | null>(null);
  const previewLocked = previewLockOverride ?? resolvedPreviewLocked;
  // `previewLocked` wins over whatever `paneModeOverride`/`resolvedPaneMode` happen to already be
  // — e.g. issue #165's edit-lock coupling engages the lock without changing the memo's own
  // paneMode field at all, so without this the toolbar would show a *disabled* "編集のみ"/"分割表示"
  // button instead of visibly landing on preview.
  const paneMode = previewLocked ? "preview" : (paneModeOverride ?? resolvedPaneMode);
  const onTogglePreviewLock = useCallback(() => {
    setPreviewLockOverride((current) => {
      const currentLocked = current !== null ? current : resolvedPreviewLocked;
      const next = !currentLocked;
      const name = memoNameRef.current;
      if (name) {
        const updated = { ...overrideRef.current, previewLocked: next };
        void setEditorOverrideRef.current(name, updated);
        if (loadedMemoNameRef.current === name) loadedOverrideRef.current = updated;
      }
      return next;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedPreviewLocked]);
  // GitHub issue #165: `memo_set_edit_lock` now also drives the persisted `previewLocked` value
  // server-side. If the user had already manually toggled the preview-lock button for the
  // currently-open memo (leaving `previewLockOverride` non-null), that stale local override would
  // otherwise mask the server-driven change until the next memo switch — re-sync to the persisted
  // value whenever the live edit-lock state changes for whichever memo is open right now.
  useEffect(() => {
    setPreviewLockOverride(null);
  }, [isMemoEditLocked]);

  // Preview-pane zoom (`docs/plans/v1.2.0/preview-zoom/PLAN.md` §2) — session-only, resets on tab
  // switch, memo switch (the third `setPreviewZoomPercent(100)` reset, alongside
  // `liveViewOverride`/`paneModeOverride`/`previewLockOverride`/`jumpToggleRef`, in the
  // `loadGeneration`-gated view-(re)creation effect below), AND on leaving `paneMode === "preview"`
  // specifically — a case NOT covered by that same memo-switch reset point, since `paneMode` can
  // change without a memo switch at all (via the pane-mode-cycle button/shortcut), so it needs this
  // own separate effect.
  const [previewZoomPercent, setPreviewZoomPercent] = useState(100);
  useEffect(() => {
    if (paneMode !== "preview") setPreviewZoomPercent(100);
  }, [paneMode]);
  const onZoomIn = useCallback(() => {
    setPreviewZoomPercent((current) => Math.min(PREVIEW_ZOOM_MAX, current + PREVIEW_ZOOM_STEP));
  }, []);
  const onZoomOut = useCallback(() => {
    setPreviewZoomPercent((current) => Math.max(PREVIEW_ZOOM_MIN, current - PREVIEW_ZOOM_STEP));
  }, []);
  const onZoomReset = useCallback(() => {
    setPreviewZoomPercent(100);
  }, []);

  const onCyclePaneMode = useCallback(() => {
    // While locked, the pane-mode-cycle toggle itself is a no-op — must be explicitly unlocked
    // first (`onTogglePreviewLock` above), not bypassable through this button/shortcut.
    if (previewLocked) return;
    setPaneModeOverride((current) => {
      const currentMode = current !== null ? current : resolvedPaneMode;
      const next = PANE_MODE_CYCLE[currentMode];
      const name = memoNameRef.current;
      if (name) {
        const updated = { ...overrideRef.current, paneMode: next };
        void setEditorOverrideRef.current(name, updated);
        if (loadedMemoNameRef.current === name) loadedOverrideRef.current = updated;
      }
      return next;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedPaneMode, previewLocked]);
  /** Reveal helper (§4.4's auto-reveal principle): forces pane-mode to `split` if (and only if) it
   * is currently `preview`, otherwise leaves it untouched (e.g. `edit` is never disturbed). Called
   * by every action that expects to focus/scroll/mutate the CodeMirror view while pane-mode is
   * `preview` — currently just the live/source toggle's own click handler (wired below); the other
   * auto-reveal call sites listed in the plan (Ctrl+F, tree-row click, cross-tab search jump,
   * `memo:` link navigation, toolbar formatting buttons) already only ever act on the CodeMirror
   * view, which stays fully mounted (just `display:none`) even while pane-mode is `preview`, per
   * §4.2 — no separate reveal call was needed at those other sites for this initial cut.
   * GitHub issue #149: a complete no-op while `previewLocked` — the whole point of the lock is that
   * these auto-reveal triggers stop forcing `split` open at all. */
  const revealSplitIfPreviewOnly = useCallback(() => {
    if (previewLocked) return;
    setPaneModeOverride((current) => {
      const currentMode = current !== null ? current : resolvedPaneMode;
      if (currentMode !== "preview") return current;
      // Session-local only: makes the just-focused editor pane visible right now, but must NOT
      // persist through `setEditorOverrideRef`/`loadedOverrideRef` — this is a display safety net,
      // not an intentional user choice. Persisting a pane-mode change is exclusive to the explicit
      // toolbar toggle (`onCyclePaneMode` above). See issue #79.
      return "split";
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resolvedPaneMode, previewLocked]);
  const handleToggleLiveView = useCallback(() => {
    revealSplitIfPreviewOnly();
    onToggleLiveView();
  }, [revealSplitIfPreviewOnly, onToggleLiveView]);

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

  // Identifies the currently-open memo for image-cache scoping (`docs/plans/markdown/image-
  // handling/PLAN.md` §5.4) — the same derived value passed into `markdownConfig` below and (once
  // implemented) the preview pane / PDF export.
  const memoKey = tabId && memoName ? `${tabId}:${memoName}` : null;

  const markdownConfig = useMemo<MarkdownDecorationConfig>(
    () => ({
      openTrigger: appSettings?.links.openTrigger ?? "singleClick",
      readOnly,
      currentTabId: tabId,
      confirmBeforeOpen: appSettings?.links.confirmBeforeOpen ?? false,
      onOpenMemo: onOpenMemoLink,
      requestExternalConfirm,
      memoKey,
      // Already-translated checkbox-widget tooltips (`checkboxClick.ts`'s `CheckboxWidget`) — `t`
      // in the dep array below means a language change alone re-memoizes this config, which the
      // existing `liveViewCompartment.reconfigure(liveViewExtension(liveViewEnabled,
      // markdownConfig))` effect already picks up on any `markdownConfig` change, same
      // "reconfigure on settings/config change" pattern the math/image marker compartments use.
      checkboxUncheckTooltip: t("editor.checkboxUncheckTooltip"),
      checkboxCheckTooltip: t("editor.checkboxCheckTooltip"),
    }),
    [appSettings, readOnly, tabId, onOpenMemoLink, requestExternalConfirm, memoKey, t],
  );

  // Evicts the previous memo's cached image Blob URLs on every memo switch and on unmount
  // (`docs/plans/markdown/image-handling/PLAN.md` §5.4's "owning memo being closed" trigger).
  useEffect(() => {
    return () => {
      if (memoKey) releaseMemoImages(memoKey);
    };
  }, [memoKey]);

  // PDF export (`docs/plans/markdown/pdf-export/PLAN.md` §3/§4, `docs/MDEditor-spec.md` §6) — reads
  // `viewRef.current` at call time (a ref, so no staleness) rather than depending on it, since a ref
  // update never triggers a re-render/re-memo. Available whenever a memo is open at all — never
  // gated on `readOnly` (§3's "printing never mutates the document" availability rule, which this
  // callback and both of its call sites below must not accidentally narrow with a `readOnly` check).
  const handlePdfExport = useCallback(() => {
    const view = viewRef.current;
    if (view && tabId && memoName && memoKey) void exportMemoToPdf(view, tabId, memoName, memoKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabId, memoName, memoKey]);

  // Refs so the debounced scroll listener / view-teardown flush always see the latest identity
  // without stale closures (same pattern as `state/useEditor.ts`).
  const memoNameRef = useRef(memoName);
  memoNameRef.current = memoName;
  const overrideRef = useRef(memoEditorOverride);
  overrideRef.current = memoEditorOverride;
  // Live mirror of the `paneMode` component variable (`docs/plans/v1.2.0/preview-scroll-fix/
  // PLAN.md` §3) -- read by `persistEditorViewState` below, whose reference may be invoked from a
  // stale closure (the unmount-flush effect has an empty dep array, so its captured function
  // instance is the very first render's) that must still see the CURRENT pane mode, not the one
  // from whenever that closure happened to be created. Not used by `flushOutgoingEditorViewState`,
  // which instead derives the OUTGOING memo's own pane mode from the `override` argument it
  // already receives (kept in sync by `onCyclePaneMode`/`onTogglePreviewLock` above) -- see that
  // function's own comment.
  const paneModeRef = useRef(paneMode);
  paneModeRef.current = paneMode;
  // One-shot auto-focus signal (`useTree.ts`'s `focusEditorToken`): `focusEditorTokenRef` always
  // holds the latest value; `consumedFocusTokenRef` records which value the view-(re)creation
  // effect below has already acted on, so a click-open focuses exactly once per open and an
  // arrow-nav-open (which never changes the token) never does.
  const focusEditorTokenRef = useRef(focusEditorToken);
  focusEditorTokenRef.current = focusEditorToken;
  const consumedFocusTokenRef = useRef<number | null>(null);
  // Same one-shot-signal shape as `focusEditorTokenRef`/`consumedFocusTokenRef` above, but for the
  // `#slug` anchor-jump signal (`docs/plans/v1.2.0/cross-tab-anchor-links/PLAN.md` §3) — kept as its
  // OWN token (not reusing `focusEditorToken`) precisely so an ordinary tree-row-click/arrow-nav
  // open (which never bumps `pendingAnchorToken`) can never accidentally replay a stale anchor left
  // over from an earlier `#slug` link navigation.
  const pendingAnchorSlugRef = useRef(pendingAnchorSlug);
  pendingAnchorSlugRef.current = pendingAnchorSlug;
  const pendingAnchorTokenRef = useRef(pendingAnchorToken);
  pendingAnchorTokenRef.current = pendingAnchorToken;
  const consumedAnchorTokenRef = useRef(pendingAnchorToken);
  // `preview`-pane-mode anchor jump target, deferred until `PreviewPane` actually mounts for this
  // memo (`PLAN.md` §4.2) — the view-(re)creation effect below runs in the SAME tick as the
  // governed-pane-mode reset/auto-reveal (`revealSplitIfPreviewOnly`), whose OWN state update has
  // not yet re-rendered `paneMode`/mounted `<PreviewPane ref={previewRef}>` for the just-opened
  // memo, so `previewRef.current` cannot be relied on synchronously here.
  const pendingPreviewAnchorRef = useRef<{ memoKey: string; slug: string } | null>(null);
  // Same deferred-until-`PreviewPane`-mounts treatment as `pendingPreviewAnchorRef` above, for the
  // `paneMode === "preview"` branch of the `savedScrollLine` restore path (`docs/plans/v1.2.0/
  // preview-scroll-fix/PLAN.md` §3/§4) -- reuses the exact same deferred-consumer effect below
  // rather than a second bespoke one, since both are "wait for `previewRef.current` to reflect this
  // memo, then act on its DOM" cases.
  const pendingPreviewScrollRef = useRef<{ memoKey: string; line: number } | null>(null);
  const setEditorOverrideRef = useRef(setEditorOverride);
  setEditorOverrideRef.current = setEditorOverride;
  const scrollDebounceRef = useRef<number | null>(null);
  // Toggle-back position for double-click-active-tab jump-to-start (`docs/plans/v1.2.0/
  // tab-double-click-jump/PLAN.md` §3) -- `null` when no jump is currently "pending toggle-back"
  // (either never jumped for this memo, or already toggled back). A SEPARATE, transient,
  // in-memory-only value -- never reused by or shared with the persisted `MemoEditorSettings.
  // cursorAnchor`/`cursorHead`/`scrollPosition` fields (GitHub issue #28).
  // Discriminated union (GitHub issue #208): `paneMode === "preview"` hides the CodeMirror view
  // entirely (`display: none`) and the user scrolls `<PreviewPane>`'s own container instead, which
  // has no cursor/selection concept and no `ChangeDesc`-mappable positions -- a raw `scrollTop`
  // pixel offset is the only meaningful saved shape there. `"edit"`/`"split"` keep the original
  // character-offset pair shape (mirrors `EditorSelection.main.{anchor,head}`), continuously
  // remapped through `ChangeDesc.mapPos()` on every doc change in the `updateListener` below, so an
  // edit between jump and toggle-back doesn't leave a stale raw offset. A saved value whose `mode`
  // no longer matches the CURRENT pane mode (the user toggled pane mode mid-toggle-back) is treated
  // as absent -- the next double-click starts fresh for the new mode rather than trying to
  // reconcile two unrelated position shapes. Reset to `null` on every memo switch (view-
  // (re)creation effect below, PLAN.md §4).
  const jumpToggleRef = useRef<
    | { mode: "edit"; anchor: number; head: number; topLinePos: number }
    | { mode: "preview"; scrollTop: number }
    | null
  >(null);
  // Imperative counterpart to `jumpToggleRef` above -- exposed via `useImperativeHandle` at the
  // top of this component. First call (no saved position for the current pane mode) captures the
  // current cursor/scroll position (or, in preview-only mode, the preview container's `scrollTop`)
  // then jumps to document start; a second call restores the saved position and clears the ref, so
  // a third call starts fresh (`PLAN.md` §3.2). Reads `paneModeRef.current` rather than the `paneMode`
  // closure variable directly since this callback's identity is memoized once (`useCallback(..., [])`)
  // -- same stale-closure concern `persistEditorViewState` above documents for `paneModeRef`.
  const toggleJumpToStart = useCallback(() => {
    if (paneModeRef.current === "preview") {
      const container = previewRef.current?.getContainer();
      // No rendered preview container to scroll (not mounted yet, or nothing rendered) -- no-op,
      // same "skip rather than act on a missing target" treatment as `currentPreviewTopLine` above.
      if (!container) return;
      const saved = jumpToggleRef.current;
      if (saved !== null && saved.mode === "preview") {
        container.scrollTop = saved.scrollTop;
        jumpToggleRef.current = null;
      } else {
        jumpToggleRef.current = { mode: "preview", scrollTop: container.scrollTop };
        container.scrollTop = 0;
      }
      return;
    }
    const view = viewRef.current;
    if (!view) return;
    const saved = jumpToggleRef.current;
    if (saved !== null && saved.mode === "edit") {
      view.dispatch({
        selection: { anchor: saved.anchor, head: saved.head },
        effects: EditorView.scrollIntoView(saved.topLinePos, { y: "start" }),
      });
      jumpToggleRef.current = null;
    } else {
      const { anchor, head } = view.state.selection.main;
      const topLinePos = view.state.doc.line(computeTopVisibleLine(view)).from;
      jumpToggleRef.current = { mode: "edit", anchor, head, topLinePos };
      view.dispatch({ selection: { anchor: 0, head: 0 }, effects: EditorView.scrollIntoView(0, { y: "start" }) });
    }
  }, []);
  // #191: paneModeRef already reflects the settled mode here (memo is already mounted), unlike the deferred pipeline's mid-creation prediction.
  const jumpToAnchor = useCallback(
    (slug: string) => {
      if (paneModeRef.current === "preview") {
        const handle = previewRef.current;
        if (!handle || !memoKey) return;
        void handle.waitForRender(memoKey).then((ready) => {
          if (!ready) return;
          const container = handle.getContainer();
          container?.querySelector(`#${CSS.escape(slug)}`)?.scrollIntoView({ block: "start" });
        });
        return;
      }
      const view = viewRef.current;
      if (!view) return;
      resolveAndScrollToHeadingAnchor(view, slug);
    },
    [memoKey],
  );
  // Which memo (and which TAB) `viewRef.current` actually belongs to (and that memo's override
  // snapshot at the time its view was created) — set ONLY inside the view-(re)creation effect
  // below, once a view has actually been (re)created for a given `ed.loadGeneration`. Unlike
  // `memoNameRef`/`overrideRef` above (which track the live `memoName`/`memoEditorOverride` props
  // and can flip to the NEXT memo/tab before this effect's next run, since `useTree.ts`'s
  // `setOpenMemoName` is synchronous while the load itself is async), these stay pinned to the
  // OUTGOING memo/tab until the next effect run overwrites them for the memo it just loaded. This
  // is what lets the outgoing-flush at the top of that next run target the correct (previous)
  // memo/tab instead of whatever the props have already drifted to (GitHub issue #28 follow-up
  // review finding; `loadedTabIdRef` added for `docs/plans/v1.1.0/tab-memo-memory` reviewer
  // finding #1 retest below).
  const loadedMemoNameRef = useRef<string | null>(null);
  const loadedOverrideRef = useRef<MemoEditorSettings | undefined>(undefined);
  const loadedTabIdRef = useRef<string | null>(null);

  // Shared by both save-path functions below (`docs/plans/v1.2.0/preview-scroll-fix/PLAN.md` §2/§3)
  // -- `previewRef.current` may be `null` (no `<PreviewPane>` mounted right now, e.g. `paneMode`
  // just flipped away from `"preview"`/`"split"` on the SAME render this flush runs in) or its
  // `getContainer()` may return `null` before first mount; either case is treated the same as "no
  // rendered block yet" (also `null`) -- the caller skips the whole write rather than persisting a
  // wrong/zero `scrollPosition`.
  const currentPreviewTopLine = (): number | null => {
    const container = previewRef.current?.getContainer();
    return container ? computeTopVisiblePreviewLine(container) : null;
  };

  // Persists both the scroll position and the cursor/selection for `name`, in the CURRENTLY active
  // tab — grabbed together from `view` since both are read at the same trigger points (debounced
  // scroll, unmount), so one combined write avoids a second `memo_set_editor_override` round-trip
  // per trigger. Routes through `treeState.setEditorOverride` (via `setEditorOverrideRef`), which
  // is safe here specifically because both call sites below are documented to only ever fire while
  // `name` is still the memo open in the tab that's still active (see each call site's own
  // comment) — unlike the OUTGOING-memo flush at the top of the view-(re)creation effect, which
  // uses `flushOutgoingEditorViewState` below instead, precisely because that guarantee does NOT
  // hold there.
  const persistEditorViewState = (view: EditorView, name: string | null, override: MemoEditorSettings | undefined) => {
    if (!name) return;
    const { anchor, head } = view.state.selection.main;
    // `paneMode === "preview"` (`docs/plans/v1.2.0/preview-scroll-fix/PLAN.md` §2/§3): the edit
    // sub-pane is `display:none` and the user scrolls the preview container directly, never
    // `view.scrollDOM` -- read the preview's own top-visible line instead. `null` (no rendered
    // `[data-source-line]` block yet, or no preview container mounted right now) skips the WHOLE
    // write rather than persisting a wrong/zero `scrollPosition` over a previously-good value.
    const savedLine = paneModeRef.current === "preview" ? currentPreviewTopLine() : computeTopVisibleLine(view);
    if (savedLine === null) return;
    void setEditorOverrideRef.current(name, {
      ...override,
      scrollPosition: savedLine,
      cursorAnchor: anchor,
      cursorHead: head,
    });
  };

  // Outgoing-memo variant of `persistEditorViewState` above, for the view-(re)creation effect's own
  // outgoing-flush only (`docs/plans/v1.1.0/tab-memo-memory` reviewer finding #1 retest): calls
  // `ipc/tree.ts`'s `memoSetEditorOverride` directly with an EXPLICIT `tabId` rather than going
  // through `treeState.setEditorOverride`/`setEditorOverrideRef`. `setEditorOverrideRef.current` is
  // always bound to whichever tab is CURRENTLY active (`useTree.ts`'s own `tabId` closure) — by the
  // time this fires (on a switch to a different tab whose own memo just finished loading), the
  // active tab has already moved on, so `setEditorOverrideRef.current` would silently target the
  // NEW tab with the OUTGOING memo's name, most often getting a `NotFound` back (surfaced as
  // `useTree.ts`'s `.ttn-tree-error` banner even though nothing is actually wrong) whenever no
  // same-named memo happens to exist there too. `outgoingTabId`/`name` are `loadedTabIdRef`/
  // `loadedMemoNameRef`'s pinned-to-the-outgoing-view values, not the live `tabId`/`memoName` props.
  // Bypasses `applySnapshot`/`treeState.nodes` on purpose and swallows errors — same "best-effort
  // bookkeeping for a tab the user has already left, not a user-visible mutation" rationale as
  // `state/useTree.ts`'s `persistLastViewed`; the position fields written here are read back
  // directly from `editor_load`'s own response on next open, never from `treeState.nodes`.
  const flushOutgoingEditorViewState = (view: EditorView, outgoingTabId: string | null, name: string | null, override: MemoEditorSettings | undefined) => {
    if (!outgoingTabId || !name) return;
    const { anchor, head } = view.state.selection.main;
    // Derives the OUTGOING memo's own effective pane mode from `override` (== `loadedOverrideRef.
    // current`, kept live-synced with any mid-session `onCyclePaneMode`/`onTogglePreviewLock`
    // toggle for that same memo -- see those functions' own comments) rather than the live
    // `paneMode`/`paneModeRef`, which by the time this runs may already reflect the INCOMING memo's
    // own governed pane mode instead (`docs/plans/v1.2.0/preview-scroll-fix/PLAN.md` §3).
    const outgoingPaneMode = override?.previewLocked ? "preview" : (override?.paneMode ?? editorDefaults.paneMode);
    const savedLine = outgoingPaneMode === "preview" ? currentPreviewTopLine() : computeTopVisibleLine(view);
    if (savedLine === null) return;
    void memoSetEditorOverride(outgoingTabId, name, {
      ...override,
      scrollPosition: savedLine,
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
    // `loadedTabIdRef`/`loadedMemoNameRef`/`loadedOverrideRef` (the tab/memo `outgoingView` actually
    // belongs to), NOT the live `tabId`/`memoName`/`memoEditorOverride` props — those already point
    // at the tab/memo being switched TO by the time this effect runs (see the refs' declaration
    // comment above). Uses `flushOutgoingEditorViewState`, not `persistEditorViewState` — the
    // outgoing tab may already differ from the active one (`docs/plans/v1.1.0/tab-memo-memory`
    // reviewer finding #1 retest; see that function's own comment).
    const outgoingView = viewRef.current;
    if (outgoingView) {
      if (scrollDebounceRef.current !== null) window.clearTimeout(scrollDebounceRef.current);
      flushOutgoingEditorViewState(outgoingView, loadedTabIdRef.current, loadedMemoNameRef.current, loadedOverrideRef.current);
      outgoingView.destroy();
      viewRef.current = null;
    }

    // Session-only jump-to-start toggle state must not leak into a newly-opened memo
    // (`docs/plans/v1.2.0/tab-double-click-jump/PLAN.md` §4) — same "any `loadGeneration` bump
    // clears it" trigger as `liveViewOverride`/`paneModeOverride`/`previewLockOverride` below, but
    // placed BEFORE the `!ed.content` early return so a switch to a tab with no open memo also
    // clears it, per PLAN.md §4's explicit "including a tab switch to a tab with no open memo"
    // requirement.
    jumpToggleRef.current = null;

    if (!ed.content) {
      loadedTabIdRef.current = null;
      loadedMemoNameRef.current = null;
      loadedOverrideRef.current = undefined;
      setRenderTick((n) => n + 1);
      return;
    }
    // From here on `tabId`/`memoName`/`memoEditorOverride` (the props captured by this effect's
    // closure, from the same render that bumped `ed.loadGeneration`) correctly identify the tab/memo
    // this view is being built for — snapshot them now so the NEXT effect run's outgoing-flush
    // (above) reads the right tab/memo once the props have moved on again.
    loadedTabIdRef.current = tabId;
    loadedMemoNameRef.current = memoName;
    loadedOverrideRef.current = memoEditorOverride;

    // 今回のロードで`#slug`アンカージャンプが必要かどうか(`docs/plans/v1.2.0/cross-tab-anchor-links/
    // PLAN.md` §3) — 下の`EditorState.selection`の初期化より前に判定する。これにより、今回アンカー
    // ジャンプが予約されている場合は保存済みカーソル/選択範囲の復元をスキップできる。トークンの消費も
    // ここで行う(アンカーディスパッチ側ではなく) — 「今回のロードでジャンプが要求されたか」を判定
    // する場所を一箇所に統一するため。
    let anchorSlugForThisLoad: string | null = null;
    if (pendingAnchorTokenRef.current !== consumedAnchorTokenRef.current) {
      consumedAnchorTokenRef.current = pendingAnchorTokenRef.current;
      anchorSlugForThisLoad = pendingAnchorSlugRef.current;
    }

    // A fresh load always starts at the *governed* view mode (per-memo override falling back to
    // the global default) — any prior session's runtime toggle never carries over to a newly
    // opened memo (`docs/plans/markdown-editing/PLAN.md` §3). Computed directly from
    // `resolvedViewMode` here (not `liveViewEnabled`) so this initial build can't see a stale
    // `liveViewOverride` left over from the memo just switched away from.
    const initialLiveView = resolvedViewMode === "live";

    // カーソル/選択範囲の復元(GitHub issue #28) — 状態生成時に設定する(下のスクロール位置のように
    // ビューの初回レイアウト計測を待って後からディスパッチするのではない)。初期`EditorState.selection`
    // として渡すことで、余分なトランザクション/履歴エントリを避けられる。保存後にファイルが外部で
    // 編集され、保存済みオフセットが範囲外になっている場合に備えて、読み込んだ文書長でクランプする。
    // 今回のロードでアンカージャンプが予約されている場合はこの復元を丸ごとスキップする(下の
    // `savedScrollLine`に既に適用されている「明示的なナビゲーション先が、直前の閲覧位置に優先する」
    // というルール#197と同じ考え方 — ここが唯一未対応だった箇所: 従来は保存済みカーソル位置を
    // 無条件にここで適用していたため、この後のアンカージャンプ側のディスパッチが何らかの理由で発火
    // しなかった場合、「ジャンプしていない」ことが「間違った場所にジャンプした」ように見えてしまい、
    // 不具合を覆い隠していた)。
    const docLength = ed.content.text.length;
    const clamp = (value: number) => Math.min(Math.max(value, 0), docLength);
    const savedAnchor = ed.content.cursorAnchor;
    const savedHead = ed.content.cursorHead;
    const initialSelection =
      anchorSlugForThisLoad == null && savedAnchor != null && savedHead != null
        ? EditorSelection.single(clamp(savedAnchor), clamp(savedHead))
        : undefined;

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
        imagePasteExtension({ tabId, memoName, onError: ed.reportError }),
        // Keeps the status bar's char/line/cursor-position derived state live on every doc
        // change or cursor/selection move, not just after (re)creating the view.
        EditorView.updateListener.of((update) => {
          if (update.docChanged || update.selectionSet) setRenderTick((n) => n + 1);
          // Continuous remapping of a pending jump-to-start toggle-back position across edits
          // (`docs/plans/v1.2.0/tab-double-click-jump/PLAN.md` §3.3) -- `ChangeDesc.mapPos()`'s
          // own built-in behavior (shift forward past an insertion, collapse to a deletion's edge)
          // is the "adjusted for lines/characters inserted or removed before it" guarantee the
          // spec describes; `update.changes` already represents the combined `ChangeDesc` for the
          // whole update even if it composed multiple sub-changes, so a single `mapPos()` call per
          // saved value handles that case too. Only the `"edit"`-shaped saved value is a document
          // position at all (GitHub issue #208) -- the `"preview"`-shaped `scrollTop` pixel offset
          // has nothing to remap here.
          if (update.docChanged && jumpToggleRef.current && jumpToggleRef.current.mode === "edit") {
            jumpToggleRef.current = {
              mode: "edit",
              anchor: update.changes.mapPos(jumpToggleRef.current.anchor),
              head: update.changes.mapPos(jumpToggleRef.current.head),
              topLinePos: update.changes.mapPos(jumpToggleRef.current.topLinePos),
            };
          }
        }),
      ],
    });
    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;
    setRenderTick((n) => n + 1);
    // Click-open (and equivalent explicit-navigation opens like `memo:` link clicks) auto-focuses
    // the editor; arrow-key tree nav does not (`[spec §2.2]`, `[spec §9.1 item 3]`). The token only
    // changes for the former, so this fires at most once per distinct open.
    // Reset the session-only toggles for the newly-opened memo (harmless if they already equalled
    // this memo's own governed values — the reconfigure/render below just reflects the same value
    // again in that case). A freshly-opened memo always starts from its governed pane-mode, never a
    // stale session toggle left over from the previously-open memo (`PLAN.md` §4.1). Done BEFORE
    // the auto-reveal call just below, since both ultimately queue updates to the same
    // `paneModeOverride` state — resetting first, then revealing, ensures the reveal's own update
    // (if any) is the one that wins, not clobbered by this reset running after it.
    setLiveViewOverride(null);
    setPaneModeOverride(null);
    setPreviewLockOverride(null);
    // Preview zoom (`docs/plans/v1.2.0/preview-zoom/PLAN.md` §2) joins the same reset group above —
    // a freshly-opened memo always starts at 100%, never a stale zoom level left over from whatever
    // memo/tab was previously open in `"preview"` mode.
    setPreviewZoomPercent(100);
    if (focusEditorTokenRef.current !== consumedFocusTokenRef.current) {
      consumedFocusTokenRef.current = focusEditorTokenRef.current;
      view.focus();
      // Auto-reveal (`docs/plans/markdown/markdown-preview/PLAN.md` §4.4): an explicit navigation
      // to this memo (tree-row click, cross-tab search-result jump, `memo:` link-click navigation
      // — every one of these bumps `focusEditorToken`, unlike arrow-key nav) reveals the edit pane
      // if this memo's own governed pane-mode is `"preview"`, so the just-focused view is visible.
      revealSplitIfPreviewOnly();
    }

    // `memo:...#slug` anchor jump (`docs/plans/v1.2.0/cross-tab-anchor-links/PLAN.md` §3/§4) — a
    // ONE-SHOT signal, consumed exactly once per distinct `pendingAnchorToken`, the same "compare
    // against the last-consumed value" shape as `focusEditorTokenRef`/`consumedFocusTokenRef`
    // directly above. Deliberately gated on its OWN token (not `focusEditorTokenRef`'s), so an
    // ordinary tree-row-click/arrow-nav open occurring later never replays a stale anchor.
    let anchorTimeout = 0;
    // 今回のeffectで実際にアンカージャンプが実行されるかどうか(下のどちらかの分岐) — 数行下の
    // `savedScrollLine`復元をゲートし、アンカージャンプと同時/直後に発火して古い保存位置で上書き
    // されないようにする(issue #197: 以前はそれぞれ独立に`setTimeout(...,0)`で予約されており、
    // `scrollRestoreTimeout`はこのeffect内で`anchorTimeout`より後に宣言されているため常に後から
    // 実行されて静かに勝ってしまい、クリックした見出しではなくこのメモが最後にいた場所へスクロール
    // していた)。トークンの消費は既に上(初期選択範囲の計算)で行っているため、ここではその判定結果を
    // 再利用するだけで、二重に判定し直すことはしない。
    const anchorJumpPending = anchorSlugForThisLoad != null;
    if (anchorSlugForThisLoad != null) {
      const anchorSlug = anchorSlugForThisLoad;
      // Split by this memo's own GOVERNED pane-mode (`PLAN.md` §4: `MemoEditorSettings.paneMode ??
      // AppSettings.editorDefaults.paneMode`) — `edit`/`split` reuse the existing CodeMirror-based
      // `resolveAndScrollToHeadingAnchor` (§4.1: works for `split` too, since `PreviewPane`'s own
      // scroll-sync listener follows the CodeMirror `scroll` event this produces as a side effect,
      // no preview-specific code needed). `preview` acts on the preview DOM directly instead (§4.2:
      // a zero-height `scrollDOM` can't compute a meaningful scroll offset there) — deferred via
      // `pendingPreviewAnchorRef` below rather than reading `previewRef.current` here, because it
      // may not be mounted for this memo yet on this exact render (see that ref's own comment).
      //
      // 単純に`resolvedPaneMode`だけでは判定できない — 実際に画面に表示されるモードが、素の
      // governed値と異なるケースが2つある:
      // 1. `previewLocked`(`memoEditorOverride?.previewLocked`、GitHub issue #149)は、
      //    `resolvedPaneMode`が何であれ、表示されるペインを強制的に`"preview"`にする —
      //    このコンポーネント自身の実効モード変数`paneMode`(先頭で
      //    `previewLocked ? "preview" : (paneModeOverride ?? resolvedPaneMode)`として計算)は
      //    既にこれを考慮しているが、`resolvedPaneMode`単体では考慮していない。これを見落として
      //    いたため、governed値が`"split"`や`"edit"`でもプレビュー固定(previewLocked)されている
      //    メモは下のCodeMirror側の分岐に入り、`display:none`のコンテナへディスパッチしてしまって
      //    いた — 実機で再現: 両方のメモをツールバーでプレビュー固定している間、ジャンプが静かに
      //    何も起こさなかった。
      // 2. 同じ明示的ナビゲーションのオープンは上で`focusEditorToken`もbumpさせ、この直前の
      //    ブロックで`revealSplitIfPreviewOnly()`を呼び出す — その中の
      //    `setPaneModeOverride("split")`は次のレンダーまで反映されない(このeffect内のstate
      //    セッターは、このレンダーのクロージャが既に取得済みの値を遡って変えられない)。そのため、
      //    governedが`"preview"`でロックされていないメモが自動的に`"split"`へ復帰(reveal)される
      //    直前でも、ここではまだ`"preview"`として読まれ、下のプレビューDOM側の分岐に入って
      //    しまう — 実際には1レンダー後に画面上は`"split"`になり、ジャンプを一度も受け取れない
      //    生きたCodeMirrorペインが表示されているにもかかわらず。
      // `revealSplitIfPreviewOnly`と、レンダー時の`paneMode`計算式、両方の判定をここで先読みする
      // ことで、そのレンダーを待たずに実際の(effect後の)表示モードを予測する。
      const effectivePaneModeForAnchor = previewLocked ? "preview" : resolvedPaneMode === "preview" ? "split" : resolvedPaneMode;
      if (effectivePaneModeForAnchor === "preview") {
        pendingPreviewAnchorRef.current = memoKey ? { memoKey, slug: anchorSlug } : null;
      } else {
        // Deferred to a macrotask (GitHub issue #195): `view` was just constructed above in this
        // same synchronous tick, so CodeMirror hasn't finished its own initial layout/measure pass
        // yet — an immediate call here dispatches `resolveAndScrollToHeadingAnchor`'s
        // `EditorView.scrollIntoView` effect before that pass completes, so the selection lands
        // correctly but the viewport never actually scrolls. `requestAnimationFrame` was tried
        // first (mirroring the `savedScrollLine` restore's own rAF deferral just below) but proved
        // unreliable here: verified via live reproduction (CDP-driven) that CodeMirror's own
        // initial-mount measure/flush cycle is *itself* scheduled through `requestAnimationFrame`,
        // so an app-level rAF callback races it — sometimes running before that internal cycle has
        // completed (silently producing a wrong, "stuck" viewport with no further re-measure ever
        // triggered), and via `view.requestMeasure`'s `write` phase it throws outright ("Calls to
        // EditorView.update are not allowed while an update is in progress"), since CM disallows a
        // nested `dispatch()` while its own update is in progress. A macrotask via `setTimeout`
        // reliably runs only after the browser has fully finished the current animation-frame's
        // rAF callbacks + style/layout/paint (and thus after CodeMirror's own internal cycle),
        // confirmed reliable across repeated live reproductions of the exact GitHub issue #195
        // repro steps.
        anchorTimeout = window.setTimeout(() => resolveAndScrollToHeadingAnchor(view, anchorSlug), 0);
      }
    }

    const savedScrollLine = ed.content.scrollPosition;
    let scrollRestoreTimeout = 0;
    if (anchorJumpPending) {
      // Skip entirely (issue #197) — an explicit cross-tab anchor-link navigation to a specific
      // heading always wins over restoring this memo's last-viewed scroll position.
      pendingPreviewScrollRef.current = null;
    } else if (savedScrollLine && resolvedPaneMode === "preview") {
      // `paneMode === "preview"` (`docs/plans/v1.2.0/preview-scroll-fix/PLAN.md` §3/§4): deferred
      // until `<PreviewPane ref={previewRef}>` actually mounts/renders for this memo -- same timing
      // gap as the `#slug` anchor-jump case above (this effect may run before that reset/reveal has
      // re-rendered), so this reuses the SAME deferred-consumer effect below rather than a second
      // bespoke one. Split by this memo's own GOVERNED pane-mode, mirroring the anchor-jump branch
      // just above for the same reason.
      pendingPreviewScrollRef.current = memoKey ? { memoKey, line: savedScrollLine } : null;
    } else if (savedScrollLine) {
      // Deferred to a macrotask (GitHub issue #196), same fix as `anchorTimeout` just above (GitHub
      // issue #195): `scrollToTopVisibleLine` also dispatches `EditorView.scrollIntoView` on this
      // same freshly-constructed `view`, before CodeMirror's own rAF-scheduled initial layout/measure
      // pass has completed. The `requestAnimationFrame` deferral this line used before proved
      // unreliable for the exact same reason `anchorTimeout` documents above -- an app-level rAF
      // callback races CodeMirror's own internal rAF-scheduled measure cycle. `window.setTimeout`
      // reliably runs after that cycle instead.
      scrollRestoreTimeout = window.setTimeout(() => scrollToTopVisibleLine(view, savedScrollLine), 0);
    } else {
      pendingPreviewScrollRef.current = null;
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
      if (anchorTimeout) window.clearTimeout(anchorTimeout);
      if (scrollRestoreTimeout) window.clearTimeout(scrollRestoreTimeout);
      view.scrollDOM.removeEventListener("scroll", scrollHandler);
    };
    // Intentionally keyed only on `loadGeneration`: settings/readOnly/whitespace changes on an
    // already-mounted view are handled by the separate compartment-reconfigure effects below,
    // not by recreating the whole view.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ed.loadGeneration]);

  // Deferred `preview`-pane-mode consumer for BOTH the `#slug` anchor jump
  // (`docs/plans/v1.2.0/cross-tab-anchor-links/PLAN.md` §4.2/§4.3) and the scroll-position restore
  // (`docs/plans/v1.2.0/preview-scroll-fix/PLAN.md` §3/§4) — the view-(re)creation effect above may
  // run before `<PreviewPane ref={previewRef}>` is actually mounted for this memo —
  // `paneModeOverride`'s own reset (also queued in that same effect) hasn't re-rendered yet.
  // `paneMode`/`memoKey`の変化、`previewMountTick`(`setPreviewRef`が`<PreviewPane>`の実際の
  // マウント時にbumpする — そのセッションでそのタブの`PreviewPane`が初めてマウントされる時に効く。
  // このeffectが最初に走る時点で既に`paneMode`/`memoKey`は最終値になっているため、`previewMountTick`
  // がないと`previewRef.current`を見るチャンスが一度しかない)、そして`ed.loadGeneration`の変化、
  // それぞれで再チェックする。最後の`ed.loadGeneration`こそが、クロスタブジャンプにとって本質的に
  // 重要なもの: `memoKey`は`focusOnMemo`が遷移先メモを選んだ瞬間(同期的なツリー状態の更新)に変わる
  // が、実際に`pendingPreviewAnchorRef`をセットする上の`[ed.loadGeneration]`effectは、`editorLoad`の
  // 非同期IPC呼び出しが解決した後にしか走らない。ここに`ed.loadGeneration`がないと、このeffectの
  // (`memoKey`変化による)唯一の実行タイミングが早すぎて何もまだ予約されておらず、その後コンテンツが
  // 読み込み終わって`paneMode`/`memoKey`/`previewMountTick`のどれも変化しないままだと、後から実際に
  // 入った`pendingPreviewAnchorRef`を読みに行く実行機会がもう残っていない — 実機で再現: 既に
  // `preview`表示になっているタブ(例えばリンク元メモ自体がgoverned `preview`で、クリック前から
  // `<PreviewPane>`が既にマウント済みだったケース、つまり`previewMountTick`が対処するマウント
  // タイミングの隙間ではない場合)へのクロスタブジャンプが、静かに一度もジャンプしなかった。
  // Runs until `previewRef.current` exists AND still belongs to a pending target memo (a pending
  // entry for an abandoned memo is simply left inert — a later navigation either overwrites it with
  // its own target before this ever matches, or never matches at all, matching this app's
  // silent-no-op-on-edge-case convention). `paneMode === "edit"`
  // means `PreviewPane` never mounts for this memo at all (governed mode was, and stayed, `edit` —
  // should not happen when either deferred entry was set for `resolvedPaneMode === "preview"`, but
  // guarded defensively). The anchor jump wins over a same-open scroll-position restore when both
  // are pending for the same memo (a link-navigation open's explicit target takes priority over the
  // previous session's leave-off reading position) — dropping the scroll entry rather than letting
  // it separately win a later run of this same effect.
  useEffect(() => {
    if (paneMode === "edit") return;
    const handle = previewRef.current;
    if (!handle) return;

    const pendingAnchor = pendingPreviewAnchorRef.current;
    if (pendingAnchor && pendingAnchor.memoKey === memoKey) {
      pendingPreviewAnchorRef.current = null;
      if (pendingPreviewScrollRef.current?.memoKey === memoKey) pendingPreviewScrollRef.current = null;
      void handle.waitForRender(pendingAnchor.memoKey).then((ready) => {
        if (!ready) return;
        const container = handle.getContainer();
        container?.querySelector(`#${CSS.escape(pendingAnchor.slug)}`)?.scrollIntoView({ block: "start" });
      });
      return;
    }

    const pendingScroll = pendingPreviewScrollRef.current;
    if (pendingScroll && pendingScroll.memoKey === memoKey) {
      pendingPreviewScrollRef.current = null;
      void handle.waitForRender(pendingScroll.memoKey).then((ready) => {
        if (!ready) return;
        const container = handle.getContainer();
        if (container) scrollPreviewToLine(container, pendingScroll.line - 1);
      });
    }
  }, [paneMode, memoKey, previewMountTick, ed.loadGeneration]);

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

  // Source-view image marker (`docs/plans/markdown-editing/PLAN.md`, GitHub issue #81) — opposite
  // polarity from the live-view toggle above (on while `liveViewEnabled` is false), reconfigured
  // via its own compartment since it has no `markdownConfig` dependency at all.
  useEffect(() => {
    viewRef.current?.dispatch({ effects: sourceImageMarkerCompartment.reconfigure(sourceImageMarkerViewExtension(!liveViewEnabled)) });
  }, [liveViewEnabled]);

  // Source-view math marker (`docs/plans/v1.2.0/math-rendering/PLAN.md` §4) — same opposite-polarity
  // reconfigure pattern as the image marker above, its own independent compartment.
  useEffect(() => {
    viewRef.current?.dispatch({ effects: sourceMathMarkerCompartment.reconfigure(sourceMathMarkerViewExtension(!liveViewEnabled)) });
  }, [liveViewEnabled]);

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
        // Auto-reveal (`docs/plans/markdown/markdown-preview/PLAN.md` §4.4): Ctrl+F expects to
        // focus/scroll the CodeMirror view, which is hidden while pane-mode is `"preview"`.
        revealSplitIfPreviewOnly();
        search.open();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [search, appSettings?.shortcuts, revealSplitIfPreviewOnly]);

  // Default keyboard shortcuts for the 18 Markdown toolbar actions plus the live/source toggle
  // (`docs/detailed-spec.md` §5.10: "every toolbar action gets a default keyboard shortcut,
  // registered as an entry in the existing generic shortcut system" — 18 actions after GitHub
  // issue #176). Dispatches through
  // `MarkdownToolbar`'s imperative handle rather than duplicating its action switch statement here.
  // Scoped to when focus is within this editor's own CodeMirror content, so these common
  // combinations (Ctrl+B, Ctrl+I, etc.) don't fire while typing elsewhere in the app (e.g. a
  // rename input or a dialog).
  useEffect(() => {
    const shortcuts = resolveShortcuts(appSettings?.shortcuts);
    const reverse = new Map<string, string>();
    for (const [actionId, combo] of Object.entries(shortcuts)) {
      // `markdown.paneModeCycle` is deliberately excluded here — it gets its own separate,
      // focus-unconstrained effect below (`docs/plans/markdown/markdown-preview/PLAN.md` §6.3),
      // since (unlike every other `markdown.*` action) it must fire even while focus is outside
      // this editor's own CodeMirror content (e.g. inside the preview pane's own DOM subtree).
      //
      // `markdown.hardBreak` is also excluded — unlike every other `markdown.*` action, its real
      // command (`hardBreakCommand`) already runs directly inside the CodeMirror-layer `"Shift-Enter"`
      // binding in `textCommands.ts` (see that binding's own comment for why a no-op there wasn't
      // viable). Routing it through this map too would double-fire it.
      if (actionId.startsWith("markdown.") && actionId !== "markdown.paneModeCycle" && actionId !== "markdown.hardBreak") reverse.set(combo, actionId);
    }
    const handler = (e: KeyboardEvent) => {
      // Deliberately NOT bailing on `e.defaultPrevented` (GitHub issue #140's fix regressed this):
      // `textCommands.ts`'s `neutralizeReservedCombos` claims every combo in `RESERVED_COMBOS` at the
      // CodeMirror layer with a no-op `{ run: () => true, preventDefault: true }` binding — purely to
      // stop CodeMirror's own destructive built-in command (and the native contenteditable fallback)
      // from running, NOT because a real command already ran. That binding's `preventDefault: true`
      // still sets `e.defaultPrevented`, so checking it here silently swallowed this handler's own
      // dispatch for every such combo (Ctrl+Z, Ctrl+Y, Ctrl+I, Ctrl+Shift+K — the exact combos
      // `neutralizeReservedCombos`'s own doc comment lists as colliding with CodeMirror's built-in
      // keymaps), leaving those shortcuts doing nothing at all instead of running this app's own
      // bound action. `markdown.hardBreak` is the one `markdown.*` action whose real command DOES
      // already run at the CodeMirror layer (see above) — it's excluded from `reverse` instead of
      // relying on this check, so removing this check doesn't reintroduce a double-fire for it.
      if (!viewRef.current) return;
      if (!containerRef.current?.contains(document.activeElement)) return;
      const actionId = reverse.get(comboFromEvent(e));
      if (!actionId) return;
      if (actionId === "markdown.toggleLiveView") {
        e.preventDefault();
        // Same auto-reveal treatment as the toolbar header's own live/source toggle button
        // (`handleToggleLiveView`, wired at the `<MarkdownToolbar>` call site below) — this
        // keyboard shortcut is an equivalent, separate invocation path for the identical toggle.
        handleToggleLiveView();
        return;
      }
      e.preventDefault();
      toolbarRef.current?.invokeAction(actionId.slice("markdown.".length) as ToolbarActionId);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [appSettings?.shortcuts, handleToggleLiveView]);

  // `markdown.paneModeCycle` (`docs/plans/markdown/markdown-preview/PLAN.md` §6.3): a separate,
  // focus-UNconstrained effect (unlike the 18-grid-action dispatcher above, which only fires while
  // focus is inside `.ttn-editor-cm-container`) — this shortcut must work regardless of focus
  // location within the editor pane, including while focus is inside the preview pane's own DOM
  // subtree. Mirrors the also-unconstrained Ctrl+F handler's pattern above. Not part of
  // `ToolbarActionId`/`toolbarSlots.ts`'s 18-slot registry, so it can't go through
  // `MarkdownToolbar`'s `invokeAction` switch — it calls `onCyclePaneMode` directly instead.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const shortcuts = resolveShortcuts(appSettings?.shortcuts);
      if (comboFromEvent(e) === shortcuts["markdown.paneModeCycle"]) {
        e.preventDefault();
        onCyclePaneMode();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [appSettings?.shortcuts, onCyclePaneMode]);

  // `markdown.pdfExport` (`docs/plans/markdown/pdf-export/PLAN.md` §4, `docs/MDEditor-spec.md` §8):
  // same focus-unconstrained treatment as `markdown.paneModeCycle` above — must fire regardless of
  // focus location within the editor pane. `e.preventDefault()` is required here specifically to
  // suppress WebView2/Chromium's own native `Ctrl+P` browser-print dialog (§8's "native-print
  // conflict" note), not just as this app's usual "don't let the browser also handle it" habit.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const shortcuts = resolveShortcuts(appSettings?.shortcuts);
      if (comboFromEvent(e) === shortcuts["markdown.pdfExport"]) {
        e.preventDefault();
        handlePdfExport();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [appSettings?.shortcuts, handlePdfExport]);

  // `preview.zoomIn`/`preview.zoomOut` (`docs/plans/v1.2.0/preview-zoom/PLAN.md` §5.2): same
  // focus-UNconstrained treatment as `markdown.paneModeCycle`/`markdown.pdfExport` above — must fire
  // regardless of focus location within the editor pane, including while focus is inside the
  // (possibly hidden, outside `"preview"` mode) preview pane's own DOM subtree. `Ctrl+=`/`Ctrl+-` are
  // also WebView2/Chromium's native page-zoom shortcuts, so both branches call `e.preventDefault()`
  // to suppress that native behavior first, same reasoning as `markdown.pdfExport`'s
  // native-print-dialog conflict.
  //
  // Numpad alias asymmetry (§5.3, confirmed via `comboFromEvent`'s `e.key`-based, not `e.code`-based,
  // implementation):
  //   - Zoom OUT: `Ctrl+NumpadSubtract`'s `e.key` is already the literal `"-"`, identical to the
  //     main-row `Ctrl+-`'s `e.key` — `comboFromEvent` already produces the same `"Ctrl+-"` string
  //     for BOTH physical keys, so the ordinary `resolveShortcuts` match below handles the numpad key
  //     for free. NOT an oversight that there's no separate numpad-subtract branch here.
  //   - Zoom IN: `Ctrl+NumpadAdd`'s `e.key` is `"+"`, but the main-row `Ctrl+=`'s `e.key` (no Shift
  //     held) is `"="` — these are DIFFERENT strings, so `comboFromEvent` does NOT already alias
  //     them. Handled as a separate, hardcoded, non-rebindable check via `e.code` (not `e.key`, and
  //     deliberately not routed through `resolveShortcuts` — this alias has no Settings entry of its
  //     own to go stale against if the user rebinds `preview.zoomIn`).
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const shortcuts = resolveShortcuts(appSettings?.shortcuts);
      if (comboFromEvent(e) === shortcuts["preview.zoomIn"]) {
        e.preventDefault();
        onZoomIn();
        return;
      }
      if (comboFromEvent(e) === shortcuts["preview.zoomOut"]) {
        e.preventDefault();
        onZoomOut();
        return;
      }
      if (e.ctrlKey && e.code === "NumpadAdd") {
        e.preventDefault();
        onZoomIn();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [appSettings?.shortcuts, onZoomIn, onZoomOut]);

  const runOnView = (fn: (view: EditorView) => void) => {
    const view = viewRef.current;
    if (view) fn(view);
  };

  // Separators group the menu by role (GitHub issue #117): history / clipboard-selection / line
  // & text ops / markdown checkbox / search / display toggle / properties.
  const buildMenuItems = (): ContextMenuEntry[] => [
    { label: t("common.undo"), onSelect: () => runOnView(undoCommand), disabled: readOnly },
    { label: t("common.redo"), onSelect: () => runOnView(redoCommand), disabled: readOnly },
    { type: "separator" },
    { label: t("common.cut"), onSelect: () => runOnView(cutCommand), disabled: readOnly },
    { label: t("common.copy"), onSelect: () => runOnView(copyCommand) },
    {
      label: t("common.paste"),
      onSelect: () => runOnView((view) => pasteCommand(view, { tabId, memoName, onError: ed.reportError })),
      disabled: readOnly,
    },
    { label: t("common.selectAll"), onSelect: () => runOnView(selectAllCommand) },
    { type: "separator" },
    {
      label: t("editor.lineOpsMenu"),
      submenu: [
        { label: t("editor.moveLineUp"), onSelect: () => runOnView(moveLineUpCommand), disabled: readOnly },
        { label: t("editor.moveLineDown"), onSelect: () => runOnView(moveLineDownCommand), disabled: readOnly },
        { label: t("editor.duplicateLine"), onSelect: () => runOnView(duplicateLineCommand), disabled: readOnly },
        { label: t("editor.deleteLine"), onSelect: () => runOnView(deleteLineCommand), disabled: readOnly },
      ],
    },
    {
      label: t("editor.convertMenu"),
      submenu: [
        { label: t("editor.convertUpper"), onSelect: () => runOnView(convertUppercase), disabled: readOnly },
        { label: t("editor.convertLower"), onSelect: () => runOnView(convertLowercase), disabled: readOnly },
        { label: t("editor.convertHiragana"), onSelect: () => runOnView(convertToHiragana), disabled: readOnly },
        { label: t("editor.convertKatakana"), onSelect: () => runOnView(convertToKatakana), disabled: readOnly },
        { label: t("editor.convertHalfWidth"), onSelect: () => runOnView(convertToHalfWidth), disabled: readOnly },
        { label: t("editor.convertFullWidth"), onSelect: () => runOnView(convertToFullWidth), disabled: readOnly },
        { label: t("editor.convertTabsToSpaces"), onSelect: () => runOnView(convertTabsToSpaces(resolvedSettings.tabWidth)), disabled: readOnly },
        { label: t("editor.convertSpacesToTabs"), onSelect: () => runOnView(convertSpacesToTabs(resolvedSettings.tabWidth)), disabled: readOnly },
      ],
    },
    { type: "separator" },
    { label: t("editor.toggleCheckbox"), onSelect: () => runOnView(cycleCheckbox), disabled: readOnly },
    { type: "separator" },
    { label: t("editor.searchGoogle"), onSelect: () => runOnView(searchSelectionWithGoogle) },
    { label: t("editor.openSearchReplace"), onSelect: () => search.open() },
    { type: "separator" },
    { label: showWhitespace ? t("editor.hideWhitespace") : t("editor.showWhitespace"), onSelect: () => setShowWhitespace((v) => !v) },
    { type: "separator" },
    {
      label: t("common.properties"),
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
    return <div className="ttn-editor-pane ttn-editor-empty">{t("editor.selectMemo")}</div>;
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
      {ed.loading && <div className="ttn-editor-loading">{t("common.loading")}</div>}

      {/* §4.2's DOM restructure: the two sub-panes are siblings inside this new wrapper, while the
          floating toolbar/status-bar/dialogs below stay direct children of `.ttn-editor-pane`
          itself (unchanged mount point — see the toolbar's own clamp-bounds rationale, §6.1). */}
      <div className="ttn-editor-panes-row">
        <div
          className="ttn-editor-cm-container"
          ref={containerRef}
          // Only `paneMode === "preview"` hides the edit pane — `"edit"`/`"split"` both keep it
          // visible (§4.4). The `EditorView` itself is never unmounted for this toggle.
          style={paneMode === "preview" ? { display: "none" } : undefined}
        />
        {paneMode !== "edit" && (
          <PreviewPane
            ref={setPreviewRef}
            getView={() => viewRef.current}
            renderTick={renderTick}
            debounceMs={appSettings?.preview.debounceMs ?? FALLBACK_PREVIEW_DEBOUNCE_MS}
            paneMode={paneMode}
            previewZoomPercent={previewZoomPercent}
            tabId={tabId}
            memoKey={memoKey ?? ""}
            readOnly={readOnly}
            openTrigger={appSettings?.links.openTrigger ?? "singleClick"}
            confirmBeforeOpen={appSettings?.links.confirmBeforeOpen ?? false}
            requestExternalConfirm={requestExternalConfirm}
            onOpenMemoLink={onOpenMemoLink}
          />
        )}
      </div>

      <MarkdownToolbar
        ref={toolbarRef}
        getView={() => viewRef.current}
        appSettings={appSettings}
        patchSettings={patchSettings}
        liveViewEnabled={liveViewEnabled}
        onToggleLiveView={handleToggleLiveView}
        paneMode={paneMode}
        onCyclePaneMode={onCyclePaneMode}
        revealSplitIfPreviewOnly={revealSplitIfPreviewOnly}
        previewLocked={previewLocked}
        onTogglePreviewLock={onTogglePreviewLock}
        previewZoomPercent={previewZoomPercent}
        onZoomIn={onZoomIn}
        onZoomOut={onZoomOut}
        onZoomReset={onZoomReset}
        readOnly={readOnly}
        tabId={tabId}
        onImageError={ed.reportError}
        onPdfExport={handlePdfExport}
        memoName={memoName}
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
            <span>{status.charCount.toLocaleString()} {t("editor.charCountSuffix")}</span>
            <span>{status.lineCount} {t("editor.lineCountSuffix")}</span>
            <span>{t("editor.cursorPositionTemplate").replace("{line}", String(status.line)).replace("{column}", String(status.column))}</span>
          </>
        )}
        {readOnly && <span className="ttn-editor-status-locked">{t("editor.editLockedStatus")}</span>}
      </div>

      {ed.lastErrorCode && (
        <div className="ttn-editor-error" role="alert">
          <span>{errorMessageFor(ed.lastErrorCode, t)}</span>
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
          title={t("editor.externalLinkTitle")}
          message={t("editor.externalLinkMessageTemplate").replace("{value}", pendingExternalLink.value)}
          confirmLabel={t("common.open")}
          onConfirm={() => settleExternalConfirm(true)}
          onCancel={() => settleExternalConfirm(false)}
        />
      )}

      {propertiesOpen && (
        <Dialog
          title={t("common.properties")}
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
                {t("properties.openLocationMnemonic")}
              </button>
              <button
                type="button"
                onClick={() => {
                  setPropertiesOpen(false);
                  setProperties(null);
                }}
              >
                {t("common.close")}
              </button>
            </>
          }
        >
          {properties ? (
            <>
              <section>
                <h3 className="ttn-properties-section-title">{t("properties.fileInfoSection")}</h3>
                <dl className="ttn-properties-list">
                  <dt>{t("properties.fileName")}</dt>
                  <dd>{properties.fileName}</dd>
                  <dt>{t("properties.filePath")}</dt>
                  <dd className="ttn-properties-file-path">{properties.filePath}</dd>
                  <dt>{t("properties.createdAt")}</dt>
                  <dd>{formatDateTime(properties.createdMs, t)}</dd>
                  <dt>{t("properties.modifiedAt")}</dt>
                  <dd>{formatDateTime(properties.modifiedMs, t)}</dd>
                  <dt>{t("properties.fileSize")}</dt>
                  <dd>{properties.fileSizeBytes.toLocaleString()} {t("properties.bytesSuffix")}</dd>
                </dl>
              </section>
              <section className="ttn-properties-section">
                <h3 className="ttn-properties-section-title">{t("properties.charCountSection")}</h3>
                <dl className="ttn-properties-list">
                  <dt>{t("properties.totalCharCount")}</dt>
                  <dd>{properties.charCount.toLocaleString()}</dd>
                  <dt>{t("properties.charCount")}</dt>
                  <dd>{properties.charCountExcludingWhitespace.toLocaleString()}{t("properties.excludingWhitespace")}</dd>
                  <dt>{t("properties.lineCount")}</dt>
                  <dd>{properties.lineCount.toLocaleString()}</dd>
                </dl>
              </section>
            </>
          ) : (
            <p>{t("common.loading")}</p>
          )}
        </Dialog>
      )}
    </div>
  );
});
