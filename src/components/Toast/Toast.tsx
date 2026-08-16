// Non-blocking, dismissible notification stack (`docs/plans/v1.1.0/folder-import/PLAN.md` §7) —
// the first use of this shared primitive is folder-import's skip-summary toast; also intended for
// reuse by future features (e.g. `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md`'s pre-timeout
// warning) needing a notification that is NOT a modal `Dialog` instance. Pure render component —
// state lives in `state/useToast.ts`.
import type { ToastItem } from "../../state/useToast";
import "./Toast.css";

export interface ToastProps {
  toasts: ToastItem[];
  onDismiss: (id: number) => void;
}

export function Toast({ toasts, onDismiss }: ToastProps) {
  if (toasts.length === 0) return null;
  return (
    <div className="ttn-toast-stack">
      {toasts.map((t) => (
        <div key={t.id} className={`ttn-toast ttn-toast-${t.kind}`} role="status">
          <span>{t.message}</span>
          <button type="button" onClick={() => onDismiss(t.id)} aria-label="閉じる">
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
