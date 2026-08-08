// Japanese user-facing text for each `ErrorCode` (`docs/spec.md` §5.1's
// `code`/`message`/`path?` wire shape sends `message` to the renderer too,
// but that string is written for internal diagnostic logging in English —
// UI surfaces that need to show the user something derive their Japanese
// text from `code` via this mapping instead of displaying `message`
// verbatim, matching this app's Japanese-end-user UI convention (e.g. the
// `label.color` palette names already established in
// `docs/DECISIONS.md`).
import type { ErrorCode } from "./result";

const MESSAGES: Record<ErrorCode, string> = {
  // Neutral wording: this code is shared by both tab operations
  // (`tab_create`/`tab_rename`) and memo operations (`memo_create`/
  // `memo_rename`/`memo_move`), so it must not name "tab" specifically.
  NAME_DUPLICATE: "同じ名前のものが既に存在します",
  NAME_INVALID: "使用できない名前です",
  NOT_FOUND: "対象が見つかりませんでした",
  IO_LOCKED: "ファイルが他のプログラムで使用されています",
  TRASH_FAILED: "ごみ箱への移動に失敗しました",
  DECRYPT_FAILED: "パスワードが正しくありません",
  CONFLICT: "外部でファイルが変更されています",
  META_CORRUPT: "管理情報を復元しました",
  LONG_PATH: "パスが長すぎる可能性があります",
  TAB_LOCKED: "ロックされたタブには操作できません",
  ANOMALOUS_FILE_STATE: "想定外の状態のファイルが見つかりました。タブをもう一度開いて復旧処理を行ってください。",
  UNKNOWN: "予期しないエラーが発生しました",
};

export function errorMessageFor(code: ErrorCode): string {
  return MESSAGES[code] ?? MESSAGES.UNKNOWN;
}
