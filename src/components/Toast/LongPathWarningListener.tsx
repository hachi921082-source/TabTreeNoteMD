// GitHub issue #319: バックエンドの `evt:path:long-warning`(操作の結果できたパスが 260 文字超。仕様 §5.11 の
// 「警告のみ、操作は止めない」)を受けて、非ブロッキングのトーストを出す。`I18nProvider` の内側に置く。
import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useT } from "../../i18n";
import { errorMessageFor } from "../../shared/errorMessages";
import type { UseToastResult } from "../../state/useToast";

export function LongPathWarningListener({ toast }: { toast: UseToastResult }) {
  const t = useT();
  const { pushToast } = toast;
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen("evt:path:long-warning", () => {
      pushToast(errorMessageFor("LONG_PATH", t), "warning");
    }).then((un) => {
      if (cancelled) un();
      else unlisten = un;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [pushToast, t]);
  return null;
}
