// Thin typed wrapper around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/autostart.rs` (`docs/plans/window-operation/PLAN.md` §4). Toggles OS-level
// launch-on-Windows-login and returns the fresh `AppSettings` with `autostart.enabled` updated to
// match, same "returns the full post-merge settings" convention as `settingsSet`.
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { AppSettings } from "../shared/models";

export async function autostartSetEnabled(enabled: boolean): Promise<Result<AppSettings>> {
  return invoke<Result<AppSettings>>("autostart_set_enabled", { enabled });
}
