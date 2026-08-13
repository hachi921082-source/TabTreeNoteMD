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
import type { AppSettings, AppSettingsPatch, EditorDefaults, ExternalLinkKind, MemoEditorSettings, MemoProperties, PaneMode } from "../../shared/models";
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
import { PreviewPane } from "./PreviewPane";
import type { ToolbarActionId } from "../../editor/toolbarSlots";
import {
  buildMarkdownExtensions,
  liveViewCompartment,
  liveViewExtension,
  sourceImageMarkerCompartment,
  sourceImageMarkerViewExtension,
} from "../../editor/codeMirrorSetup";
import type { MarkdownDecorationConfig } from "../../editor/markdownDecorations";
import { imagePasteExtension } from "../../editor/imagePaste";
import { releaseMemoImages } from "../../editor/imageLoader";
import { exportMemoToPdf } from "../../editor/pdfExport";
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
const FALLBACK_EDITOR_DEFAULTS: EditorDefaults = { wrap: "window", tabWidth: 4, showLineNumbers: true, fontSize: 14, paneMode: "edit" };

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
  { tabId, memoName, focusEditorToken, appSettings, memoEditorOverride, isMemoEditLocked, setEditorOverride, patchSettings, onOpenMemoLink },
  ref,
) {
  const ed = useEditor(tabId, memoName, appSettings, isMemoEditLocked);
  useImperativeHandle(ref, () => ({ flushSaveNow: ed.flushSaveNow }));
  const search = useSearch();

  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
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
  const paneMode = paneModeOverride ?? resolvedPaneMode;
  const onCyclePaneMode = useCallback(() => {
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
  }, [resolvedPaneMode]);
  /** Reveal helper (§4.4's auto-reveal principle): forces pane-mode to `split` if (and only if) it
   * is currently `preview`, otherwise leaves it untouched (e.g. `edit` is never disturbed). Called
   * by every action that expects to focus/scroll/mutate the CodeMirror view while pane-mode is
   * `preview` — currently just the live/source toggle's own click handler (wired below); the other
   * auto-reveal call sites listed in the plan (Ctrl+F, tree-row click, cross-tab search jump,
   * `memo:` link navigation, toolbar formatting buttons) already only ever act on the CodeMirror
   * view, which stays fully mounted (just `display:none`) even while pane-mode is `preview`, per
   * §4.2 — no separate reveal call was needed at those other sites for this initial cut. */
  const revealSplitIfPreviewOnly = useCallback(() => {
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
  }, [resolvedPaneMode]);
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
    }),
    [appSettings, readOnly, tabId, onOpenMemoLink, requestExternalConfirm, memoKey],
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
    // Reset the session-only toggles for the newly-opened memo (harmless if they already equalled
    // this memo's own governed values — the reconfigure/render below just reflects the same value
    // again in that case). A freshly-opened memo always starts from its governed pane-mode, never a
    // stale session toggle left over from the previously-open memo (`PLAN.md` §4.1). Done BEFORE
    // the auto-reveal call just below, since both ultimately queue updates to the same
    // `paneModeOverride` state — resetting first, then revealing, ensures the reveal's own update
    // (if any) is the one that wins, not clobbered by this reset running after it.
    setLiveViewOverride(null);
    setPaneModeOverride(null);
    if (focusEditorTokenRef.current !== consumedFocusTokenRef.current) {
      consumedFocusTokenRef.current = focusEditorTokenRef.current;
      view.focus();
      // Auto-reveal (`docs/plans/markdown/markdown-preview/PLAN.md` §4.4): an explicit navigation
      // to this memo (tree-row click, cross-tab search-result jump, `memo:` link-click navigation
      // — every one of these bumps `focusEditorToken`, unlike arrow-key nav) reveals the edit pane
      // if this memo's own governed pane-mode is `"preview"`, so the just-focused view is visible.
      revealSplitIfPreviewOnly();
    }

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

  // Source-view image marker (`docs/plans/markdown-editing/PLAN.md`, GitHub issue #81) — opposite
  // polarity from the live-view toggle above (on while `liveViewEnabled` is false), reconfigured
  // via its own compartment since it has no `markdownConfig` dependency at all.
  useEffect(() => {
    viewRef.current?.dispatch({ effects: sourceImageMarkerCompartment.reconfigure(sourceImageMarkerViewExtension(!liveViewEnabled)) });
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
      // `markdown.paneModeCycle` is deliberately excluded here — it gets its own separate,
      // focus-unconstrained effect below (`docs/plans/markdown/markdown-preview/PLAN.md` §6.3),
      // since (unlike every other `markdown.*` action) it must fire even while focus is outside
      // this editor's own CodeMirror content (e.g. inside the preview pane's own DOM subtree).
      if (actionId.startsWith("markdown.") && actionId !== "markdown.paneModeCycle") reverse.set(combo, actionId);
    }
    const handler = (e: KeyboardEvent) => {
      if (e.defaultPrevented || !viewRef.current) return;
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
  // focus-UNconstrained effect (unlike the 16-grid-action dispatcher above, which only fires while
  // focus is inside `.ttn-editor-cm-container`) — this shortcut must work regardless of focus
  // location within the editor pane, including while focus is inside the preview pane's own DOM
  // subtree. Mirrors the also-unconstrained Ctrl+F handler's pattern above. Not part of
  // `ToolbarActionId`/`toolbarSlots.ts`'s 16-slot registry, so it can't go through
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
            getView={() => viewRef.current}
            renderTick={renderTick}
            debounceMs={appSettings?.preview.debounceMs ?? FALLBACK_PREVIEW_DEBOUNCE_MS}
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
