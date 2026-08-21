// Crash-mid-lock / crash-mid-permanent-unlock recovery dialog
// (`docs/plans/tab-lock/CRASH-RECOVERY.md` "Frontend",
// `docs/plans/tab-lock-permanent-unlock/PLAN.md` §7.6). Shown by `components/Tree/TreeView.tsx`
// when `tree_open` reports `recoveryPending: true` for the active tab — `recoveryKind` picks
// which direction's copy/labels this dialog uses via the `variant` prop below. Reuses the shared
// `Dialog` shell and the same password-input UI as `PasswordDialog.tsx`, with two named actions
// ("Finish" / "Restore") instead of a single OK button.
//
// Esc IS allowed to close this dialog (it is not one of the Esc-disabled exceptions,
// `[detailed-spec §6.5]`) — closing without choosing an action just leaves the marker in place;
// the tab stays unusable as a normal tab until recovery is completed on a later visit
// (`CRASH-RECOVERY.md` "Frontend").
import { useState } from "react";
import { Dialog } from "./Dialog";
import { useT } from "../../i18n";

/** Normalized shape both directions' differently-named/differently-valued status fields map onto
 * (`tab-lock-permanent-unlock/PLAN.md` §7.6) — `variant` only changes copy/labels. */
export interface RecoveryDialogProps {
  variant: "lock" | "permanentUnlock";
  /** `RecoveryStatus.renamedCount` (lock direction), or a `Classified` permanent-unlock status's
   * `restoredCount`. */
  doneCount: number;
  total: number;
  anomalous: boolean;
  anomalousCount: number;
  /** Normalized from either direction's own enum (`RecoveryDefaultAction`/
   * `PermanentUnlockRecoveryDefaultAction`). */
  defaultAction: "finish" | "restore";
  /** `true` (default) for the lock direction, which always needs a fresh password to derive its
   * key. The permanent-unlock direction can reach this "classified" stage with the key already
   * resolved (an active session, or a password already verified in an earlier stage,
   * `PermanentUnlockRecoveryDialog.tsx`) — pass `false` there so Finish/Restore aren't blocked on
   * typing a password that would just be ignored by the backend anyway. */
  passwordRequired?: boolean;
  errorMessage?: string | null;
  busy?: boolean;
  onFinish: (password: string) => void;
  onRestore: (password: string) => void;
  onCancel: () => void;
}

export function RecoveryDialog({
  variant,
  doneCount,
  total,
  anomalous,
  anomalousCount,
  defaultAction,
  passwordRequired = true,
  errorMessage,
  busy,
  onFinish,
  onRestore,
  onCancel,
}: RecoveryDialogProps) {
  const t = useT();
  const [password, setPassword] = useState("");
  const canSubmit = (!passwordRequired || password.length > 0) && !busy;
  // Per-variant copy, resolved via `useT()` at render time (`docs/plans/v1.2.0/i18n/PLAN.md` §6) —
  // was a module-level `COPY` constant with literal strings/template functions; moved inside the
  // component since translation lookups need `useT()`'s render-time context.
  const copy =
    variant === "lock"
      ? {
          title: t("recoveryDialog.lockTitle"),
          restoreLabel: t("recoveryDialog.lockRestore"),
          finishLabel: t("recoveryDialog.lockFinish"),
          summary: t("recoveryDialog.lockSummaryTemplate").replace("{done}", String(doneCount)).replace("{total}", String(total)),
          anomalousSummary: t("recoveryDialog.lockAnomalousSummaryTemplate").replace("{count}", String(anomalousCount)),
        }
      : {
          title: t("permanentUnlockRecovery.title"),
          restoreLabel: t("recoveryDialog.permanentUnlockRestore"),
          finishLabel: t("permanentUnlockRecovery.finish"),
          summary: t("recoveryDialog.permanentUnlockSummaryTemplate").replace("{done}", String(doneCount)).replace("{total}", String(total)),
          anomalousSummary: t("recoveryDialog.permanentUnlockAnomalousSummaryTemplate").replace("{count}", String(anomalousCount)),
        };

  // `CRASH-RECOVERY.md`'s anomalous (both-or-neither) case is "surfaced as a distinct error
  // state to the user rather than auto-resolved" — no Finish/Restore choice is offered for it,
  // since neither action's on-disk-evidence assumptions hold for an anomalous entry.
  if (anomalous) {
    return (
      <Dialog title={copy.title} footer={<button type="button" onClick={onCancel}>{t("common.close")}</button>}>
        <p className="ttn-recovery-summary">{copy.anomalousSummary}</p>
      </Dialog>
    );
  }

  return (
    <Dialog
      title={copy.title}
      onClose={onCancel}
      footer={
        <>
          <button type="button" onClick={onCancel}>
            {t("common.cancel")}
          </button>
          <button type="button" disabled={!canSubmit} autoFocus={defaultAction === "restore"} onClick={() => onRestore(password)}>
            {copy.restoreLabel}
          </button>
          <button type="button" disabled={!canSubmit} autoFocus={defaultAction === "finish"} onClick={() => onFinish(password)}>
            {busy ? t("common.processing") : copy.finishLabel}
          </button>
        </>
      }
    >
      <p className="ttn-recovery-summary">{copy.summary}</p>
      {passwordRequired && (
        <label className="ttn-dialog-field">
          <span>{t("passwordDialog.password")}</span>
          <input type="password" autoFocus value={password} onChange={(e) => setPassword(e.target.value)} />
        </label>
      )}
      {errorMessage && <div className="ttn-dialog-error">{errorMessage}</div>}
    </Dialog>
  );
}
