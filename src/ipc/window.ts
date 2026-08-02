// Thin typed wrappers around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/window.rs` (`docs/plans/project-architecture/PLAN.md`
// §6). `windowSetAlwaysOnTop` is session-only: the caller owns the toggle state
// and simply forwards it on every click (`docs/plans/tab-management/PLAN.md`
// §4.1).
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";

export async function windowSetAlwaysOnTop(enabled: boolean): Promise<Result<void>> {
  return invoke<Result<void>>("window_set_always_on_top", { enabled });
}

/** Whether this process was launched with the tray-autostart `--tray` argument
 * (`docs/plans/window-operation/PLAN.md` §4). `state/useWindowGeometry.ts` checks this before
 * revealing the main window at startup. */
export async function windowIsTrayLaunch(): Promise<Result<boolean>> {
  return invoke<Result<boolean>>("window_is_tray_launch");
}
