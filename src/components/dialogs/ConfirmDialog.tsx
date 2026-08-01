// OK/Cancel confirmation dialog built on the shared `Dialog` shell — used by
// tab remove (`docs/plans/tab-management/PLAN.md` §2 `tab_remove`, "a
// confirm step per the app's general custom-dialog convention is still
// appropriate UX").
import { Dialog } from "./Dialog";

export interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ title, message, confirmLabel, cancelLabel, onConfirm, onCancel }: ConfirmDialogProps) {
  return (
    <Dialog
      title={title}
      onClose={onCancel}
      footer={
        <>
          <button type="button" onClick={onCancel}>
            {cancelLabel ?? "キャンセル"}
          </button>
          <button type="button" onClick={onConfirm}>
            {confirmLabel ?? "OK"}
          </button>
        </>
      }
    >
      <p>{message}</p>
    </Dialog>
  );
}
