# Session Handoff

> Written to carry context into a fresh conversation. Read once at the start of the new session,
> then treat as disposable — delete once acted on / absorbed into `spec.v1.2.0.md` / `DECISIONS.md` /
> `RESOLVED-ISSUES.md` / commits. Not a permanent part of the documentation set.
>
> **Verify against actual state before acting** (`git status`, `git log`, `git branch -a`,
> `gh issue list`) — snapshot written 2026-11-01.

## Index

1. Where the project stands
2. Next action
3. Done in this session (2026-10-27 – 2026-11-01)
4. Decisions and open questions
5. Lessons from this session
6. Working conventions
7. Repo-root clutter / leftover branches

## 1. Where the project stands

- **Default branch is `V1.0.0`, and it now holds v1.2.0** (PR #313 merged `V1.2.0` into it; tag `v1.2.0`
  on the merge commit; tag `v1.0.0` still marks the old v1.0.0). `V1.1.0` / `V1.2.0` branches are fully
  merged and only kept as references. **Cut all new branches from `V1.0.0`.**
- The GitHub repo is still **private**. Public-release prep is done (see below); the visibility switch,
  `v1.2.1` tag and a possible rename of `V1.0.0` → `main` have **not** been done.
- App version is `1.2.0` in all 5 places (`package.json`, `package-lock.json`, `src-tauri/Cargo.toml`,
  `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`) — bump all five together on a release.
- Working tree: `src-tauri/Cargo.toml` can show a line-endings-only modification after a build. Never commit it.
- **Open issues:** #323 (settings version display), #325 (shortcut-settings input validation).
- Public-release prep already committed: `README.md` (ja on top / en below), `MANUAL.md` (user guide, ja/en,
  ~1500 lines, fact-checked against code in two passes), MIT `LICENSE` (holder `hachi921082-source`),
  `EULA.md` rewritten as "MIT, no separate EULA", `THIRD-PARTY-NOTICES.md` de-"closed-source"d (the app embeds
  it in the About/licence screen), `.gitignore` additions (`.env*`, root `/*.exe`, `/data/`, `/.tabtreenote.json`,
  `.claude/settings.json`, `.claude/settings.local.json`), Windows user name replaced by `%USERPROFILE%` in
  `CLAUDE.md` / `docs/DECISIONS*.md`, `.claude/settings.json` removed from git (file kept locally).
- `/security-review` of the whole branch ran twice (see §4); not read in full: `tree.rs` L1100–1370 (`memo_set_*`,
  `mutate_node_field`, `memo_get_properties`; lock check confirmed by grep only), `search.rs` after L260,
  `watch/conflict.rs`, `fs/self_write_registry.rs`, `store/memo_file_service.rs`, the three password/recovery dialogs.

## 2. Next action

Ask the user. Candidates they already know about: **#325** (small), **#323** (small), then tag `v1.2.1`
and switch the repo to public. Do not pick up anything Claude itself deferred without the user asking (see §5).

## 3. Done in this session

| Issue | PR | What |
|---|---|---|
| #294 | #296 | Tree row: expand toggle/spacer no longer shrink on long names |
| #295 | #297 | `editorDefaults.splitOrientation` (horizontal / vertical) — spec §13 |
| #299 | #302 | Right-click "リンクをコピー" in live view and preview; only the left button opens links |
| #298 | #303 | `AllowSetForegroundWindow` before opening a link so the browser comes to front |
| #300 / #301 / #306 | #304 / #305 / #307 | Non-ASCII URL and email detection, live = preview — spec §14 |
| #308 | #309, #310 | Executable/script file-path links always confirm (backend `CONFIRM_REQUIRED`); settings hint |
| #311 | #312 | Locked-tab `fileMap`/`imageFolders` validated (blocks out-of-folder delete/overwrite) |
| #314 | #315 | Next/previous-tab shortcuts now use the saved key assignment |
| #319 | #321 | Long-path (>260 chars) warning toast via `evt:path:long-warning` |
| #318 | #322 | "管理情報を復元しました" toast on auto-recovery (pending flag + `meta_take_pending_recovery`; once per scope/tab per run; "file missing" is not a recovery) |
| #320 | #324 | "Hard Break" shortcut honours the saved key (`hardBreakCompartment`, `comboToCodeMirrorKey`) |
| — | #316, #317 | `MANUAL.md` fact-check passes (≈25 corrections) |
| — | #313 | `V1.2.0` → `V1.0.0` merge, tag `v1.2.0` |

All issue entries are in `RESOLVED-ISSUES.md`.

## 4. Decisions and open questions

- **License:** MIT, chosen for portfolio publication (user will show the repo when job hunting).
- **#323 (decided):** settings version = `v<package.json version>`; development builds (non-default branch or
  dirty tree) append `(branch, short hash)`; must not fail without Git. Not implemented yet.
- **#325 (decided, simple first):** warn (don't block) on duplicate key assignments; reject Shift + symbol-key
  combos in the capture UI (letters, digits, named keys stay allowed); check digit-row behaviour on a JIS
  keyboard (Shift+digit gives symbols) before tightening. Not implemented yet.
- **#300:** URLs are also cut at emoji / other `\p{S}` symbols (identical in live and preview); intent never confirmed.
- **#308:** confirm dialog (not a hard block) for executable/script extensions, enforced in the backend; limits:
  extension-based, no symlink/junction resolution, `file:///` URL form untouched.
- **#311:** invalid `fileMap`/`imageFolders` reject the whole payload/marker (surfaces as `DecryptFailed`); a pre-lock
  check for names Windows cannot create (`COM1.md`) was offered and **declined — don't re-propose**.
- **Not filed (low value):** the "tab name duplicate" dialog on create/register is dead code (auto-numbering since
  #97); only "rename" can still return `NAME_DUPLICATE`.
- **Pending for publication:** `MANUAL.md` still says "based on the v1.2.0 development branch" in its header note
  (ja and en) — reword when publishing; the user has not yet read through `MANUAL.md` against the real app.
- **Branch name:** `V1.0.0` holding v1.2.0 is confusing; renaming the default branch to `main` was suggested, undecided.
- **`/security-review`** was skipped for the #300/#301/#306 PRs (a blanket "go ahead" is not an explicit yes); the
  whole-branch reviews afterwards covered them. Findings: #308, #311.

## 5. Lessons from this session

- A copied portable `data/` keeps `settings.json`'s **absolute `folderPath`** for tabs: the isolated copy silently
  used the previous test's folder. After copying `data/`, rewrite `folderPath` and check the tab/memo list.
  (Also: build the path with a real separator — a missing `\` once put a test tab outside `data\tabs`.)
- The app is single-instance: the user's own copy (e.g. `D:\TabNoteTree\…exe`) blocks test launches — ask them to
  close it; never kill a process Claude didn't start.
- **Toasts last ~5 s**: sample the DOM continuously (every 200 ms for ~10 s), never once after a fixed wait — a
  single late check wrongly reported "no toast".
- CDP recipe that works (release exe, isolated dir): launch with env
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`, find the `page` target at
  `http://127.0.0.1:9333/json`, drive via WebSocket; open a memo by clicking its tree row (it is not auto-opened);
  send keys with `Input.dispatchKeyEvent` (`rawKeyDown` + `keyUp`, modifiers bitmask Alt=1 Ctrl=2 Shift=8) and text with
  `Input.insertText`; verify by reading the autosaved `.md` file. Kill the process tree and wait for port 9333 to
  close between scenarios (zombie WebView2 targets otherwise answer the next run). In `node -e`, backslashes get eaten
  by the shell — put scripts in files.
- Verify a reviewer's claim before filing it (a suspected "unreachable dialog" was only partly true).
- "全て進めて良し" after a list that includes an optional `/security-review` question: confirm that item separately.
- reviewer-lite findings about pre-existing behaviour → separate issue, don't widen the PR (user agreed, e.g. #306).
- Write manual-test pass criteria as a per-step table (what to do / what counts as OK), and say up front which
  outcomes are expected errors.
- `git pull` after a local `RESOLVED-ISSUES.md` commit can create a "Merge branch 'V1.0.0' of …" commit when the PR
  merge landed on origin meanwhile — harmless.

## 6. Working conventions

- Every user-visible message in Japanese.
- Flow per issue: `fix/<slug>` from `V1.0.0` → developer-lite → reviewer-lite (static) → release build
  (`npm run tauri build -- --no-bundle`) → live test (Claude via isolated scratchpad copy + CDP where possible, user for
  native dialogs / real browser / mail) → ask `/security-review` if paths/deletion/tab-lock/URL parsing/sanitize →
  **ask before** push / PR / merge / branch delete (one list, user answers "すべて進めてよい" or trims it) → PR with
  `Closes #N` → `gh pr merge --merge` → pull → `RESOLVED-ISSUES.md` entry (commit directly on `V1.0.0`) → delete
  branch (local + origin) → push `V1.0.0`.
- Larger docs changes (e.g. `MANUAL.md`, spec files) go through a short-lived `docs/<slug>` branch + PR; `RESOLVED-ISSUES.md` and this file are committed directly on `V1.0.0`.
- Live tests: copy the timestamped exe into `scratchpad/<name>/` and run it there (portable `data/` stays isolated).
- `gh issue create` / `gh pr create`: write the body to a scratchpad file and use `--body-file`.
- Commit messages: no file-path prefix, Japanese body, one file per commit, trailer
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Version bump touches 5 files (see §1).

## 7. Repo-root clutter / leftover branches

- Untracked timestamped exes, `data/`, `.tabtreenote.json` in the repo root (now gitignored). **User manages cleanup —
  don't offer to delete.**
- Merged branches `V1.1.0` / `V1.2.0` and some older `fix/*` remain on origin — user may delete manually. This
  session's feature/fix/docs branches are all deleted.
