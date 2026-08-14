# PLAN: Image Handling (Paste, Storage, Resize)

> Status: §1-§3 (storage rule, filename generation, `image_save_pasted` command,
> `src-tauri/src/commands/image.rs`) and §6's frontend flow (`src/editor/imagePaste.ts`'s real paste
> handler and toolbar "Image insert" file-picker action, both inserting `![alt](memo_name/NNN.png)` as
> a single undoable transaction, error surfaced via the existing `state/useEditor.ts` `lastErrorCode`
> banner) are implemented, including GitHub issue #139's per-memo subfolder + sequential naming (a
> plain-tab-only change; a locked tab's active unlock session still uses the original flat/anonymous
> scheme — §1/§2 below). §4's second/third bullets (resize-handle interaction converting markup to
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

**Plain (non-locked) tab** (GitHub issue #139): images are saved under a **per-memo subfolder**,
`<tab_folder>/<memo_name>/` — the memo body (`.md`) itself is unaffected, still saved flat directly
under the tab folder (`[spec §4.2]`, `[spec §3.2]`'s flat-rebuild-on-metadata-loss crash-recovery
guarantee scopes only to `.md` bodies, not attachments). `memo_rename` follow-renames the subfolder
(`<old_name>/` → `<new_name>/`) so existing `![](old_name/NNN.png)` references stay valid; a memo
with no attachments simply has no subfolder. `memo_delete` does **not** delete the subfolder (§5).

**Locked tab, active unlock session**: keeps the original flat, anonymously-named scheme instead — no
per-memo subfolder. During a session the frontend deals in real memo names, and `.md` bodies are
stored under obfuscated random-hex tokens specifically so nothing about a locked tab's real structure
is recoverable from the plain filesystem without the password (`tab-lock/PLAN.md`, `[spec §4.1]`). A
folder literally named after the real memo name would leak exactly that, so this case is deliberately
excluded from issue #139's change.

Either way, images are never scanned as memos (only `.md` files are scanned per `[spec §3.1]` — a
per-memo image subfolder is simply ignored, the same as the old flat `img_*.png` files were), are
never subject to tab-lock's own `.md`-body filename obfuscation (`tab-lock/PLAN.md`), and are never
indexed by full-text search (`search-index/PLAN.md`).

## 2. Filename Generation

**Plain tab**: sequential, zero-padded to (at least) 3 digits — `001.<ext>`, `002.<ext>`, ... — scoped
to the memo's own subfolder. Computed as one past the largest existing numeric stem in that folder,
regardless of extension (so `001.png` + `003.jpg` present → next is `004`, not `002`). On collision
(shouldn't happen given the above, but handled defensively), re-scan and retry.

**Locked tab, active unlock session**: unchanged from before issue #139 —
`img_<16 lowercase hex chars>.<ext>`, the hex portion from a cryptographically random 8-byte value
(`rand::rngs::OsRng` or equivalent), formatted lowercase hex. On collision (astronomically unlikely at
128 bits of randomness, but handled defensively), regenerate the random suffix and retry.

Either case: extension derived from the actual pasted/inserted image format (`png`, `jpg`, `gif`,
`webp`, etc. — sniff from the image byte header / MIME type provided by the clipboard API, not from
any user-supplied name). Clipboard data that is a raw bitmap with no format info defaults to `.png`
(`[spec §4.2]`, `[detailed-spec §3.6]`).

## 3. Command

### `image_save_pasted(tab_id: String, memo_name: String, bytes: Vec<u8>, mime_hint: Option<String>) -> AppResult<SavedImage>`
```rust
struct SavedImage { relative_path: String, width: u32, height: u32 }
```
1. Validate `memo_name` (`validate_name`, `ValidationMode::Strict` — same check `memo_create`/
   `memo_rename` apply) before using it as a filesystem path component; an IPC command can be invoked
   directly, bypassing the frontend's own memo-open state (`docs/MDEditor-spec.md` §5).
2. Determine target folder = the tab's registered `folderPath`, plus which naming scheme applies (§1):
   a **plain** tab resolves to `<folderPath>/<memo_name>/` (created if missing) and the sequential
   scheme; a **locked tab with an active session** resolves to `folderPath` directly (no subfolder) and
   the flat/anonymous scheme — the image still saves as a **plaintext, unobfuscated** file either way
   (images are explicitly out of scope for tab-lock's `.md`-file encryption mechanism per
   `[spec §4.1 pt.1]`, `[detailed-spec §3.4]`'s fileMap scope). If the tab is locked with **no** session,
   this command should not be reachable (the editor for that tab isn't open at all), so no special
   handling is needed beyond normal `tab_id`-not-open validation.
3. Sniff format from `bytes` (magic-number detection, e.g. via the `image` crate's format-guessing, or
   trust `mime_hint` if the frontend clipboard API supplies a reliable MIME type — `developer-lite`'s
   choice which is primary/fallback).
4. Generate filename (§2), write via a plain `fs::write` (this is a brand-new file with no prior
   version — the atomic-write temp→bak→target dance in `persistence-fs/PLAN.md` §3 exists to protect
   against corrupting an *existing* file mid-write; a fresh image file has nothing to protect against
   losing, so a simple write-then-rename-from-temp — or even a direct write — is sufficient here, though
   using the same temp-file-then-rename pattern for crash-safety of the write itself, minus the bak
   step, is a reasonable middle ground left to `developer-lite`). Self-write-register the path so the
   watcher doesn't misclassify it as an external addition needing tree reconciliation (it isn't a `.md`
   file so reconciliation wouldn't touch it anyway, but registering is cheap and consistent).
5. Decode dimensions (`image` crate) for `width`/`height`.
6. Return `{ relative_path, width, height }`, relative to the tab folder — `<memo_name>/<filename>`
   (plain tab) or just `<filename>` (locked-session, flat, as before), always `/`-separated regardless
   of platform. The Markdown body always references images relatively (`[spec §4.2]`).

## 4. Insertion Markup

Decided by the frontend (`src/editor/imagePaste.ts`, `markdown-editing/PLAN.md`), not this command:
- Unresized paste: `![alt](memo_name/001.png)` (standard Markdown, `alt` may be empty or a generic
  placeholder — `spec.md` doesn't mandate specific alt text; `img_xxxx.png` in the locked-session case).
- After a resize interaction (drag handle on the rendered image in live view): the single image's
  markup converts to `<img src="memo_name/001.png" width=".." height="..">` (`[spec §4.2]`, HTML form
  because standard `![alt](url)` has no width/height slot).
- Reverting a resize to "natural size": either restore `![alt](url)` or keep `<img>` with natural-size
  dimensions — either acceptable, not spec-mandated (`[detailed-spec §5.10]`).

## 5. No Orphan Cleanup

**Confirmed, permanent**: no automatic cleanup of orphaned image files ever occurs, whether triggered
by memo deletion, reference removal, or any other event (`[spec §4.2]`, `[detailed-spec §3.6]`).
Images persist on disk indefinitely unless the user manually deletes them outside the app. **Do not
add this later without a spec change** — this is an explicit, deliberate decision (avoids the risk of
mis-detecting a still-referenced or intentionally-kept image as orphaned and destroying it). No UI
surface for "find orphaned images" is in scope either. For a plain tab (§1), this extends to the
per-memo subfolder itself: `memo_delete` deletes only the memo's `.md` file, never the subfolder — an
orphaned subfolder is no different in kind from an orphaned flat image file pre-#139, just now grouped
under its old memo name instead of unlabeled.

## 6. Frontend Flow

1. User pastes (Ctrl+V with image data on clipboard) or uses the toolbar's image-insert action
   (`markdown-editing/PLAN.md` §4) while the cursor is in the editor.
2. `imagePaste.ts` reads the clipboard image bytes (or, for toolbar-insert, opens a file picker and
   reads the chosen file's bytes), downscales via `imageDownscale.ts`'s `downscaleImageIfNeeded`
   (below) if oversized, then calls `image_save_pasted`.
3. On success, inserts the returned Markdown/HTML markup at the cursor as a single CodeMirror
   transaction (undoable in one step).
4. On failure (e.g. `IoLocked` if the folder is momentarily locked by another process), shows a
   non-blocking error toast; no partial insertion occurs.

### 6.1 Paste/Insert Downscaling (GitHub issue #82)

`imageDownscale.ts`'s `downscaleImageIfNeeded(bytes, mimeType)`, called from `saveAndInsertImage`
(the function shared by both the paste handler and `insertImageViaFilePicker`) before
`image_save_pasted`:
- Threshold: 2000px on the longer edge, fixed constant (`MAX_DIMENSION_PX`), not user-configurable
  in this pass — no `AppSettings` field or Settings UI.
- Already-small images (both dimensions ≤ threshold) pass through unchanged.
- `image/gif` is always passed through unchanged (canvas draw would flatten to one frame).
- Otherwise: `createImageBitmap` → scale to fit threshold (aspect ratio preserved) →
  `OffscreenCanvas` → `convertToBlob({ type: <original mimeType> })` — same format as the source.
- Any decode/encode failure falls back to the original bytes (never blocks the paste/insert).

## Open Items

None.
