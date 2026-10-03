# Session Handoff

> Written to carry context into a fresh conversation when the previous one has grown too large.
> Read this once at the start of the new session, then treat it as disposable — once its content is
> acted on or absorbed into `spec.v1.2.0.md` / `DECISIONS.md` / `RESOLVED-ISSUES.md` / commits, this
> file can be deleted. Not a permanent part of the documentation set.
>
> **Verify against actual state before acting** (`git status`, `git log`, `git branch -a`,
> `gh issue list`) — this is a snapshot (written 2026-10-03).

## Index

1. Where the project stands
2. Next action: #271 on the integration branch
3. Image-folder issue series — decisions and order
4. Merged this session (2026-10-02 / 10-03)
5. Open issues not in the series
6. Working conventions confirmed this session
7. Repo-root clutter

## 1. Where the project stands

- Current branch `V1.2.0` at `f81677d`, local == origin, nothing in progress on it.
- Local-only branch `feature/locked-tab-image-folders` (cut from `V1.2.0` @ `f81677d`) — **NOT pushed
  yet**. User has not yet approved pushing it (asked, pending).
- Working tree: `src-tauri/Cargo.toml` shows modified, but it is a line-ending-only change from builds
  (`git diff --ignore-all-space` empty). Never commit it.

## 2. Next action: #271 on the integration branch

1. Ask the user to approve pushing `feature/locked-tab-image-folders` to origin (PR base for #271–#273).
2. Cut `fix/<slug>` for #271 **from `feature/locked-tab-image-folders`** (not from `V1.2.0`).
3. developer-lite implements #271 (`gh issue view 271` is the spec). Ask it for a live-test estimate.
4. reviewer-lite static review → release build (timestamped exe at repo root, delete installers) →
   user manual test → **ask user about `/security-review`** (tab-lock internals; recommend yes) →
   PR **into `feature/locked-tab-image-folders`** → merge after user OK.
5. Repeat for #272, #273. Only after all three are done and verified: merge the integration branch
   into `V1.2.0` (with user confirmation), then close #264 and the three, add `RESOLVED-ISSUES.md`
   entries.

## 3. Image-folder issue series — decisions and order

Background: #139 stores plain-tab images in `<tab>/<memo_name>/NNN.ext`; locked tabs store images flat
in the tab root with random names, **unencrypted** (tab-lock encrypts `.md` only). A code survey found
lifecycle holes A–H; A, E/F, H are fixed (§4).

Order (user-confirmed): ~~#261~~ → ~~#263~~ → ~~#265~~ → **#264 (split #271 → #272 → #273)** → #262 →
locked-tab import (deferred part of #265).

| Issue | Content | Key decisions |
|---|---|---|
| #264 | Umbrella, stays open until #271–#273 land | Branch strategy comment on the issue |
| #271 | Locked tab: paste saves to per-memo random-name folder; **name→folder map in encrypted metadata** (decision B) | Rename = update map only; delete = trash folder; legacy flat images left as-is; schema versioning needed |
| #272 | Lock (plain→locked): `<memo_name>/` → random name + link rewrite before encryption | Must integrate with lock-in-progress marker/recovery |
| #273 | Permanent unlock: random → `<memo_name>/` + link rewrite | Collision → keep random + warning; unlock-in-progress recovery |
| #262 | Cross-tab move/copy doesn't carry image folder | Locked-tab moves IN scope (Q1); pre-#139 flat images carried only if cheap, else skipped (Q2); depends on #271 layout |

Explicitly deferred by the user: encrypting images (large); an in-app trash (Windows recycle bin stays).

Per-issue confirmation with the user between issues applies to **this series only** (they're
intertwined) — not a standing rule.

## 4. Merged this session (2026-10-02 / 10-03)

All into `V1.2.0`, issues closed, `RESOLVED-ISSUES.md` updated:

| Issue | What | PR |
|---|---|---|
| #256, #257 | PDF export in release builds (PrintToPdfAsync) + missing images (NavigateToString 2 MB limit) | #258 |
| #259 | Inserted images still > 1 MB after #82 resize → re-encoded as WebP q0.85 | #260 |
| #261 | Rename now rewrites the memo's own image links to the new folder name | #268 |
| #263 | Memo delete trashes the image folder; rename-into-existing-folder warns; **path guard** in `fs/path_safety.rs` (`memo_image_folder_path` / `existing_memo_image_folder`) — review caught `..md`/`...md` stems that could have trashed/deleted the tab's parent folder | #269 |
| #265 | `.md` import copies relative-path images into `<memo_name>/NNN.ext` (plain tabs only) | #270 |

## 5. Open issues not in the series

- #266 — memo names with spaces produce `![](新 メモ/001.png)` (needs `<...>` destination); paste and #261 rewrite both affected.
- #267 — renaming the open memo with a pending autosave loses the unsaved edits (flush before rename).

## 6. Working conventions confirmed this session

- Every user-visible message in Japanese (one English slip happened this session).
- If verification looks live-test heavy, say so up front and hand the user a manual checklist + a
  timestamped release exe; the user tests by hand. (memory: `feedback_report_heavy_live_testing_user_helps.md`)
- Before telling the user a test failed, check which exe is actually running
  (`Get-Process | ? Path -like "*TabTreeNote*"`) — one test this session ran the previous build.
- Test-file content given in chat gets mangled on copy (`[` → `\[`); write test files directly when
  the user gives a path.
- reviewer-lite: static review only (no CDP) for this series; the user does live tests.
- `/security-review` check-in before PR on anything touching paths/deletion/tab-lock.

## 7. Repo-root clutter

Timestamped test exes (`TabTreeNoteMD_2026100*-*.exe`) and `data/` (the exes' portable data folder,
contains the user's test tabs `111` / `test`) are untracked. **User manages cleanup — don't offer to
delete them.**
