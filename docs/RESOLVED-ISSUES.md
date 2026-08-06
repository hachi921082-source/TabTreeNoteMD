# Resolved Issues (RESOLVED-ISSUES.md)

> Log of closed GitHub issues (repo `hachi921082-source/TabTreeNoteMD`). Per [CLAUDE.md](../CLAUDE.md)'s
> "GitHub Issue Workflow": once an issue's fix is merged, the issue itself is **closed** (not
> deleted — GitHub keeps the full discussion history), and a short entry is added here so the
> resolution history is also browsable directly in the repo without going through GitHub.

> **Format per entry**: `- #<number> <title> — <one-line summary of the fix> (PR #<pr-number>)`

---

## Closed

- #20 Bare file-path auto-link click-to-open has no `..`-traversal guard (security-relevant) — `resolve_file_path` now rejects relative targets with a `..` segment or a rooted-but-not-absolute path (e.g. `\Windows\System32\cmd.exe`), mirroring `markdownSanitize.ts`'s `isSafeRelativeImageSrc` (PR #32)
- #11 `markdown-editing/PLAN.md` described the toolbar's "Update" slot as calling a removed stub — corrected to describe the real `tocGenerator.ts` wiring (PR #33)
- #14 `Cargo.toml`'s `tauri-plugin-autostart` comment described the abandoned Startup-folder approach — corrected to describe the shipped Registry `HKCU\...\Run` key (PR #34)
- #16 Toolbar image-insert file input leaked on dialog cancel — `insertImageViaFilePicker` now cleans up on `cancel`/`focus` fallback, not just `change` (PR #35)
- #19 Bare `memo:` with nothing after the colon matched as a link — `BARE_MEMO_RE` now requires at least one non-whitespace target char (PR #36)
- #17 Tab Lock session mutations weren't rolled back if the final lock-blob save failed — all 7 session mutators now route through a build-then-commit `commit_session_mutation` helper that only applies the in-memory `tree`/`file_map` change after `save_lock_blob` succeeds (PR #37)
- #18 `cleanup_session_move_source` swallowed source file-delete failures instead of aborting, risking orphaned encrypted files after a cross-tab move — reordered to delete the physical file first and abort (leaving `tree`/`file_map` untouched) on failure, matching `session_delete_memo`'s pattern (PR #38)
- #23 Clicking a memo no longer auto-focused the editor (spec-mandated) — added a one-shot `focusEditorToken` signal in `useTree.ts`, bumped by click-open/`focusOnMemo` but not arrow-key nav, consumed by `EditorPane.tsx`'s view-creation effect to call `view.focus()`; user-tested and confirmed (PR #39)
- #29 Cursor landed before (not after) the inserted `"> "`/`"- "` prefix after the Quote/List toolbar button — `toggleLinePrefix` now maps the pre-edit selection through its `ChangeSet` with `assoc=1` instead of relying on CodeMirror's default left-sticking mapping (PR #40)
- #25 Markdown toolbar's rearrange mode didn't work — its slot-reorder drag used the HTML5 DnD API, which conflicts with Tauri's native drag-drop on Windows; switched to the same pointer-event pattern `TabBar.tsx` uses for tab reordering (PR #41)
- #5 Settings dialog content (20+-row Shortcuts section) overflowed the window with no scrollbar — `.ttn-dialog-body`'s missing flex/`min-height:0` broke the intended flex-scroll chain; fixed with a scoped `.ttn-dialog.ttn-settings-dialog .ttn-dialog-body` rule rather than the shared `Dialog.css`, to avoid a margin-collapse regression in `ConflictDialog` (PR #42)
- #31 The Markdown toolbar's live/source view toggle only affected the current session, never surviving a memo reopen or app restart — `onToggleLiveView` now persists into the memo's `MemoEditorSettings.viewMode` override, following `persistScrollPosition`'s existing per-memo pattern; `detailed-spec.md` §5.10 updated to match (PR #43)
- #28 Reopening a memo restored scroll position but not cursor position — added paired `cursorAnchor`/`cursorHead` fields alongside `scrollPosition`, threaded through both Rust load paths and restored via `EditorState.create`; also fixed a pre-existing memo-switch race where the outgoing memo's final position was written into the newly-opened memo's override instead (PR #44)
