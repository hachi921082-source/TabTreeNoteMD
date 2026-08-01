// External-change conflict resolution dialog (`docs/plans/editor-save/PLAN.md` §3). Built on the
// shared `Dialog` shell with `onClose` intentionally omitted — the one named Esc-disabled
// exception in `[detailed-spec §6.5]`, so a user cannot accidentally dismiss an unresolved
// on-disk conflict without making an explicit choice.
import { Dialog } from "../dialogs/Dialog";
import type { ConflictKind } from "../../state/useEditor";

export interface ConflictDialogProps {
  kind: ConflictKind;
  onKeepMine: () => void;
  onReloadFromDisk: () => void;
}

export function ConflictDialog({ kind, onKeepMine, onReloadFromDisk }: ConflictDialogProps) {
  const message =
    kind === "save"
      ? "このメモは保存しようとした時点で、外部で内容が変更されていました。"
      : "このメモは開いている間に、外部で内容が変更されました。";

  return (
    <Dialog
      title="外部でファイルが変更されています"
      footer={
        <>
          <button type="button" onClick={onReloadFromDisk}>
            ディスクから再読み込み（自分の変更を破棄）
          </button>
          <button type="button" onClick={onKeepMine}>
            自分の内容を保持して上書き
          </button>
        </>
      }
    >
      <p>{message}</p>
      <p>どちらの内容を残すか選択してください。</p>
    </Dialog>
  );
}
