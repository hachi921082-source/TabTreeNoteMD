// Thin typed wrapper around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/about.rs`. Fetches the third-party license notices text (baked into the
// binary at compile time from the repo-root `THIRD-PARTY-NOTICES.md`) for the Settings > General
// license link (`SettingsDialog.tsx`).
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";

export async function aboutGetThirdPartyNotices(): Promise<Result<string>> {
  return invoke<Result<string>>("about_get_third_party_notices");
}
