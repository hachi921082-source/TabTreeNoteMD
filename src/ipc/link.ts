// Thin typed wrapper around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/link.rs` (`docs/plans/link/PLAN.md` §2).
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { ExternalLinkKind, ResolvedLink } from "../shared/models";

export type { ExternalLinkKind } from "../shared/models";

/** Parses a `memo:` link target (already stripped of the leading `memo:` scheme — `MemoName` /
 * `TabName/MemoName` / `TabName/`) into a `ResolvedLink`. Pure resolution: never opens or
 * navigates anything itself (`docs/plans/link/PLAN.md` §2) — on `{ kind: "memo" }`, the caller
 * performs the actual tab-switch + memo-open via the shared "open memo" path
 * (`docs/plans/memo-tree/PLAN.md`). */
export async function linkResolve(currentTabId: string, target: string): Promise<Result<ResolvedLink>> {
  return invoke<Result<ResolvedLink>>("link_resolve", { currentTabId, target });
}

/** Opens a URL/email/file-path via the OS-standard "open with default handler" API
 * (`docs/plans/link/PLAN.md` §2). `currentTabId` is only used to resolve a relative `filePath`
 * against that tab's folder — safe to omit for `url`/`email` targets. This command performs no
 * confirmation of its own; gate it behind `AppSettings.links.confirmBeforeOpen` on the caller side
 * (`link/PLAN.md` §2, §4). */
export async function linkOpenExternal(kind: ExternalLinkKind, value: string, currentTabId?: string): Promise<Result<void>> {
  return invoke<Result<void>>("link_open_external", { kind, value, currentTabId: currentTabId ?? null });
}
