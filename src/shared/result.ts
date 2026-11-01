// Mirrors `docs/spec.md` §3.4 / §5.1 and the Rust side `model/result.rs`
// (see `docs/plans/project-architecture/PLAN.md` §3). This is the wire shape
// every Tauri command returns; `ipc/*.ts` wrappers never throw for expected/
// recoverable failures, they resolve to `{ ok: false, error }` instead.

export type Result<T = void> =
  | { ok: true; value: T }
  | { ok: false; error: AppError };

export interface AppError {
  code: ErrorCode;
  message: string;
  path?: string;
}

export type ErrorCode =
  | "NAME_DUPLICATE"
  | "NAME_INVALID"
  | "NOT_FOUND"
  | "IO_LOCKED"
  | "TRASH_FAILED"
  | "DECRYPT_FAILED"
  | "CONFLICT"
  | "META_CORRUPT"
  | "LONG_PATH"
  | "TAB_LOCKED"
  | "ANOMALOUS_FILE_STATE"
  | "MEMO_EDIT_LOCKED"
  | "IMAGE_FOLDER_RENAME_FAILED"
  | "CONFIRM_REQUIRED"
  | "UNKNOWN";
