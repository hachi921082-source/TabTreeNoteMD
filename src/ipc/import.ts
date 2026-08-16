// Thin typed wrapper around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/import.rs` (`docs/plans/v1.1.0/folder-import/PLAN.md` §3). No business
// logic here — collision detection/resolution happens client-side in `state/useImport.ts`; this
// command only ever executes one already-resolved item per call.
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { TreeSnapshot } from "../shared/models";

/** Imports one already-collision-resolved source `.md` file as a memo. `parent: null` appends to
 * the root level's end; a non-null `parent` appends to that node's children end. */
export async function importFile(tabId: string, parent: string | null, sourcePath: string, targetName: string, overwrite: boolean): Promise<Result<TreeSnapshot>> {
  return invoke<Result<TreeSnapshot>>("import_file", { tabId, parent, sourcePath, targetName, overwrite });
}
