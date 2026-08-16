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
> This rewrite supersedes the version previously sitting uncommitted in the working tree (which itself
> was a rewrite of committed `4465001`, never committed) — that draft's "next step" (PLAN.md creation) is
> now done; nothing from it needed to carry forward except what's restated below.

## Index

1. Where the project stands
2. What this session did
3. The 5 v1.1.0 features and their PLAN.md status
4. Cross-cutting decisions worth knowing
5. Branch policy for v1.1.0 implementation (new this session)
6. Committed/pushed state
7. Next step

## 1. Where the project stands

- All v1.1.0 planning is **complete**: `docs/spec.v1.1.0.md`, `docs/detailed-spec.v1.1.0.md`, and all 5
  feature `docs/plans/v1.1.0/<feature-name>/PLAN.md` files are finished, reviewed, and committed to
  `V1.1.0` (9 commits, not yet pushed — see §6).
- `docs/QA.md` is empty (no open questions).
- Implementation (Phase 2) has **not started**. Currently checked out on a new branch,
  `feature/v1.1.0-folder-import`, created off `V1.1.0` specifically to hold `folder-import`'s
  implementation once it begins (see §5) — no commits on it yet, working tree otherwise matches `V1.1.0`.

## 2. What this session did

Picked up from a prior session's handoff (`docs/HANDOFF.md` as it stood at commit `4465001`, plus an
uncommitted in-progress rewrite of it — both are now superseded by this file). This session:

1. Read the prior handoff, verified it against actual `git status`/`git log` (matched).
2. Asked `developer-lite` to create Phase-1 implementation plans for the 5 confirmed v1.1.0 features,
   based on `docs/detailed-spec.v1.1.0.md`, organized **by feature name** (not build/dependency order),
   under a **new `docs/plans/v1.1.0/` folder** — explicit user instruction to never touch any existing
   `docs/plans/<feature-name>/PLAN.md` file. Confirmed via `AskUserQuestion` before proceeding, since a
   prior handoff had flagged this exact "existing plan needs a version-specific change" scenario as
   deliberately undecided.
3. `developer-lite` found 2 real problems while grounding the plans in the actual `src/` implementation
   (not just spec text) and correctly escalated both to `docs/QA.md` instead of deciding unilaterally —
   see §4 for both. Both were resolved by the user via `AskUserQuestion`, then formalized by `designer-lite`
   into `docs/spec.v1.1.0.md` / `docs/detailed-spec.v1.1.0.md` / `docs/DECISIONS.md`, and `docs/QA.md` was
   cleared. `developer-lite` then finalized the two previously-blocked plans.
4. Updated `docs/plans/README.md`'s index with a new "v1.1.0 Feature Plans" section listing all 5 new
   plans and explaining the frozen-existing-plans convention.
5. Committed all 9 changed/new files individually via the `commit-with-role` agent, each commit message
   describing that file's role (Japanese, per this repo's commit convention) — see §6 for hashes.
6. Established a **branch-per-feature-implementation policy** for v1.1.0 (§5) and created the first
   branch, `feature/v1.1.0-folder-import`, per the user's explicit choice of `folder-import` as the
   feature to start with. **Stopped here at the user's request, before starting Phase 2
   implementation.**

## 3. The 5 v1.1.0 features and their PLAN.md status (all complete)

1. **`docs/plans/v1.1.0/folder-import/PLAN.md`** — OS-Folder → Tree-Pane Drag-and-Drop Import. Copy-only,
   `.md`-only, in-app Rename/Overwrite/Skip collision prompt. No dependency on the other 4 features.
2. **`docs/plans/v1.1.0/tab-memo-memory/PLAN.md`** — Per-Tab Last-Viewed-Memo and Scroll-Position Memory,
   extended to every in-session tab switch (not just startup). Quick Open's MRU ordering depends on this.
3. **`docs/plans/v1.1.0/quick-open/PLAN.md`** — Cross-Tab Quick Open (`Ctrl+O`, see §4), reuses the
   existing full-text index. Favorites reuses this feature's overlay in a second mode.
4. **`docs/plans/v1.1.0/favorites/PLAN.md`** — Cross-Tab Favorites, adds `favorite` to `MetaTreeNode`;
   builds on Quick Open's overlay/index/ordering mechanics.
5. **`docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md`** — Tab-Lock Auto-Relock on Inactivity, single
   global timer, foreground-active-tab-only (see §4). Independent of the other 4 features.

Per `docs/plans/README.md`, these are **not** meant to be implemented in a fixed sequence — each is
independently plannable/buildable/testable, which is why the branch policy in §5 splits them apart rather
than implementing all 5 on one branch.

## 4. Cross-cutting decisions worth knowing

- **Quick Open's default shortcut is `Ctrl+O`, not `Ctrl+Shift+P`**: `spec.v1.1.0.md`'s original
  conflict-check only cross-referenced `window-operation/PLAN.md`'s original shortcut table and missed
  that `Ctrl+Shift+P` was later bound to `markdown.paneModeCycle` (added by a later plan). `developer-lite`
  caught this by checking the live `src/shared/shortcuts.ts` directly. User chose to assign Quick Open a
  new unused combo rather than reassign `paneModeCycle`'s already-shipped binding. `Ctrl+O` was verified
  free in `shortcuts.ts` by both `designer-lite` and `developer-lite` independently. **Lesson recorded in
  `docs/DECISIONS.md`**: future shortcut conflict-checks should verify against `src/shared/shortcuts.ts`
  directly, not just `window-operation/PLAN.md`'s table, since the table can go stale as later plans add
  bindings.
- **Tab-Lock Auto-Relock is foreground-active-tab-only, by deliberate choice**: `spec.v1.1.0.md` §5.1
  originally justified a single global timer on the premise "at most one locked session can ever be alive
  at a time" — `developer-lite` found this false for the already-shipped Tab Lock feature (a cross-tab
  drop-onto-a-locked-tab can leave a session alive in the background with no end-trigger at all,
  documented in `docs/plans/tab-lock/PLAN.md`'s own Implementation Notes and in `src/state/useTabLock.ts`).
  User chose to keep Auto-Relock scoped to the foreground tab only and explicitly accept the backgrounded-
  session gap as a pre-existing condition this feature does not need to close. `spec.v1.1.0.md` §5.1 now
  states this scope boundary explicitly rather than resting on the disproven premise.
- **Citation-not-restatement discipline in `docs/plans/v1.1.0/`**: every new v1.1.0 plan that builds on a
  data structure/mechanism from an existing (untouched) plan — `TabMeta`/`AppSettings`
  (`tab-management/PLAN.md`, `persistence-fs/PLAN.md`), `MetaTreeNode` (`memo-tree/PLAN.md`), the search
  index (`search-index/PLAN.md`), Tab Lock's session lifecycle (`tab-lock/PLAN.md`) — cites the relevant
  section rather than duplicating or editing it. Full citation list is in `developer-lite`'s Phase-1
  report (not persisted anywhere else); spot-check by grepping `docs/plans/v1.1.0/*/PLAN.md` for
  references to the other feature folders if this needs re-verification later.

## 5. Branch policy for v1.1.0 implementation (new this session)

User-confirmed policy (via `AskUserQuestion`), replacing "everything on `V1.1.0` directly":

- **One branch per feature implementation**, cut from `V1.1.0`, named **`feature/v1.1.0-<feature-name>`**
  (matches the `docs/plans/v1.1.0/<feature-name>/` folder names in §3).
- Flow: branch → `developer-lite` Phase 2 implementation on that branch → review → merge back into
  `V1.1.0` → repeat for the next feature. Order across the 5 features is arbitrary (no dependency between
  them, per §3), chosen by the user each time.
- This is **distinct** from the existing GitHub-issue workflow (`fix/<short-slug>` branches, documented in
  this repo's `CLAUDE.md`) — that convention is unchanged and still applies to issue fixes.
- First branch created: **`feature/v1.1.0-folder-import`** (user's explicit choice of which feature to
  start with). No implementation commits on it yet.

## 6. Committed/pushed state

All committed to `V1.1.0` this session (9 commits, one file each, Japanese role-description messages via
`commit-with-role`), **not pushed** to `origin/V1.1.0`:

- `699bd57` — `docs/spec.v1.1.0.md`
- `420a959` — `docs/detailed-spec.v1.1.0.md`
- `a50aa73` — `docs/DECISIONS.md`
- `82f10a9` — `docs/plans/v1.1.0/folder-import/PLAN.md`
- `f36e3e7` — `docs/plans/v1.1.0/tab-memo-memory/PLAN.md`
- `e71e659` — `docs/plans/v1.1.0/quick-open/PLAN.md`
- `623e24e` — `docs/plans/v1.1.0/favorites/PLAN.md`
- `bfe98f9` — `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md`
- `3f8704f` — `docs/plans/README.md`

Then `feature/v1.1.0-folder-import` was created from that same point (no new commits on it).

Uncommitted in the working tree right now:

```
 M docs/HANDOFF.md   <- this rewrite itself; not committed, left for the user to review
?? .claude/files/
?? "CLAUDE - コピー.md"
```

The last two are **pre-existing, unrelated to any v1.1.0 work** (present since before this file's
predecessor was written) — do not assume they need handling.

## 7. Next step

On branch `feature/v1.1.0-folder-import`: run `developer-lite` Phase 2 (implementation) for the
`folder-import` feature, based on `docs/plans/v1.1.0/folder-import/PLAN.md`. After implementation +
`reviewer-lite` sign-off, merge into `V1.1.0` and, per §5's policy, branch again from `V1.1.0` for
whichever of the remaining 4 features the user picks next (`tab-memo-memory`, `quick-open`, `favorites`,
`tab-lock-auto-relock`). No push decision has been made yet for `V1.1.0`'s 9 local commits — hold until
the user asks.
