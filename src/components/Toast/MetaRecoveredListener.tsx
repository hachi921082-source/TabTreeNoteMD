// GitHub issue #318: 管理用ファイル(settings.json / .tabtreenote.json)の自動復旧を、トースト
// (`errors.META_CORRUPT`)で知らせる。`I18nProvider` の内側に置く。起動時の復旧は listener 登録前に
// 起きてイベントが失われるため、登録後に保留中の通知を `meta_take_pending_recovery` で取りに行く。
import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../../i18n";
import { errorMessageFor } from "../../shared/errorMessages";
import type { Result } from "../../shared/result";
import type { UseToastResult } from "../../state/useToast";

async function takePending(): Promise<boolean> {
  try {
    const r = await invoke<Result<boolean>>("meta_take_pending_recovery");
    return r.ok && r.value;
  } catch {
    return false;
  }
}

export function MetaRecoveredListener({ toast }: { toast: UseToastResult }) {
  const t = useT();
  const { pushToast } = toast;
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen("evt:meta:recovered", () => {
      pushToast(errorMessageFor("META_CORRUPT", t), "warning");
      // 同じ復旧を後続の取得で二重に通知しないよう、保留を解除する。
      void takePending();
    })
      .then(async (un) => {
        if (cancelled) {
          un();
          return;
        }
        unlisten = un;
        if (await takePending()) pushToast(errorMessageFor("META_CORRUPT", t), "warning");
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [pushToast, t]);
  return null;
}
