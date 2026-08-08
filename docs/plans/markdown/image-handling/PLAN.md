# PLAN: Image Read-Back Pipeline (Live-View Real Rendering, Preview, PDF Export)

> Status: planning complete, not yet implemented. Strictly additive — reads and renders image bytes
> that `image-handling/PLAN.md` (the pre-existing, top-level plan, unmodified by this one) already
> writes to disk; nothing about how/where images are saved changes.

**Disambiguation (read this first)**: this file is easy to confuse with the pre-existing top-level
[`image-handling/PLAN.md`](../../image-handling/PLAN.md) (two directory levels up from here) because
the folder names are identical apart from nesting. They cover disjoint concerns:

- [`image-handling/PLAN.md`](../../image-handling/PLAN.md) (top-level, **not** touched by this plan) —
  the **write path**: image paste/insert, storage rule (saved directly under the tab folder), random
  `img_<16 hex>.<ext>` filename generation, the `image_save_pasted` command, and the "no orphan
  cleanup" policy. Already implemented (its own status line).
- This file (`docs/plans/markdown/image-handling/PLAN.md`) — the **read-back path**: a new
  `image_read_bytes` command that reads an already-saved image's bytes back off disk, and the
  frontend pipeline (`src/editor/imageLoader.ts`) that turns those bytes into a `blob:` URL for
  actually *displaying* an image — in the live-view editor pane, in the preview pane
  ([`markdown-preview/PLAN.md`](../markdown-preview/PLAN.md)), and in PDF export
  ([`pdf-export/PLAN.md`](../pdf-export/PLAN.md)).

This file lives under the new `docs/plans/markdown/` parent alongside its two siblings
(`markdown-preview/PLAN.md`, `pdf-export/PLAN.md`) because all three were split out of one originally
combined plan for one large feature addition (split-pane preview, PDF export, and live-view real image
rendering) — see `docs/plans/README.md`'s note on this grouping.

Sources: `docs/MDEditor-spec.md` §5 (primary/authoritative source for this plan), §4/§6 (the preview
pane and PDF export consumers this plan's pipeline feeds); `docs/spec.md` §4.2; `docs/detailed-spec.md`
§3.6, §4.7; `docs/plans/image-handling/PLAN.md` (the write path this plan's read path is the
counterpart to — disambiguated above); `docs/plans/markdown-editing/PLAN.md` (extended here, in
particular `src/editor/markdownDecorations.ts`'s `ImagePlaceholderWidget`, replaced by this plan);
`docs/plans/link/PLAN.md` (`src-tauri/src/commands/link.rs`'s path-safety pattern, reused here);
`docs/plans/project-architecture/PLAN.md` §2-§4 (module layout, `Result<T>`/command-inventory
conventions); `docs/QA.md`'s "Real `<img>` Rendering + Drag-Resize-Handle in Live View" entry (this
plan resolves the image-rendering half of that question; the drag-resize-handle half remains deferred,
see §7 below).

---

## 1. Scope

Elaborates `docs/MDEditor-spec.md` §5. Two things, both new:

1. A backend command, `image_read_bytes`, that reads an image file's bytes off disk and returns them
   as a raw byte IPC response (not JSON/base64-wrapped).
2. A frontend shared image-loading pipeline (`src/editor/imageLoader.ts`) built on top of that command
   — a path-keyed Blob-URL cache — consumed by three call sites: the live-view editor pane (this
   plan's own §6), the preview pane (`markdown-preview/PLAN.md` §5.5), and the PDF export iframe
   (`pdf-export/PLAN.md` §3).

**Non-goals (deferred, not in scope here)**: image drag-to-resize handle, click-to-enlarge lightbox,
paste-time downscaling. See §7.

## 2. Module Layout

New files:

| File | Purpose |
|---|---|
| `src-tauri/src/commands/image.rs` (existing file, new function added) | `image_read_bytes` command (§3). |
| `src-tauri/src/fs/path_safety.rs` (existing file, new functions added — see note below) | `resolve_relative_within_base`/`has_parent_dir_segment` (§3.1), hoisted out of `commands/link.rs` so `image_read_bytes` can reuse them instead of duplicating. |
| `src/editor/imageLoader.ts` | Shared path-keyed Blob-URL cache, `getOrLoadImage`/`releaseMemoImages` (§5). |
| `src/ipc/image.ts` (existing file, new function added) | `imageReadBytes` wrapper around the new command (§4). |

**Note on `fs/path_safety.rs` not being a brand-new file**: `project-architecture/PLAN.md` §2 already
lists this path for a different, pre-existing concern — `validate_name`/`ValidationMode`/`is_long_path`
(single-path-component name validation: Windows-forbidden characters, reserved device names, the
260-character long-path warning; `persistence-fs/PLAN.md` §8). That existing content is **untouched**
by this plan. §3.1 below only *adds* two new, unrelated public functions to the same file (a natural
fit thematically — both are "is this path safe" checks — even though they solve different problems).

Modified files:

| File | Change |
|---|---|
| `src-tauri/src/commands/link.rs` | `resolve_file_path`'s body refactored to delegate to the new shared `path_safety::resolve_relative_within_base` (§3.1) — its own public signature, error codes, and all existing tests are unchanged (a behavior-preserving internal refactor, not a behavior change). |
| `src-tauri/src/main.rs` | `image_read_bytes` added to the `tauri::generate_handler![...]` list (alongside the existing `commands::image::image_save_pasted` line). |
| `src/editor/markdownDecorations.ts` | `MarkdownDecorationConfig` gains a `memoKey: string \| null` field (§5.4, §6.1); `ImagePlaceholderWidget`'s two call sites (`Image` node handling, bare `<img>` tag handling) replaced with a new `ImageWidget` (§6.2); `ImagePlaceholderWidget` itself is kept, unrenamed, as the read-failure fallback's own DOM builder (§6.5) rather than deleted. |
| `src/components/Editor/EditorPane.tsx` | Derives `memoKey` (§5.4) and calls `releaseMemoImages(memoKey)` on memo switch/unmount (§5.3); passes `memoKey` into `markdownConfigFacet`'s config alongside the existing `currentTabId` (`markdown-editing/PLAN.md`'s existing facet-update effect — one more field, same effect). |
| `src/editor/markdownDecorations.css` (or wherever `.cm-md-image-placeholder` is currently styled) | New `.cm-md-image`/`.cm-md-image-loading` rules (§6.3). |

## 3. Backend: `image_read_bytes` Command

Elaborates `docs/MDEditor-spec.md` §5's "Backend command" and "Backend path validation" bullets.

### 3.1 Shared Path-Safety Helper — `src-tauri/src/fs/path_safety.rs`

`docs/MDEditor-spec.md` §5 explicitly requires reusing `commands/link.rs`'s existing
`resolve_file_path`/`has_parent_dir_segment` pattern rather than writing new validation logic from
scratch. That logic is currently private to `link.rs`. Hoisted out, generalized, and made `pub` in
`fs/path_safety.rs`:

```rust
// src-tauri/src/fs/path_safety.rs — additions below the existing validate_name/is_long_path content
use std::path::{Path, PathBuf};
use crate::model::result::ErrorCode;

/// `true` if any component of `path` is a `..` parent-directory segment (parses both `/` and `\`
/// separators on Windows). Hoisted verbatim from `commands/link.rs`'s formerly-private
/// `has_parent_dir_segment`, so `image_read_bytes` (`commands/image.rs`) can reuse the identical
/// check instead of duplicating it.
pub fn has_parent_dir_segment(path: &Path) -> bool {
    path.components().any(|c| matches!(c, std::path::Component::ParentDir))
}

/// How `resolve_relative_within_base` treats an absolute `value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbsolutePathPolicy {
    /// Returned as-is, unresolved against `base` — `link.rs`'s `link_open_external` (`FilePath`
    /// kind) needs this: a user-typed absolute path is a legitimate, supported open target.
    AllowPassthrough,
    /// Rejected (`ErrorCode::NameInvalid`) — any command whose `value` must always resolve to
    /// somewhere *inside* `base` (e.g. `image_read_bytes`, where an image path is never
    /// legitimately absolute) needs this instead. Without this policy, an absolute `value` would
    /// silently discard `base` entirely when joined (`PathBuf::join`'s own semantics on Windows),
    /// defeating the containment check this function exists for.
    Reject,
}

/// Resolves `value` against `base`, rejecting directory traversal. Generalizes the containment
/// logic that used to be private to `commands/link.rs::resolve_file_path` (that function's own
/// doc comment, describing the Windows `has_root()`-without-`is_absolute()` edge case in full, is
/// not repeated here — read it for the complete rationale). Rejects: any `..` component
/// (`has_parent_dir_segment`), and a "rooted but not absolute" value (leading `\` or `/` with no
/// drive-letter prefix). A `None` base returns `value` unresolved-but-validated (the "no known tab
/// context" case, e.g. a `memo:` link typed with no tab currently open) — `image_read_bytes` never
/// calls this with `None`, since an image read always has a known tab/folder.
pub fn resolve_relative_within_base(
    value: &str,
    base: Option<&Path>,
    absolute_policy: AbsolutePathPolicy,
) -> Result<PathBuf, ErrorCode> {
    let path = Path::new(value);
    if path.is_absolute() {
        return match absolute_policy {
            AbsolutePathPolicy::AllowPassthrough => Ok(PathBuf::from(value)),
            AbsolutePathPolicy::Reject => Err(ErrorCode::NameInvalid),
        };
    }
    if has_parent_dir_segment(path) {
        return Err(ErrorCode::NameInvalid);
    }
    if path.has_root() {
        return Err(ErrorCode::NameInvalid);
    }
    Ok(match base {
        Some(b) => b.join(path),
        None => PathBuf::from(value),
    })
}
```

`commands/link.rs::resolve_file_path` is refactored to delegate to this (its own signature, error
codes, and doc-comment content otherwise untouched):

```rust
// src-tauri/src/commands/link.rs — body replaced, signature/behavior unchanged
fn resolve_file_path(value: &str, current_tab_id: Option<&str>, settings: &AppSettings) -> Result<String, AppErrorPayload> {
    let base = current_tab_id.and_then(|id| settings.tabs.iter().find(|t| t.id == id)).map(|t| PathBuf::from(&t.folder_path));
    crate::fs::path_safety::resolve_relative_within_base(value, base.as_deref(), crate::fs::path_safety::AbsolutePathPolicy::AllowPassthrough)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|code| AppErrorPayload::new(code, "file path is invalid"))
}
```

This is a pure internal refactor: every one of `link.rs`'s existing `resolve_file_path_*` tests
(absolute passthrough, relative-joins-against-tab-folder, no-known-base passthrough, `..`-segment
rejection, rooted-but-not-absolute rejection, subdirectory acceptance) asserts on `resolve_file_path`'s
public signature and `err.code`, not on its internal implementation, so all of them must continue to
pass unmodified after this refactor (verify with `cargo test` at implementation time — see Open Items).

### 3.2 `image_read_bytes` Command

```rust
// src-tauri/src/commands/image.rs — new function, alongside the existing image_save_pasted
use tauri::ipc::Response;
use crate::commands::tab_lock::{resolve_tab_access, TabAccess};
use crate::fs::path_safety::{resolve_relative_within_base, AbsolutePathPolicy};

/// Reads an image file's raw bytes and returns them as a raw-byte IPC response (Tauri 2.11.5's
/// `tauri::ipc::Response`), not base64/JSON-wrapped (`docs/MDEditor-spec.md` §5). See §3.3 for why
/// this command's return type deviates from the project's usual `AppResult<T>` JSON-envelope
/// convention (`project-architecture/PLAN.md` §3).
#[tauri::command]
pub fn image_read_bytes(
    app: AppHandle,
    tab_id: String,
    relative_path: String,
    store: State<AppSettingsStore>,
    registry: State<SelfWriteRegistry>,
    sessions: State<LockSessionStore>,
) -> Result<Response, AppErrorPayload> {
    let (settings, outcome) = store.get(&registry);
    emit_settings_recovery_event(&app, outcome);

    // Same tab-folder resolution `image_save_pasted` uses (`image-handling/PLAN.md` §3 step 1):
    // a locked tab with an active session still resolves to the tab's own plain, unobfuscated
    // folder, since images are out of scope for tab-lock's `.md`-only encryption/obfuscation.
    let folder = match resolve_tab_access(&settings, &sessions, &tab_id) {
        Ok(TabAccess::Plain { folder, .. }) => folder,
        Ok(TabAccess::Session { folder, .. }) => folder,
        Err(e) => return Err(e),
    };

    // Server-side path validation, independent of the frontend's own `isSafeRelativeImageSrc`
    // gate (`docs/MDEditor-spec.md` §5: "an IPC command can be invoked directly, bypassing any
    // frontend gate"). `AbsolutePathPolicy::Reject` — unlike `link.rs`'s file-path resolution, an
    // image path is never legitimately absolute.
    let resolved = resolve_relative_within_base(&relative_path, Some(&folder), AbsolutePathPolicy::Reject)
        .map_err(|code| AppErrorPayload::new(code, "image path is invalid"))?;

    let bytes = std::fs::read(&resolved).map_err(|err| {
        tracing::error!(error = %err, path = %resolved.display(), "image_read_bytes: failed to read image file");
        AppErrorPayload::new(ErrorCode::NotFound, "failed to read image file")
    })?;

    Ok(Response::new(bytes))
}
```

`registry` is threaded through only to satisfy `AppSettingsStore::get`'s existing signature (every
command's standard preamble, `image_save_pasted`'s own first two lines) — nothing is self-write-
registered here, since this is a pure read with no write side effect.

`main.rs`: add `commands::image::image_read_bytes,` to the existing `tauri::generate_handler![...]`
list, next to the existing `commands::image::image_save_pasted,` line.

### 3.3 Deviation from the `AppResult<T>` Convention (Load-Bearing — Read This)

`project-architecture/PLAN.md` §3 mandates that "all Tauri `#[command]` functions return
`AppResult<T>`" (a hand-rolled `Serialize` impl producing `{ok: true, value}` / `{ok: false, error}`
JSON) and that every `src/ipc/*.ts` wrapper "returns `Promise<Result<T>>` and never throws for
expected/recoverable failure paths." `tauri::ipc::Response` is fundamentally incompatible with that
JSON envelope: it is a special marker return type Tauri's IPC layer detects and uses to send the
raw byte body directly, bypassing JSON serialization of the return value entirely — there is no way to
wrap a raw-byte response inside `{ok: true, value: ...}` and still have it arrive as raw bytes.

**Resolution, scoped to this one command only**: `image_read_bytes` returns
`Result<tauri::ipc::Response, AppErrorPayload>` instead of `AppResult<Response>`. A backend `Err`
therefore surfaces as a **rejected** JS promise from `invoke()` (Tauri's own IPC error path), not as
an `{ok: false, error}` value — the opposite of every other command in this codebase. `src/ipc/image.ts`'s
`imageReadBytes` wrapper (§4) must `try/catch` around `invoke()` rather than checking `.ok`, and
`imageLoader.ts`'s caller (§5.2) treats any thrown error the same as a `NotFound`/read-failure (§6.5's
fallback) — the specific `AppErrorPayload.code` is not distinguished at the call site, since every
failure mode here (missing file, path rejected, locked tab) has the identical, single correct UI
response: fall back to the placeholder. This narrow, documented exception is the only command in the
codebase that behaves this way; do not generalize the pattern to other commands.

## 4. Frontend: `src/ipc/image.ts` Wrapper

```ts
// src/ipc/image.ts — new function, alongside the existing imageSavePasted
import { invoke } from "@tauri-apps/api/core";

/** Reads an image file's raw bytes back from disk (`docs/MDEditor-spec.md` §5). Unlike every other
 * function in this file (and every other `ipc/*.ts` wrapper in the codebase), this one can
 * **throw** — `image_read_bytes` returns a raw `tauri::ipc::Response` on the Rust side, which is
 * incompatible with the `AppResult<T>`/`{ok, value|error}` envelope every other command uses, so a
 * backend error surfaces as a rejected promise instead (§3.3 of this plan's PLAN.md). Callers
 * (`src/editor/imageLoader.ts`) must `try/catch`, not check `.ok`. */
export async function imageReadBytes(tabId: string, relativePath: string): Promise<Uint8Array> {
  const buffer = await invoke<ArrayBuffer>("image_read_bytes", { tabId, relativePath });
  return new Uint8Array(buffer);
}
```

(`invoke()`'s TypeScript return type for a raw-byte `tauri::ipc::Response` command is an `ArrayBuffer`
at runtime — verify this empirically against the actual installed `@tauri-apps/api` version at
implementation time, see Open Items.)

## 5. Frontend: Shared Image-Loading Pipeline — `src/editor/imageLoader.ts`

Elaborates `docs/MDEditor-spec.md` §5's "Blob-URL cache" bullet. Shared, module-level state (a single
cache instance for the whole app, not per-component) — consumed by this plan's own live-view widget
(§6), `markdown-preview/PLAN.md` §5.5's `getOrLoadImage` call, and `pdf-export/PLAN.md` §3's fallback
`entry.bytes` access.

### 5.1 Cache Design

```ts
interface ImageCacheEntry {
  blobUrl: string;
  bytes: Uint8Array;
  lastAccessMs: number;
}

const cache = new Map<string, ImageCacheEntry>();
// De-dupes concurrent `getOrLoadImage` calls for the same key (e.g. the live-view widget and the
// preview pane both resolving the same image at nearly the same moment) into a single IPC read.
const inFlight = new Map<string, Promise<ImageCacheEntry | null>>();

// Tunable; bounds total Blob memory retained across all currently-open memos combined, not just
// one memo's own images. Exact value is implementation discretion (`docs/MDEditor-spec.md` §5 only
// specifies "a fixed size bound", not a number).
const MAX_CACHE_ENTRIES = 200;

function cacheKey(memoKey: string, tabId: string, relativePath: string): string {
  return `${memoKey}::${tabId}::${relativePath}`;
}
```

**Key includes `memoKey`, not just `tabId`+`relativePath`**: `docs/MDEditor-spec.md` §5 says eviction
triggers on "the owning memo being closed — revoke all of that memo's cached entries." Scoping the
cache key itself by `memoKey` makes that eviction a trivial prefix-match delete (§5.3) with no
reference-counting needed. Trade-off, accepted: if the same image path is referenced from two
*different* currently-open memos in the same tab, it is fetched and cached twice (once per
`memoKey`) rather than shared — an extra IPC read in an already-rare case, far simpler than tracking
per-entry referrer sets across memo lifecycles.

### 5.2 `getOrLoadImage`

```ts
/** Returns a `blob:` URL for `relativePath` (relative to `tabId`'s folder), fetching and caching it
 * on first use. Returns `null` on any failure (missing file, rejected path, locked-tab-with-no-
 * session, IPC error) — callers render their own fallback (§6.5) rather than this function
 * distinguishing failure kinds (§3.3's rationale: every failure mode has the same correct UI
 * response here). `memoKey` scopes the cache entry to the calling memo (§5.1, §5.4). */
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
    bytes = await imageReadBytes(tabId, relativePath); // src/ipc/image.ts — throws on failure, §3.3
  } catch {
    return null;
  }
  const blobUrl = URL.createObjectURL(new Blob([bytes]));
  const entry: ImageCacheEntry = { blobUrl, bytes, lastAccessMs: Date.now() };
  cache.set(key, entry);
  evictIfOverBound();
  return entry;
}
```

### 5.3 Eviction

```ts
/** Called on memo close/switch (`EditorPane.tsx`, §5.4) — revokes and removes every cache entry
 * belonging to `memoKey`, regardless of which tab/path they point at. */
export function releaseMemoImages(memoKey: string): void {
  const prefix = `${memoKey}::`;
  for (const [key, entry] of cache) {
    if (key.startsWith(prefix)) {
      URL.revokeObjectURL(entry.blobUrl);
      cache.delete(key);
    }
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
```

No special handling at app shutdown — the OS reclaims the underlying memory automatically
(`docs/MDEditor-spec.md` §5, restated here since it's directly relevant to this section).

### 5.4 `memoKey`: Definition (Fills a Gap Left Open by the Sibling Plans)

`markdown-preview/PLAN.md`'s `PreviewPaneProps` and `pdf-export/PLAN.md`'s `exportMemoToPdf` both
already reference a `memoKey: string` parameter (for this cache, per each plan's own text) without
defining how it's derived — that definition belongs here, since this plan owns the cache it identifies
entries in:

```ts
// src/components/Editor/EditorPane.tsx
const memoKey = tabId && memoName ? `${tabId}:${memoName}` : null;
```

Passed down as `MarkdownDecorationConfig.memoKey` (§6.1), `PreviewPaneProps.memoKey`, and
`exportMemoToPdf`'s `memoKey` argument — the same derived value reused at all three call sites, computed
once per render in `EditorPane.tsx` (not recomputed independently by each consumer).

**Eviction wiring** (`EditorPane.tsx`):

```ts
useEffect(() => {
  return () => {
    if (memoKey) releaseMemoImages(memoKey);
  };
}, [memoKey]);
```

Fires on every `memoKey` change (switching to a different memo evicts the *previous* memo's entries)
and on unmount — matching `docs/MDEditor-spec.md` §5's "owning memo being closed" trigger.

## 6. Live-View Real Image Rendering — `markdownDecorations.ts`

Elaborates `docs/MDEditor-spec.md` §5's remaining bullets (backend/cache covered above). Closes the
image-rendering half of `docs/QA.md`'s "Real `<img>` Rendering + Drag-Resize-Handle in Live View"
entry (the drag-resize-handle half stays deferred, §7).

### 6.1 `MarkdownDecorationConfig` Addition

```ts
// src/editor/markdownDecorations.ts
export interface MarkdownDecorationConfig {
  // ...existing fields (openTrigger, readOnly, currentTabId, confirmBeforeOpen, onOpenMemo,
  // requestExternalConfirm)...
  /** Identifies the currently-open memo for image-cache scoping (§5.4 of
   * `docs/plans/markdown/image-handling/PLAN.md`). `null` only when no memo is open at all, in
   * which case no `Image`/`<img>` node can be reached anyway (there is no document to decorate). */
  memoKey: string | null;
}
```

`DEFAULT_CONFIG` gains `memoKey: null`. `currentTabId` already exists on this config (used by
link-resolution) — `image_read_bytes`'s `tabId` argument reuses that same field, no new field needed
for it.

### 6.2 New `ImageWidget`

Replaces `ImagePlaceholderWidget` at both of its current call sites (`markdownDecorations.ts`'s
`Image` lezer-node handling and its bare `<img>`-tag handling via `findImageTags`) — `import {
getOrLoadImage } from "./imageLoader";`:

```ts
class ImageWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly alt: string,
    readonly tabId: string | null,
    readonly memoKey: string | null,
  ) {
    super();
  }

  eq(other: ImageWidget): boolean {
    return other.src === this.src && other.alt === this.alt && other.tabId === this.tabId && other.memoKey === this.memoKey;
  }

  toDOM(): HTMLElement {
    // No tab/memo context at all — nothing to resolve against; render the fallback immediately
    // rather than attempting a call that can't succeed.
    if (!this.tabId || !this.memoKey) {
      return new ImagePlaceholderWidget(this.src, this.alt).toDOM();
    }
    const img = document.createElement("img");
    img.className = "cm-md-image cm-md-image-loading";
    img.alt = this.alt;
    img.title = this.src;
    // Lazy by construction (§6.4): this fetch only starts once CodeMirror actually calls toDOM()
    // for a widget that entered the viewport, not eagerly during decoration-set build.
    void getOrLoadImage(this.tabId, this.src, this.memoKey).then((blobUrl) => {
      if (blobUrl) {
        img.src = blobUrl;
        img.classList.remove("cm-md-image-loading");
      } else {
        // Read-failure fallback (§6.5): swap in the same placeholder markup used before this
        // feature existed, rather than leaving a broken native <img> icon on screen.
        img.replaceWith(new ImagePlaceholderWidget(this.src, this.alt).toDOM());
      }
    });
    return img;
  }
}
```

Call-site changes (both existing lines, `config` already in scope at each):

```ts
// Image lezer-node handling (was: new ImagePlaceholderWidget(urlText, altText))
addReplace(node.from, node.to, Decoration.replace({ widget: new ImageWidget(urlText, altText, config.currentTabId, config.memoKey) }));

// Bare <img>-tag handling (was: new ImagePlaceholderWidget(img.src, ""))
addReplace(img.from, img.to, Decoration.replace({ widget: new ImageWidget(img.src, "", config.currentTabId, config.memoKey) }));
```

`ImagePlaceholderWidget` itself is **not deleted** — it becomes purely a fallback DOM builder, called
from `ImageWidget.toDOM()`'s two failure branches above (no tab/memo context; async load failure), and
is no longer directly used as a decoration widget on its own.

### 6.3 Display Cap CSS

```css
.cm-md-image { max-width: 240px; max-height: 240px; width: auto; height: auto; object-fit: contain; vertical-align: middle; }
.cm-md-image-loading { opacity: 0.5; }
```

`max-width`/`max-height` with `width:auto`/`height:auto` preserves aspect ratio and never touches the
image's stored `width`/`height` attribute values in the Markdown/HTML source (`docs/MDEditor-spec.md`
§5) — this is a purely cosmetic display cap for the editing view. Exact pixel value (`240px` above) is
implementation discretion; the spec only requires "a small fixed size."

### 6.4 Lazy Loading

No dedicated mechanism needed, per `docs/MDEditor-spec.md` §5: CodeMirror 6 only calls a widget's
`toDOM()` for widgets that actually enter the rendered viewport, so triggering the `getOrLoadImage`
call from inside `toDOM()` itself (§6.2, not during decoration-set build) is lazy by construction —
scrolling a large document with many images never fetches images that never entered view.

### 6.5 Read-Failure Fallback

On any failure — missing/moved file, rejected path, `image_read_bytes`'s thrown promise rejection
(§3.3) — `getOrLoadImage` resolves to `null` (§5.2), and `ImageWidget.toDOM()`'s `.then()` branch
swaps in `ImagePlaceholderWidget`'s own DOM (§6.2) in place of the (never-`src`-set) broken `<img>`
element, rather than leaving a native broken-image icon on screen.

### 6.6 Source-View Mode

Unaffected: source mode disables the whole decoration `Compartment` (returns `[]`), so images — like
every other construct — render as literal raw text there, unchanged by this plan.

## 7. Deferred Items

Per `docs/MDEditor-spec.md` §9, carried into `docs/QA.md`'s existing "Real `<img>` Rendering +
Drag-Resize-Handle in Live View" entry (not resolved by this plan, tracked as explicit future
follow-ups, not rejected):

- **Image drag-to-resize handle** — dragging a handle on a rendered image would rewrite
  `![alt](path)` → `<img src width height>`. This plan renders real images but implements no resize
  interaction.
- **Click-to-enlarge lightbox preview** for images.
- **Downscaling/re-encoding images at paste time** to save disk space (a concern for the write-path
  plan, `image-handling/PLAN.md`, not this one — noted here only because it was raised alongside the
  other two deferred items in the same spec section).

## 8. Consumers (Cross-References, Not Duplicated Here)

- `markdown-preview/PLAN.md` §5.5 — resolves `img[data-relative-src]` elements in the rendered preview
  HTML via this plan's `getOrLoadImage`, after the container's `innerHTML` is set.
- `pdf-export/PLAN.md` §3 — resolves images inside the hidden print iframe's own document via the same
  cache, with a fallback to `peekCachedBytes` (§5.2 above) if a `blob:` URL minted in the main window
  doesn't resolve across documents (that plan's own open item).

## Open Items

1. **`link.rs` refactor verification**: after hoisting `has_parent_dir_segment`/adding
   `resolve_relative_within_base` to `fs/path_safety.rs` and reworking `resolve_file_path` to delegate
   to it (§3.1), run `link.rs`'s existing test suite (`cargo test`) to confirm every
   `resolve_file_path_*` test still passes unmodified — this refactor must be behavior-preserving.
2. **`AppResult<T>` deviation (§3.3)**: verify empirically, early in implementation, exactly what
   shape `invoke()`'s rejection takes for `image_read_bytes`'s `Err(AppErrorPayload)` case against the
   actual installed Tauri/`@tauri-apps/api` version (2.11.5) — whether the rejected value is the
   `AppErrorPayload` JSON object itself, a wrapped error object, or a plain string — since
   `imageLoader.ts`'s `try/catch` (§5.2) only needs to know "did it fail," but a future consumer that
   wants the actual error code would need to know the exact shape.
3. **`invoke()`'s raw-`ArrayBuffer` return type (§4)**: confirm the installed `@tauri-apps/api`
   version's TypeScript types and runtime behavior actually hand back an `ArrayBuffer` (not, e.g., a
   `Uint8Array` already, or something requiring an extra unwrap step) for a command returning
   `tauri::ipc::Response`.
4. **Command-inventory registration**: add a new row for `image_read_bytes` to
   `project-architecture/PLAN.md` §4's command inventory table once implemented (not done as part of
   this planning pass, since that table is meant to reflect implemented commands — see that plan's own
   convention).
