// Thin typed wrapper around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/image.rs` (`docs/plans/image-handling/PLAN.md` §3).
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { SavedImage } from "../shared/models";

/** Saves a pasted/inserted image's raw bytes directly under the given tab's folder and returns
 * its generated filename (relative to that folder) plus decoded pixel dimensions
 * (`docs/plans/image-handling/PLAN.md` §3). `bytes` is a plain array of byte values (0-255) —
 * `Array.from(new Uint8Array(...))`, not a `Uint8Array` itself, since Tauri's IPC serializes
 * arguments as JSON and a typed array would serialize as an object, not an array. `mimeHint` is
 * the clipboard/File API's reported MIME type, if any — a fallback only; the backend sniffs the
 * actual image format from `bytes` first. */
export async function imageSavePasted(tabId: string, bytes: number[], mimeHint: string | null): Promise<Result<SavedImage>> {
  return invoke<Result<SavedImage>>("image_save_pasted", { tabId, bytes, mimeHint });
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
