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
> stale before it's even read. The prior version of this file described the `docs/plans/v1.2.0/` nine
> `PLAN.md` files as freshly created, no implementation started. This version reflects the first
> implementation item landing: `preview-link-encoding-fix` implemented, reviewed, merged (locally) into
> `V1.2.0`, with two git operations still blocked pending user action (§3), plus an unfinished side
> thread on formalizing the per-plan branch workflow itself (§4).

## Index

1. Where the project stands
2. `preview-link-encoding-fix` — implemented, reviewed (approved), merged into `V1.2.0` (local)
3. Blocked git operations awaiting user action: push, feature-branch deletion
4. Unfinished side thread: `dev-workflow.md` rename + PLAN-implementation-flow rule (not yet executed)
5. Two long-carried-forward untracked files — now actually inspected
6. Separate track: GitHub issue #190 (TOC/color-tag bug) — filed only, still not started
7. Next step — resume the nine-item implementation order, item 2 next
8. Branch policy for v1.2.0 (carried forward, now confirmed by precedent)

## 1. Where the project stands

- v1.1.0 is fully complete and merged into `V1.1.0` (unchanged).
- Current branch: **`V1.2.0`**. `docs/spec.v1.2.0.md` (9 items, fully decided) and
  `docs/detailed-spec.v1.2.0.md` (trimmed, implementation-planning-level) both exist and are committed.
  `docs/plans/v1.2.0/` holds all nine feature `PLAN.md` files, committed and pushed as of the prior
  session (`8d5c172`).
- **This session's new work**: implemented and merged the first item of the nine-item recommended order
  (`preview-link-encoding-fix`). No other feature has been touched yet.

## 2. `preview-link-encoding-fix` — implemented, reviewed (approved), merged into `V1.2.0` (local)

Followed the newly-adopted per-plan flow (branch → implement → review → merge), applied manually this
once since its formal write-up is still pending (§4):

- Branch `feature/v1.2.0-preview-link-encoding-fix` cut from `V1.2.0`, mirroring v1.1.0's
  `feature/v1.1.0-<name>` naming (confirmed against actual v1.1.0 merge-commit history, not just the
  prior `HANDOFF.md`'s hedge about it).
- `developer-lite` implemented both fixes in `src/editor/preview/previewInteractions.ts`'s
  `handleAnchorClick` per `docs/plans/v1.2.0/preview-link-encoding-fix/PLAN.md` §2/§3: `decodeURIComponent`
  on the `#slug` branch and the full `memo:`-prefixed href, each wrapped in `try/catch` degrading to a
  silent no-op on a malformed percent-sequence. `tsc --noEmit` and `vite build` both clean.
- `reviewer-lite` approved after live-testing via `tauri dev` + CDP: all `PLAN.md` §5 scenarios passed
  (non-ASCII `#slug` click, non-ASCII `memo:` click, live-view regression, malformed-sequence no-op,
  literal-`%XX`-name regression). One non-blocking note: `PLAN.md` §5's example malformed-percent-sequence
  (a stray `%` not followed by two hex digits) actually gets auto-corrected to `%25` by markdown-it's
  `normalizeLink` before reaching the click handler, so it never exercises the `catch` branch in practice —
  a different malformed shape (e.g. `%E3%80`, an incomplete UTF-8 sequence) does reach and correctly
  exercise it. Worth a one-line correction to `PLAN.md` §5's example next time that file is touched; not
  functionally blocking.
- Commit `8c608f3` (previewInteractions.ts, no path prefix per convention), then
  `git merge --no-ff feature/v1.2.0-preview-link-encoding-fix` into `V1.2.0`, mirroring v1.1.0's
  `Merge feature/v1.1.0-<name>: <description>` merge-commit message style.

## 3. Blocked git operations awaiting user action: push, feature-branch deletion

Both `git push origin V1.2.0` and `git branch -d feature/v1.2.0-preview-link-encoding-fix` (branch is
already fully merged, `-d` not `-D`) were denied by Claude Code's Auto Mode permission classifier — it
blocks git operations that touch shared/remote state or delete branches, regardless of prior user
go-ahead in chat. `git commit` and `git merge` (both local) went through without issue.

**Whoever picks this up next**: either run these two commands directly, or grant the tool permission when
prompted. Until the push happens, `origin/V1.2.0` does not yet have this fix — local `V1.2.0` is ahead by
one merge commit plus the feature commit.

## 4. Unfinished side thread: `dev-workflow.md` rename + PLAN-implementation-flow rule (not yet executed)

Before implementation started this session, the user asked to formalize "cut a branch before every
`PLAN.md` implementation, implement, review, merge, write a handoff, move to next context" as a standing
rule — parallel to the existing per-issue `fix/<short-slug>` flow. Discussion reached an agreed approach
but **execution never happened** (the conversation pivoted straight to implementing
`preview-link-encoding-fix` instead, which is why §2 above says the flow was "applied manually this once").

**Agreed so far, not yet written anywhere:**
- Target file: rename `~/.claude/shared/issue-and-doc-workflow.md` → `~/.claude/shared/dev-workflow.md`.
  Rationale: the file already covers two topics (issue flow, doc-structure standard including
  `docs/plans/<feature-name>/PLAN.md`), so the current name reads as issue-only and is misleading; a new
  "PLAN Implementation Flow" section would be a third topic under the same file, not a new file.
- Add an index to that file — it already has 7 `##` headings (meets this user's own global "≥4 headings
  → needs an index" rule from `~/.claude/CLAUDE.md`), independent of whether the new section is added.
- Update the two files that reference the old name: `~/.claude/CLAUDE.md` line 60 (prose reference, not an
  active import) and this project's own `CLAUDE.md` line 3 (`@import`, the only live import found via a
  repo-wide grep — no other project currently opts into this shared file).
- **Not yet decided**: the actual content of the new "PLAN Implementation Flow" section (branch naming
  convention, merge-target rule, handoff-file convention). §2 and §7/§8 of this file establish real
  precedent for all three now (branch name `feature/v1.2.0-<name>`, direct merge into `V1.2.0`, this very
  `HANDOFF.md` file) — the next session doing this write-up should draw on that precedent rather than
  re-deciding from scratch.

## 5. Two long-carried-forward untracked files — now actually inspected

Every prior `HANDOFF.md` back to the v1.1.0 era repeated the same two lines about these files
("pre-existing, unrelated, still worth a fresh check") without anyone actually checking. This session
finally looked:

```
?? .claude/files/
?? "CLAUDE - コピー.md"
```

- **`.claude/files/`**: contains `designer-lite.md`, `developer-lite.md`, `reviewer-lite.md`, `README.md`,
  `setup-agents.bat`, `setup-agents.ps1`. Reads like a leftover distribution/bootstrap kit from when the
  lite agent set was set up — the actual live agents already exist at `.claude/agents/*.md`, so this looks
  redundant, but **not verified byte-for-byte identical** and not deleted this session (no unilateral
  deletions of unexplained files).
- **`"CLAUDE - コピー.md"`**: a stale backup of `CLAUDE.md`, confirmed **not** identical to the current
  `CLAUDE.md` (2685 vs. 2766 chars) — an old snapshot from before some since-made edit, not a duplicate.
- Neither was touched. Whoever has a reason to clean these up should ask the user first — this session
  only got as far as confirming what they are.

## 6. Separate track: GitHub issue #190 (TOC/color-tag bug) — filed only, still not started

Unchanged. Not part of the v1.2.0 feature set — do not fold it into any v1.2.0 `PLAN.md`.

- **Bug**: a heading containing the app's own color/font-size `<span style="...">`/`<mark>` tag produces
  a garbled slug in `src/editor/tocGenerator.ts`'s `scanHeadings()`.
- **Filed**: https://github.com/hachi921082-source/TabTreeNoteMD/issues/190
- **User's explicit instruction**: file the issue, proceed later. Do not start a `fix/` branch on your
  own initiative — wait for the user to say to proceed.

## 7. Next step — resume the nine-item implementation order, item 2 next

Recommended order (unchanged from the prior `HANDOFF.md`, item 1 now done):

1. ~~`preview-link-encoding-fix`~~ — **done, merged locally into `V1.2.0`, push pending (§3).**
2. **`cross-tab-anchor-links`** (213 lines, high difficulty) — next up. Hard dependency on item 1 (now
   satisfied). Defines the shared preview-render-readiness-gate primitive that item 3 reuses; do this
   while item 1's encoding-fix details are still fresh, per the original ordering rationale.
3. `preview-scroll-fix` (140 lines, medium) — reuses item 2's primitive.
4. `quick-open-discoverability` (56 lines, trivial)
5. `pane-mode-toolbar-semantics` (85 lines, low)
6. `tab-double-click-jump` (168 lines, medium)
7. `preview-zoom` (172 lines, medium) — flagged visual-interaction risk at 180% upper bound, see
   `DECISIONS.md`'s zoom-mechanism entry.
8. `math-rendering` (155 lines, medium-high) — new KaTeX dependency.
9. `i18n` (248 lines, largest, deliberately last) — sweeps up every other feature's new UI strings.

Items 4–8 remain mutually independent and reorderable; 1→2→3 is a hard dependency chain. Per
`docs/plans/README.md`'s "Independent Implementation, Mandatory Final Integration Pass" section,
`reviewer-lite` must still do one whole-app integration pass after all nine land.

Ask the user before starting item 2, same as every stage transition in this project's workflow.

## 8. Branch policy for v1.2.0 (carried forward, now confirmed by precedent)

**`V1.2.0` does NOT merge back into `V1.1.0`.** Per the user: "v1.1.0とは別にv1.2.0として開発を継続するの
でv1.1.0へマージはしない".

The prior `HANDOFF.md` left the actual per-item branch strategy as an open question ("confirm the actual
merge target before assuming it's `V1.2.0` by default"). §2 above resolves this in practice for at least
one item: `feature/v1.2.0-<name>` branches, merged directly into `V1.2.0` (no PR), mirroring v1.1.0's
established pattern exactly. Treat this as working precedent, not yet a written-down rule (see §4).
