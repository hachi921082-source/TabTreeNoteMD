// Thin typed wrapper around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/image.rs` (`docs/plans/image-handling/PLAN.md` §3).
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { SavedImage } from "../shared/models";

/** Saves a pasted/inserted image's raw bytes under `memoName`'s own per-memo image subfolder
 * (GitHub issue #139; falls back to the tab folder directly, flat, for a locked tab's active
 * unlock session — see `image_save_pasted`'s own Rust-side doc comment) and returns its generated
 * filename (relative to the tab folder) plus decoded pixel dimensions
 * (`docs/plans/image-handling/PLAN.md` §3). `bytes` is a plain array of byte values (0-255) —
 * `Array.from(new Uint8Array(...))`, not a `Uint8Array` itself, since Tauri's IPC serializes
 * arguments as JSON and a typed array would serialize as an object, not an array. `mimeHint` is
 * the clipboard/File API's reported MIME type, if any — a fallback only; the backend sniffs the
 * actual image format from `bytes` first. */
export async function imageSavePasted(tabId: string, memoName: string, bytes: number[], mimeHint: string | null): Promise<Result<SavedImage>> {
  return invoke<Result<SavedImage>>("image_save_pasted", { tabId, memoName, bytes, mimeHint });
}

/** Reads an image file's raw bytes back from disk (`docs/MDEditor-spec.md` §5). Unlike every other
 * function in this file (and every other `ipc/*.ts` wrapper in the codebase), this one can
 * **throw** — `image_read_bytes` returns a raw `tauri::ipc::Response` on the Rust side, which is
 * incompatible with the `AppResult<T>`/`{ok, value|error}` envelope every other command uses, so a
 * backend error surfaces as a rejected promise instead
 * (`docs/plans/markdown/image-handling/PLAN.md` §3.3). Callers (`src/editor/imageLoader.ts`) must
 * `try/catch`, not check `.ok`. */
export async function imageReadBytes(tabId: string, relativePath: string): Promise<Uint8Array> {
  const buffer = await invoke<ArrayBuffer>("image_read_bytes", { tabId, relativePath });
  return new Uint8Array(buffer);
}

/** レガシー画像リンク(GitHub issue #266 以前の半角スペース入り裸形 `![](新 メモ/001.png)`)を
 * 山括弧形に直した本文を返す。参照先がタブフォルダ基準で実在する場合のみ修復し、修復が無ければ
 * `null`。ファイルには書き込まない(呼び出し側が通常の保存経路で保存する)。 */
export async function imageRepairLegacyLinks(tabId: string, body: string): Promise<Result<string | null>> {
  return invoke<Result<string | null>>("image_repair_legacy_links", { tabId, body });
}

/** Reads raw bytes from an arbitrary absolute filesystem path (GitHub issue #244's re-fix —
 * `components/Editor/EditorPane.tsx`'s native-OS-drag-drop listener only ever receives real
 * absolute paths from `getCurrentWebview().onDragDropEvent`, never `File` objects, so there is no
 * client-side way to read the dropped file's bytes the way clipboard paste/the toolbar file-picker
 * already do). Unlike `imageReadBytes` above, this uses the project's normal `AppResult<T>`
 * envelope (not a raw `tauri::ipc::Response`) — `path` is always absolute here, not a
 * relative-to-tab-folder path, so there's no containment check to bypass the envelope for. */
export async function imageReadExternalFile(path: string): Promise<Result<number[]>> {
  return invoke<Result<number[]>>("image_read_external_file", { path });
}
