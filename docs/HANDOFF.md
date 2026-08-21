# Session Handoff

> Written to carry context into a fresh conversation when the previous one has grown too large.
> Read this once at the start of the new session, then treat it as disposable — once its content is
> either acted on or absorbed back into `spec.v1.2.0.md` / `detailed-spec.v1.2.0.md` / `DECISIONS.md` /
> `QA.md` / commits, this file can be deleted. It is not a permanent part of the documentation set
> (unlike `spec.md`, `spec.v1.2.0.md`, `DECISIONS.md`, `detailed-spec.md`, `detailed-spec.v1.2.0.md`,
> `QA.md`, which `docs/README.md` and `CLAUDE.md` already index).
>
> **Before acting on anything below, verify it against actual current state** (`git status`, `git log`,
> `git branch`, whether files described here still exist/match) — this file is a snapshot and can go
> stale before it's even read. This session's merge and push both completed cleanly; **branch deletion
> (local + remote) was blocked by the Auto Mode permission classifier and is still pending** (see §3) —
> unlike the prior `cross-tab-anchor-links` session, where all three steps went through without a block.

## Index

1. Where the project stands
2. `preview-scroll-fix` — implemented, one blocking bug found and fixed, re-reviewed (approved), merged into `V1.2.0`, pushed
3. **Pending**: local + remote feature branch deletion (blocked by permission classifier)
4. Separate track: GitHub issues #190 and #191 — both filed only, neither started
5. Two long-carried-forward untracked files — still unaddressed
6. Next step — resume the nine-item implementation order, item 4 next
7. Branch policy for v1.2.0 (unchanged)
8. Process lesson reinforced this session — scoping re-review after a fix

## 1. Where the project stands

- v1.1.0 is fully complete and merged into `V1.1.0` (unchanged).
- Current branch: **`V1.2.0`**, pushed to `origin/V1.2.0` at `d3edadd`.
- **This session's new work**: implemented, debugged, and merged the third item of the nine-item
  recommended order (`preview-scroll-fix`). Items 1 (`preview-link-encoding-fix`) and 2
  (`cross-tab-anchor-links`) were already done in prior sessions.

## 2. `preview-scroll-fix` — implemented, one blocking bug found and fixed, re-reviewed (approved), merged into `V1.2.0`, pushed

Followed the standard per-plan flow (branch → implement → review → fix → re-review → confirm with user →
merge → push → [branch delete pending, see §3] → write handoff), per `dev-workflow.md` §7 and this
project's "PLAN Implementation Workflow" section in `CLAUDE.md`:

- Branch `feature/v1.2.0-preview-scroll-fix` cut from `V1.2.0`.
- `developer-lite` implemented `docs/plans/v1.2.0/preview-scroll-fix/PLAN.md` (§2–§4) across 2 commits:
  `src/editor/preview/scrollSync.ts` (new `computeTopVisiblePreviewLine`, PLAN §2) and
  `src/components/Editor/EditorPane.tsx` (save-path/restore-path `paneMode === "preview"` branches, PLAN
  §3/§4, reusing `cross-tab-anchor-links`' `PreviewPaneHandle.waitForRender` readiness gate — that
  primitive's placeholder status in the PLAN's own text is now resolved/landed). `tsc`/`vite build` clean
  at this point, but **not yet correctly working** (see next bullet).
- `reviewer-lite`'s first review pass (full `PLAN.md` §6 checklist, live via `tauri dev` + CDP) found a
  **blocking bug**: `PreviewPaneHandle.getContainer()` in `PreviewPane.tsx` returned the inner
  `.ttn-editor-preview-pane-content` div (never scrolls) instead of the outer `.ttn-editor-preview-pane`
  div (the actual `overflow-y: auto` scroll container) — so `computeTopVisiblePreviewLine`'s `scrollTop`
  read was always `0`, the save-path always computed `null`, and `scrollPosition` silently never persisted
  in preview mode. Confirmed via 3 independent live methods (direct DOM `scrollTop` check, actual-scroll +
  on-disk persisted-value check, restore-path-unaffected check).
- `developer-lite` fixed it in a new commit `4577979`: added a separate `scrollContainerRef` on the outer
  div, switched `getContainer()` to return it, left the pre-existing `containerRef` (inner div) untouched
  for its other uses (`dangerouslySetInnerHTML`, click delegation, image resolution). Self-verified live
  (scroll-position round-trip + cross-memo anchor-jump regression check) before handing back.
- `reviewer-lite`'s **re-review was deliberately scoped down** (see §8 below — this session's own
  live-testing-scope-discipline lesson, reinforced after the user flagged token-usage worry) to: code
  review of the fix diff + exactly 2 live checks (scroll-position round-trip via on-disk value, one
  cross-memo anchor-jump sanity click) — NOT a full re-run of the whole §6 checklist. **Approved.**
- Commits: `eb86d8b`, `cc486ad`, `4577979` on the feature branch, then merge commit `d3edadd`
  (`git merge --no-ff`, message `Merge feature/v1.2.0-preview-scroll-fix: ...`) into `V1.2.0`. User
  confirmed merge; push succeeded (`origin/V1.2.0` now at `d3edadd`).

## 3. Pending: local + remote feature branch deletion (blocked by permission classifier)

Unlike the prior `cross-tab-anchor-links` session (where merge/push/branch-delete all went through with no
block), this session's `git branch -d feature/v1.2.0-preview-scroll-fix` +
`git push origin --delete feature/v1.2.0-preview-scroll-fix` was **blocked by the Auto Mode permission
classifier**. The branch (local and remote) is still present as of this writing — already fully merged
into `V1.2.0` (`d3edadd`), so deleting it is safe/inert whenever the user runs it themselves or grants the
permission. Not yet done — check `git branch` / `git branch -r` to confirm current state before assuming
either way.

## 4. Separate track: GitHub issues #190 and #191 — both filed only, neither started

Not part of the v1.2.0 feature set — do not fold either into any v1.2.0 `PLAN.md`.

- **#190** (unchanged from prior `HANDOFF.md`): a heading containing the app's own color/font-size
  `<span style="...">`/`<mark>` tag produces a garbled slug in `src/editor/tocGenerator.ts`'s
  `scanHeadings()`. https://github.com/hachi921082-source/TabTreeNoteMD/issues/190
- **#191** (new this session): a self-referential `memo:TabName/MemoName#slug` anchor link — one whose
  target memo is the memo already open/focused (typically a memo linking to one of its own other
  headings) — does not jump; the anchor-jump deferred-consumer effect triggers off a `memoKey` *change*,
  which never happens for a same-memo/same-tab target. Cross-memo/cross-tab links are unaffected. Found
  by `developer-lite` while live-testing the `preview-scroll-fix` fix; pre-existing gap in the
  already-merged `cross-tab-anchor-links` feature, unrelated to that session's own change.
  https://github.com/hachi921082-source/TabTreeNoteMD/issues/191
- **User's explicit instruction for both**: file the issue, proceed later. Do not start a `fix/` branch
  on your own initiative — wait for the user to say to proceed.

## 5. Two long-carried-forward untracked files — still unaddressed

Unchanged from prior `HANDOFF.md` versions — still present, still not touched this session:

```
?? .claude/files/
?? "CLAUDE - コピー.md"
```

`.claude/files/` looks like a redundant leftover agent-distribution kit (not byte-verified against the
live `.claude/agents/*.md`); `"CLAUDE - コピー.md"` is a confirmed-stale backup, not a duplicate. Neither
deleted — ask the user first if cleanup is ever wanted.

## 6. Next step — resume the nine-item implementation order, item 4 next

Recommended order (unchanged from prior `HANDOFF.md`, item 3 now done):

1. ~~`preview-link-encoding-fix`~~ — done, merged, pushed.
2. ~~`cross-tab-anchor-links`~~ — done, merged, pushed.
3. ~~`preview-scroll-fix`~~ — **done, merged, pushed (this session, §2).**
4. **`quick-open-discoverability`** (56 lines, trivial) — next up. No known dependency on items 1–3.
5. `pane-mode-toolbar-semantics` (85 lines, low)
6. `tab-double-click-jump` (168 lines, medium)
7. `preview-zoom` (172 lines, medium) — flagged visual-interaction risk at 180% upper bound, see
   `DECISIONS.md`'s zoom-mechanism entry.
8. `math-rendering` (155 lines, medium-high) — new KaTeX dependency.
9. `i18n` (248 lines, largest, deliberately last) — sweeps up every other feature's new UI strings.

Items 4–8 remain mutually independent and reorderable. Per `docs/plans/README.md`'s "Independent
Implementation, Mandatory Final Integration Pass" section, `reviewer-lite` must still do one whole-app
integration pass after all nine land.

Ask the user before starting item 4, same as every stage transition in this project's workflow.

## 7. Branch policy for v1.2.0 (unchanged)

**`V1.2.0` does NOT merge back into `V1.1.0`.** Per the user: "v1.1.0とは別にv1.2.0として開発を継続するの
でv1.1.0へマージはしない".

`feature/v1.2.0-<name>` branches, merged directly into `V1.2.0` (no PR) — this session's
`preview-scroll-fix` merge (§2) is the third confirmed instance of this pattern.

## 8. Process lesson reinforced this session — scoping re-review after a fix

The user flagged (mid-session) a worry about live-testing token burn recurring "like before." This led to
two concrete scope-narrowing decisions, both worth repeating in future fix/re-review cycles:

- When a `reviewer-lite` pass finds a bug and the fix goes back to `developer-lite`, the **re-verification
  request should target only the specific broken behavior** (plus a light regression sanity check on
  adjacent code the fix touches) — not a full re-run of every scenario the first pass already confirmed
  passing. Applied both when re-briefing `developer-lite` (self-verify) and when re-briefing `reviewer-lite`
  (independent re-review).
- A heavy scenario (e.g. "restart the whole app") can sometimes be swapped for an equivalent
  cheaper check (reading the persisted value straight from the on-disk settings file) without losing
  verification value, when the user prefers to do the truly-heavy manual step themselves.
- This is now recorded in the persistent memory file `feedback_live_testing_scope_discipline.md` (item 5)
  — read that before writing the next round of `developer-lite`/`reviewer-lite` prompts in a fix cycle.
