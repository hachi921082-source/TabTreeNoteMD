// Thin typed wrappers around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/editor.rs` (`docs/plans/project-architecture/PLAN.md`
// §6; `docs/plans/editor-save/PLAN.md` §2). No business logic here.
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { MemoContent, SaveResult } from "../shared/models";

export async function editorLoad(tabId: string, path: string): Promise<Result<MemoContent>> {
  return invoke<Result<MemoContent>>("editor_load", { tabId, path });
}

export async function editorSave(tabId: string, path: string, text: string, expectedHash: string): Promise<Result<SaveResult>> {
  return invoke<Result<SaveResult>>("editor_save", { tabId, path, text, expectedHash });
}

/** Clears the backend's `ActiveMemoTracker` slot for `(tabId, path)`, if it's still the one
 * tracked — call this on every real "memo closed" transition (`state/useEditor.ts`), not just a
 * memo-to-memo switch, so `tab_lock`'s defensive open-memo check (`docs/plans/tab-lock/PLAN.md`
 * §3 step 2) doesn't stay tripped forever after a memo was already saved and closed. */
export async function editorClose(tabId: string, path: string): Promise<Result<void>> {
  return invoke<Result<void>>("editor_close", { tabId, path });
}
