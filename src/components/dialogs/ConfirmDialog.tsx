// OK/Cancel confirmation dialog built on the shared `Dialog` shell — used by
// tab remove (`docs/plans/tab-management/PLAN.md` §2 `tab_remove`, "a
// confirm step per the app's general custom-dialog convention is still
// appropriate UX") and the permanent-unlock confirmation
// (`docs/plans/tab-lock-permanent-unlock/PLAN.md` §7.5, §7.8).
import { useEffect } from "react";
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

  // Issue #206 — Enter triggers the same action as clicking the confirm button, mirroring
  // Dialog.tsx's window-level Escape→onCancel handler. Scoped to ConfirmDialog specifically
  // (not the generic Dialog shell) since only this dialog has one well-defined "primary
  // confirm" action; other Dialog usages (e.g. Settings) do not.
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        onConfirm();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onConfirm]);

  return (
    <Dialog
      title={title}
      onClose={onCancel}
      footer={
        <>
          <button type="button" onClick={onCancel}>
            {cancelLabel ?? t("common.cancel")}
          </button>
          <button type="button" onClick={onConfirm}>
            {confirmLabel ?? t("common.ok")}
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
