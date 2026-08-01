// Thin typed wrapper around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/link.rs` (`docs/plans/project-architecture/PLAN.md`
// §6). Only the minimal `link_open_external` seam exists so far — see that
// Rust module's doc comment. First consumer: `src/editor/textCommands.ts`'s
// "Search with Google" right-click action (`docs/plans/editor-save/PLAN.md` §4).
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";

export async function linkOpenExternal(url: string): Promise<Result<void>> {
  return invoke<Result<void>>("link_open_external", { url });
}
