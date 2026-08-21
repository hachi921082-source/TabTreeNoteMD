// Rename/Overwrite/Skip collision prompt for OS-folder -> tree-pane drag-and-drop import
// (`docs/plans/v1.1.0/folder-import/PLAN.md` §4). Shown once per colliding item (unless a
// batch-wide "apply to all" policy already took over, `state/useImport.ts`). Esc closes this
// dialog via `Dialog`'s own built-in handling, which `state/useImport.ts` interprets as "stop the
// whole batch" (`onCancel`), not merely "skip this one item".
import { useState } from "react";
import { Dialog } from "./Dialog";
import { useT } from "../../i18n";

export type ImportCollisionChoice = "rename" | "overwrite" | "skip";

export interface ImportCollisionDialogProps {
  /** The sanitized candidate name that collided with an existing memo in this tab. */
  sourceName: string;
  /** Preview of the name a "Rename" choice would actually produce (`shared/uniqueName.ts`). */
  renamePreview: string;
  /** `true` when the colliding tree node carries the per-memo Edit Lock — Overwrite is disabled,
   * only Rename/Skip are choosable (`[detailed-spec v1.1.0 §2.1]` step 3d). */
  overwriteDisabled: boolean;
  onChoose: (choice: ImportCollisionChoice, applyToAll: boolean) => void;
  /** Esc / backdrop click — stops the whole remaining batch, not just this one item. */
  onCancel: () => void;
}

export function ImportCollisionDialog({ sourceName, renamePreview, overwriteDisabled, onChoose, onCancel }: ImportCollisionDialogProps) {
  const t = useT();
  const [applyToAll, setApplyToAll] = useState(false);

  return (
    <Dialog
      title={t("importCollision.title")}
      onClose={onCancel}
      footer={
        <>
          <button type="button" onClick={() => onChoose("skip", applyToAll)}>
            {t("importCollision.skip")}
          </button>
          <button type="button" disabled={overwriteDisabled} onClick={() => onChoose("overwrite", applyToAll)}>
            {t("importCollision.overwrite")}
          </button>
          <button type="button" onClick={() => onChoose("rename", applyToAll)}>
            {t("importCollision.renameImport")}
          </button>
        </>
      }
    >
      <p>{t("importCollision.existsMessageTemplate").replace("{name}", sourceName)}</p>
      <p>{t("importCollision.renamePreviewMessageTemplate").replace("{name}", renamePreview)}</p>
      {overwriteDisabled && <p className="ttn-dialog-error">{t("importCollision.overwriteDisabledMessage")}</p>}
      <label className="ttn-dialog-field">
        <span>
          <input type="checkbox" checked={applyToAll} onChange={(e) => setApplyToAll(e.target.checked)} /> {t("importCollision.applyToAll")}
        </span>
      </label>
    </Dialog>
  );
}
