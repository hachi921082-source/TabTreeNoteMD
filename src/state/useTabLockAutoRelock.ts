// Single global inactivity timer, foreground-active-tab-only, auto-ends a Tab Lock session
// (`docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md`). Extends `state/useTabLock.ts`'s session
// lifecycle — introduces NO new backend command; `tabEndSession` (`tab-lock/PLAN.md` §3) is
// unchanged, this hook is purely the frontend timer/warning/pause-gate mechanics layered on top of
// it via the new `useTabLock.ts`'s `endSession` method.
//
// Deliberately does NOT cover backgrounded sessions (`PLAN.md` §2, `[spec v1.1.0 §5.1]`): the timer
// only exists while `activeTabId` itself currently holds a session (`hasSession` below) — a session
// on some OTHER tab (e.g. a cross-tab drop-onto-a-locked-tab that never became the active tab) never
// starts a timer, by construction, since this hook only ever reads `tabLockState.activeSessionRealNames`
// for `activeTabId`.
import { useCallback, useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppSettings } from "../shared/models";
import type { UseTabLockResult } from "./useTabLock";
import type { UseToastResult } from "./useToast";
import { isModalActive } from "../shared/modalActivityGate";
import { isFilePickerActive } from "../shared/filePickerGate";
import { useT } from "../i18n";

/** `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md` §3 (`[detailed-spec v1.1.0 §1.4]`) — fixed,
 * not configurable, not proportional to the configured timeout. Always shorter than the smallest
 * preset (5 min = 300s), so the warning window never needs clamping. */
const WARNING_LEAD_MS = 30_000;
const TICK_MS = 1_000;

export interface UseTabLockAutoRelockParams {
  activeTabId: string | null;
  tabLockState: UseTabLockResult;
  settings: AppSettings | null;
  toast: UseToastResult;
  /** Flushes and closes the currently-open memo if it belongs to `tabId` — reuses `App.tsx`'s
   * `beforeLockTab`, the exact same "flush pending edit, then close" routine `tab_lock` already
   * uses before locking a tab, per `PLAN.md` §7's "no new teardown logic." */
  flushAndCloseActiveMemo: (tabId: string) => Promise<void>;
}

/** No return value — this hook is pure side-effect wiring, mirroring `useWindowGeometry`/
 * `useAppearance`'s "wire up and return nothing" shape in `App.tsx`. */
export function useTabLockAutoRelock({ activeTabId, tabLockState, settings, toast, flushAndCloseActiveMemo }: UseTabLockAutoRelockParams): void {
  const hasSession = activeTabId !== null && tabLockState.activeSessionRealNames.has(activeTabId);
  const t = useT();

  // Read live on every tick/activity event rather than being effect dependencies — lets a
  // mid-session Settings change (or a toast/flush function identity change) take effect on the very
  // next tick without tearing down and restarting the timer (which would also wrongly reset
  // `elapsedRef` to 0, see the effect below). `tRef` follows the same pattern for the current-
  // language `t` (`docs/plans/v1.2.0/i18n/PLAN.md` §6) — a mid-session language change should not
  // tear down/restart the timer either.
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const toastRef = useRef(toast);
  toastRef.current = toast;
  const tRef = useRef(t);
  tRef.current = t;
  const flushAndCloseActiveMemoRef = useRef(flushAndCloseActiveMemo);
  flushAndCloseActiveMemoRef.current = flushAndCloseActiveMemo;
  const tabLockStateRef = useRef(tabLockState);
  tabLockStateRef.current = tabLockState;

  // Accumulated elapsed-ms-since-last-activity, milliseconds. Pausing (`isModalActive()`) simply
  // stops this from incrementing rather than resetting it — "pauses... does not reset... resumes
  // counting from where it left off" (`PLAN.md` §5).
  const elapsedRef = useRef(0);
  const warningShownRef = useRef(false);
  const warningToastIdRef = useRef<number | null>(null);
  const firedRef = useRef(false);

  const dismissWarningToast = useCallback(() => {
    if (warningToastIdRef.current !== null) {
      toastRef.current.dismissToast(warningToastIdRef.current);
      warningToastIdRef.current = null;
    }
    warningShownRef.current = false;
  }, []);

  /** Shared by both triggers (§7): flush-then-`endSession`, reusing the exact same routine
   * `tab_lock`'s pre-lock step and ordinary "leaving the tab" already use — no new teardown logic.
   * `showToast` distinguishes the inactivity-timeout trigger (toast shown, Q5-3) from the
   * app-focus-loss trigger (silent, indistinguishable from any other ordinary session end). */
  const teardown = useCallback(
    async (tabId: string, showToast: boolean) => {
      await flushAndCloseActiveMemoRef.current(tabId);
      tabLockStateRef.current.endSession(tabId);
      dismissWarningToast();
      if (showToast) toastRef.current.pushToast(tRef.current("toast.autoRelocked"));
    },
    [dismissWarningToast],
  );

  // The inactivity timer itself: (re)starts the moment `activeTabId` begins holding a session
  // (`PLAN.md` §4 "Created"/"Destroyed"), including switching directly from one session tab to
  // another (both `activeTabId` and `hasSession` are dependencies, so that case tears down the old
  // timer and creates a fresh one, per §4's explicit callout). Ticks every second rather than using
  // a single `setTimeout` for the full duration, so pausing (`isModalActive()`) can simply skip an
  // increment instead of needing to cancel/reschedule a pending timeout.
  useEffect(() => {
    if (!hasSession || !activeTabId) return;
    const tabId = activeTabId;
    elapsedRef.current = 0;
    warningShownRef.current = false;
    warningToastIdRef.current = null;
    firedRef.current = false;

    const tick = () => {
      if (firedRef.current) return;
      const minutes = settingsRef.current?.tabLockAutoRelockMinutes;
      const enabled = typeof minutes === "number" && minutes > 0;
      if (!enabled) return; // disabled: never fires, no accumulation needed (`PLAN.md` §4)
      if (isModalActive()) return; // paused: does not accumulate, does not reset

      elapsedRef.current += TICK_MS;
      const timeoutMs = minutes * 60_000;
      const warnAtMs = timeoutMs - WARNING_LEAD_MS;

      if (elapsedRef.current >= timeoutMs) {
        firedRef.current = true;
        void teardown(tabId, true);
        return;
      }
      if (!warningShownRef.current && elapsedRef.current >= warnAtMs) {
        warningShownRef.current = true;
        // Sticky (`autoDismissMs: null`) — must stay visible for up to the full 30s lead time (or
        // longer while paused), not the shared `Toast` primitive's default 5s. Dismissed explicitly
        // by `dismissWarningToast` (activity-reset) or `teardown` (timeout fires).
        warningToastIdRef.current = toastRef.current.pushToast(tRef.current("toast.autoRelockWarning"), "warning", null);
      }
    };

    const interval = window.setInterval(tick, TICK_MS);
    return () => {
      window.clearInterval(interval);
      dismissWarningToast();
    };
  }, [activeTabId, hasSession, teardown, dismissWarningToast]);

  // Activity reset (`PLAN.md` §4, `[detailed-spec v1.1.0 §3.5]`): any keyboard/pointer input at the
  // app-window level while a session is active AND the timer is not currently paused. While paused,
  // input is ignored entirely rather than resetting anything — the pause/resume mechanism above is
  // what "remembers" the accumulated elapsed time across the paused interval.
  useEffect(() => {
    if (!hasSession) return;
    const handleActivity = () => {
      if (isModalActive()) return;
      elapsedRef.current = 0;
      if (warningShownRef.current) dismissWarningToast();
    };
    const events: (keyof WindowEventMap)[] = ["keydown", "mousedown", "mousemove", "wheel", "touchstart"];
    for (const evt of events) window.addEventListener(evt, handleActivity, { passive: true });
    return () => {
      for (const evt of events) window.removeEventListener(evt, handleActivity);
    };
  }, [hasSession, dismissWarningToast]);

  // App-window OS-focus-loss trigger (`PLAN.md` §7, `[spec v1.1.0 §5.2]`'s broadening of
  // `[spec §4.1 pt.5]`): independent of, and not gated by, the timer's pause/warning state or the
  // `tabLockAutoRelockMinutes` disabled setting — losing OS focus is treated as unconditionally
  // "leaving the tab," the same existing rule that has no disable option either.
  useEffect(() => {
    if (!hasSession || !activeTabId) return;
    const tabId = activeTabId;
    const appWindow = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void appWindow
      .onFocusChanged(({ payload: focused }) => {
        // 例外 (#274, `[spec v1.2.0 §11]`): アプリ自身の画像挿入ファイル選択ダイアログ表示中の
        // フォーカス喪失では再ロックしない。他のフォーカス喪失(エクスプローラーからの D&D 等)は従来どおり。
        if (!focused && !isFilePickerActive()) void teardown(tabId, false);
      })
      .then((un) => {
        if (cancelled) un();
        else unlisten = un;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [hasSession, activeTabId, teardown]);
}
