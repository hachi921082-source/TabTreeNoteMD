// Editor domain state: autosave debounce, every save-trigger flush point, Ctrl+S, and
// conflict handling (`docs/plans/editor-save/PLAN.md` §2, §3, §6). Owns no CodeMirror instance
// itself — `components/Editor/EditorPane.tsx` mounts the `EditorView` and feeds this hook's
// `content`/`loadGeneration` into it on (re)load, then reports every doc change back via
// `notifyTextChanged`.
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { editorClose, editorLoad, editorSave } from "../ipc/editor";
import type { AppSettings, MemoContent } from "../shared/models";
import type { ErrorCode } from "../shared/result";
import { comboFromEvent, resolveShortcuts } from "../shared/shortcuts";

/** `AppSettings.autosave.plainDebounceMs`'s factory default (`model/app_settings.rs`), used
 * until `appSettings` has loaded. */
const DEFAULT_AUTOSAVE_DEBOUNCE_MS = 1500;

export type ConflictKind = "save" | "external";

export interface UseEditorResult {
  loading: boolean;
  /** `null` when no memo is open. */
  content: MemoContent | null;
  /** Bumped every time `content.text` should be pushed into the CodeMirror doc wholesale
   * (initial load, memo switch, or a conflict-resolution reload) — `EditorPane` watches this
   * counter (not `content` itself) so ordinary typing (which only updates internal refs, not
   * `content`) never re-syncs the doc out from under the user. */
  loadGeneration: number;
  lastErrorCode: ErrorCode | null;
  clearLastError: () => void;
  /** `null` when no conflict is active. While non-null, further autosave attempts for this memo
   * are suspended (`[spec §5.8]`) and the editor should be made read-only. */
  conflict: ConflictKind | null;
  resolveConflictKeepMine: () => Promise<void>;
  resolveConflictReloadFromDisk: () => Promise<void>;
  /** Called by `EditorPane` on every CodeMirror doc change — the debounced-autosave timer's
   * source of truth for "did anything change." */
  notifyTextChanged: (text: string) => void;
  /** Ctrl+S / explicit flush; also used internally by the switch/minimize/exit triggers. */
  flushSaveNow: () => Promise<void>;
}

export function useEditor(tabId: string | null, memoName: string | null, appSettings: AppSettings | null): UseEditorResult {
  const [loading, setLoading] = useState(false);
  const [content, setContent] = useState<MemoContent | null>(null);
  const [loadGeneration, setLoadGeneration] = useState(0);
  const [lastErrorCode, setLastErrorCode] = useState<ErrorCode | null>(null);
  const [conflict, setConflict] = useState<ConflictKind | null>(null);

  // Refs mirror the state above so flush/save logic (called from effect cleanups, the debounce
  // timer, and window event listeners registered once) always sees the latest identity without
  // stale closures.
  const tabIdRef = useRef<string | null>(null);
  const memoNameRef = useRef<string | null>(null);
  const textRef = useRef<string>("");
  const lastSavedTextRef = useRef<string>("");
  const lastSavedHashRef = useRef<string>("");
  const editLockedRef = useRef(false);
  const conflictOpenRef = useRef(false);
  const debounceTimerRef = useRef<number | null>(null);
  const appSettingsRef = useRef<AppSettings | null>(appSettings);
  appSettingsRef.current = appSettings;

  const clearLastError = useCallback(() => setLastErrorCode(null), []);

  const clearDebounce = () => {
    if (debounceTimerRef.current !== null) {
      window.clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
  };

  /** "Don't write if no change" (`[spec §2.6]`) applies uniformly to every trigger, not just the
   * debounce timer: compares against the last-saved text before calling `editor_save` at all. */
  const flushSave = useCallback(async (tid: string, name: string) => {
    if (conflictOpenRef.current) return; // `[spec §5.8]`: suspended while the conflict dialog is open
    if (editLockedRef.current) return; // defensive: never attempted for an edit-locked memo
    if (textRef.current === lastSavedTextRef.current) return;

    const result = await editorSave(tid, name, textRef.current, lastSavedHashRef.current);
    if (result.ok) {
      lastSavedHashRef.current = result.value.hash;
      lastSavedTextRef.current = textRef.current;
      return;
    }
    if (result.error.code === "CONFLICT") {
      conflictOpenRef.current = true;
      setConflict("save");
      return;
    }
    setLastErrorCode(result.error.code);
  }, []);

  const flushSaveNow = useCallback(async () => {
    clearDebounce();
    const tid = tabIdRef.current;
    const name = memoNameRef.current;
    if (!tid || !name) return;
    await flushSave(tid, name);
  }, [flushSave]);

  const notifyTextChanged = useCallback(
    (text: string) => {
      textRef.current = text;
      clearDebounce();
      const tid = tabIdRef.current;
      const name = memoNameRef.current;
      if (!tid || !name) return;
      const debounceMs = appSettingsRef.current?.autosave.plainDebounceMs ?? DEFAULT_AUTOSAVE_DEBOUNCE_MS;
      debounceTimerRef.current = window.setTimeout(() => {
        void flushSave(tid, name);
      }, debounceMs);
    },
    [flushSave],
  );

  // Load on memo switch. The cleanup below flushes the memo being switched AWAY from
  // (`docs/plans/editor-save/PLAN.md` §6 "Memo switch"/"Tab switch" triggers) before the load
  // effect for the new one runs — React guarantees a changed-dependency effect's cleanup runs
  // before the next effect body, which gives us "flush old, then load new" for free.
  useEffect(() => {
    clearDebounce();
    setConflict(null);
    conflictOpenRef.current = false;

    if (!tabId || !memoName) {
      tabIdRef.current = null;
      memoNameRef.current = null;
      setContent(null);
      return;
    }

    let cancelled = false;
    setLoading(true);
    void (async () => {
      const result = await editorLoad(tabId, memoName);
      if (cancelled) return;
      if (result.ok) {
        tabIdRef.current = tabId;
        memoNameRef.current = memoName;
        textRef.current = result.value.text;
        lastSavedTextRef.current = result.value.text;
        lastSavedHashRef.current = result.value.hash;
        editLockedRef.current = result.value.editLocked;
        setContent(result.value);
        setLoadGeneration((g) => g + 1);
      } else {
        setLastErrorCode(result.error.code);
      }
      setLoading(false);
    })();

    return () => {
      cancelled = true;
      const tid = tabIdRef.current;
      const name = memoNameRef.current;
      if (tid && name) {
        void flushSave(tid, name);
        // Every switch-away is a real "memo closed" transition for the backend's
        // `ActiveMemoTracker` (`docs/plans/tab-lock/PLAN.md` §3 step 2's defensive check relies
        // on this) — harmless when this is actually a switch to a *different* memo, since that
        // memo's own `editor_load` re-sets the tracker right after this clears it.
        void editorClose(tid, name);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabId, memoName]);

  // Explicit save shortcut, default Ctrl+S (`docs/plans/editor-save/PLAN.md` §6;
  // `docs/plans/window-operation/PLAN.md` §3's rebindable-shortcut system, action id
  // "editor.save"): global, immediate, regardless of debounce timer state.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const shortcuts = resolveShortcuts(appSettingsRef.current?.shortcuts);
      if (comboFromEvent(e) === shortcuts["editor.save"]) {
        e.preventDefault();
        void flushSaveNow();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [flushSaveNow]);

  // App minimize / app exit (`docs/plans/editor-save/PLAN.md` §6). Registered once (not
  // per-memo). Tauri v2 has no dedicated "minimize" event, so `onResized` + `isMinimized()`
  // stands in for it, matching the trigger's intent (flush before the window leaves the user's
  // immediate attention).
  useEffect(() => {
    const appWindow = getCurrentWindow();
    let unlistenResized: (() => void) | undefined;
    let unlistenClose: (() => void) | undefined;
    let cancelled = false;

    void appWindow
      .onResized(() => {
        void appWindow.isMinimized().then((minimized) => {
          if (minimized) void flushSaveNow();
        });
      })
      .then((un) => {
        if (cancelled) un();
        else unlistenResized = un;
      });

    void appWindow
      .onCloseRequested(async (event) => {
        // Must be awaited before the process is allowed to terminate
        // (`docs/plans/editor-save/PLAN.md` §6 "App exit").
        event.preventDefault();
        await flushSaveNow();
        await appWindow.destroy();
      })
      .then((un) => {
        if (cancelled) un();
        else unlistenClose = un;
      });

    return () => {
      cancelled = true;
      unlistenResized?.();
      unlistenClose?.();
    };
  }, [flushSaveNow]);

  // `evt:editor:conflict` (`docs/plans/editor-save/PLAN.md` §3) — not yet emitted by anything
  // until `persistence-fs/PLAN.md`'s folder watcher is actually wired up to a running tab
  // folder; this listener is a forward seam, harmless while nothing fires the event.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen<{ tabId: string; memoName: string }>("evt:editor:conflict", (event) => {
      if (event.payload.tabId === tabIdRef.current && event.payload.memoName === memoNameRef.current) {
        conflictOpenRef.current = true;
        setConflict("external");
      }
    }).then((un) => {
      if (cancelled) un();
      else unlisten = un;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const resolveConflictKeepMine = useCallback(async () => {
    const tid = tabIdRef.current;
    const name = memoNameRef.current;
    if (!tid || !name) {
      conflictOpenRef.current = false;
      setConflict(null);
      return;
    }
    // "Keep my version (overwrite)": re-fetch the current on-disk hash (without touching the
    // buffer) so `editor_save`'s hash check passes, then save this session's text over it.
    const fresh = await editorLoad(tid, name);
    if (fresh.ok) {
      const result = await editorSave(tid, name, textRef.current, fresh.value.hash);
      if (result.ok) {
        lastSavedHashRef.current = result.value.hash;
        lastSavedTextRef.current = textRef.current;
      } else {
        setLastErrorCode(result.error.code);
      }
    } else {
      setLastErrorCode(fresh.error.code);
    }
    conflictOpenRef.current = false;
    setConflict(null);
  }, []);

  const resolveConflictReloadFromDisk = useCallback(async () => {
    const tid = tabIdRef.current;
    const name = memoNameRef.current;
    if (!tid || !name) {
      conflictOpenRef.current = false;
      setConflict(null);
      return;
    }
    const result = await editorLoad(tid, name);
    if (result.ok) {
      textRef.current = result.value.text;
      lastSavedTextRef.current = result.value.text;
      lastSavedHashRef.current = result.value.hash;
      editLockedRef.current = result.value.editLocked;
      setContent(result.value);
      setLoadGeneration((g) => g + 1);
    } else {
      setLastErrorCode(result.error.code);
    }
    conflictOpenRef.current = false;
    setConflict(null);
  }, []);

  return {
    loading,
    content,
    loadGeneration,
    lastErrorCode,
    clearLastError,
    conflict,
    resolveConflictKeepMine,
    resolveConflictReloadFromDisk,
    notifyTextChanged,
    flushSaveNow,
  };
}
