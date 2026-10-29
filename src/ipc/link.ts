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
 * against that tab's folder — safe to omit for `url`/`email` targets. `extendedSearchText` (issue
 * #235, `filePath` only) is the rest of the source line from the bare-file-path match's start to
 * end-of-line — wider than `value` (the regex-truncated match), letting the backend try
 * progressively shorter word-trimmed prefixes of it against the real filesystem so a path
 * containing a space or `(`/`)` (e.g. `C:\Program Files (x86)\Steam\userdata`) still opens
 * correctly, even though `value` itself stays truncated at the first space/bracket. This command
 * performs no confirmation of its own; callers use `openExternalWithConfirm` below. Exception
 * (issue #308): an executable/script `filePath` is refused with `CONFIRM_REQUIRED` unless `confirmed` is true. */
export async function linkOpenExternal(kind: ExternalLinkKind, value: string, currentTabId?: string, extendedSearchText?: string, confirmed?: boolean): Promise<Result<void>> {
  return invoke<Result<void>>("link_open_external", { kind, value, currentTabId: currentTabId ?? null, extendedSearchText: extendedSearchText ?? null, confirmed: confirmed ?? false });
}

/** 確認ダイアログの文言切替専用の簡易判定(issue #308)。実行ファイル・スクリプトかの最終判断はバックエンド
 * (`link.rs` の `is_executable_file_path`)が行う。ここは「最初から実行ファイル用の文言で確認を出す」ためだけに使い、
 * 判定が外れてもバックエンドの `CONFIRM_REQUIRED` で必ず確認に回る。 */
const EXECUTABLE_EXT_RE = /\.(?:exe|bat|cmd|com|scr|ps1|ps1xml|ps2|psm1|psd1|vbs|vbe|js|jse|wsf|wsh|ws|hta|lnk|url|msi|msp|mst|reg|cpl|jar|pif|application|gadget|msc|inf|sct|scf|msh|msh1|msh2|mshxml|appref-ms|settingcontent-ms|diagcab|appx|msix|appinstaller|xll|wll|library-ms)[ .]*(?::.*)?$/i;
export function looksLikeExecutableFilePath(value: string): boolean {
  return EXECUTABLE_EXT_RE.test(value);
}

/** URL/メール/ファイルパスを、確認の要否を含めて開く(ライブビュー・プレビュー共通、issue #308)。
 * 確認が必要なのは「`confirmBeforeOpen` が true」または「実行ファイル・スクリプトのファイルパス」。
 * 実行ファイルの最終判断はバックエンドで、未確認なら `CONFIRM_REQUIRED` が返るため、その場合に実行ファイル用
 * ダイアログを出して `confirmed=true` で再要求する。キャンセルは何もしない。 */
export async function openExternalWithConfirm(
  kind: ExternalLinkKind,
  value: string,
  opts: {
    currentTabId?: string;
    extendedSearchText?: string;
    confirmBeforeOpen: boolean;
    requestExternalConfirm: (kind: ExternalLinkKind, value: string, executable?: boolean) => Promise<boolean>;
  },
): Promise<void> {
  const predictedExecutable = kind === "filePath" && looksLikeExecutableFilePath(value);
  if (opts.confirmBeforeOpen || predictedExecutable) {
    if (!(await opts.requestExternalConfirm(kind, value, predictedExecutable))) return;
    await linkOpenExternal(kind, value, opts.currentTabId, opts.extendedSearchText, true);
    return;
  }
  const res = await linkOpenExternal(kind, value, opts.currentTabId, opts.extendedSearchText, false);
  if (!res.ok && res.error.code === "CONFIRM_REQUIRED") {
    if (!(await opts.requestExternalConfirm(kind, value, true))) return;
    await linkOpenExternal(kind, value, opts.currentTabId, opts.extendedSearchText, true);
  }
}
