// Rename/Overwrite/Skip collision prompt for OS-folder -> tree-pane drag-and-drop import
// (`docs/plans/v1.1.0/folder-import/PLAN.md` §4). Shown once per colliding item (unless a
// batch-wide "apply to all" policy already took over, `state/useImport.ts`). Esc closes this
// dialog via `Dialog`'s own built-in handling, which `state/useImport.ts` interprets as "stop the
// whole batch" (`onCancel`), not merely "skip this one item".
import { useState } from "react";
import { Dialog } from "./Dialog";

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
  const [applyToAll, setApplyToAll] = useState(false);

  return (
    <Dialog
      title="インポート時の名前の重複"
      onClose={onCancel}
      footer={
        <>
          <button type="button" onClick={() => onChoose("skip", applyToAll)}>
            スキップ
          </button>
          <button type="button" disabled={overwriteDisabled} onClick={() => onChoose("overwrite", applyToAll)}>
            上書き
          </button>
          <button type="button" onClick={() => onChoose("rename", applyToAll)}>
            名前を変更してインポート
          </button>
        </>
      }
    >
      <p>「{sourceName}」という名前のメモは既にこのタブに存在します。</p>
      <p>名前を変更する場合、「{renamePreview}」としてインポートします。</p>
      {overwriteDisabled && <p className="ttn-dialog-error">このメモは編集ロック中のため上書きできません。</p>}
      <label className="ttn-dialog-field">
        <span>
          <input type="checkbox" checked={applyToAll} onChange={(e) => setApplyToAll(e.target.checked)} /> 残りの重複項目すべてにこの選択を適用する
        </span>
      </label>
    </Dialog>
  );
}
