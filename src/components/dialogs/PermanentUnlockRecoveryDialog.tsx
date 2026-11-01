// Crash-mid-permanent-unlock recovery dialog
// (`docs/plans/tab-lock-permanent-unlock/PLAN.md` §7.7,
// `docs/plans/tab-lock-permanent-unlock/CRASH-RECOVERY.md` §4). Shown by
// `components/Tree/TreeView.tsx` when `tree_open` reports `recoveryPending: true` /
// `recoveryKind: "permanentUnlock"` for the active tab.
//
// Thin staging wrapper around the shared `RecoveryDialog`, handling the extra classification
// stage `PermanentUnlockRecoveryStatus` has that the lock direction's own `RecoveryStatus` does
// not (`CRASH-RECOVERY.md` §4's "no password needed to classify" asymmetry — the still-present
// `lock` field can sometimes be classified via an already-active session with no password at
// all): `alreadyFinished` (no password, only "Finish" makes sense) -> `passwordRequired` (no key
// available yet) -> `classified` (delegates to `RecoveryDialog`).
import { useEffect, useState } from "react";
import { Dialog } from "./Dialog";
import { RecoveryDialog } from "./RecoveryDialog";
import { errorMessageFor } from "../../shared/errorMessages";
import type { UseTabLockResult } from "../../state/useTabLock";
import type { PermanentUnlockRecoveryStatus } from "../../shared/models";
import { useT } from "../../i18n";

export interface PermanentUnlockRecoveryDialogProps {
  tabId: string;
  tabLock: UseTabLockResult;
}

export function PermanentUnlockRecoveryDialog({ tabId, tabLock }: PermanentUnlockRecoveryDialogProps) {
  const t = useT();
  const [status, setStatus] = useState<PermanentUnlockRecoveryStatus | null>(null);
  /** The password verified during the `passwordRequired` stage, if this session ever went
   * through one — cached so the final `classified` stage's Finish/Restore calls reuse it instead
   * of re-deriving the key from a fresh (possibly different) input. */
  const [verifiedPassword, setVerifiedPassword] = useState("");
  const [passwordInput, setPasswordInput] = useState("");

  // On mount (and whenever the recovering tab changes), probe for a cached session key or the
  // deterministic `alreadyFinished` case — no password prompt shown yet.
  useEffect(() => {
    setStatus(null);
    setVerifiedPassword("");
    setPasswordInput("");
    let cancelled = false;
    void tabLock.fetchPermanentUnlockRecoveryStatus(tabId, undefined).then((next) => {
      if (!cancelled) setStatus(next);
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabId]);

  const errorMessage = tabLock.lastErrorCode ? errorMessageFor(tabLock.lastErrorCode, t) : null;

  if (!status) return null;

  if (status.kind === "alreadyFinished") {
    return (
      <Dialog
        title={t("permanentUnlockRecovery.title")}
        onClose={() => tabLock.clearLastError()}
        footer={
          <button type="button" disabled={tabLock.busy} onClick={() => void tabLock.finishPermanentUnlockRecovery(tabId, undefined)}>
            {tabLock.busy ? t("common.processing") : t("permanentUnlockRecovery.finish")}
          </button>
        }
      >
        <p className="ttn-recovery-summary">{t("permanentUnlockRecovery.alreadyFinishedMessage")}</p>
        {errorMessage && <div className="ttn-dialog-error">{errorMessage}</div>}
      </Dialog>
    );
  }

  if (status.kind === "passwordRequired") {
    return (
      <Dialog
        title={t("permanentUnlockRecovery.title")}
        onClose={() => tabLock.clearLastError()}
        footer={
          <button
            type="button"
            disabled={tabLock.busy || passwordInput.length === 0}
            onClick={() => {
              tabLock.clearLastError();
              const password = passwordInput;
              void tabLock.fetchPermanentUnlockRecoveryStatus(tabId, password).then((next) => {
                if (!next) return;
                setVerifiedPassword(password);
                setStatus(next);
              });
            }}
          >
            {tabLock.busy ? t("common.processing") : t("permanentUnlockRecovery.continue")}
          </button>
        }
      >
        <p className="ttn-recovery-summary">{t("permanentUnlockRecovery.passwordRequiredMessage")}</p>
        <label className="ttn-dialog-field">
          <span>{t("passwordDialog.password")}</span>
          <input type="password" autoFocus value={passwordInput} onChange={(e) => setPasswordInput(e.target.value)} />
        </label>
        {errorMessage && <div className="ttn-dialog-error">{errorMessage}</div>}
      </Dialog>
    );
  }

  // status.kind === "classified": the key is already resolved by this point (either an active
  // session, or a password already verified during the `passwordRequired` stage above) — no
  // fresh password needs to be typed into `RecoveryDialog`'s own field to unblock its buttons.
  return (
    <RecoveryDialog
      variant="permanentUnlock"
      doneCount={status.restoredCount}
      total={status.total}
      anomalous={status.anomalous}
      anomalousCount={status.anomalousCount}
      defaultAction={status.defaultAction === "finishUnlocking" ? "finish" : "restore"}
      passwordRequired={false}
      busy={tabLock.busy}
      errorMessage={errorMessage}
      onCancel={() => tabLock.clearLastError()}
      // The cached, already-verified password (if any) takes priority over whatever this
      // dialog's own password field happens to hold — relevant when this stage was reached
      // directly via an active session (no `passwordRequired` stage ever ran).
      onFinish={(formPassword) => void tabLock.finishPermanentUnlockRecovery(tabId, verifiedPassword || formPassword)}
      onRestore={(formPassword) => void tabLock.restorePermanentUnlockRecovery(tabId, verifiedPassword || formPassword)}
    />
  );
}
