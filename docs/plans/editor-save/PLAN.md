# PLAN: Editor / Save (Right Pane — Core Editing & Persistence)

> Status: implemented (backend commands in `src-tauri/src/commands/editor.rs` +
> `src-tauri/src/store/memo_file_service.rs`, frontend `components/Editor/`,
> `components/Search/`, `state/useEditor.ts`, `state/useSearch.ts`, `editor/textCommands.ts`).
> The encrypt-on-save/decrypt-on-load path for a locked tab's active session
> (`tab-lock/PLAN.md`) this file originally deferred is now implemented too — `editor_load`/
> `editor_save` dispatch to `commands/tab_lock.rs`'s `session_editor_load`/`session_editor_save`
> via `resolve_tab_access`'s `TabAccess::Session` branch before ever reaching this file's own
> plain-file path below. Still deferred: live full-text index patching on save
> (`search-index/PLAN.md`, currently a marked no-op seam), and `evt:editor:conflict` is never
> actually emitted yet since `persistence-fs/PLAN.md`'s folder watcher isn't wired up to a running
> tab folder (the frontend listener exists as a harmless forward seam). The Markdown-specific
> rendering layer (live view, CodeMirror decoration extension, toolbar, image resize markup) is a
> **separate** plan, `markdown-editing/PLAN.md`, layered on top of the plain-text editing surface
> implemented here — not built by this pass. Depends on `project-architecture/PLAN.md` and
> `persistence-fs/PLAN.md`.

Sources: `docs/spec.md` §2.3, §2.6; `docs/detailed-spec.md` §4.3, §5.3, §5.6, §6.4, §7.

## Implementation Notes (resolutions of this plan's discretion points)

- §2 `editor_save` step 1's flagged open item (exact `ErrorCode` for a save attempt against an
  edit-locked memo) did not turn out to be reachable in a way that matters — the frontend makes
  the editor read-only whenever `editLocked` is true, so this stayed a defensive guard. Resolved
  with `Unknown`, exactly as the plan's own fallback already specified; not escalated to
  `designer-lite`.
- `store/memo_file_service.rs` is the shared encoding/EOL/hash module the plan's §2 anticipated;
  `commands/tree.rs`'s pre-existing `decode_memo_body` now delegates to it instead of keeping its
  own copy (that function's own prior TODO comment invited exactly this consolidation).
- `editor_save`'s EOL restoration (step 3) needed no separate client-supplied `eol` parameter,
  contrary to a first read of the command signature possibly implying otherwise: since the
  hash-conflict check (step 2) already re-reads the current on-disk bytes and proves them
  byte-identical to what was loaded, the EOL style is re-derived from those same bytes at save
  time rather than trusting a round-tripped client value.
- "Search with Google" (§4) needed an OS-standard external-open primitive that `link/PLAN.md`
  hasn't implemented yet. Added a minimal `link_open_external` command
  (`src-tauri/src/commands/link.rs`) — only the plain `open_url` pass-through, explicitly not
  `link/PLAN.md`'s `memo:`-resolution/confirm-dialog/email-special-casing scope — as a forward
  seam, matching the master command inventory's reserved name.
- Checkbox cycle (§4 "Check" menu action) mechanics (which glyph on the cursor's line gets
  cycled, insert-if-absent behavior) aren't spelled out beyond "insert/cycles at cursor" —
  resolved as: cycle the first `☐`/`☑` found on the current line, else insert a fresh `☐ ` at the
  cursor. Implementation discretion (`[detailed-spec §9]`-style), not escalated.
- Word wrap's "character-count" mode (§4) has no accompanying numeric column setting anywhere in
  the confirmed `AppSettings`/`MemoEditorSettings` schema (`[detailed-spec §3.2, §3.3]`) — used a
  fixed 80-column constant (`CHARS_WRAP_COLUMN` in `editor/textCommands.ts`).
- §4's grouping of "auto-indent, line numbers, whitespace/line-break visibility, cursor-line
  highlight" alongside the genuinely `MemoEditorSettings`-backed settings (wrap/tab
  width/soft-tabs/line-numbers/font-size) turned out to be looser than the actual schema: only
  the latter group has real fields. Auto-indent-on-Enter and cursor-line highlight are
  implemented as fixed, always-on behaviors; whitespace/line-break visibility is a session-only,
  non-persisted per-EditorPane toggle (context-menu item) rather than a settings field.
- `state/useSettings.ts` (a `project-architecture/PLAN.md`-anticipated file with no prior owner)
  was added by this pass, scoped narrowly to what this plan actually needs: load `AppSettings`
  once, expose it, and a generic `patch()` for later features to reuse — `settings_get`/
  `settings_set` already existed with no frontend consumer before this.
- Hiragana/katakana and half-width/full-width conversion use `jaconv` (a single well-tested
  library covering all four operations), per the plan's own "avoid a hand-rolled Unicode-range
  map" guidance.
- App-minimize detection uses `getCurrentWindow().onResized()` + `isMinimized()`, since Tauri v2
  has no dedicated "minimize" event of its own.
- Scroll-position persistence reuses `memo_set_editor_override` exactly as specified, debounced
  during scroll (500ms) and additionally flushed immediately whenever the memo being viewed is
  switched away from or the `EditorView` is torn down, at the same trigger points as the autosave
  flush.
- Functionally verified end-to-end via `tauri dev` + WebView2 CDP (see `EditorPane.tsx`'s own
  commit message for the exact scenarios exercised): load/display, live status bar, debounced
  autosave, immediate Ctrl+S, a real save-time hash conflict correctly surfacing the Esc-disabled
  dialog, "keep mine" overwrite resolution, and per-memo history isolation across a memo switch.
- Added post-review (found live by `reviewer-lite` while reviewing `tab-lock/PLAN.md`): a new
  `editor_close(tab_id, path)` command, the missing counterpart to `editor_load`'s
  `ActiveMemoTracker::set()` that this plan never added a caller for. `state/useEditor.ts` calls
  it on every switch-away (its load effect's cleanup), so `tab_lock`'s defensive open-memo check
  (`tab-lock/PLAN.md` §3 step 2) doesn't stay permanently tripped once a memo has ever been
  opened in a tab.
- Added for GitHub issue #28 ("Reopening a memo restores scroll position but not cursor
  position"): `MemoEditorSettings.cursorAnchor`/`cursorHead` (character offsets, mirroring
  CodeMirror's `EditorSelection.main.anchor`/`.head`), persisted through `MemoContent` in both
  `editor.rs`'s plain-tab path and `tab_lock.rs`'s session path. Written at the same trigger
  points and on the same debounce timer as `scrollPosition` (memo switch, debounced scroll,
  unmount) — grabbed together from the `EditorView` in one combined `memo_set_editor_override`
  call. Restored via `EditorState.create`'s `selection` option (not a post-mount dispatch, since
  scroll restoration needs the view's first layout pass but selection does not), clamped to the
  loaded document's length in case the file changed externally since the value was saved.

---

## 1. Data Flow Summary

CodeMirror 6 holds the memo's plain-text buffer (LF-normalized internally, per
`[detailed-spec §5.3]`) as the single source of truth for the currently-open memo. The Rust backend
never holds a "live" in-memory copy of the buffer between loads/saves — it only reads/writes the file
on disk at `editor_load`/`editor_save` call boundaries. Undo/redo history is entirely a CodeMirror
concern (frontend-only) and is discarded on memo switch (`[spec §2.3]`, "no cross-memo undo" — achieved
simply by creating a fresh `EditorState`/history per memo rather than reusing one editor instance's
history across switches).

## 2. Commands

### `editor_load(tab_id, path: NodePath) -> AppResult<MemoContent>`
```rust
struct MemoContent {
    text: String,        // LF-normalized for the editor
    encoding: Encoding,   // 'utf-8' | 'shift-jis' (informational, for status bar + re-save policy)
    eol: Eol,             // 'crlf' | 'lf' | 'cr' — detected dominant style, to restore on save
    hash: String,         // hash of the on-disk byte sequence AT LOAD TIME (§6.6 of spec.md scope: whole file bytes)
    edit_locked: bool,    // this memo's per-memo Edit Lock flag, read from tree metadata
    scroll_position: Option<u32>,  // MemoEditorSettings.scrollPosition, 1-indexed top-visible line number, None if never recorded
}
```
- `scroll_position` is read straight from this memo's `MemoEditorSettings.scrollPosition` field in the
  tab's tree metadata (`[detailed-spec §3.3]`) — **confirmed to persist across app restarts**, not just
  within a session (`DECISIONS.md` "UI-State Persistence Decisions"). This is a metadata-domain field,
  not a file-body field, so `editor_load` reads it via the same tree-metadata access
  `memo-tree/PLAN.md` §2/§3 already uses, not a second file read.
- **Document-position-based, not pixel-based** (`DECISIONS.md` "Scroll Position: Document-Position-Based
  Anchor, Not Raw Pixel Offset"): the stored value is a **1-indexed document line number**, not a raw
  `scrollTop` pixel offset — a pixel value is meaningless across a live/source view toggle or a
  font-size change, since decorations (headings, resized images, etc.) change visual line heights
  independently of document position. On load, once the editor mounts, the frontend restores scroll
  position by calling CodeMirror 6's `EditorView.scrollIntoView(pos, { y: 'start' })` with `pos` = the
  document offset of that line's start (`state.doc.line(n).from`), deferred to after the view's first
  layout/measure pass (e.g. via `requestAnimationFrame`) since accurate line-height measurement isn't
  available at the instant of mount. Clamp the saved line number to `state.doc.lines` if the memo has
  since become shorter than the saved value. `None`/absent → open at the top, e.g. a never-opened or
  freshly-created memo.
- Encoding detection (`[detailed-spec §5.3]`): (1) UTF-8 BOM present → decode as UTF-8, strip BOM.
  (2) No BOM → attempt strict UTF-8; on invalid byte sequence, retry as Shift_JIS (`encoding_rs` crate).
  (3) Both fail → hard read error, mapped to `ErrorCode::Unknown` (no dedicated code exists in
  `[spec §3.4]`'s enum for this rare case, `[detailed-spec §9]` confirms this is implementation
  discretion, not an ambiguity).
- EOL detection: scan for the dominant line-ending sequence; empty file or no line breaks → default to
  `crlf` (Windows platform default, `[detailed-spec §5.3]`).
- Sets this memo as `watch::active_memo` (see `persistence-fs/PLAN.md` §5) so the watcher knows which
  open memo to raise `evt:editor:conflict` for if an external change lands while it's open.
- If a locked tab is opened via `editor_load` while in an active session, the body is decrypted
  on-demand at this point (not pre-decrypted at unlock time) — see `tab-lock/PLAN.md`; the returned
  `hash` in that case is a hash of the **decrypted plaintext** (there is no meaningful "on-disk bytes"
  concept for a locked memo's plaintext, since the disk copy is always ciphertext) used purely for this
  session's own conflict-check symmetry with `editor_save`.

### `editor_save(tab_id, path: NodePath, text: String, expected_hash: String) -> AppResult<SaveResult>`
```rust
struct SaveResult { hash: String }
```
1. Refuse (`ErrorCode::Conflict`, no write) if this memo's per-memo Edit Lock is set — actually, per
   `[spec §2.2]`, Edit Lock blocks "text editing," which the frontend should already prevent by making
   the editor read-only; this command still defensively rejects a save attempt against a locked memo,
   returning a validation-style error (reuse `TabLocked`'s sibling concept — since there's no dedicated
   `ErrorCode` for edit-lock-blocked saves in `[spec §3.4]`'s enum, and this scenario is not one of the
   `[detailed-spec §9]`-enumerated discretion items, if this exact situation arises during
   implementation it should be raised to `designer-lite` rather than guessed — most likely resolution
   is "the frontend already prevents this input, so the backend path is unreachable in practice and can
   use `Unknown` defensively," but confirm rather than assume).
2. Compute hash of current on-disk bytes; compare against `expected_hash`. Mismatch → `Conflict`, no
   write (`[spec §5.5]`, hash-only check at this layer, no mtime/size).
3. Restore the original EOL style detected at load (unless `AppSettings.eol.policy` requests
   unification — `preserve` is the default meaning "keep what was detected," `crlf`/`lf`/`cr` force
   that style regardless of what was detected).
4. Re-encode per `AppSettings.encoding.defaultSave` (`utf-8` is the only currently-specified value,
   `[spec §3.4]` — always save as UTF-8 regardless of load-time-detected encoding, unless a future
   setting adds more values; not ambiguous, just a one-value enum today).
5. `atomic_write` (via `persistence-fs/PLAN.md`).
6. Patch the full-text index for this memo (`search-index/PLAN.md`).
7. Compute + return the new hash (of the just-written bytes) so the frontend updates its
   `expected_hash` for the next save without a round-trip re-read.
8. If the tab is locked with an active session, this same command encrypts before writing (fresh IV
   per save) instead of steps 3–5's plain path — see `tab-lock/PLAN.md` for the exact byte layout; the
   hash-conflict check in step 2 still applies, computed against the previously-known plaintext hash
   from this session (not the on-disk ciphertext, which changes every save due to the fresh IV and
   would make hash comparison meaningless).

### Autosave
Not a separate command — the frontend's `state/useEditor.ts` hook calls `editor_save` on:
- a debounce timer (`AppSettings.autosave.plainDebounceMs`, default 1500ms per
  `[detailed-spec §3.2]`'s example) after the last keystroke,
- tab switch, memo switch (switching away from the currently-open memo),
- app minimize, app exit (must be awaited before the process is allowed to terminate,
  `[detailed-spec §5.6]`),
- explicit `Ctrl+S`.

"Don't write if no change" (`[spec §2.6]`) applies uniformly: the hook compares current buffer content
against the last-saved content (or its hash) before calling `editor_save` at all, for every trigger
listed above, not just the debounce timer.

### Scroll Position Write Path
`MemoEditorSettings.scrollPosition` (`[detailed-spec §3.3]`) is metadata, not memo-body content, so it
is written via `memo-tree/PLAN.md`'s `memo_set_editor_override` command (not `editor_save`, which only
ever touches the `.md` file's bytes) — the frontend calls it with the current scroll position whenever
the memo is switched away from (same trigger points as an autosave flush, §6 below), debounced the same
way as ordinary scroll-driven UI state so a continuous scroll doesn't fire a metadata write per frame.
The value written is computed via CodeMirror 6's `EditorView.lineBlockAtHeight(view, view.scrollDOM.scrollTop)`
→ `state.doc.lineAt(block.from).number` (the 1-indexed line number currently at the top of the
viewport, `DECISIONS.md` "Scroll Position: Document-Position-Based Anchor, Not Raw Pixel Offset") — not
the raw `scrollTop` pixel value. Because `memo_set_editor_override` already atomically saves the tab's
`.tabtreenote.json` on every call (`memo-tree/PLAN.md` §3), no separate persistence mechanism is needed
for this field.

## 3. Conflict Resolution UI

On `Conflict` from `editor_save`, or on `evt:editor:conflict` from the watcher (external change to the
currently-open memo), the frontend shows the shared non-blocking dialog (Esc-disabled for this specific
dialog, per `[detailed-spec §6.5]`'s named exception) offering choices such as "keep my version
(overwrite)," "reload from disk (discard my changes)," or "view diff" (diff view is not mandated by
`spec.md`, optional nicety, implementation discretion). While this dialog is open, further autosave
attempts for this memo are suspended (`[spec §5.8]` "auto-save/input lock during resolution dialog").

## 4. Text Editing Operations (Plain-Text Layer, Non-Markdown-Specific)

These are CodeMirror commands/transactions, implemented in `src/editor/textCommands.ts`, feeding the
same undo history as manual typing (`[detailed-spec §4.3]`):

- Undo/redo, cut/copy/paste, select-all, line-wise operations (move line up/down, duplicate line,
  delete line) — `[spec §2.3]`.
- Word wrap (none / character-count / window-width), tab width, soft tabs (spaces vs. tab character),
  auto-indent, line numbers, whitespace/line-break visibility, cursor-line highlight — all
  `MemoEditorSettings`-backed, falling back to `AppSettings.editorDefaults` when unset per-memo
  (`[spec §3.4]`).
- Status bar: character count, line count, cursor position (line/col) — pure derived state from
  CodeMirror's `EditorState`, no backend call.
- Right-click "Convert": uppercase/lowercase/hiragana/katakana/half-width/full-width, tab↔space —
  applies to current selection, or current line if no selection (`[detailed-spec §5.3]`). Pure
  client-side text transform producing a CodeMirror transaction (so it's undoable in one step).
  Hiragana↔katakana and half-width↔full-width conversion should use a well-tested JS conversion
  routine/library rather than a hand-rolled character-range map, to avoid subtle Unicode-range bugs —
  library choice is implementation discretion.
- Checkbox cycle (`☐` → `☑` → `☐`...): right-click "Check" menu action inserts/cycles at cursor;
  **separately**, direct click on an already-rendered `☐`/`☑` glyph in live-view mode toggles it in
  place (this click-to-toggle path is markdown-decoration-specific — implemented in
  `markdown-editing/PLAN.md`'s `checkboxClick.ts`, listed here only because the right-click menu
  version is plain-text-layer scoped and shares the same underlying glyph-cycle logic).
- "Search with Google": opens the default browser with a search-engine URL for the selected text, via
  the OS-standard external-open API (`link/PLAN.md` §4 covers the shared "open external URL" primitive
  this reuses).
- "Properties" (right-click on editor body): same `memo_get_properties` command as the tree's
  Properties entry (`memo-tree/PLAN.md` §3).
- Right-click menu here is the **same shared custom context-menu component** used by the tree and tab
  bar (`[spec §9.1 item 2]`), not a separate implementation.
- Right-click is suppressed on the filename/encoding status bar area (`[spec §2.3]`, existing behavior
  to preserve).
- No "Format" submenu — intentionally omitted, overlaps with the Settings screen (`[spec §2.3]`).

## 5. In-Memo Search & Replace

`components/Search/` (in-memo mode, distinct from the cross-tab full-text search covered in
`search-index/PLAN.md`): incremental search, next/prev, replace (single/all/selection/one-by-one),
regex, case-sensitive/word-boundary options — implemented via CodeMirror 6's `@codemirror/search`
package (already a dependency in the legacy renderer), no backend call needed since it operates on the
already-loaded buffer (`[detailed-spec §4.4]`). **Search-string history is confirmed session-only**
(in-memory, cleared on app restart, not persisted anywhere including `AppSettings` — user decision,
`[detailed-spec §5.4]`, `DECISIONS.md` "UI-State Persistence Decisions"; no longer a
`developer-lite`-discretion default), stored in a small ring buffer in `state/useSearch.ts`.

## 6. Persistence Save Triggers — Consolidated (elaborates `[detailed-spec §5.6]`)

| Trigger | Where implemented |
|---|---|
| Debounce after typing | `state/useEditor.ts` timer |
| Tab switch | `state/useTabs.ts` switch handler calls the editor hook's flush-save before changing active tab |
| Memo switch (within same tab) | `state/useTree.ts` open-memo handler, same flush-save-first pattern |
| App minimize | Tauri window `minimize` event listener → flush-save |
| App exit | Tauri `close-requested` event listener → flush-save, **awaited** before allowing the window to actually close |
| Explicit `Ctrl+S` | Global shortcut handler → immediate `editor_save` regardless of debounce timer state |
| Periodic autosave | Same debounce timer as "typing," re-armed on each keystroke (this is not a *separate* periodic timer distinct from the debounce — `[spec §2.6]`'s "periodic autosave" and "don't write if no change" are satisfied by one debounce-driven save path, not two overlapping timers) |

### Startup Auto-Open
On app start, after tabs are loaded, if `AppSettings.lastOpened` references a tab still present in
`AppSettings.tabs` and (after that tab's `tree_open` reconciliation) a memo still present in its tree,
open it automatically. If either lookup fails, do nothing — no error, no fallback memo selected
(`[spec §2.6]`, `[detailed-spec §5.6]`). A locked tab referenced by `lastOpened` cannot be auto-opened
(no password available at startup) — treat this the same as "memo no longer exists," i.e. silently do
nothing, tab bar still shows the tab as 🔒 available for manual unlock.

## Open Items

One item flagged above (§2, `editor_save` step 1: exact `ErrorCode` for a save attempt against an
edit-locked memo) is not fully resolved by `spec.md`'s enum and is not listed in
`[detailed-spec §9]`'s discretion list either. It is **not expected to be reachable in practice**
(frontend makes the editor read-only for edit-locked memos before any keystroke could produce a save
attempt), so this plan proceeds without blocking on it, using `Unknown` as the defensive fallback code
if the backend path is ever hit. If `developer-lite` finds during implementation that this path *is*
reachable in a way that matters to the user (not just a defensive dead-code guard), it should be
escalated to `designer-lite` at that time rather than assumed.
