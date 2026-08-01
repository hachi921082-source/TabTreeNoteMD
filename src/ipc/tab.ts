// Thin typed wrappers around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/tab.rs` (`docs/plans/project-architecture/PLAN.md`
// §6). No business logic here; validation/branching on the `Result` happens
// in `state/useTabs.ts` or the calling component.
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { TabInfo } from "../shared/models";

export async function tabList(): Promise<Result<TabInfo[]>> {
  return invoke<Result<TabInfo[]>>("tab_list");
}

/**
 * `initialPath`/`name` omitted (or `undefined`) opens the OS folder-picker
 * and derives the tab name from the folder's base name. Pass `name` to
 * retry with a user-chosen name after a `NAME_DUPLICATE` rejection, without
 * re-opening the dialog (`docs/plans/tab-management/PLAN.md` §2 `tab_create`
 * refinement). Resolves to `{ ok: true, value: null }` if the user cancels
 * the dialog — not an error.
 */
export async function tabCreate(initialPath?: string, name?: string): Promise<Result<TabInfo | null>> {
  return invoke<Result<TabInfo | null>>("tab_create", {
    initialPath: initialPath ?? null,
    name: name ?? null,
  });
}

/** No dialog — registers a folder path already known to the caller (the
 * OS-drag-and-drop-onto-tab-bar entry point). */
export async function tabRegisterExisting(path: string, name?: string): Promise<Result<TabInfo>> {
  return invoke<Result<TabInfo>>("tab_register_existing", { path, name: name ?? null });
}

export async function tabRename(id: string, newName: string): Promise<Result<TabInfo>> {
  return invoke<Result<TabInfo>>("tab_rename", { id, newName });
}

export async function tabReorder(order: string[]): Promise<Result<void>> {
  return invoke<Result<void>>("tab_reorder", { order });
}

export async function tabRemove(id: string): Promise<Result<void>> {
  return invoke<Result<void>>("tab_remove", { id });
}

export async function tabOpenLocation(id: string): Promise<Result<void>> {
  return invoke<Result<void>>("tab_open_location", { id });
}
