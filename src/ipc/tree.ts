// Thin typed wrappers around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/tree.rs` (`docs/plans/project-architecture/PLAN.md`
// §6; `docs/plans/memo-tree/PLAN.md` §3). No business logic here.
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { LabelColor, MemoEditorSettings, MemoMoveResult, MemoProperties, MoveTarget, TreeSnapshot } from "../shared/models";

export async function treeOpen(tabId: string): Promise<Result<TreeSnapshot>> {
  return invoke<Result<TreeSnapshot>>("tree_open", { tabId });
}

/** `parent` omitted appends to the end of the root level. */
export async function memoCreate(tabId: string, name: string, parent?: string): Promise<Result<TreeSnapshot>> {
  return invoke<Result<TreeSnapshot>>("memo_create", { tabId, name, parent: parent ?? null });
}

export async function memoCreateChild(tabId: string, name: string, parent: string): Promise<Result<TreeSnapshot>> {
  return invoke<Result<TreeSnapshot>>("memo_create_child", { tabId, name, parent });
}

/** Also the backend for F2 rename (single-selection only, enforced by the caller). */
export async function memoRename(tabId: string, path: string, newName: string): Promise<Result<TreeSnapshot>> {
  return invoke<Result<TreeSnapshot>>("memo_rename", { tabId, path, newName });
}

/** `forceDirect: true` is only sent after the user explicitly consents to a direct (non-trash)
 * delete, following a prior `TRASH_FAILED` response (`[spec §5.8]`). */
export async function memoDelete(tabId: string, paths: string[], forceDirect = false): Promise<Result<TreeSnapshot>> {
  return invoke<Result<TreeSnapshot>>("memo_delete", { tabId, paths, forceDirect });
}

export async function memoMove(tabId: string, paths: string[], target: MoveTarget): Promise<Result<MemoMoveResult>> {
  return invoke<Result<MemoMoveResult>>("memo_move", { tabId, paths, target });
}

export async function memoSetExpanded(tabId: string, path: string, expanded: boolean): Promise<Result<TreeSnapshot>> {
  return invoke<Result<TreeSnapshot>>("memo_set_expanded", { tabId, path, expanded });
}

/** `color: null` is the "None" choice of the confirmed 5-choice palette. */
export async function memoSetLabel(tabId: string, path: string, color: LabelColor | null): Promise<Result<TreeSnapshot>> {
  return invoke<Result<TreeSnapshot>>("memo_set_label", { tabId, path, color });
}

export async function memoSetEditorOverride(tabId: string, path: string, editor: MemoEditorSettings | null): Promise<Result<TreeSnapshot>> {
  return invoke<Result<TreeSnapshot>>("memo_set_editor_override", { tabId, path, editor });
}

/** Edit lock (`locked?: boolean`) — body-edit-only block, no password. Distinct from tab lock. */
export async function memoSetEditLock(tabId: string, path: string, locked: boolean): Promise<Result<TreeSnapshot>> {
  return invoke<Result<TreeSnapshot>>("memo_set_edit_lock", { tabId, path, locked });
}

export async function memoGetProperties(tabId: string, path: string): Promise<Result<MemoProperties>> {
  return invoke<Result<MemoProperties>>("memo_get_properties", { tabId, path });
}

/** Reveals the memo's file in Explorer (plain tabs) or the tab's folder (locked/session tabs,
 * since the on-disk file is an obfuscated token — `[spec §2.8]`, `commands/tree.rs`'s
 * `memo_open_location`). */
export async function memoOpenLocation(tabId: string, path: string): Promise<Result<void>> {
  return invoke<Result<void>>("memo_open_location", { tabId, path });
}
