// Thin typed wrappers around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/settings.rs` (`docs/plans/project-architecture/PLAN.md`
// §6). No business logic here; validation/branching on the `Result` happens in
// the calling hook/component.
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { AppSettings, AppSettingsPatch } from "../shared/models";

export async function settingsGet(): Promise<Result<AppSettings>> {
  return invoke<Result<AppSettings>>("settings_get");
}

export async function settingsSet(patch: AppSettingsPatch): Promise<Result<AppSettings>> {
  return invoke<Result<AppSettings>>("settings_set", { patch });
}
