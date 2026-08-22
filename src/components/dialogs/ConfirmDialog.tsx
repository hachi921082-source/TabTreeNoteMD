// OK/Cancel confirmation dialog built on the shared `Dialog` shell — used by
// tab remove (`docs/plans/tab-management/PLAN.md` §2 `tab_remove`, "a
// confirm step per the app's general custom-dialog convention is still
// appropriate UX") and the permanent-unlock confirmation
// (`docs/plans/tab-lock-permanent-unlock/PLAN.md` §7.5, §7.8).
import type { ReactNode } from "react";
import { Dialog } from "./Dialog";
import { useT } from "../../i18n";

export interface ConfirmDialogProps {
  title: string;
  /** A plain `string` remains valid (it's a `ReactNode`); widened so callers can pass a
   * multi-paragraph fragment, e.g. the permanent-unlock irreversibility + backup-copy warning
   * (`tab-lock-permanent-unlock/PLAN.md` §7.5). */
  message: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ title, message, confirmLabel, cancelLabel, onConfirm, onCancel }: ConfirmDialogProps) {
  const t = useT();
  return (
    <Dialog
      title={title}
      onClose={onCancel}
      footer={
        <>
          <button type="button" onClick={onConfirm}>
            {confirmLabel ?? t("common.ok")}
          </button>
          <button type="button" onClick={onCancel}>
            {cancelLabel ?? t("common.cancel")}
          </button>
        </>
      }
    >
      {/* A plain `string` keeps rendering as a single `<p>` (unchanged markup/spacing for every
          pre-existing caller); a richer `ReactNode` (e.g. the permanent-unlock confirmation's
          two-`<p>` fragment) is expected to already supply its own block elements, so it renders
          as-is rather than being wrapped in an extra one. */}
      {typeof message === "string" ? <p>{message}</p> : message}
    </Dialog>
  );
}
