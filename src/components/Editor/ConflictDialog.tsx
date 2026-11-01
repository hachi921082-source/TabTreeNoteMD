// External-change conflict resolution dialog (`docs/plans/editor-save/PLAN.md` §3). Built on the
// shared `Dialog` shell with `onClose` intentionally omitted — the one named Esc-disabled
// exception in `[detailed-spec §6.5]`, so a user cannot accidentally dismiss an unresolved
// on-disk conflict without making an explicit choice.
import { Dialog } from "../dialogs/Dialog";
import type { ConflictKind } from "../../state/useEditor";
import { useT } from "../../i18n";

export interface ConflictDialogProps {
  kind: ConflictKind;
  onKeepMine: () => void;
  onReloadFromDisk: () => void;
}

export function ConflictDialog({ kind, onKeepMine, onReloadFromDisk }: ConflictDialogProps) {
  const t = useT();
  const message = kind === "save" ? t("conflict.saveMessage") : t("conflict.openMessage");

  return (
    <Dialog
      title={t("errors.CONFLICT")}
      footer={
        <>
          <button type="button" onClick={onReloadFromDisk}>
            {t("conflict.reloadFromDisk")}
          </button>
          <button type="button" onClick={onKeepMine}>
            {t("conflict.keepMine")}
          </button>
        </>
      }
    >
      <p>{message}</p>
      <p>{t("conflict.chooseMessage")}</p>
    </Dialog>
  );
}
