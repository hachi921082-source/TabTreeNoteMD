// Shown in the main content area when zero tabs are registered
// (`docs/plans/tab-management/PLAN.md` §4 — two entry points, mirroring `TabBar.tsx`'s own split:
// "register an existing folder" (`tab_create`, OS picker) and "create a new, empty folder"
// (`tab_create_new`, name prompt), GitHub issue #2). Needs its own local dialog state for the
// name-prompt flow for the "create new" button, mirroring `TabBar.tsx`'s `PendingDialog`
// pattern but scoped down — this screen doesn't need rename/remove/lock, none of which are
// reachable with zero tabs registered. Any error not specially handled here still surfaces via `TabBar.tsx`'s own `ttn-tab-bar-error` banner, since
// `TabBar` stays mounted above this screen at all times (`App.tsx`) and shares the same `tabsState`
// — this screen doesn't need its own separate error banner.
import { useState } from "react";
import type { UseTabsResult } from "../../state/useTabs";
import { PromptDialog } from "../dialogs/PromptDialog";
import { validateNameStrict } from "../../shared/nameSanitize";
import { useT } from "../../i18n";
import "./EmptyState.css";

export interface EmptyStateProps {
  tabsState: UseTabsResult;
}

type PendingDialog = { kind: "createNew" };

export function EmptyState({ tabsState }: EmptyStateProps) {
  const t = useT();
  const { createTab, createNewTab } = tabsState;
  const [pendingDialog, setPendingDialog] = useState<PendingDialog | null>(null);

  return (
    <div className="ttn-empty-state">
      <p>{t("emptyState.noTabs")}</p>
      <button type="button" onClick={() => void createTab()}>
        📂 {t("tabBar.registerExisting")}
      </button>
      <button type="button" onClick={() => setPendingDialog({ kind: "createNew" })}>
        ＋ {t("tab.createNewTitle")}
      </button>

      {pendingDialog?.kind === "createNew" && (
        <PromptDialog
          title={t("tab.createNewTitle")}
          label={t("tab.createNewLabel")}
          submitLabel={t("common.create")}
          validate={(value) => validateNameStrict(value, t)}
          onCancel={() => setPendingDialog(null)}
          onSubmit={async (value) => {
            const result = await createNewTab(value);
            if (result.ok) setPendingDialog(null);
          }}
        />
      )}
    </div>
  );
}
