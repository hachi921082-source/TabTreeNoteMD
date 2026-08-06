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
