# Session Handoff

> Written to carry context into a fresh conversation. Read once at the start of the new session,
> then treat as disposable — delete once acted on / absorbed into `spec.v1.2.0.md` / `DECISIONS.md` /
> `RESOLVED-ISSUES.md` / commits. Not a permanent part of the documentation set.
>
> **Verify against actual state before acting** (`git status`, `git log`, `git branch -a`,
> `gh issue list`) — snapshot written 2026-10-04.

## Index

1. Where the project stands
2. Next action: #262
3. Other open issues
4. Done in the previous session (2026-10-03 / 10-04)
5. Lessons from the previous session
6. Working conventions
7. Repo-root clutter / leftover branches

## 1. Where the project stands

- Branch `V1.2.0` at `b3ccaca` (+ this HANDOFF commit), local == origin, nothing in progress.
- Image-folder series #264 (#271 → #272 → #276 → #273) is complete and merged into `V1.2.0` via the
  integration branch `feature/locked-tab-image-folders` (PR #281). All five issues closed,
  `RESOLVED-ISSUES.md` updated.
- Working tree: `src-tauri/Cargo.toml` shows modified — line endings only. Never commit it.

## 2. Next action: #262

#262 — cross-tab move/copy doesn't carry the memo's image folder, so images break at the destination.
Remaining item of the original series order (#264 → **#262** → locked-tab import).

Decisions already recorded (on the issue / previous HANDOFF):
- Locked-tab moves are IN scope (Q1).
- Pre-#139 flat images carried only if cheap, else skipped (Q2).

Must build on the now-final layout:
- Plain tab: `<memo_name>/NNN.ext`. Locked tab (session): `<random16hex>/NNN.ext`, memo→folder map
  in encrypted `LockPayload::image_folders` (`commands/tab_lock.rs`).
- Cross-tab move out of a locked tab already drops the source map entry (#271 fix in
  `cleanup_session_move_source`) but does NOT move the folder — #262 must.
- Four combinations (plain/locked × plain/locked) each need folder rename + link rewrite
  (`util/image_link_rewrite.rs`), #263 path guard, collision handling at destination.

Flow: `fix/<slug>` from `V1.2.0` → developer-lite → reviewer-lite (static) → release build → user
manual test → ask `/security-review` (touches tab-lock + paths; recommend yes) → PR → local merge
after user OK.

After #262: locked-tab `.md` import (deferred part of #265).

## 3. Other open issues

| Issue | Content |
|---|---|
| #274 | Toolbar image insert fails in an unlock session: OS file dialog takes focus → focus-loss auto-relock. User decision: add an exception for the app's own file picker only; D&D from Explorer stays as-is (security); clipboard paste works. |
| #279 | Split view: editor shorter than rendered preview (big images) → editor hits its end, preview stops at the line-matched position. Proposed: blend to preview end over the editor's last viewport. One-directional sync stays. |
| #266 | Memo names with spaces produce `![](新 メモ/001.png)` (needs `<...>`); paste and #261 rewrite affected. Full-width space works. |
| #267 | Renaming the open memo with a pending autosave loses unsaved edits (flush before rename). |

## 4. Done in the previous session (2026-10-03 / 10-04)

| Issue | PR | What |
|---|---|---|
| #271 | #275 | Locked-tab paste → per-memo random folder; map in encrypted payload |
| #272 | #277 | Lock converts `<memo>/` → random + link rewrite; rename failure aborts + rolls back |
| #276 | #278 | Lock removes plaintext `.bak` residue (found during #272 manual test; pre-existing since tab-lock) |
| #273 | #280 | Permanent unlock restores `<memo>/`; collision / in-use → keep random + warning toast |
| #264 | #281 | Integration branch merged into `V1.2.0` |

Crash-recovery order for all three tab-lock paths: `docs/plans/tab-lock/CRASH-RECOVERY.md` §7–§9.

## 5. Lessons from the previous session

- Manual-test instructions must state the precondition precisely. Two #273 runs were invalid:
  a collision folder created before lock (got adopted as the memo's image folder), and `cmd`
  opened in the tab folder instead of inside the random folder. Paint does NOT hold a file lock;
  use `cmd` with cwd inside the target folder (Explorer address bar → `cmd`).
- Test tabs reused across builds accumulate mixed states (e.g. a pre-#273 permanent unlock drops the
  map, leaving random folders that later can't be restored). Tell the user to use a fresh tab per
  feature test.
- User sends screen recordings (`unknown_*.mp4` at repo root). No ffmpeg; extract frames with Python
  `cv2` (OpenCV installed) into the scratchpad, ~8 frames, 900–1000 px wide.
- Toasts show bottom-right for 5 s — easy to miss if Explorer overlaps.

## 6. Working conventions

- Every user-visible message in Japanese.
- Heavy live verification → hand the user a manual checklist + timestamped release exe
  (`TabTreeNoteMD_YYYYMMDD-HHMMSS.exe` at repo root; delete msi/nsis installers).
- reviewer-lite: static review only (no CDP); user does live tests.
- `/security-review` check-in before PR on anything touching paths/deletion/tab-lock; run it as a
  subagent scoped to `git diff <base>...<branch>` (the skill's default diff is vs `V1.0.0`, ~1 MB).
- `gh pr merge` and branch deletion are blocked by the auto-mode classifier; user chose "merge via
  local git" (`git merge --no-ff` + push) — PR then shows MERGED automatically. A merge into
  `V1.2.0` also hit the classifier once; it went through after the user re-confirmed.
- `gh issue create` / `gh pr create` with long bodies: write body to a scratchpad file and use
  `--body-file` (PowerShell quoting breaks inline bodies).

## 7. Repo-root clutter / leftover branches

- Untracked timestamped exes, `unknown_*.mp4` recordings, and `data/` (portable data; test tabs
  `111`, `456`, `457`, `888`, `999`). **User manages cleanup — don't offer to delete.**
- Merged branches not deleted (deletion blocked by classifier; user may delete manually):
  `feature/locked-tab-image-folders`, `fix/locked-tab-per-memo-image-folder`,
  `fix/lock-convert-image-folders`, `fix/lock-plaintext-bak-cleanup`,
  `fix/unlock-restore-image-folders` (local + origin).
