# Session Handoff

> Written to carry context into a fresh conversation. Read once at the start of the new session,
> then treat as disposable — delete once acted on / absorbed into `spec.v1.2.0.md` / `DECISIONS.md` /
> `RESOLVED-ISSUES.md` / commits. Not a permanent part of the documentation set.
>
> **Verify against actual state before acting** (`git status`, `git log`, `git branch -a`, `git stash list`,
> `gh issue list`) — snapshot written 2026-11-02.

## Index

1. Where the project stands
2. Next action
3. Done in this session (2026-10-27 – 2026-11-02)
4. Decisions and open questions
5. Lessons from this session
6. Working conventions
7. Repo-root clutter / leftover branches / stash

## 1. Where the project stands

- **Default branch is `main` (renamed from `V1.0.0` on 2026-11-02) and holds v1.2.1** (tag `v1.2.1` on PR #328's merge commit; `v1.2.0` and the old
  `v1.0.0` tags also exist). `V1.1.0` / `V1.2.0` branches are fully merged and only kept as references.
  **Cut all new branches from `main`.**
- **No open GitHub issues** (#334 resolved, PR #337).
- The GitHub repo is **public** since 2026-11-03 (history still contains the Windows user name `m9112` in 13 old commits — accepted, not rewritten; a secret scan of all history found nothing).
  The rename to `main` is done (PR #335 fixed `DEFAULT_BRANCH`); the stale `master` branch was deleted from origin (last commit db42632).
- App version is `1.2.1` in all 5 places (`package.json`, `package-lock.json`, `src-tauri/Cargo.toml`,
  `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`); `README.md` / `MANUAL.md` state "v1.2.1". Bump all together.
  The settings-screen version label is `v<package.json version>` for a clean build of `main`, otherwise
  `v1.2.1 (branch, shortHash[, dirty])` (CR-only diffs, e.g. a rebuilt `Cargo.toml`, don't count as dirty; the default
  branch name is the `DEFAULT_BRANCH` constant in `vite.config.ts` — change it if the branch is renamed).
- Working tree: `src-tauri/Cargo.toml` can show a line-endings-only modification after a build. Never commit it.
- Public-release prep already committed: `README.md` (ja on top / en below), `MANUAL.md` (user guide, ja/en,
  ~1500 lines, fact-checked against code in two passes), MIT `LICENSE` (holder `hachi921082-source`), `EULA.md`
  rewritten as "MIT, no separate EULA", `THIRD-PARTY-NOTICES.md` (embedded in the app's licence screen),
  `.gitignore` additions, Windows user name replaced by `%USERPROFILE%` in `CLAUDE.md` / `docs/DECISIONS*.md`,
  `.claude/settings.json` untracked (file kept locally).
- `/security-review` ran in three passes over the whole branch (findings → #308, #311, #330, #334), then a final
  read of the previously unread areas on 2026-11-03 (no new findings): `PermanentUnlockRecoveryDialog.tsx`,
  `useTabLockAutoRelock.ts`, all `src/ipc/*.ts` wrappers, `tree.rs` cross-tab move, `editor_load`/`editor_save`,
  `image_save_pasted`/`image_read_bytes`, `tab_open_location`, `model/app_settings.rs` L250–402. Only `RecoveryDialog.tsx`
  (grepped, not read in full) and the backend of `link_resolve` / search commands were not re-read. Observations, not filed
  (inside the trust boundary: webview trusted): `settings_set` can replace `tabs`/`schemaVersion` wholesale and
  `resolve_tab_access` trusts `entry.locked` from settings.json (hardening option: drop `tabs`/`schemaVersion` from the
  patch); `image_read_external_file` reads any absolute path by design (drag-and-drop of a dropped file).

## 2. Next action
Ask the user. Candidates: the user's own read-through of `MANUAL.md` against the real app (not yet done; fix any
mismatches in a `docs/` branch + PR), tiny cleanups (§4).
Do not pick up anything Claude itself deferred without the user asking (see §5).

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
| #314 | #315 | Next/previous-tab shortcuts use the saved key assignment |
| #319 | #321 | Long-path (>260 chars) warning toast via `evt:path:long-warning` |
| #318 | #322 | "管理情報を復元しました" toast on auto-recovery (pending flag + `meta_take_pending_recovery`; once per scope/tab per run; "file missing" is not a recovery) |
| #320 | #324 | "Hard Break" shortcut honours the saved key (`hardBreakCompartment`, `comboToCodeMirrorKey`) |
| #325 | #326 | Settings → Shortcuts: duplicate-key warning; Shift+symbol combos rejected (digit row allowed by `e.code`) |
| #323 | #327 | Settings version label = app version (see §1) |
| #329 | #331 | Removed the unreachable "tab name duplicate" dialog code (create/register auto-number; only rename errors) |
| #330 | #332 | `memo_get_properties` / `memo_open_location` check `path` against the tab tree first |
| — | #313, #328 | `V1.2.0` → `V1.0.0` merge (tag `v1.2.0`); version bump to 1.2.1 (tag `v1.2.1`) |
| — | #316, #317 | `MANUAL.md` fact-check passes (≈25 corrections) |

All issue entries are in `RESOLVED-ISSUES.md`.

## 4. Decisions and open questions

- **License:** MIT, chosen for portfolio publication (the user will show the repo when job hunting).
- **#300:** URLs are also cut at emoji / other `\p{S}` symbols (identical in live and preview); intent never confirmed.
- **#308:** confirm dialog (not a hard block) for executable/script extensions, enforced in the backend; limits:
  extension-based, no symlink/junction resolution, `file:///` URL form untouched.
- **#311:** invalid `fileMap`/`imageFolders` reject the whole payload/marker; a pre-lock check for names Windows
  cannot create (`COM1.md`) was offered and **declined — don't re-propose**.
- **#325:** simple rule first; fixed keys outside the rebindable set (Escape, OS copy/paste) are not checked for
  duplicates; already-saved Shift+symbol assignments are neither removed nor warned about.
- **Not filed, optional tiny cleanups:** unused `name` parameters of backend `tab_create` / `tab_register_existing`
  and `tabCreate(path, name)` in `src/ipc/tab.ts` (nothing passes them since #329), one stale comment in `tab.rs`
  (~L69 "retry after NAME_DUPLICATE"), an old "create/duplicate-name dialogs" heading comment in `ja.ts`.
- **Branch name:** default branch renamed `V1.0.0` → `main` on 2026-11-02 (GitHub + local + `DEFAULT_BRANCH` + CLAUDE.md). Older entries in this file and `RESOLVED-ISSUES.md` that say `V1.0.0` mean this branch. `master` is still on origin.
- **Not driven live:** "メモの場所を開く" (opens Explorer) and real mail-client / browser opening beyond what the
  user tried by hand.
- **`/security-review`** was skipped for the #300/#301/#306 PRs (a blanket "go ahead" is not an explicit yes); later
  whole-branch reviews covered them.

## 5. Lessons from this session

- **Never `git stash pop` unconditionally.** `git stash -q` with nothing to stash does nothing, and a following
  `git stash pop` then applies an OLD stash — it applied the user's pre-existing stash `stash@{0}` ("pre-existing
  in-progress work (table-of-contents docs, Cargo.toml) before issue #79-90 batch") and conflicted. It was undone
  (the stash is still there — **don't drop it without asking**). Since the tree is normally clean after a merge, don't
  stash at all; switch branches directly.
- The auto-mode classifier blocks `git checkout -- <files>` (discard); when blocked, ask via `AskUserQuestion` with
  concrete options rather than describing the block.
- A developer-lite agent working in the shared working tree must not be disturbed by `git checkout`: while one runs,
  only use `git push origin <branch>`, `gh pr …` etc., and defer local branch switches / commits on `main`.
- A copied portable `data/` keeps `settings.json`'s **absolute `folderPath`** for tabs: re-point it, and build paths with
  a real separator (a missing `\` once put a test tab outside `data\tabs`).
- The app is single-instance: the user's own copy (e.g. `D:\TabNoteTree\…exe`, sometimes in the tray) blocks test
  launches — ask them to close it; never kill a process Claude didn't start.
- **Wait for `Built application` before copying the exe** (a stale exe was copied once while the build was still
  compiling); `until grep -q "Built application\|error" log` works.
- **Toasts last ~5 s**: sample the DOM continuously (every 200 ms for ~10 s) rather than once after a fixed wait.
- CDP recipe that works (release exe, isolated dir): launch with env
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`, find the `page` target at
  `http://127.0.0.1:9333/json`, drive via WebSocket; open a memo by clicking its tree row (not auto-opened); settings:
  click the ⚙ button, then the `.ttn-settings-tab`; keys via `Input.dispatchKeyEvent` (`rawKeyDown` + `keyUp`,
  modifiers Alt=1 Ctrl=2 Shift=8, JIS Shift+digit = `key:"!"`, `code:"Digit1"`) and text via `Input.insertText`; read results from
  the DOM or the autosaved `.md`. Kill the process tree and wait for port 9333 to close between scenarios. Put scripts in
  files (a shell eats backslashes in `node -e`; a regex with `\n` inside a JS string became a real newline once).
- Verify a reviewer's claim before filing it (a suspected "unreachable dialog" was only partly true).
- "全て進めて良し" after a list that includes an optional `/security-review` question: confirm that item separately.
- reviewer-lite findings about pre-existing behaviour → separate issue, don't widen the PR (user agreed, e.g. #306).
- Write manual-test pass criteria as a per-step table (what to do / what counts as OK), say up front which outcomes
  are expected errors, and do whatever Claude can drive itself (the user asked for that) before handing over the rest.

## 6. Working conventions

- Every user-visible message in Japanese.
- Flow per issue: `fix/<slug>` from `main` → developer-lite → reviewer-lite (static) → release build
  (`npm run tauri build -- --no-bundle`) → live test (Claude via isolated scratchpad copy + CDP where possible, user for
  native dialogs / real browser / mail) → ask `/security-review` if paths/deletion/tab-lock/URL parsing/sanitize →
  **ask before** push / PR / merge / branch delete (one list; the user answers "すべて進めてよい" or trims it) → PR with
  `Closes #N` → `gh pr merge --merge` → `git checkout main && git pull origin main` (no stash) → `RESOLVED-ISSUES.md`
  entry (commit directly on `main`) → delete branch (local + origin) → push `main`.
- Larger docs changes (`MANUAL.md`, spec files) go through a short-lived `docs/<slug>` branch + PR; `RESOLVED-ISSUES.md`
  and this file are committed directly on `main`.
- Live tests: copy the timestamped exe into `scratchpad/<name>/` and run it there (portable `data/` stays isolated).
- `gh issue create` / `gh pr create`: write the body to a scratchpad file and use `--body-file`.
- Commit messages: no file-path prefix, Japanese body, one file per commit, trailer
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Releases: bump the 5 version files (+ `README.md` / `MANUAL.md` version line) on a `chore/bump-version-X` branch → PR →
  merge → annotated tag `vX` on the merge commit → push the tag.

## 7. Repo-root clutter / leftover branches / stash

- Untracked timestamped exes, `data/`, `.tabtreenote.json` in the repo root (gitignored), plus untracked
  `docs/plans/.tabtreenote.json*`. **User manages cleanup — don't offer to delete.**
- Merged branches `V1.1.0` / `V1.2.0` and some older `fix/*` remain on origin — user may delete manually. This
  session's feature/fix/docs/chore branches are all deleted.
- `git stash list` holds one old user stash (see §5) — leave it.
