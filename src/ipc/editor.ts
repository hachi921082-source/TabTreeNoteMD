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
