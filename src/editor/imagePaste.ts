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
// GitHub issue #244's real-OS-drag-and-drop image insert does NOT live here as a CodeMirror
// `domEventHandlers` extension (an earlier attempt, merged as PR #246, added `dragover`/`drop`
// HTML5 DOM handlers here — that never fires for a genuine Explorer drag, since Tauri's native
// drag-drop interception, `tauri.conf.json`'s `dragDropEnabled`, conflicts with the browser's own
// HTML5 DnD events on Windows; see `TabBar.tsx`'s OS-folder-drop listener and
// `MarkdownToolbar.tsx`'s slot-reordering for the same documented constraint). The real fix
// (`components/Editor/EditorPane.tsx`) uses `getCurrentWebview().onDragDropEvent(...)` instead,
// which hands over real absolute filesystem paths rather than HTML5 `File` objects — so it needs
// its own bytes-from-path read step (`saveAndInsertDroppedPaths` below) rather than reusing a
// `domEventHandlers` extension at all.
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
import { imageReadExternalFile, imageSavePasted } from "../ipc/image";
import { downscaleImageIfNeeded } from "./imageDownscale";
import type { ErrorCode } from "../shared/result";
import { enterFilePicker, exitFilePicker } from "../shared/filePickerGate";

/** Extensions accepted by both the toolbar's file-picker (`insertImageViaFilePicker`'s `accept`
 * list below) and the native-OS-drag-drop path (GitHub issue #244) — kept as one shared set so the
 * two entry points can never silently drift apart on which image types are supported. */
const IMAGE_FILE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "gif", "webp", "bmp"]);

function fileExtensionOf(path: string): string | null {
  const dot = path.lastIndexOf(".");
  if (dot === -1 || dot === path.length - 1) return null;
  return path.slice(dot + 1).toLowerCase();
}

/** `true` for a path whose extension matches `IMAGE_FILE_EXTENSIONS` — used to filter a native
 * OS drag-drop's `event.payload.paths` down to image files, silently skipping the rest (GitHub
 * issue #244: "non-image files are silently ignored", matching the existing OS→tree-pane `.md`-only
 * import behavior's own quiet-skip treatment, `docs/spec.v1.1.0.md` §1.2). */
export function isImageFilePath(path: string): boolean {
  const ext = fileExtensionOf(path);
  return ext !== null && IMAGE_FILE_EXTENSIONS.has(ext);
}

/** Best-effort MIME hint derived from a dropped file's extension — native drag-drop paths carry no
 * MIME type at all (unlike `DataTransferItem`/`File`), so `saveAndInsertImage`'s `mimeHint`
 * parameter (used for downscale-eligibility and as the backend's format-sniff fallback, see that
 * function's own doc comment) is reconstructed from the extension instead. `null` for an
 * unrecognized extension — `saveAndInsertImage`'s downstream callees already treat a `null` hint as
 * "defer to magic-number sniffing" (`imageDownscale.ts`'s `isDownscaleEligible`,
 * `commands/image.rs`'s `detect_extension`). */
function mimeHintFromExtension(path: string): string | null {
  switch (fileExtensionOf(path)) {
    case "png":
      return "image/png";
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "gif":
      return "image/gif";
    case "webp":
      return "image/webp";
    case "bmp":
      return "image/bmp";
    default:
      return null;
  }
}

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

/** 画像リンク先を Markdown に書ける形にする(GitHub issue #266)。CommonMark は半角スペース・
 * ASCII 制御文字・対応の取れない丸括弧を含む裸形リンク先を認めない(メモ名 `新 メモ` だと
 * `![](新 メモ/001.png)` は画像にならない)ため、その場合は山括弧形 `<新 メモ/001.png>` で書く。
 * それ以外(全角スペースを含む)は裸形のまま。Rust 側 `util::image_link_rewrite::needs_angle_brackets`
 * と同じ判定。メモ名は Windows のファイル名規則で `<` `>` `\` を含められないためエスケープ不要。 */
export function toMarkdownImageDestination(path: string): string {
  let depth = 0;
  let needsBrackets = false;
  for (const ch of path) {
    const code = ch.charCodeAt(0);
    // 半角スペース・ASCII 制御文字は裸形に書けない
    if (ch === " " || code < 0x20 || code === 0x7f) {
      needsBrackets = true;
      break;
    }
    if (ch === "(") depth++;
    else if (ch === ")") {
      // 対応の取れない閉じ括弧
      if (depth === 0) {
        needsBrackets = true;
        break;
      }
      depth--;
    }
  }
  return needsBrackets || depth !== 0 ? `<${path}>` : path;
}

/** Core save-then-insert logic shared by the paste handler, the toolbar's file-picker
 * (`docs/plans/image-handling/PLAN.md` §6 steps 2-4), and the right-click context menu's Paste
 * item (`textCommands.ts`'s `pasteCommand`, GitHub issue #203 — reused rather than duplicated so
 * both paste paths save/downscale/insert identically). `insertPos` is captured by the caller
 * *before* the async `image_save_pasted` round-trip (at paste/pick time), matching where the user
 * was actually pointing — not recomputed after the await, since the selection may have moved by
 * then. `![alt](url)` with an empty `alt` (`PLAN.md` §4: "`alt` may be empty... not mandated"). */
export async function saveAndInsertImage(view: EditorView, tabId: string, memoName: string, insertPos: number, bytes: number[], mimeHint: string | null, onError: (code: ErrorCode) => void): Promise<void> {
  // Downscale/recompress oversized source images before saving (GitHub issues #82, #259) —
  // shared by all call sites since they funnel through this function. No-op (returns `bytes`
  // unchanged) for already-small images, `image/gif`, missing `mimeHint`, or any decode/encode
  // failure. The effective MIME is passed on because the WebP step (#259) may change the format.
  const downscaled = await downscaleImageIfNeeded(bytes, mimeHint);
  const result = await imageSavePasted(tabId, memoName, downscaled.bytes, downscaled.mimeType);
  if (!result.ok) {
    onError(result.error.code);
    return;
  }
  const markup = `![](${toMarkdownImageDestination(result.value.relativePath)})`;
  const pos = Math.min(insertPos, view.state.doc.length);
  view.dispatch({
    changes: { from: pos, to: pos, insert: markup },
    selection: { anchor: pos + markup.length },
  });
  view.focus();
}

/** Sequential multi-path save-then-insert for a single native OS drag-drop (GitHub issue #244's
 * re-fix). Unlike the paste/toolbar-pick paths above, `getCurrentWebview().onDragDropEvent`
 * (`components/Editor/EditorPane.tsx`) hands over real absolute filesystem paths rather than
 * `File` objects, so each path's bytes must be fetched from the backend
 * (`image_read_external_file`) instead of read client-side via `File.arrayBuffer()` — everything
 * else mirrors the old `saveAndInsertImages`: `saveAndInsertImage` is awaited before starting the
 * next path (insert position for path N+1 must land right after path N's just-inserted markup,
 * not the drop point used for every path, which would otherwise overwrite/interleave them out of
 * drop order), a single space is inserted between consecutive images purely for readability (a
 * judgment call, not a spec requirement), and a given path's read/save failure (`onError`) does not
 * abort the remaining paths. */
export async function saveAndInsertDroppedPaths(view: EditorView, tabId: string, memoName: string, startPos: number, paths: string[], onError: (code: ErrorCode) => void): Promise<void> {
  let pos = startPos;
  for (let i = 0; i < paths.length; i++) {
    const path = paths[i];
    const result = await imageReadExternalFile(path);
    if (!result.ok) {
      onError(result.error.code);
      continue;
    }
    await saveAndInsertImage(view, tabId, memoName, pos, result.value, mimeHintFromExtension(path), onError);
    pos = view.state.selection.main.from;
    if (i < paths.length - 1) {
      view.dispatch({ changes: { from: pos, to: pos, insert: " " }, selection: { anchor: pos + 1 } });
      pos += 1;
    }
  }
}

/** Paste handler extension — intercepts an image clipboard payload, saves it, and inserts the
 * resulting Markdown. Returns `false` (lets normal text paste proceed) whenever the clipboard has
 * no image item or no tab is open. Native-OS-drag-drop image insert (GitHub issue #244) is
 * deliberately NOT handled here — see this file's top-of-file comment for why (HTML5 `dragover`/
 * `drop` DOM events never fire for a genuine Explorer drag under Tauri's native drag-drop
 * interception); that path lives in `components/Editor/EditorPane.tsx` instead, using
 * `saveAndInsertDroppedPaths` above. */
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
    // ダイアログ終了(change / cancel / フォーカス復帰のどれでも)でフォーカス喪失再ロックの例外を解除 (#274)
    exitFilePicker();
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
  // ダイアログ表示中の OS フォーカス喪失で再ロックしないよう例外を有効化 (#274)。
  // 解除は上の cleanup() が担い、change / cancel / フォーカス復帰のどの経路でも 1 回だけ走る。
  enterFilePicker();
  input.click();
}
