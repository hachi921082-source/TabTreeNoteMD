// In-memory non-blocking notification queue (`docs/plans/v1.1.0/folder-import/PLAN.md` §7) —
// backs `components/Toast/Toast.tsx`. No persistence: the queue is purely a per-session UI
// concern, reset on reload like every other transient UI state in this app. First introduced by
// folder-import's skip-summary toast; also intended for reuse by future features needing a
// notification that is NOT a modal `Dialog` instance (e.g.
// `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md`'s pre-timeout warning, which must not pause on
// a modal focus trap).
import { useCallback, useRef, useState } from "react";

export type ToastKind = "info" | "warning";

export interface ToastItem {
  id: number;
  message: string;
  kind: ToastKind;
}

const AUTO_DISMISS_MS = 5000;

export interface UseToastResult {
  toasts: ToastItem[];
  /** `autoDismissMs` defaults to `AUTO_DISMISS_MS` (5s); pass `null` for a "sticky" toast that only
   * ever goes away via an explicit `dismissToast` call — used by
   * `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md` §6's pre-timeout warning, which must stay
   * visible for as long as the timer keeps running (up to 30s, or indefinitely while paused), not a
   * fixed 5s. Returns the new toast's id so a caller that needs to dismiss it early (e.g. on
   * activity resetting the timer) can. */
  pushToast: (message: string, kind?: ToastKind, autoDismissMs?: number | null) => number;
  dismissToast: (id: number) => void;
}

export function useToast(): UseToastResult {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const nextId = useRef(0);

  const dismissToast = useCallback((id: number) => {
    setToasts((current) => current.filter((t) => t.id !== id));
  }, []);

  const pushToast = useCallback(
    (message: string, kind: ToastKind = "info", autoDismissMs: number | null = AUTO_DISMISS_MS): number => {
      const id = nextId.current++;
      setToasts((current) => [...current, { id, message, kind }]);
      if (autoDismissMs !== null) window.setTimeout(() => dismissToast(id), autoDismissMs);
      return id;
    },
    [dismissToast],
  );

  return { toasts, pushToast, dismissToast };
}
