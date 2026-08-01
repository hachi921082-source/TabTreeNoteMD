// Right-pane plain-text editor surface (`docs/plans/editor-save/PLAN.md`). Mounts one CodeMirror
// 6 `EditorView` per open memo — a fresh `EditorState`/history is created on every memo switch
// (`editor-save/PLAN.md` §1: "no cross-memo undo"), while settings/read-only/whitespace-visibility
// changes reconfigure the existing view's compartments in place. Owns: the CodeMirror mount, the
// status bar, the right-click context menu (Convert/Check/Search-with-Google/Properties, shared
// component per `[spec §9.1 item 2]`), the in-memo search panel, and the conflict-resolution
// dialog. The Markdown live-view decoration layer is `markdown-editing/PLAN.md`'s separate,
// not-yet-implemented concern — this component only renders the plain-text "source" surface.
import { useEffect, useMemo, useRef, useState } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { useEditor } from "../../state/useEditor";
import { useSearch } from "../../state/useSearch";
import { memoGetProperties } from "../../ipc/tree";
import type { AppSettings, EditorDefaults, MemoEditorSettings, MemoProperties } from "../../shared/models";
import { errorMessageFor } from "../../shared/errorMessages";
import { ContextMenu } from "../ContextMenu/ContextMenu";
import type { ContextMenuItem } from "../ContextMenu/ContextMenu";
import { Dialog } from "../dialogs/Dialog";
import { SearchPanel } from "../Search/SearchPanel";
import { ConflictDialog } from "./ConflictDialog";
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
  appSettings: AppSettings | null;
  memoEditorOverride: MemoEditorSettings | undefined;
  setEditorOverride: (path: string, editor: MemoEditorSettings | null) => Promise<void>;
}

export function EditorPane({ tabId, memoName, appSettings, memoEditorOverride, setEditorOverride }: EditorPaneProps) {
  const ed = useEditor(tabId, memoName, appSettings);
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

  const editorDefaults = appSettings?.editorDefaults ?? FALLBACK_EDITOR_DEFAULTS;
  const resolvedSettings = useMemo(() => resolveEditorSettings(memoEditorOverride, editorDefaults), [memoEditorOverride, editorDefaults]);
  const readOnly = Boolean(ed.content?.editLocked) || ed.conflict !== null;

  // Refs so the debounced scroll listener / view-teardown flush always see the latest identity
  // without stale closures (same pattern as `state/useEditor.ts`).
  const memoNameRef = useRef(memoName);
  memoNameRef.current = memoName;
  const overrideRef = useRef(memoEditorOverride);
  overrideRef.current = memoEditorOverride;
  const setEditorOverrideRef = useRef(setEditorOverride);
  setEditorOverrideRef.current = setEditorOverride;
  const scrollDebounceRef = useRef<number | null>(null);

  const persistScrollPosition = (line: number) => {
    const name = memoNameRef.current;
    if (!name) return;
    void setEditorOverrideRef.current(name, { ...overrideRef.current, scrollPosition: line });
  };

  // Fresh EditorState/history per memo (`editor-save/PLAN.md` §1: "no cross-memo undo") —
  // recreated whenever `loadGeneration` bumps (initial load, memo switch, or a conflict-
  // resolution "reload from disk"), not merely re-synced via a doc-replace on a persistent view.
  useEffect(() => {
    if (!containerRef.current) return;

    // Flush this memo's scroll position immediately when switching away, same trigger point as
    // the autosave flush (`editor-save/PLAN.md` §2 "Scroll Position Write Path").
    const outgoingView = viewRef.current;
    if (outgoingView) {
      if (scrollDebounceRef.current !== null) window.clearTimeout(scrollDebounceRef.current);
      persistScrollPosition(computeTopVisibleLine(outgoingView));
      outgoingView.destroy();
      viewRef.current = null;
    }

    if (!ed.content) {
      setRenderTick((n) => n + 1);
      return;
    }

    const state = EditorState.create({
      doc: ed.content.text,
      extensions: [
        buildPlainTextExtensions({
          settings: resolvedSettings,
          readOnly,
          showWhitespace,
          onDocChanged: (text) => ed.notifyTextChanged(text),
        }),
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

    const savedScrollLine = ed.content.scrollPosition;
    let raf = 0;
    if (savedScrollLine) {
      raf = requestAnimationFrame(() => scrollToTopVisibleLine(view, savedScrollLine));
    }
    const scrollHandler = () => {
      if (scrollDebounceRef.current !== null) window.clearTimeout(scrollDebounceRef.current);
      scrollDebounceRef.current = window.setTimeout(() => {
        persistScrollPosition(computeTopVisibleLine(view));
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

  // Destroy the view (and flush its scroll position) on unmount.
  useEffect(() => {
    return () => {
      const view = viewRef.current;
      if (view) {
        if (scrollDebounceRef.current !== null) window.clearTimeout(scrollDebounceRef.current);
        persistScrollPosition(computeTopVisibleLine(view));
        view.destroy();
        viewRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Ctrl+F opens the in-memo search panel (`editor-save/PLAN.md` §5).
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.ctrlKey && !e.altKey && (e.key === "f" || e.key === "F")) {
        e.preventDefault();
        search.open();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [search]);

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

      {propertiesOpen && (
        <Dialog
          title="プロパティ"
          onClose={() => {
            setPropertiesOpen(false);
            setProperties(null);
          }}
          footer={
            <button
              type="button"
              onClick={() => {
                setPropertiesOpen(false);
                setProperties(null);
              }}
            >
              閉じる
            </button>
          }
        >
          {properties ? (
            <dl className="ttn-properties-list">
              <dt>ファイルサイズ</dt>
              <dd>{properties.fileSizeBytes.toLocaleString()} バイト</dd>
              <dt>作成日時</dt>
              <dd>{properties.created}</dd>
              <dt>更新日時</dt>
              <dd>{properties.modified}</dd>
              <dt>文字数</dt>
              <dd>{properties.charCount.toLocaleString()}</dd>
            </dl>
          ) : (
            <p>読み込み中...</p>
          )}
        </Dialog>
      )}
    </div>
  );
}
