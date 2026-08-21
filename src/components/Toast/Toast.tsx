// Non-blocking, dismissible notification stack (`docs/plans/v1.1.0/folder-import/PLAN.md` §7) —
// the first use of this shared primitive is folder-import's skip-summary toast; also intended for
// reuse by future features (e.g. `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md`'s pre-timeout
// warning) needing a notification that is NOT a modal `Dialog` instance. Pure render component —
// state lives in `state/useToast.ts`.
import type { ToastItem } from "../../state/useToast";
import { useT } from "../../i18n";
import "./Toast.css";

export interface ToastProps {
  toasts: ToastItem[];
  onDismiss: (id: number) => void;
}

export function Toast({ toasts, onDismiss }: ToastProps) {
  const t = useT();
  if (toasts.length === 0) return null;
  return (
    <div className="ttn-toast-stack">
      {toasts.map((toast) => (
        <div key={toast.id} className={`ttn-toast ttn-toast-${toast.kind}`} role="status">
          <span>{toast.message}</span>
          <button type="button" onClick={() => onDismiss(toast.id)} aria-label={t("common.close")}>
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
