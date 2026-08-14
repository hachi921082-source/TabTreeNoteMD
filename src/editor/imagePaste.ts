// Clipboard paste + toolbar "Image insert" handling (`docs/plans/image-handling/PLAN.md` §6):
// intercepts an image paste (or a toolbar-triggered file pick), saves the bytes via
// `image_save_pasted`, and inserts the resulting Markdown at the cursor as a single, undoable
// CodeMirror transaction. Plain text paste is completely unaffected — only clipboard payloads
// that are actually image files (or a file explicitly chosen via the toolbar's file picker) go
// through this path. On failure (e.g. `IO_LOCKED`), no text is inserted and the caller-supplied
// `onError` is invoked instead — `components/Editor/EditorPane.tsx` wires this to
// `state/useEditor.ts`'s `reportError`, which surfaces the existing non-blocking
// `.ttn-editor-error` banner (the same mechanism already used for save/load failures), rather than
// introducing a second, parallel notification UI for this one feature.
//
// Real image *rendering* (an actual `<img>` in the live-view decoration layer) and the resize-
// handle interaction that converts `![alt](path)` markup into `<img src width height>`
// (`image-handling/PLAN.md` §4's second/third bullets) are explicitly NOT implemented here — see
// `docs/QA.md`'s "Real `<img>` Rendering + Drag-Resize-Handle in Live View" entry (added during
// this pass, confirmed by `designer-lite`): that sub-feature needs a new architectural decision
// (Tauri asset-protocol scope vs. a base64-read IPC command) with real security tradeoffs that
// neither `image-handling/PLAN.md` nor `markdown-editing/PLAN.md` actually specifies, so it's left
// as an open, tracked follow-up rather than decided unilaterally here. Pasted/inserted images
// still render via `markdownDecorations.ts`'s existing `ImagePlaceholderWidget` stub in the
// meantime — no regression from before this module's real save/insert logic was wired up.
import { EditorView } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import { imageSavePasted } from "../ipc/image";
import { downscaleImageIfNeeded } from "./imageDownscale";
import type { ErrorCode } from "../shared/result";

export interface ImagePasteConfig {
  /** Tab the currently-open memo belongs to; `null` only when no tab is open at all (in which
   * case paste/insert is a no-op — there is nowhere to save the image). */
  tabId: string | null;
  /** The currently-open memo — GitHub issue #139's per-memo image subfolder is named after this.
   * `null` only when no memo is open (in which case, like `tabId`, paste/insert is a no-op). */
  memoName: string | null;
  /** Reports a save failure so the caller can surface it as a non-blocking error
   * (`docs/plans/image-handling/PLAN.md` §6 step 4). Never called on success. */
  onError: (code: ErrorCode) => void;
}

function extractImageItem(dataTransfer: DataTransfer | null): DataTransferItem | null {
  if (!dataTransfer) return null;
  for (const item of Array.from(dataTransfer.items)) {
    if (item.kind === "file" && item.type.startsWith("image/")) return item;
  }
  return null;
}

/** `Array.from(new Uint8Array(...))`, not the `Uint8Array` itself — see `ipc/image.ts`'s doc
 * comment on why a plain number array is required for Tauri's JSON-serialized IPC. */
async function readFileBytes(file: File): Promise<number[]> {
  const buffer = await file.arrayBuffer();
  return Array.from(new Uint8Array(buffer));
}

/** Core save-then-insert logic shared by the paste handler and the toolbar's file-picker
 * (`docs/plans/image-handling/PLAN.md` §6 steps 2-4). `insertPos` is captured by the caller
 * *before* the async `image_save_pasted` round-trip (at paste/pick time), matching where the user
 * was actually pointing — not recomputed after the await, since the selection may have moved by
 * then. `![alt](url)` with an empty `alt` (`PLAN.md` §4: "`alt` may be empty... not mandated"). */
async function saveAndInsertImage(view: EditorView, tabId: string, memoName: string, insertPos: number, bytes: number[], mimeHint: string | null, onError: (code: ErrorCode) => void): Promise<void> {
  // Downscale oversized source images before saving (GitHub issue #82) — shared by both call
  // sites below since they both funnel through this function. No-op (returns `bytes` unchanged)
  // for already-small images, `image/gif`, missing `mimeHint`, or any decode/encode failure.
  const downscaledBytes = await downscaleImageIfNeeded(bytes, mimeHint);
  const result = await imageSavePasted(tabId, memoName, downscaledBytes, mimeHint);
  if (!result.ok) {
    onError(result.error.code);
    return;
  }
  const markup = `![](${result.value.relativePath})`;
  const pos = Math.min(insertPos, view.state.doc.length);
  view.dispatch({
    changes: { from: pos, to: pos, insert: markup },
    selection: { anchor: pos + markup.length },
  });
  view.focus();
}

/** Paste handler extension — intercepts an image clipboard payload, saves it, and inserts the
 * resulting Markdown. Returns `false` (lets normal text paste proceed) whenever the clipboard has
 * no image item or no tab is open. */
export function imagePasteExtension(config: ImagePasteConfig): Extension {
  return EditorView.domEventHandlers({
    paste(event, view) {
      const imageItem = extractImageItem(event.clipboardData);
      if (!imageItem || !config.tabId || !config.memoName) return false;
      const file = imageItem.getAsFile();
      if (!file) return false;
      event.preventDefault();
      const tabId = config.tabId;
      const memoName = config.memoName;
      const insertPos = view.state.selection.main.from;
      void readFileBytes(file).then((bytes) => saveAndInsertImage(view, tabId, memoName, insertPos, bytes, file.type || null, config.onError));
      return true;
    },
  });
}

/** Toolbar "Image insert" action (`docs/plans/markdown-editing/PLAN.md` §4 row 2 col 7,
 * `docs/plans/image-handling/PLAN.md` §6 step 1's "toolbar's image-insert action" branch): opens a
 * plain HTML file-picker (no Tauri plugin needed — the WebView's native OS file dialog, same as
 * any other web page) scoped to image files, reads the chosen file's bytes, and reuses the same
 * save-then-insert path as paste. No-op if no tab is open. Fire-and-forget (matches how the
 * toolbar's other actions dispatch synchronously; the actual save+insert completes asynchronously
 * once the user finishes the picker). */
export function insertImageViaFilePicker(view: EditorView, tabId: string | null, memoName: string | null, onError: (code: ErrorCode) => void): void {
  if (!tabId || !memoName) return;
  const input = document.createElement("input");
  input.type = "file";
  input.accept = "image/png,image/jpeg,image/gif,image/webp,image/bmp";
  input.style.display = "none";
  const insertPos = view.state.selection.main.from;
  // Guards against the hidden <input> being permanently orphaned in the DOM when the user
  // dismisses the native file dialog without picking a file — `change` never fires in that case
  // (GitHub issue #16). `cancel` is the modern, direct signal for this (fired by
  // Chromium/WebView2 when the dialog closes with no selection); the `window focus` listener is a
  // belt-and-suspenders fallback for environments where `cancel` isn't reliably fired, since focus
  // always returns to the window once the native dialog closes, selection or not. `cleaned` makes
  // teardown idempotent so whichever fires first (change/cancel/focus) does the removal and the
  // others become no-ops.
  let cleaned = false;
  const cleanup = () => {
    if (cleaned) return;
    cleaned = true;
    window.removeEventListener("focus", onFocus);
    input.removeEventListener("cancel", onCancel);
    if (input.parentNode) input.parentNode.removeChild(input);
  };
  const onFocus = () => {
    // Deferred: if a file was actually picked, `change` fires essentially immediately once focus
    // returns to the window, so it wins the race and calls `cleanup()` first (a no-op here since
    // `cleaned` is already true by the time this timeout runs).
    window.setTimeout(cleanup, 0);
  };
  const onCancel = () => cleanup();
  input.addEventListener("change", () => {
    const file = input.files?.[0];
    cleanup();
    if (!file) return;
    void readFileBytes(file).then((bytes) => saveAndInsertImage(view, tabId, memoName, insertPos, bytes, file.type || null, onError));
  });
  input.addEventListener("cancel", onCancel);
  window.addEventListener("focus", onFocus);
  document.body.appendChild(input);
  input.click();
}
