// Crash-mid-lock recovery dialog (`docs/plans/tab-lock/CRASH-RECOVERY.md` "Frontend"). Shown by
// `components/Tree/TreeView.tsx` when `tree_open` reports `recoveryPending: true` for the active
// tab. Reuses the shared `Dialog` shell and the same password-input UI as `PasswordDialog.tsx`,
// with two named actions ("Finish Locking" / "Restore") instead of a single OK button.
//
// Esc IS allowed to close this dialog (it is not one of the Esc-disabled exceptions,
// `[detailed-spec §6.5]`) — closing without choosing an action just leaves the marker in place;
// the tab stays unusable as a normal tab until recovery is completed on a later visit
// (`CRASH-RECOVERY.md` "Frontend").
import { useState } from "react";
import { Dialog } from "./Dialog";
import type { RecoveryStatus } from "../../shared/models";

export interface RecoveryDialogProps {
  status: RecoveryStatus;
  errorMessage?: string | null;
  busy?: boolean;
  onFinish: (password: string) => void;
  onRestore: (password: string) => void;
  onCancel: () => void;
}

export function RecoveryDialog({ status, errorMessage, busy, onFinish, onRestore, onCancel }: RecoveryDialogProps) {
  const [password, setPassword] = useState("");
  const canSubmit = password.length > 0 && !busy;

  // `CRASH-RECOVERY.md`'s anomalous (both-or-neither) case is "surfaced as a distinct error
  // state to the user rather than auto-resolved" — no Finish/Restore choice is offered for it,
  // since neither action's on-disk-evidence assumptions hold for an anomalous entry.
  if (status.anomalous) {
    return (
      <Dialog title="ロック処理の復旧" footer={<button type="button" onClick={onCancel}>閉じる</button>}>
        <p className="ttn-recovery-summary">
          前回のロック処理が中断され、想定外の状態のファイルが見つかりました（
          {status.anomalousCount} 件）。手動での確認が必要です。
        </p>
      </Dialog>
    );
  }

  return (
    <Dialog
      title="ロック処理の復旧"
      onClose={onCancel}
      footer={
        <>
          <button type="button" onClick={onCancel}>
            キャンセル
          </button>
          <button type="button" disabled={!canSubmit} autoFocus={status.defaultAction === "restore"} onClick={() => onRestore(password)}>
            元に戻す（ロックを取り消す）
          </button>
          <button type="button" disabled={!canSubmit} autoFocus={status.defaultAction === "finish"} onClick={() => onFinish(password)}>
            {busy ? "処理中..." : "ロックを完了する"}
          </button>
        </>
      }
    >
      <p className="ttn-recovery-summary">
        前回のこのタブのロック処理が完了しませんでした。パスワードを入力して、ロックを完了するか、元に戻すかを選択してください。
        （完了: {status.renamedCount} / {status.total} 件）
      </p>
      <label className="ttn-dialog-field">
        <span>パスワード</span>
        <input type="password" autoFocus value={password} onChange={(e) => setPassword(e.target.value)} />
      </label>
      {errorMessage && <div className="ttn-dialog-error">{errorMessage}</div>}
    </Dialog>
  );
}
