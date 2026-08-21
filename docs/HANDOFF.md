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
> stale before it's even read.

## Index

1. Where the project stands
2. `tab-double-click-jump` (item 6) — implemented, reviewed, merged, pushed (this session)
3. Separate track: GitHub issues #190 and #191 — both filed only, neither started
4. Untracked file — `CLAUDE - コピー.md` still unaddressed
5. Next step — resume the nine-item implementation order, item 7 next
6. Branch policy for v1.2.0 (unchanged)
7. Process note — commit-hygiene lapse this session (caught and fixed before push)
8. Process note — token-waste/orchestrator-responsibility lesson this session (pointer to memory, not repeated in full here)

## 1. Where the project stands

- v1.1.0 is fully complete and merged into `V1.1.0` (unchanged).
- Current branch: **`V1.2.0`**, pushed to `origin/V1.2.0` at `eb34b91` (this session's last push).
- **This session's work**: item 6 (`tab-double-click-jump`) implemented, reviewed, merged into `V1.2.0`,
  and pushed. Feature branch deleted locally after merge (per user confirmation). 6 of 9 items now done.

## 2. `tab-double-click-jump` (item 6) — implemented, reviewed, merged, pushed (this session)

- Branch `feature/v1.2.0-tab-double-click-jump` cut from `V1.2.0`, implemented per
  `docs/plans/v1.2.0/tab-double-click-jump/PLAN.md` §2–§5:
  - `TabBar.tsx`: new `TabBarProps.onActiveTabDoubleClick`; `onDoubleClick` on the tab `<div>` guarded by
    `tab.id === activeTabId`; active tab's `title` tooltip gains `(ダブルクリックで先頭へ)`.
  - `EditorPane.tsx`: new `EditorPaneHandle.toggleJumpToStart`; `jumpToggleRef` (`{anchor, head,
    topLinePos}` or `null`) — first double-click captures position and jumps to doc start, second
    restores and clears; `ChangeDesc.mapPos()` remaps the saved ref on every `docChanged` update so an
    edit between jump and toggle-back doesn't land on a stale offset; reset added to the
    `loadGeneration`-gated view-recreation effect, placed before the `!ed.content` early-return.
  - `App.tsx`: wires `onActiveTabDoubleClick={() => editorRef.current?.toggleJumpToStart()}`.
- **Flagged deviation, resolved as non-blocking**: PLAN.md §4 claimed the three pre-existing session-only
  resets (`liveViewOverride`/`paneModeOverride`/`previewLockOverride`) already ran before the
  `!ed.content` early-return "at lines ~514–516" — `reviewer-lite` confirmed this line-number/ordering
  claim is factually wrong against current code (those three resets actually sit *after* the early
  return, unrelated to this feature). No interaction risk between the two reset groups found; not a
  blocker. Worth fixing PLAN.md's stale claim in a future doc-consistency pass.
- `reviewer-lite` ran all eight PLAN §6 scenarios live via CDP-driven `tauri dev` (jump/toggle-back/
  fresh-recapture, edit-then-toggle-back remap check, inactive-tab no-op, tab-switch discards toggle,
  no-memo-open no-op, tooltip check) — all passed, no console errors. **Approved, no blocking issues.**
- Commits `901a3fc` (TabBar.tsx), `6c2af29` (EditorPane.tsx), `32d126b` (App.tsx) → merge commit
  (message: "Merge feature/v1.2.0-tab-double-click-jump: ...") into `V1.2.0` at `eb34b91`, pushed,
  feature branch deleted locally after user confirmation.

## 3. Separate track: GitHub issues #190 and #191 — both filed only, neither started

Not part of the v1.2.0 feature set — do not fold either into any v1.2.0 `PLAN.md`. Unchanged this
session.

- **#190**: a heading containing the app's own color/font-size `<span style="...">`/`<mark>` tag
  produces a garbled slug in `src/editor/tocGenerator.ts`'s `scanHeadings()`.
  https://github.com/hachi921082-source/TabTreeNoteMD/issues/190
- **#191**: a self-referential `memo:TabName/MemoName#slug` anchor link (target memo already
  open/focused) doesn't jump — pre-existing gap in the already-merged `cross-tab-anchor-links` feature.
  https://github.com/hachi921082-source/TabTreeNoteMD/issues/191
- **User's explicit instruction for both**: file the issue, proceed later. Do not start a `fix/` branch
  on your own initiative — wait for the user to say to proceed.

## 4. Untracked file — `CLAUDE - コピー.md` still unaddressed

`"CLAUDE - コピー.md"` (a confirmed-stale backup, not a duplicate) remains untracked and untouched,
unchanged across this session. Ask the user first if cleanup is ever wanted.

## 5. Next step — resume the nine-item implementation order, item 7 next

Recommended order (unchanged; items 1–6 done):

1. ~~`preview-link-encoding-fix`~~ — done, merged, pushed.
2. ~~`cross-tab-anchor-links`~~ — done, merged, pushed.
3. ~~`preview-scroll-fix`~~ — done, merged, pushed.
4. ~~`quick-open-discoverability`~~ — done, merged, pushed.
5. ~~`pane-mode-toolbar-semantics`~~ — done, merged, pushed.
6. ~~`tab-double-click-jump`~~ — done, merged, pushed (§2, this session).
7. **`preview-zoom`** (172 lines, medium) — next up. Flagged visual-interaction risk at 180% upper
   bound, see `DECISIONS.md`'s zoom-mechanism entry.
8. `math-rendering` (155 lines, medium-high) — new KaTeX dependency.
9. `i18n` (248 lines, largest, deliberately last) — sweeps up every other feature's new UI strings.

Items 7–8 remain mutually independent and reorderable. Per `docs/plans/README.md`'s "Independent
Implementation, Mandatory Final Integration Pass" section, `reviewer-lite` must still do one whole-app
integration pass after all nine land — items 7–9 still unimplemented, so this pass is not due yet.

Ask the user before starting item 7, same as every stage transition in this project's workflow.

## 6. Branch policy for v1.2.0 (unchanged)

**`V1.2.0` does NOT merge back into `V1.1.0`.** Per the user: "v1.1.0とは別にv1.2.0として開発を継続するの
でv1.1.0へマージはしない".

`feature/v1.2.0-<name>` branches, merged directly into `V1.2.0` (no PR) — item 6 (§2) is the sixth
confirmed instance of this pattern.

## 7. Process note — commit-hygiene lapse this session (caught and fixed before push)

While committing item 6's changes, `git commit -m ... -- src/components/TabBar/TabBar.tsx` was run
without noticing `docs/HANDOFF.md` (a prior session's leftover, staged-but-never-committed change) was
already sitting in the index — the commit swept both files in together, violating the "one file per
commit" convention. Caught immediately by checking `git show --stat HEAD` after the commit rather than
assuming it matched what was staged. Fixed locally and safely (`git reset --soft HEAD~1`, nothing had
been pushed yet) by re-splitting into two commits. Lesson: after any commit, verify its actual file list
when anything was already staged going in — don't trust that `git commit -m ... -- <one path>` only
touched that path without checking, since a stale staged file elsewhere in the tree can ride along.

## 8. Process note — token-waste/orchestrator-responsibility lesson this session

During item 6's review, `reviewer-lite`'s live-testing tool-call volume looked excessive from the
outside, and the assistant separately wrote an overly long, unrequested memory entry about it — both
traced back to the orchestrator (the assistant), not subagent misbehavior. Full detail recorded in
persistent memory (`feedback_live_testing_scope_discipline.md` item 6, `feedback_ask_dont_dump_
conserve_tokens.md`'s recurrence note, and new `feedback_responsibility_means_enforcement.md`) —
not repeated here in full; read those before the next subagent-heavy session if token discipline
matters again.
