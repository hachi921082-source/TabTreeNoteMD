// Thin typed wrapper around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/pdf_export.rs` (issue #256).
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";

/** Shows a native save-file dialog (suggesting `{memoName}.pdf`) and, if the user picks a
 * destination, writes `html` there as a PDF via WebView2's native `PrintToPdfAsync` — see
 * `pdf_export.rs`'s own header comment for why this replaced an earlier `window.print()`-based
 * flow. Resolves to `{ ok: true, value: null }` (not an error) if the user cancels the save
 * dialog — callers should not treat a `null` value as a failure. */
export async function pdfExportWrite(html: string, memoName: string): Promise<Result<string | null>> {
  return invoke<Result<string | null>>("pdf_export_write", { html, memoName });
}
