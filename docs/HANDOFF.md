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
> stale before it's even read. **This session did no feature implementation** — it was a short repo-
> hygiene session (see §4). Everything in §1–§3, §5–§7 below describes the *prior* session's work
> (`preview-scroll-fix`, items 1–3 of the nine-item order) and is carried forward unchanged, still
> believed current as of this writing.

## Index

1. Where the project stands
2. `preview-scroll-fix` — implemented, reviewed, merged, pushed (prior session, unchanged since)
3. Separate track: GitHub issues #190 and #191 — both filed only, neither started
4. Untracked files — `.claude/files/` now gitignored (this session); `CLAUDE - コピー.md` still unaddressed
5. Next step — resume the nine-item implementation order, item 4 next
6. Branch policy for v1.2.0 (unchanged)
7. Process lesson from a prior session — scoping re-review after a fix (still applicable)

## 1. Where the project stands

- v1.1.0 is fully complete and merged into `V1.1.0` (unchanged).
- Current branch: **`V1.2.0`**, pushed to `origin/V1.2.0` at `d7c66e2` (prior session's last push).
  This session added two more commits on top (§4) — not yet pushed, see below.
- **This session's actual work**: none of the nine-item implementation order — purely a small repo-
  hygiene fix (§4). Item 4 (`quick-open-discoverability`) has **not** been started.

## 2. `preview-scroll-fix` — implemented, reviewed, merged, pushed (prior session)

Third item of the nine-item recommended order. Completed in the session before this one; nothing about
it changed this session. Kept here verbatim as it's still the most recent feature-implementation record:

- Branch `feature/v1.2.0-preview-scroll-fix` cut from `V1.2.0`, implemented per
  `docs/plans/v1.2.0/preview-scroll-fix/PLAN.md` §2–§4 (new `computeTopVisiblePreviewLine` in
  `src/editor/preview/scrollSync.ts`; `paneMode === "preview"` save/restore branches in
  `EditorPane.tsx`, reusing `cross-tab-anchor-links`'s `PreviewPaneHandle.waitForRender` readiness gate).
- `reviewer-lite`'s first pass found a **blocking bug**: `PreviewPaneHandle.getContainer()` returned the
  inner non-scrolling `.ttn-editor-preview-pane-content` div instead of the outer scroll container
  (`.ttn-editor-preview-pane`), so `scrollPosition` never persisted in preview mode. Fixed in commit
  `4577979` (new `scrollContainerRef` on the outer div). Re-review was deliberately scoped down (§7) to
  the fix diff + 2 targeted live checks, not a full re-run — **approved**.
- Commits `eb86d8b`, `cc486ad`, `4577979` → merge commit `d3edadd` into `V1.2.0`, pushed, feature branch
  deleted (first deletion attempt was blocked by the Auto Mode permission classifier; succeeded on retry
  after the user explicitly asked again).

## 3. Separate track: GitHub issues #190 and #191 — both filed only, neither started

Not part of the v1.2.0 feature set — do not fold either into any v1.2.0 `PLAN.md`. Unchanged this
session.

- **#190**: a heading containing the app's own color/font-size `<span style="...">`/`<mark>` tag
  produces a garbled slug in `src/editor/tocGenerator.ts`'s `scanHeadings()`.
  https://github.com/hachi921082-source/TabTreeNoteMD/issues/190
- **#191**: a self-referential `memo:TabName/MemoName#slug` anchor link (target memo already
  open/focused) doesn't jump — the anchor-jump deferred-consumer effect triggers off a `memoKey`
  *change*, which never happens for a same-memo/same-tab target. Found while live-testing
  `preview-scroll-fix`; pre-existing gap in the already-merged `cross-tab-anchor-links` feature.
  https://github.com/hachi921082-source/TabTreeNoteMD/issues/191
- **User's explicit instruction for both**: file the issue, proceed later. Do not start a `fix/` branch
  on your own initiative — wait for the user to say to proceed.

## 4. Untracked files — `.claude/files/` now gitignored (this session); `CLAUDE - コピー.md` still unaddressed

The prior `HANDOFF.md` carried these two forward, unaddressed, across many sessions:

```
?? .claude/files/
?? "CLAUDE - コピー.md"
```

**This session, `.claude/files/` was resolved**: the user asked what it was; inspection showed it's an
uncustomized Japanese template copy of the `designer-lite`/`developer-lite`/`reviewer-lite`
agent-definition files (`{{APP_NAME}}`-style placeholders never filled in), plus `setup-agents.bat`/
`setup-agents.ps1`. It is unrelated to and stale relative to the live, actually-used agent definitions
in `.claude/agents/` (English, `TabTreeNoteMD`-specific, last touched Aug 15–17 vs. this folder's Aug 10).
It had never been committed. The user asked to exclude it from tracking rather than delete it — added
`.claude/files/` to `.gitignore` (commit `4744fcb`). No file inside it was modified or deleted.

`"CLAUDE - コピー.md"` (a confirmed-stale backup, not a duplicate) is still untouched — ask the user
first if cleanup is ever wanted.

## 5. Next step — resume the nine-item implementation order, item 4 next

Recommended order (unchanged; items 1–3 done):

1. ~~`preview-link-encoding-fix`~~ — done, merged, pushed.
2. ~~`cross-tab-anchor-links`~~ — done, merged, pushed.
3. ~~`preview-scroll-fix`~~ — done, merged, pushed (§2).
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

## 6. Branch policy for v1.2.0 (unchanged)

**`V1.2.0` does NOT merge back into `V1.1.0`.** Per the user: "v1.1.0とは別にv1.2.0として開発を継続するの
でv1.1.0へマージはしない".

`feature/v1.2.0-<name>` branches, merged directly into `V1.2.0` (no PR) — `preview-scroll-fix`'s merge
(§2) is the third confirmed instance of this pattern.

## 7. Process lesson from a prior session — scoping re-review after a fix (still applicable)

When a `reviewer-lite` pass finds a bug and the fix goes back to `developer-lite`, the **re-verification
request should target only the specific broken behavior** (plus a light regression sanity check on
adjacent code the fix touches) — not a full re-run of every scenario the first pass already confirmed
passing. A heavy scenario (e.g. "restart the whole app") can sometimes be swapped for an equivalent
cheaper check (reading the persisted value straight from the on-disk settings file) without losing
verification value. Recorded in the persistent memory file `feedback_live_testing_scope_discipline.md`
(item 5) — read that before writing the next round of `developer-lite`/`reviewer-lite` prompts in a fix
cycle.
