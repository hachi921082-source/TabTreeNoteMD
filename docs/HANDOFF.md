# Session Handoff

> Written to carry context into a fresh conversation. Read once at the start of the new session,
> then treat as disposable — delete once acted on / absorbed into `spec.v1.2.0.md` / `DECISIONS.md` /
> `RESOLVED-ISSUES.md` / commits. Not a permanent part of the documentation set.
>
> **Verify against actual state before acting** (`git status`, `git log`, `git branch -a`,
> `gh issue list`) — snapshot written 2026-10-04 (afternoon).

## Index

1. Where the project stands
2. Next action
3. Done in this session (2026-10-04)
4. Decisions made this session
5. Known stale text in other docs
6. Lessons from this session
7. Working conventions
8. Repo-root clutter / leftover branches

## 1. Where the project stands

- Branch `V1.2.0`, local == origin, nothing in progress.
- **No open GitHub issues.**
- Working tree: `src-tauri/Cargo.toml` shows modified — line endings only. Never commit it.

## 2. Next action

None queued. Ask the user what to do next. Do not pick up anything Claude itself deferred in an
earlier doc/issue without the user asking (see §6).

## 3. Done in this session (2026-10-04)

| Issue | PR | What |
|---|---|---|
| #262 | #283 | Cross-tab move/copy carries the memo's image folder (plain/locked × plain/locked) |
| #267 | #285 | Open memo rename / cross-tab move flush pending autosave first; delete discards it (no stale NOT_FOUND banner) |
| #282 | #287 | `.md.bak` follows rename and plain→plain move; deleted on locked-involved moves and on memo delete |
| #266 | #288 | Spaced memo names → `![](<新 メモ/001.png>)`; legacy broken links auto-repaired (rewrite ops + on open) when the file exists |
| #274 | #289 | Unlock session: app's own image file picker doesn't trigger focus-loss relock (2-min cap, fail closed) — spec §11 |
| #279 | #290 | Split view: preview blends to its end over the editor's last viewport — spec §12 |
| #291 | #292 | Split view preview was one line ahead (1- vs 0-indexed); editor top → preview 0 |
| #286 | #293 | No stale 「ロックされたタブには操作できません」 banner when dropping onto a locked tab |
| #284 | — | Closed **not planned** (see §4) |

All entries are in `RESOLVED-ISSUES.md`.

## 4. Decisions made this session

- **#284 not planned:** locked-tab session relocks on app focus loss (security), so Explorer D&D
  import into a locked tab is unreachable by design. Workaround: import into a plain tab (#265
  carries images), then move into the locked tab (#262 carries the folder).
- **#262:** pre-#139 flat-root images are NOT carried on cross-tab move.
- **#282:** carry `.bak` (not delete) on plain rename/move; locked-involved moves delete it.
- **#266:** legacy repair in both places (rewrite ops and on open); edit-locked memos untouched on open.
- **#274:** exception only for the toolbar image picker; PDF save / folder picker / Explorer D&D
  still relock. 2-minute cap per picker opening.
- Ctrl+Shift+P pane toggle is session-local by design (reverts on memo switch); per-memo preview
  lock (#149) persists.

## 5. Known stale text in other docs

- `RESOLVED-ISSUES.md` #265 entry ends with "ロック中タブへの取り込みは#264の保存方式確定後に対応" —
  superseded by #284 (not planned). Not yet edited (user hasn't asked).

## 6. Lessons from this session

- "次へ" after listing several candidates is ambiguous — ask which. Claude-recorded deferrals are
  not user requests; before implementing one, confirm the user wants it AND that the UI can reach
  it (#284 was fully built before discovering it was unreachable).
- Write manual-test pass criteria only after checking real OS behaviour: closing a native file
  dialog returns focus to the owner window, so "cancel → expect relock" (#274 step 3) was wrong.
- When the user tests by hand, a CDP monitor script (`scratchpad/mon274.mjs` pattern: wrap
  `__TAURI_INTERNALS__.invoke`, log focus/blur, picker open/change/cancel, UI state via
  `console.log('[MON]')` → file) gives exact event order without asking the user to describe it.
- The app is single-instance: a running copy (user's own, possibly in the tray) blocks test
  launches. Don't kill a process Claude didn't start — ask the user.
- Cross-tab tree drag is app-internal (document mouse events), so CDP drag tests are faithful;
  only Explorer↔app D&D and native dialogs need the user's hands.
- Shrunk test windows let the floating toolbar overlap click targets — check `elementFromPoint`
  before scripted clicks (a test once typed `~~~~` into a fixture memo this way).
- User instruction: Claude may run live tests itself at human-realistic speed; ask before testing
  abnormal/racy timings.

## 7. Working conventions

- Every user-visible message in Japanese.
- Flow per issue: `fix/<slug>` from `V1.2.0` → developer-lite → reviewer-lite (static) → release
  build → live test (Claude via isolated scratchpad copy, or user for native dialogs/Explorer D&D)
  → ask `/security-review` if paths/deletion/tab-lock → ask before PR/merge → PR → local
  `git merge --no-ff` + push → close issue → `RESOLVED-ISSUES.md` entry → delete branch (local +
  origin) once the user approves.
- Live tests: copy the timestamped exe into `scratchpad/<name>/` and run it there (its portable
  `data/` stays isolated); confirm empty tab list before mutating; report every created/deleted file.
- `/security-review`: run as a subagent scoped to `git diff V1.2.0...<branch>`.
- `gh issue create` / `gh pr create`: write the body to a scratchpad file and use `--body-file`.
- Commit messages: no file-path prefix, Japanese body, one file per commit.

## 8. Repo-root clutter / leftover branches

- Untracked timestamped exes and `data/` (portable data). **User manages cleanup — don't offer to delete.**
- This session's branches are all deleted. Older merged branches remain locally/on origin
  (e.g. `feature/locked-tab-image-folders`, `fix/lock-convert-image-folders`,
  `fix/lock-plaintext-bak-cleanup`, `fix/locked-tab-per-memo-image-folder`,
  `fix/unlock-restore-image-folders`, many older `fix/*`) — user may delete manually.
