# Session Handoff

> Written to carry context into a fresh conversation. Read once at the start of the new session,
> then treat as disposable — delete once acted on / absorbed into `spec.v1.2.0.md` / `DECISIONS.md` /
> `RESOLVED-ISSUES.md` / commits. Not a permanent part of the documentation set.
>
> **Verify against actual state before acting** (`git status`, `git log`, `git branch -a`,
> `gh issue list`) — snapshot written 2026-10-29.

## Index

1. Where the project stands
2. Next action
3. Done in this session (2026-10-27 – 2026-10-29)
4. Decisions made this session
5. Lessons from this session
6. Working conventions
7. Repo-root clutter / leftover branches

## 1. Where the project stands

- Branch `V1.2.0`, local == origin (after this file's commit is pushed), nothing in progress.
- **No open GitHub issues.**
- Working tree: `src-tauri/Cargo.toml` shows modified — line endings only. Never commit it.
- A `/security-review` of `V1.2.0` (whole branch vs `master`) was started at the user's request on
  2026-10-29; result not yet reported when this was written. Re-run or ask the user.

## 2. Next action

None queued. Ask the user what to do next. Do not pick up anything Claude itself deferred in an
earlier doc/issue without the user asking (see §5).

## 3. Done in this session

| Issue | PR | What |
|---|---|---|
| #294 | #296 | Tree row: expand toggle/spacer no longer shrink on long names |
| #295 | #297 | `editorDefaults.splitOrientation` (horizontal / vertical) — spec §13 |
| #299 | #302 | Right-click "リンクをコピー" in live view and preview; only the left mouse button opens links |
| #298 | #303 | `AllowSetForegroundWindow(ASFW_ANY)` before opening a link so the browser comes to front (Windows) |
| #300 | #304 | Non-ASCII URL detection: live = preview range; IDN in live view; preview copy keeps as-written URL (`data-source-url`) |
| #301 | #305 | Non-ASCII email detection, live = preview, `mailto:` href |
| #306 | #307 | Preview links an ASCII email directly after Japanese text (same as live) |

All entries are in `RESOLVED-ISSUES.md`; limits of #300/#301/#306 are in `spec.v1.2.0.md` §14.

## 4. Decisions made this session

- **#299:** menu shown for URL / email / file-path links only (not `memo:` / `#slug`); no menu item on non-links.
- **#300:** cut URLs at non-ASCII punctuation and closing brackets; `・ 〜 ～` stay inside URLs; kana
  directly attached stays part of the URL; live view and preview aligned. Still open (unanswered):
  URLs are also cut at emoji / other `\p{S}` symbols — behaves identically in both views, intent unconfirmed.
- **#301/#306:** non-ASCII local part → whole token is the email; ASCII email after Japanese → ASCII side only.
- **`/security-review` was skipped** for #300/#301/#306 PRs ("全て進めて" is not an explicit yes); the
  whole-branch review requested afterwards covers them.

## 5. Lessons from this session

- A copied portable `data/` keeps `settings.json`'s **absolute `folderPath`** for tabs: the isolated copy
  silently kept editing/reading the previous test's folder (memo content looked "stale"). After copying
  `data/` into a new scratchpad dir, rewrite `folderPath` to the new dir and check the memo tree.
- The app is single-instance: ask the user to close their running copy; never kill a process Claude
  didn't start (a stray old build in the repo root blocked a test launch).
- "全て進めて良し" after a list that includes an optional `/security-review` question: confirm that item
  separately instead of reading it as yes or no.
- reviewer-lite findings on pre-existing behavior (e.g. #306) → split into a separate issue instead of
  widening the PR; the user accepted this.
- Write manual-test pass criteria as a per-section table (what is underlined / what is excluded), and
  state expected "cannot access" outcomes for nonexistent test domains up front.

## 6. Working conventions

- Every user-visible message in Japanese.
- Flow per issue: `fix/<slug>` from `V1.2.0` → developer-lite → reviewer-lite (static) → release
  build → live test (Claude via isolated scratchpad copy, or user for native dialogs/real browser/mail)
  → ask `/security-review` if paths/deletion/tab-lock/URL parsing/sanitize → ask before PR/merge → PR →
  `gh pr merge --merge` + push → close issue → `RESOLVED-ISSUES.md` entry → delete branch (local +
  origin) once the user approves.
- Live tests: copy the timestamped exe into `scratchpad/<name>/` and run it there (its portable
  `data/` stays isolated); confirm empty/expected tab list before mutating; report every created/deleted file.
- `gh issue create` / `gh pr create`: write the body to a scratchpad file and use `--body-file`.
- Commit messages: no file-path prefix, Japanese body, one file per commit.

## 7. Repo-root clutter / leftover branches

- Untracked timestamped exes, `data/`, `.tabtreenote.json` (portable data). **User manages cleanup — don't offer to delete.**
- This session's branches are all deleted. Older merged branches may remain locally/on origin — user may delete manually.
