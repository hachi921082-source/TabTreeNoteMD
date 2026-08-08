// Shared path-keyed Blob-URL cache for reading image bytes back off disk
// (`docs/plans/markdown/image-handling/PLAN.md` §5). A single module-level cache instance for the
// whole app, not per-component — consumed by this plan's own live-view widget
// (`markdownDecorations.ts`'s `ImageWidget`), the preview pane (`markdown-preview/RENDERING.md`
// §5's `getOrLoadImage` call), and PDF export (`pdf-export/PLAN.md` §3's `peekCachedBytes`
// fallback).
import { imageReadBytes } from "../ipc/image";

interface ImageCacheEntry {
  blobUrl: string;
  bytes: Uint8Array;
  lastAccessMs: number;
}

const cache = new Map<string, ImageCacheEntry>();
// De-dupes concurrent `getOrLoadImage` calls for the same key (e.g. the live-view widget and the
// preview pane both resolving the same image at nearly the same moment) into a single IPC read.
const inFlight = new Map<string, Promise<ImageCacheEntry | null>>();
// Keys (same `cacheKey` scheme as `cache` above) that have already failed to load at least once.
// Consulted by `markdownDecorations.ts`'s `MarkdownDecorationsPlugin.build()` so a path already
// known to fail is rendered as `ImagePlaceholderWidget` directly on the *next* decoration rebuild,
// instead of re-attempting `ImageWidget`'s async load. This is also what makes the CM6-transaction-
// safe fix work at all: `ImageWidget.toDOM()`'s failure branch no longer mutates `contentDOM`
// directly (see that file) — it just records the failure here and asks CodeMirror to rebuild
// decorations, and *this* set is what the rebuilt decoration set consults to pick the placeholder.
const failedKeys = new Set<string>();

// Tunable; bounds total Blob memory retained across all currently-open memos combined, not just
// one memo's own images. Exact value is implementation discretion (`docs/MDEditor-spec.md` §5 only
// specifies "a fixed size bound", not a number).
const MAX_CACHE_ENTRIES = 200;

function cacheKey(memoKey: string, tabId: string, relativePath: string): string {
  return `${memoKey}::${tabId}::${relativePath}`;
}

/** Returns a `blob:` URL for `relativePath` (relative to `tabId`'s folder), fetching and caching it
 * on first use. Returns `null` on any failure (missing file, rejected path, locked-tab-with-no-
 * session, IPC error) — callers render their own fallback rather than this function distinguishing
 * failure kinds (every failure mode has the same correct UI response here: fall back to a
 * placeholder). `memoKey` scopes the cache entry to the calling memo (see `cacheKey` above and
 * `releaseMemoImages` below). */
export async function getOrLoadImage(tabId: string, relativePath: string, memoKey: string): Promise<string | null> {
  const key = cacheKey(memoKey, tabId, relativePath);
  const hit = cache.get(key);
  if (hit) {
    hit.lastAccessMs = Date.now();
    return hit.blobUrl;
  }
  const pending = inFlight.get(key);
  if (pending) {
    const entry = await pending;
    return entry?.blobUrl ?? null;
  }
  const promise = loadAndCache(key, tabId, relativePath);
  inFlight.set(key, promise);
  try {
    const entry = await promise;
    return entry?.blobUrl ?? null;
  } finally {
    inFlight.delete(key);
  }
}

/** `true` if `relativePath` has already failed to load at least once for this `tabId`/`memoKey`
 * (cleared on success and on `releaseMemoImages`, below) — lets a decoration-rebuild consult a
 * synchronous known-bad-path check instead of re-running an async load it already knows will fail. */
export function hasImageLoadFailed(tabId: string, relativePath: string, memoKey: string): boolean {
  return failedKeys.has(cacheKey(memoKey, tabId, relativePath));
}

/** Exposes the raw cached bytes for a key already resolved by `getOrLoadImage`, without
 * triggering a fetch — used by `pdf-export/PLAN.md` §3's fallback path (minting a *fresh* object
 * URL from already-cached bytes inside the print iframe's own document, if the primary approach of
 * reusing the main window's `blob:` URL string directly doesn't resolve there). */
export function peekCachedBytes(tabId: string, relativePath: string, memoKey: string): Uint8Array | undefined {
  return cache.get(cacheKey(memoKey, tabId, relativePath))?.bytes;
}

async function loadAndCache(key: string, tabId: string, relativePath: string): Promise<ImageCacheEntry | null> {
  let bytes: Uint8Array;
  try {
    bytes = await imageReadBytes(tabId, relativePath); // src/ipc/image.ts — throws on failure
  } catch {
    failedKeys.add(key);
    return null;
  }
  // A previously-failed path that now succeeds (e.g. the missing file was restored and the memo
  // re-fetches it) is no longer known-bad.
  failedKeys.delete(key);
  // `Uint8Array`'s `ArrayBufferLike`-vs-`ArrayBuffer` generic mismatch (TS 5.7+'s DOM lib) is
  // spurious here — `bytes` always wraps a freshly-allocated `ArrayBuffer` (`imageReadBytes`'s own
  // `new Uint8Array(buffer)`), never a `SharedArrayBuffer` — so the array-literal form is cast
  // through `BlobPart[]` rather than restructuring `ImageCacheEntry`'s type just for this call.
  const blobUrl = URL.createObjectURL(new Blob([bytes] as BlobPart[]));
  const entry: ImageCacheEntry = { blobUrl, bytes, lastAccessMs: Date.now() };
  cache.set(key, entry);
  evictIfOverBound();
  return entry;
}

/** Called on memo close/switch (`EditorPane.tsx`) — revokes and removes every cache entry
 * belonging to `memoKey`, regardless of which tab/path they point at. */
export function releaseMemoImages(memoKey: string): void {
  const prefix = `${memoKey}::`;
  for (const [key, entry] of cache) {
    if (key.startsWith(prefix)) {
      URL.revokeObjectURL(entry.blobUrl);
      cache.delete(key);
    }
  }
  for (const key of failedKeys) {
    if (key.startsWith(prefix)) failedKeys.delete(key);
  }
}

function evictIfOverBound(): void {
  if (cache.size <= MAX_CACHE_ENTRIES) return;
  const overBy = cache.size - MAX_CACHE_ENTRIES;
  const oldest = [...cache.entries()].sort((a, b) => a[1].lastAccessMs - b[1].lastAccessMs).slice(0, overBy);
  for (const [key, entry] of oldest) {
    URL.revokeObjectURL(entry.blobUrl);
    cache.delete(key);
  }
}

// No special handling at app shutdown — the OS reclaims the underlying memory automatically
// (`docs/MDEditor-spec.md` §5).
