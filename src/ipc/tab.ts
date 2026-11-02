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
 * `initialPath` omitted (or `undefined`) opens the OS folder-picker. The tab
 * name is derived from the folder's base name (auto-numbered on a clash,
 * issue #329). Resolves to `{ ok: true, value: null }` if the user cancels
 * the dialog — not an error.
 */
export async function tabCreate(initialPath?: string): Promise<Result<TabInfo | null>> {
  return invoke<Result<TabInfo | null>>("tab_create", {
    initialPath: initialPath ?? null,
  });
}

/** No dialog — registers a folder path already known to the caller (the
 * OS-drag-and-drop-onto-tab-bar entry point). */
export async function tabRegisterExisting(path: string): Promise<Result<TabInfo>> {
  return invoke<Result<TabInfo>>("tab_register_existing", { path });
}

/** Creates a brand-new, empty folder under the app's default tabs root and registers it — no OS
 * dialog (`docs/plans/tab-management/PLAN.md` §2 `tab_create_new`, distinct from `tabCreate`'s
 * OS-folder-picker "register existing" behavior). */
export async function tabCreateNew(name: string): Promise<Result<TabInfo>> {
  return invoke<Result<TabInfo>>("tab_create_new", { name });
}

export async function tabRename(id: string, newName: string): Promise<Result<TabInfo>> {
  return invoke<Result<TabInfo>>("tab_rename", { id, newName });
}

/** Sets or clears (`label: null`) the distinguishing label shown for a locked tab in place of the
 * bare 🔒 glyph (`docs/spec.v1.2.0.md` §10). Works regardless of lock/session state, unlike
 * `tabRename`. */
export async function tabSetLockLabel(id: string, label: string | null): Promise<Result<TabInfo>> {
  return invoke<Result<TabInfo>>("tab_set_lock_label", { id, label });
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
