// Single-line text-input dialog built on the shared `Dialog` shell — used by
// new-tab creation, tab rename and tree item create/rename
// (`docs/plans/tab-management/PLAN.md` §4).
import { useState } from "react";
import { Dialog } from "./Dialog";
import type { NameValidationResult } from "../../shared/nameSanitize";
import { useT } from "../../i18n";

export interface PromptDialogProps {
  title: string;
  label: string;
  initialValue?: string;
  submitLabel?: string;
  validate?: (value: string) => NameValidationResult;
  onSubmit: (value: string) => void;
  onCancel: () => void;
}

export function PromptDialog({ title, label, initialValue, submitLabel, validate, onSubmit, onCancel }: PromptDialogProps) {
  const t = useT();
  const [value, setValue] = useState(initialValue ?? "");
  const validation = validate?.(value);
  const canSubmit = !validation || validation.valid;

  const submit = () => {
    if (canSubmit) onSubmit(value);
  };

  return (
    <Dialog
      title={title}
      onClose={onCancel}
      footer={
        <>
          <button type="button" onClick={onCancel}>
            {t("common.cancel")}
          </button>
          <button type="button" disabled={!canSubmit} onClick={submit}>
            {submitLabel ?? t("common.ok")}
          </button>
        </>
      }
    >
      <label className="ttn-dialog-field">
        <span>{label}</span>
        <input
          autoFocus
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
          }}
        />
      </label>
      {validation && !validation.valid && <div className="ttn-dialog-error">{validation.reason}</div>}
    </Dialog>
  );
}
