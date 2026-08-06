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
