// Clipboard paste handler (`docs/plans/markdown-editing/PLAN.md` §2). Real behavior delegates the
// actual file write to `image-handling/PLAN.md`'s `image_save_pasted` command, then inserts the
// resulting Markdown (`![alt](relative/path.png)`) at the cursor — that command does not exist yet,
// so this is an explicit stub: pasted image data is intercepted (so no broken/garbage text gets
// inserted from the raw clipboard payload) and logged as a clearly-labeled no-op instead. Plain
// text paste is completely unaffected — only clipboard payloads that are actually image files are
// intercepted here.
import { EditorView } from "@codemirror/view";
import type { Extension } from "@codemirror/state";

function extractImageItem(dataTransfer: DataTransfer | null): DataTransferItem | null {
  if (!dataTransfer) return null;
  for (const item of Array.from(dataTransfer.items)) {
    if (item.kind === "file" && item.type.startsWith("image/")) return item;
  }
  return null;
}

/** Stub — see this module's header comment. Becomes real once `image-handling/PLAN.md` lands, by
 * replacing this function's body with an actual `image_save_pasted` call + Markdown insertion. */
function handleImagePasteStub(item: DataTransferItem): void {
  console.info(
    `[markdown-editing] Image paste is not implemented yet (see image-handling/PLAN.md); clipboard item type="${item.type}" was not inserted.`,
  );
}

export function imagePasteExtension(): Extension {
  return EditorView.domEventHandlers({
    paste(event) {
      const imageItem = extractImageItem(event.clipboardData);
      if (!imageItem) return false; // not an image — let normal text paste proceed
      event.preventDefault();
      handleImagePasteStub(imageItem);
      return true;
    },
  });
}
