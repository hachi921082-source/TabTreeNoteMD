// Shared password-entry dialog built on the shared `Dialog` shell — used for Tab Lock's setup
// (lock a tab), unlock (open a locked tab / cross-tab drop-onto-a-locked-tab), and as the
// password-input UI reused by `RecoveryDialog.tsx`'s "resume interrupted lock" variant
// (`docs/plans/tab-lock/PLAN.md` §3, `docs/plans/tab-lock/CRASH-RECOVERY.md` "Frontend": "a
// dedicated 'Resume interrupted lock' dialog ... reusing the same password-input UI").
//
// `mode: "setup"` (locking a tab) additionally shows a confirm-password field and an optional
// hint field (`docs/plans/tab-lock/PLAN.md` §3 `tab_lock`: "Password (+ optional hint) is
// provided by the frontend ... entered in a setup dialog before invoking"). `mode: "unlock"` is a
// single password field, optionally showing a previously-set hint for reference.
import { useState } from "react";
import { Dialog } from "./Dialog";

export interface PasswordDialogProps {
  title: string;
  mode: "setup" | "unlock";
  /** Shown as reference text in `unlock` mode when the lock was created with one. */
  hint?: string | null;
  /** e.g. "パスワードが正しくありません" after a `DECRYPT_FAILED` response. Cleared by the
   * caller on the next submit attempt. */
  errorMessage?: string | null;
  /** Disables the submit button and shows a busy state while a request is in flight (scrypt
   * key derivation is deliberately slow, `docs/plans/tab-lock/PLAN.md` §2). */
  busy?: boolean;
  submitLabel?: string;
  onSubmit: (password: string, hint: string | undefined) => void;
  onCancel: () => void;
}

export function PasswordDialog({ title, mode, hint, errorMessage, busy, submitLabel, onSubmit, onCancel }: PasswordDialogProps) {
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [hintInput, setHintInput] = useState("");

  const isSetup = mode === "setup";
  const confirmMismatch = isSetup && confirmPassword.length > 0 && password !== confirmPassword;
  const canSubmit = password.length > 0 && !(isSetup && password !== confirmPassword) && !busy;

  const submit = () => {
    if (!canSubmit) return;
    onSubmit(password, isSetup ? (hintInput.trim() ? hintInput.trim() : undefined) : undefined);
  };

  return (
    <Dialog
      title={title}
      onClose={onCancel}
      footer={
        <>
          <button type="button" onClick={onCancel}>
            キャンセル
          </button>
          <button type="button" disabled={!canSubmit} onClick={submit}>
            {busy ? "処理中..." : (submitLabel ?? "OK")}
          </button>
        </>
      }
    >
      {!isSetup && hint && (
        <p className="ttn-dialog-hint-text">
          ヒント: {hint}
        </p>
      )}
      <label className="ttn-dialog-field">
        <span>パスワード</span>
        <input
          type="password"
          autoFocus
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !isSetup) {
              e.preventDefault();
              submit();
            }
          }}
        />
      </label>
      {isSetup && (
        <>
          <label className="ttn-dialog-field">
            <span>パスワード（確認）</span>
            <input
              type="password"
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  submit();
                }
              }}
            />
          </label>
          {confirmMismatch && <div className="ttn-dialog-error">パスワードが一致しません</div>}
          <label className="ttn-dialog-field">
            <span>ヒント（任意）</span>
            <input value={hintInput} onChange={(e) => setHintInput(e.target.value)} />
          </label>
        </>
      )}
      {errorMessage && <div className="ttn-dialog-error">{errorMessage}</div>}
    </Dialog>
  );
}
