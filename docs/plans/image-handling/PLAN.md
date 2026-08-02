# PLAN: Image Handling (Paste, Storage, Resize)

> Status: §1-§3 (storage rule, filename generation, `image_save_pasted` command,
> `src-tauri/src/commands/image.rs`) and §6's frontend flow (`src/editor/imagePaste.ts`'s real paste
> handler and toolbar "Image insert" file-picker action, both inserting `![alt](img_xxxx.png)` as a
> single undoable transaction, error surfaced via the existing `state/useEditor.ts` `lastErrorCode`
> banner) are implemented. §4's second/third bullets (resize-handle interaction converting markup to
> `<img width height>`) and the real `<img>` rendering it depends on are **not** implemented —
> `markdownDecorations.ts`'s `ImagePlaceholderWidget` stub still renders pasted/inserted images as a
> placeholder, unchanged from before this pass. This gap spans this plan and
> `markdown-editing/PLAN.md` (each pointed at the other for who owns rendering/resize, neither
> actually specifies the mechanism) and needs a new architectural decision with real security
> tradeoffs (Tauri asset-protocol scope for arbitrary tab folders vs. a new base64/bytes-read IPC
> command) that isn't decidable from existing confirmed spec — see `docs/QA.md`'s "Real `<img>`
> Rendering + Drag-Resize-Handle in Live View" entry, added during this pass. §5 (no orphan cleanup)
> required no implementation (a confirmed non-behavior). Depends on `project-architecture/PLAN.md` and
> `persistence-fs/PLAN.md` (for the name-collision-retry pattern, though images use a separate,
> simpler write path — see §3).

Sources: `docs/spec.md` §4.2; `docs/detailed-spec.md` §3.6, §4.7.

---

## 1. Storage Rule

Images are saved **directly under the tab's folder** (no subfolder), sibling to `.md` files
(`[spec §4.2]`). They are never scanned as memos (only `.md` files are scanned per `[spec §3.1]`), so
they never appear in the tree, are never subject to tab-lock filename obfuscation
(`tab-lock/PLAN.md`), and are never indexed by full-text search (`search-index/PLAN.md`).

## 2. Filename Generation

`img_<16 lowercase hex chars>.<ext>` — the hex portion comes from a cryptographically random 8-byte
value (`rand::rngs::OsRng` or equivalent), formatted lowercase hex. Extension derived from the actual
pasted/inserted image format (`png`, `jpg`, `gif`, `webp`, etc. — sniff from the image byte header /
MIME type provided by the clipboard API, not from any user-supplied name). Clipboard data that is a
raw bitmap with no format info defaults to `.png` (`[spec §4.2]`, `[detailed-spec §3.6]`). On filename
collision (astronomically unlikely at 128 bits of randomness, but handled defensively), regenerate the
random suffix and retry.

## 3. Command

### `image_save_pasted(tab_id: String, bytes: Vec<u8>, mime_hint: Option<String>) -> AppResult<SavedImage>`
```rust
struct SavedImage { relative_path: String, width: u32, height: u32 }
```
1. Determine target folder = the tab's registered `folderPath`. If the tab is locked with an active
   session, the image still saves as a **plaintext, unobfuscated** file directly in the folder (images
   are explicitly out of scope for tab-lock's file-obfuscation/encryption mechanism, which only applies
   to `.md` files per `[spec §4.1 pt.1]`, `[detailed-spec §3.4]`'s fileMap scope) — this is a known,
   accepted consequence of tab lock's `.md`-only scope, not an ambiguity requiring escalation, since
   `spec.md` never describes image encryption anywhere and §4.1's mechanism is explicitly `.md`-file-
   scoped throughout. If the tab is locked with **no** session, this command should not be reachable
   (the editor for that tab isn't open at all), so no special handling is needed beyond normal
   `tab_id`-not-open validation.
2. Sniff format from `bytes` (magic-number detection, e.g. via the `image` crate's format-guessing, or
   trust `mime_hint` if the frontend clipboard API supplies a reliable MIME type — `developer-lite`'s
   choice which is primary/fallback).
3. Generate filename (§2), write via a plain `fs::write` (this is a brand-new file with no prior
   version — the atomic-write temp→bak→target dance in `persistence-fs/PLAN.md` §3 exists to protect
   against corrupting an *existing* file mid-write; a fresh image file has nothing to protect against
   losing, so a simple write-then-rename-from-temp — or even a direct write — is sufficient here, though
   using the same temp-file-then-rename pattern for crash-safety of the write itself, minus the bak
   step, is a reasonable middle ground left to `developer-lite`). Self-write-register the path so the
   watcher doesn't misclassify it as an external addition needing tree reconciliation (it isn't a `.md`
   file so reconciliation wouldn't touch it anyway, but registering is cheap and consistent).
4. Decode dimensions (`image` crate) for `width`/`height`.
5. Return `{ relative_path: "<filename>", width, height }` — relative to the tab folder, since the
   Markdown body always references images relatively (`[spec §4.2]`).

## 4. Insertion Markup

Decided by the frontend (`src/editor/imagePaste.ts`, `markdown-editing/PLAN.md`), not this command:
- Unresized paste: `![alt](img_xxxx.png)` (standard Markdown, `alt` may be empty or a generic
  placeholder — `spec.md` doesn't mandate specific alt text).
- After a resize interaction (drag handle on the rendered image in live view): the single image's
  markup converts to `<img src="img_xxxx.png" width=".." height="..">` (`[spec §4.2]`, HTML form
  because standard `![alt](url)` has no width/height slot).
- Reverting a resize to "natural size": either restore `![alt](url)` or keep `<img>` with natural-size
  dimensions — either acceptable, not spec-mandated (`[detailed-spec §5.10]`).

## 5. No Orphan Cleanup

**Confirmed, permanent**: no automatic cleanup of orphaned image files ever occurs, whether triggered
by memo deletion, reference removal, or any other event (`[spec §4.2]`, `[detailed-spec §3.6]`).
Images persist on disk indefinitely unless the user manually deletes them outside the app. **Do not
add this later without a spec change** — this is an explicit, deliberate decision (avoids the risk of
mis-detecting a still-referenced or intentionally-kept image as orphaned and destroying it). No UI
surface for "find orphaned images" is in scope either.

## 6. Frontend Flow

1. User pastes (Ctrl+V with image data on clipboard) or uses the toolbar's image-insert action
   (`markdown-editing/PLAN.md` §4) while the cursor is in the editor.
2. `imagePaste.ts` reads the clipboard image bytes (or, for toolbar-insert, opens a file picker and
   reads the chosen file's bytes), calls `image_save_pasted`.
3. On success, inserts the returned Markdown/HTML markup at the cursor as a single CodeMirror
   transaction (undoable in one step).
4. On failure (e.g. `IoLocked` if the folder is momentarily locked by another process), shows a
   non-blocking error toast; no partial insertion occurs.

## Open Items

None.
