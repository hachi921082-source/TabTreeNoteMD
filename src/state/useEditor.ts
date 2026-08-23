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
  /** Surfaces an error via the same non-blocking `lastErrorCode` banner as save/load failures —
   * used by callers outside this hook's own save/load flow (currently only
   * `components/Editor/EditorPane.tsx`'s image-paste/insert wiring,
   * `docs/plans/image-handling/PLAN.md` §6 step 4) rather than each such caller needing its own
   * notification mechanism. */
  reportError: (code: ErrorCode) => void;
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

export function useEditor(
  tabId: string | null,
  memoName: string | null,
  appSettings: AppSettings | null,
  /** Live Edit Lock state for the open memo (`EditorPane.tsx`'s `isMemoEditLocked`, sourced from
   * live tree state, issue #12/#109) — kept in sync into `editLockedRef` by a dedicated effect
   * below, replacing the load-time-snapshot `content.editLocked` this ref used to mirror (GitHub
   * issue #110: that snapshot never refreshed after an in-session unlock, so `flushSave` kept
   * silently dropping saves for a window after unlocking, same staleness class #109 fixed for
   * `readOnly`). */
  isMemoEditLocked: boolean,
): UseEditorResult {
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
  // Live mirror of the `memoName` param (GitHub issue #212 follow-up) -- distinct from
  // `memoNameRef` above, which tracks the memo this hook's internal state/refs are actually
  // LOADED for and is only updated inside the reload effect below. This one is updated on every
  // render, unconditionally, so the reload effect's `!memoName` guard (see its own comment) and
  // `resolveConflictKeepMine`/`resolveConflictReloadFromDisk` can both tell whether the live prop
  // has since diverged from what they're holding onto.
  const memoNamePropRef = useRef<string | null>(memoName);
  memoNamePropRef.current = memoName;

  // Kept live rather than only set from `editor_load`'s load-time snapshot (GitHub issue #110) —
  // see the constructor param's own doc comment above for the staleness bug this replaces.
  useEffect(() => {
    editLockedRef.current = isMemoEditLocked;
  }, [isMemoEditLocked]);

  const clearLastError = useCallback(() => setLastErrorCode(null), []);
  const reportError = useCallback((code: ErrorCode) => setLastErrorCode(code), []);

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

    // GitHub issue #212 follow-up (live-CDP-verified): an external delete of the currently-open
    // memo fires BOTH `evt:fs:external-change` (drives `useTree.ts`'s tree refresh, which then
    // force-clears its `openMemoName` -- see that file's "open memo no longer exists" cleanup
    // effect) and `evt:editor:conflict` (sets `conflict` below) for the same event. Without this
    // guard, `memoName` going `null` for that reason re-runs this effect and the unconditional
    // `setConflict(null)` a few lines down wipes the just-raised conflict before the user ever
    // sees a chance to click a resolution button -- confirmed by live reproduction: with only
    // `EditorPane.tsx`'s render-gating fixed (keeping `ConflictDialog` mounted while
    // `conflict !== null`), the dialog still vanished in the same frame `memoName` went `null`,
    // because THIS effect cleared `conflict` in that very same render pass before the browser
    // ever painted the intermediate state. Only skips for the EXACT tab/memo the active conflict
    // is for (`tabId === tabIdRef.current`) -- an actual tab switch or a genuinely different
    // memo still falls through to the normal reset below. `resolveConflictKeepMine`/
    // `resolveConflictReloadFromDisk` below already read `tabIdRef.current`/`memoNameRef.current`
    // (not the live params), so they keep working correctly while this guard holds those refs in
    // place, and each finishes the deferred reset once the user actually resolves the conflict.
    if (conflictOpenRef.current && !memoName && tabId === tabIdRef.current) {
      return;
    }

    setConflict(null);
    conflictOpenRef.current = false;
    // GitHub issue #212 follow-up: a resolve-while-`memoName`-is-null failure (deleted-file
    // conflict resolution hitting a NOT_FOUND) sets `lastErrorCode` while `tabId`/`memoName` are
    // unchanged from this effect's point of view (still `T`/`null`), so this effect does NOT
    // re-run at that moment -- the banner set by the resolve handler survives untouched, giving
    // the user a chance to actually see it (`EditorPane.tsx`'s `!memoName` branches now render
    // it). Only once the user genuinely moves on -- a real tab switch or opening a different memo,
    // either of which changes `tabId`/`memoName` and re-runs this effect past the guard above --
    // does this line clear the stale code, before it can leak onto that new, unrelated memo/tab.
    setLastErrorCode(null);

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

  // `evt:editor:conflict` (`docs/plans/editor-save/PLAN.md` §3), emitted by the folder watcher
  // (GitHub issue #211) whenever the currently-open memo changes on disk out from under this tab.
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
    // GitHub issue #212 follow-up: the reload effect's guard above may have deferred resetting
    // `tabIdRef`/`memoNameRef` (external delete raced the tree's own cleanup) -- finish that reset
    // now if the live `memoName` prop has since moved on, so a later, unrelated memo open doesn't
    // inherit this dead memo's identity.
    if (memoNamePropRef.current !== name) {
      tabIdRef.current = null;
      memoNameRef.current = null;
    }
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
      setContent(result.value);
      setLoadGeneration((g) => g + 1);
    } else {
      setLastErrorCode(result.error.code);
    }
    conflictOpenRef.current = false;
    setConflict(null);
    // GitHub issue #212 follow-up: same deferred-reset finish as `resolveConflictKeepMine` above.
    if (memoNamePropRef.current !== name) {
      tabIdRef.current = null;
      memoNameRef.current = null;
    }
  }, []);

  return {
    loading,
    content,
    loadGeneration,
    lastErrorCode,
    clearLastError,
    reportError,
    conflict,
    resolveConflictKeepMine,
    resolveConflictReloadFromDisk,
    notifyTextChanged,
    flushSaveNow,
  };
}
