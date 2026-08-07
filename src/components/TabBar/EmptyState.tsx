// Shown in the main content area when zero tabs are registered
// (`docs/plans/tab-management/PLAN.md` §4 — two entry points, mirroring `TabBar.tsx`'s own split:
// "register an existing folder" (`tab_create`, OS picker) and "create a new, empty folder"
// (`tab_create_new`, name prompt), GitHub issue #2). Needs its own local dialog state for the
// name-prompt + duplicate-name-retry flow for each button, mirroring `TabBar.tsx`'s `PendingDialog`
// pattern but scoped down — this screen doesn't need rename/remove/lock, none of which are
// reachable with zero tabs registered. Any error not specially handled here (i.e. anything other
// than `NAME_DUPLICATE`) still surfaces via `TabBar.tsx`'s own `ttn-tab-bar-error` banner, since
// `TabBar` stays mounted above this screen at all times (`App.tsx`) and shares the same `tabsState`
// — this screen doesn't need its own separate error banner.
import { useState } from "react";
import type { UseTabsResult } from "../../state/useTabs";
import { PromptDialog } from "../dialogs/PromptDialog";
import { validateNameStrict, suggestSanitized } from "../../shared/nameSanitize";
import "./EmptyState.css";

export interface EmptyStateProps {
  tabsState: UseTabsResult;
}

type PendingDialog = { kind: "createNew"; suggestedName?: string } | { kind: "duplicateName"; path: string; suggestedName: string };

function baseNameOf(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

export function EmptyState({ tabsState }: EmptyStateProps) {
  const { createTab, createNewTab, retryCreateWithName } = tabsState;
  const [pendingDialog, setPendingDialog] = useState<PendingDialog | null>(null);

  const handleRegisterExisting = async () => {
    const result = await createTab();
    // Same duplicate-name retry affordance `TabBar.tsx`'s own OS-picker button gives
    // (`tab-management/PLAN.md` §2 step 2) — a name collision here must not silently do nothing.
    if (result && !result.ok && result.code === "NAME_DUPLICATE" && result.path) {
      setPendingDialog({ kind: "duplicateName", path: result.path, suggestedName: suggestSanitized(baseNameOf(result.path)) });
    }
  };

  return (
    <div className="ttn-empty-state">
      <p>登録されているタブがありません</p>
      <button type="button" onClick={() => void handleRegisterExisting()}>
        📂 既存フォルダを登録
      </button>
      <button type="button" onClick={() => setPendingDialog({ kind: "createNew" })}>
        ＋ 新しいタブを作成
      </button>

      {pendingDialog?.kind === "createNew" && (
        <PromptDialog
          title="新しいタブを作成"
          label="タブ（フォルダ）名"
          initialValue={pendingDialog.suggestedName}
          submitLabel="作成"
          validate={validateNameStrict}
          onCancel={() => setPendingDialog(null)}
          onSubmit={async (value) => {
            const result = await createNewTab(value);
            if (result.ok) setPendingDialog(null);
            else if (result.code === "NAME_DUPLICATE") {
              setPendingDialog({ kind: "createNew", suggestedName: suggestSanitized(value) });
            }
          }}
        />
      )}

      {pendingDialog?.kind === "duplicateName" && (
        <PromptDialog
          title="タブ名の重複"
          label="別の名前を入力してください"
          initialValue={pendingDialog.suggestedName}
          submitLabel="作成"
          validate={validateNameStrict}
          onCancel={() => setPendingDialog(null)}
          onSubmit={async (value) => {
            const result = await retryCreateWithName(pendingDialog.path, value);
            if (result.ok) setPendingDialog(null);
            else if (result.code === "NAME_DUPLICATE") {
              setPendingDialog({ kind: "duplicateName", path: result.path ?? pendingDialog.path, suggestedName: value });
            }
          }}
        />
      )}
    </div>
  );
}
