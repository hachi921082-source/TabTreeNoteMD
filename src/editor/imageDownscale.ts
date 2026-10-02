// Downscales/recompresses an oversized pasted/inserted image before it reaches
// `image_save_pasted` (GitHub issues #82, #259). Called from `imagePaste.ts`'s
// `saveAndInsertImage`, between reading the source file's raw bytes and handing them to the
// backend — keeps very large source images (e.g. full-resolution photos/screenshots) from being
// written to disk at full size and from bloating preview-pane rendering
// (`PreviewPane.tsx`/`previewRenderer.ts`) unnecessarily.
//
// Two steps:
//   1. (#82) If the longer edge exceeds `MAX_DIMENSION_PX`, resize to that longer edge and
//      re-encode in the same MIME type.
//   2. (#259) If the result of step 1 (or the untouched original) is still larger than
//      `MAX_BYTES`, re-encode as WebP at `WEBP_QUALITY`. The WebP output is only adopted when it
//      is actually WebP (`convertToBlob` silently falls back to PNG for unsupported types) and
//      strictly smaller than the step-1 bytes; otherwise the step-1 bytes/MIME are kept.
//
// Deliberately minimal: fixed constants, no `AppSettings` field, no Settings UI. Mirrors
// `imageLoader.ts`'s `getOrLoadImage` "never let a non-critical enhancement block the core
// action" pattern — any decode/encode failure falls back to the original bytes rather than
// blocking the paste/insert.

/** Max length (px) of the longer edge after downscaling. Images already at or under this on both
 * dimensions skip the resize step. Not user-configurable (GitHub issue #82's explicit scope). */
const MAX_DIMENSION_PX = 2000;

/** Byte size above which the (possibly resized) image is re-encoded as WebP (GitHub issue #259). */
const MAX_BYTES = 1024 * 1024;

/** `convertToBlob` quality for the WebP re-encode step (GitHub issue #259). */
const WEBP_QUALITY = 0.85;

const WEBP_MIME = "image/webp";

/** Result of `downscaleImageIfNeeded`: the bytes to save and the MIME type they are actually
 * encoded in (differs from the input MIME when the WebP step was adopted). */
export interface DownscaleResult {
  bytes: number[];
  mimeType: string | null;
}

/** `image/gif` is excluded entirely — decoding through `createImageBitmap`/drawing onto a canvas
 * only ever captures the first frame, which would silently destroy the animation. */
function isDownscaleEligible(mimeType: string | null): boolean {
  return mimeType !== null && mimeType.startsWith("image/") && mimeType !== "image/gif";
}

/** Pure decision for the WebP step (GitHub issue #259): adopt the WebP blob only if the encoder
 * really produced WebP and it is strictly smaller than the current bytes. */
function shouldAdoptWebp(currentSize: number, webpBlobType: string, webpSize: number): boolean {
  return webpBlobType === WEBP_MIME && webpSize < currentSize;
}

/** Pure decision: whether the WebP re-encode step should be attempted at all. */
function needsWebpRecompress(currentSize: number): boolean {
  return currentSize > MAX_BYTES;
}

/** Returns the bytes to save plus their effective MIME type — resized to fit within
 * `MAX_DIMENSION_PX` and/or re-encoded as WebP per the steps in this file's header — or the
 * original `bytes`/`mimeType` unchanged if no step applies, `mimeType` is missing/ineligible
 * (see `isDownscaleEligible`), or decoding/encoding fails for any reason. Never throws. */
export async function downscaleImageIfNeeded(bytes: number[], mimeType: string | null): Promise<DownscaleResult> {
  const original: DownscaleResult = { bytes, mimeType };
  if (!isDownscaleEligible(mimeType)) return original;
  try {
    const bitmap = await createImageBitmap(new Blob([new Uint8Array(bytes)], { type: mimeType ?? undefined }));
    try {
      const { width, height } = bitmap;
      const needsResize = width > MAX_DIMENSION_PX || height > MAX_DIMENSION_PX;
      if (!needsResize && !needsWebpRecompress(bytes.length)) return original;

      let targetWidth = width;
      let targetHeight = height;
      if (needsResize) {
        const scale = MAX_DIMENSION_PX / Math.max(width, height);
        targetWidth = Math.max(1, Math.round(width * scale));
        targetHeight = Math.max(1, Math.round(height * scale));
      }
      const canvas = new OffscreenCanvas(targetWidth, targetHeight);
      const ctx = canvas.getContext("2d");
      if (!ctx) return original;
      ctx.drawImage(bitmap, 0, 0, targetWidth, targetHeight);

      // Step 1 (#82): same-MIME re-encode of the resized canvas.
      let currentBytes: Uint8Array = new Uint8Array(bytes);
      let currentMime: string | null = mimeType;
      // True once any step replaced the original bytes (resize or adopted WebP).
      let changed = needsResize;
      if (needsResize) {
        const resized = await canvas.convertToBlob({ type: mimeType ?? undefined });
        currentBytes = new Uint8Array(await resized.arrayBuffer());
      }

      // Step 2 (#259): WebP re-encode of the same canvas, only if still over `MAX_BYTES`.
      if (needsWebpRecompress(currentBytes.length)) {
        const webp = await canvas.convertToBlob({ type: WEBP_MIME, quality: WEBP_QUALITY });
        if (shouldAdoptWebp(currentBytes.length, webp.type, webp.size)) {
          currentBytes = new Uint8Array(await webp.arrayBuffer());
          currentMime = WEBP_MIME;
          changed = true;
        }
      }

      if (!changed) return original;
      return { bytes: Array.from(currentBytes), mimeType: currentMime };
    } finally {
      bitmap.close();
    }
  } catch {
    return original;
  }
}
