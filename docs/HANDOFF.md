# Session Handoff

> Written to carry context into a fresh conversation when the previous one has grown too large.
> Read this once at the start of the new session, then treat it as disposable — once its content is
> either acted on or absorbed back into `spec.md` / `DECISIONS.md` / `QA.md` / commits, this file can be
> deleted. It is not a permanent part of the documentation set (unlike `spec.md`, `DECISIONS.md`,
> `detailed-spec.md`, `QA.md`, which `docs/plans/README.md` and `CLAUDE.md` already index).
>
> **Before acting on anything below, verify it against actual current state** (`git status`, `git log`,
> `git branch`, whether files described here still exist/match) — this file is a snapshot and can go
> stale before it's even read.
>
> This rewrite supersedes the prior `docs/HANDOFF.md` (committed `cae6121`) — that version's "next step"
> (Phase 2 implementation of `folder-import`) is now done and merged; nothing from it needed to carry
> forward except what's restated below.

## Index

1. Where the project stands
2. What this session did
3. `folder-import` implementation summary (merged)
4. Remaining 4 v1.1.0 features
5. Branch policy (unchanged from prior handoff)
6. Committed/pushed state
7. Next step
8. Minor unconfirmed observation worth watching

## 1. Where the project stands

- v1.1.0 planning is complete (unchanged from prior handoff): `docs/spec.v1.1.0.md`,
  `docs/detailed-spec.v1.1.0.md`, and all 5 feature `docs/plans/v1.1.0/<feature-name>/PLAN.md` files.
- **`folder-import` is now implemented, reviewed, and merged into `V1.1.0`** (see §3). `V1.1.0` has been
  pushed to `origin/V1.1.0` (currently at `d6c2385`).
- `docs/QA.md` is empty (no open questions) — nothing from this session's implementation/review work
  needed spec-level escalation.
- Currently checked out on a new branch, `feature/v1.1.0-tab-memo-memory`, created off `V1.1.0` at
  `d6c2385` to hold `tab-memo-memory`'s implementation — **no commits on it yet**, working tree otherwise
  matches `V1.1.0`.

## 2. What this session did

Picked up from the prior handoff (`docs/HANDOFF.md` as it stood at commit `cae6121`), whose next step was
"run `developer-lite` Phase 2 for `folder-import`, on `feature/v1.1.0-folder-import`". This session:

1. Read the prior handoff, verified it against actual `git status`/`git log`/`git branch` (matched).
2. Ran the full worker/reviewer cycle for `folder-import` (`.claude/agents/README.md`'s documented flow):
   `developer-lite` implementation → `reviewer-lite` review (found 1 real bug + a live-testing gap it
   couldn't close) → `developer-lite` fix → `reviewer-lite` live E2E retest → **Approved**. Full detail in
   §3.
3. Merged `feature/v1.1.0-folder-import` into `V1.1.0` (merge commit `d6c2385`), pushed `V1.1.0` to
   `origin`, and deleted the now-merged `feature/v1.1.0-folder-import` branch (local only; it was never
   pushed to origin, so nothing to clean up there).
4. Per the branch policy in §5, cut the next feature branch: user chose `tab-memo-memory` (of the
   remaining 4) as the next feature to implement. Created `feature/v1.1.0-tab-memo-memory` from `V1.1.0`.
   **Stopped here to write this handoff, before starting Phase 2 implementation for `tab-memo-memory`.**

## 3. `folder-import` implementation summary (merged)

OS-Folder → Tree-Pane Drag-and-Drop Import, per `docs/plans/v1.1.0/folder-import/PLAN.md`. Copy-only,
`.md`-only, in-app Rename/Overwrite/Skip collision prompt, Tab-Lock session-aware.

- **`developer-lite`** implemented backend (`src-tauri/src/commands/import.rs` new `import_file` command;
  `tab_lock.rs`'s new `session_import_file` for the Tab-Lock session path) and frontend
  (`state/useImport.ts` collision-resolution loop, `components/dialogs/ImportCollisionDialog.tsx`, a new
  reusable `Toast` primitive under `state/useToast.ts` + `components/Toast/`, `ipc/import.ts`,
  `shared/uniqueName.ts`, and the drop listener wired into `components/Tree/TreeView.tsx`). 12 commits,
  one file each. Backend tests (148/148) and frontend build passed at this point.
- **`reviewer-lite`**'s first pass found a real, reproducible bug: `useImport.ts`'s `overwriteDisabled`
  check (deciding whether the Overwrite option is disabled for an edit-locked collision target) used an
  exact-case lookup (`findNode`), while collision detection elsewhere in the same file and the backend's
  own lock check are case-insensitive. Net effect: a collision that only matches case-insensitively (e.g.
  dropped `note.md` vs. existing `Note`) let the user click a Overwrite button that should have been
  disabled — backend still rejected it safely (no data-integrity risk), but the UI-level restriction
  from `detailed-spec.v1.1.0.md` §2.1 wasn't honored. Also flagged (non-blocking): no dedicated unit test
  for `session_import_file`. Live E2E testing could not be completed in that pass — the reviewer's debug
  build kept forwarding to the user's already-running production instance
  (`G:\TabTreeNote\tabtreenotemd.exe`) via Tauri's single-instance plugin, and the reviewer correctly
  declined to close another running instance without asking first.
- User confirmed (via `AskUserQuestion`): send the bug back to `developer-lite`, and close the production
  instance so `reviewer-lite` could retest live.
- **`developer-lite`** fixed the case-sensitivity bug (commit `0e71f9a`, new `findNodeCI` helper matching
  the existing lowercase convention; `treeOps.ts`'s general-purpose `findNode` was deliberately left
  exact-match, only this call site was fixed) and added the two missing `session_import_file` unit tests
  (commit `607d81a`, mirroring the `session_create_memo`/`session_delete_memo` pattern). 150/150 backend
  tests, clean build.
- **`reviewer-lite`**'s second pass: production instance was confirmed no longer running (`tasklist`), so
  the reviewer launched its own debug build (had to use `npx tauri build --debug --no-bundle` — a plain
  `cargo build` produces a binary pointed at `localhost:1420` dev mode instead of embedded assets) and
  performed live E2E testing. Since genuine OS-level drag input isn't achievable in this sandboxed
  environment, and Tauri v2 delivers file-drop via backend-emitted `tauri://drag-*` events (not DOM drag
  events), the reviewer replayed those exact events via `window.__TAURI__.event.emit(...)` from the page's
  own JS context over CDP — this exercises 100% of this feature's own frontend+backend logic, but not
  wry's native `IDropTarget` glue itself; flagged explicitly rather than claimed as a full native-level
  test. Every scenario passed (basic import, 3-way collision resolution + apply-to-all, Esc-cancel,
  Tab-Lock session import with the bug-fix verified live, non-`.md` filtering, toast). **Verdict:
  Approved.** One minor unconfirmed anomaly was also flagged — see §8.
- Merged into `V1.1.0` (`d6c2385`), pushed. `feature/v1.1.0-folder-import` deleted (local only).

## 4. Remaining 4 v1.1.0 features

Per `docs/plans/README.md`, still not meant to be implemented in a fixed sequence — each is independently
plannable/buildable/testable:

1. **`tab-memo-memory`** (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md`) — **chosen as next, branch already
   created.** Per-Tab Last-Viewed-Memo and Scroll-Position Memory, extended to every in-session tab switch
   (not just startup). Quick Open's MRU ordering depends on this, so doing it before `quick-open` keeps
   that dependency direction clean (not a hard requirement, just why it was picked first here).
2. **`quick-open`** (`docs/plans/v1.1.0/quick-open/PLAN.md`) — Cross-Tab Quick Open (`Ctrl+O`), reuses the
   existing full-text index. Favorites reuses this feature's overlay in a second mode.
3. **`favorites`** (`docs/plans/v1.1.0/favorites/PLAN.md`) — Cross-Tab Favorites, adds `favorite` to
   `MetaTreeNode`; builds on Quick Open's overlay/index/ordering mechanics — safest done after
   `quick-open`.
4. **`tab-lock-auto-relock`** (`docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md`) — Tab-Lock Auto-Relock on
   Inactivity, single global timer, foreground-active-tab-only. Independent of the other 3 remaining
   features; can slot in anywhere.

## 5. Branch policy (unchanged from prior handoff)

- **One branch per feature implementation**, cut from `V1.1.0`, named **`feature/v1.1.0-<feature-name>`**.
- Flow: branch → `developer-lite` Phase 2 implementation → `reviewer-lite` review (with escalation back to
  `developer-lite` for fixes as needed, like `folder-import`'s case-sensitivity bug) → merge back into
  `V1.1.0` → push → delete the merged feature branch → repeat for the next feature. Order across the
  remaining features is arbitrary, chosen by the user each time.
- Distinct from the GitHub-issue workflow (`fix/<short-slug>` branches, documented in this repo's
  `CLAUDE.md`) — that convention is unchanged.
- Current branch: **`feature/v1.1.0-tab-memo-memory`** (created from `V1.1.0` at `d6c2385`, no
  implementation commits on it yet).

## 6. Committed/pushed state

- `V1.1.0` is at `d6c2385` (merge commit for `folder-import`) and **is pushed** to `origin/V1.1.0`.
- `feature/v1.1.0-folder-import` was merged and deleted (local-only branch, nothing to clean up on
  origin).
- `feature/v1.1.0-tab-memo-memory` exists locally only, at the same commit as `V1.1.0` (`d6c2385`), not
  yet pushed (nothing to push — no new commits on it yet).

Uncommitted in the working tree right now:

```
?? .claude/files/
?? "CLAUDE - コピー.md"
```

These are **pre-existing, unrelated to any v1.1.0 work** (present since before this file's predecessor was
written) — do not assume they need handling.

## 7. Next step

On branch `feature/v1.1.0-tab-memo-memory`: run `developer-lite` Phase 2 (implementation) for the
`tab-memo-memory` feature, based on `docs/plans/v1.1.0/tab-memo-memory/PLAN.md`. Follow the same
worker/reviewer cycle used for `folder-import` (§3) — `developer-lite` implements → `reviewer-lite`
reviews and live-tests → fix/retest loop if needed → merge into `V1.1.0` → push → delete branch → pick the
next feature from §4.

## 8. Minor unconfirmed observation worth watching

During `folder-import`'s live E2E retest, `reviewer-lite` observed a generic error banner
(`.ttn-tree-error`, bottom-left — distinct from this feature's own `Toast`, bottom-right) reading
`"対象が見つかりませんでした"` (`NOT_FOUND`) appear once, right after a plain memo-creation dialog
confirm, unconnected to any import/drop action just performed. It did not recur across ~10 subsequent
drop scenarios in the same session, and every import scenario's own success/skip counts and on-disk file
state were independently verified correct regardless — so this was **not** treated as a blocker or
attributed to `folder-import`. Worth keeping an eye out for if it resurfaces during `tab-memo-memory`
testing (which also touches memo creation/tab-switch flows) — if it reproduces with a clear trigger, it
should go to `docs/QA.md` or a GitHub issue rather than being re-discovered from scratch.
