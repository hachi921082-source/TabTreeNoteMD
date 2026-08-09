// Downscales an oversized pasted/inserted image before it reaches `image_save_pasted`
// (GitHub issue #82). Called from `imagePaste.ts`'s `saveAndInsertImage` call sites, between
// reading the source file's raw bytes and handing them to the backend — keeps very large source
// images (e.g. full-resolution photos/screenshots) from being written to disk at full size and
// from bloating preview-pane rendering (`PreviewPane.tsx`/`previewRenderer.ts`) unnecessarily.
//
// Deliberately minimal (per the issue's chosen scope): one fixed threshold constant, no
// `AppSettings` field, no Settings UI, no user-configurable threshold. Mirrors
// `imageLoader.ts`'s `getOrLoadImage` "never let a non-critical enhancement block the core
// action" pattern — any decode/encode failure falls back to the original bytes rather than
// blocking the paste/insert.

/** Max length (px) of the longer edge after downscaling. Images already at or under this on both
 * dimensions are left untouched (see `downscaleImageIfNeeded`'s early-return). Not user-
 * configurable in this pass (GitHub issue #82's explicit scope). */
const MAX_DIMENSION_PX = 2000;

/** `image/gif` is excluded entirely — decoding through `createImageBitmap`/drawing onto a canvas
 * only ever captures the first frame, which would silently destroy the animation. */
function isDownscaleEligible(mimeType: string | null): boolean {
  return mimeType !== null && mimeType.startsWith("image/") && mimeType !== "image/gif";
}

/** Returns `bytes` downscaled to fit within `MAX_DIMENSION_PX` on its longer edge (aspect ratio
 * preserved), re-encoded in the same `mimeType` — or the original `bytes` unchanged if the image
 * is already small enough, `mimeType` is missing/ineligible (see `isDownscaleEligible`), or
 * decoding/encoding fails for any reason (corrupt file, unsupported case, etc.). Never throws. */
export async function downscaleImageIfNeeded(bytes: number[], mimeType: string | null): Promise<number[]> {
  if (!isDownscaleEligible(mimeType)) return bytes;
  try {
    const bitmap = await createImageBitmap(new Blob([new Uint8Array(bytes)], { type: mimeType ?? undefined }));
    try {
      const { width, height } = bitmap;
      if (width <= MAX_DIMENSION_PX && height <= MAX_DIMENSION_PX) return bytes;
      const scale = MAX_DIMENSION_PX / Math.max(width, height);
      const targetWidth = Math.max(1, Math.round(width * scale));
      const targetHeight = Math.max(1, Math.round(height * scale));
      const canvas = new OffscreenCanvas(targetWidth, targetHeight);
      const ctx = canvas.getContext("2d");
      if (!ctx) return bytes;
      ctx.drawImage(bitmap, 0, 0, targetWidth, targetHeight);
      const blob = await canvas.convertToBlob({ type: mimeType ?? undefined });
      const buffer = await blob.arrayBuffer();
      return Array.from(new Uint8Array(buffer));
    } finally {
      bitmap.close();
    }
  } catch {
    return bytes;
  }
}
