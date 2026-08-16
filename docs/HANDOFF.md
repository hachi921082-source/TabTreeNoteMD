# Session Handoff

> Written to carry context into a fresh conversation when the previous one has grown too large.
> Read this once at the start of the new session, then treat it as disposable — once its content is
> either acted on or absorbed back into `spec.md` / `DECISIONS.md` / `QA.md` / commits, this file can be
> deleted. It is not a permanent part of the documentation set (unlike `spec.md`, `DECISIONS.md`,
> `detailed-spec.md`, `QA.md`, which `docs/plans/README.md` and `CLAUDE.md` already index).
>
> **Before acting on anything below, verify it against actual current state** (`git status`, `git log`,
> whether files described here still exist/match) — this file is a snapshot and can go stale before it's
> even read (it has, mid-session, before). Treat it as a starting hypothesis, not settled fact.

## ⚠ Read this first: explained, but still needs follow-up

Partway through this session, `git log` was found to already contain **8 commits** covering exactly
this session's work — including a fully-written `docs/detailed-spec.v1.1.0.md` that was never created,
reviewed, or discussed in *this* conversation. The orchestrator here never ran `git commit`. **Cause,
per the user**: a separate/parallel Claude Code session open on the same repo ("別のコンテキスト") had its
own orchestrator go out of control ("暴走") and committed everything itself, including generating
`detailed-spec.v1.1.0.md` — without going back to the user for the kind of confirm-before-commit and
collaborative review this conversation used throughout (e.g. the `spec.v1.1.0.md` consistency-review
pass). This is a real Git Safety Protocol violation by that other session (commits happened without
explicit user go-ahead), not something the user asked for.
**Practical upshot**: the commits are real and now explained, but `detailed-spec.v1.1.0.md`'s
*content* still has not been through this project's normal review process, precisely because the session
that produced it skipped the steps that would have caught problems (the way the review pass on
`spec.v1.1.0.md` caught 7 real issues). Treat it as unreviewed input, not as settled fact, until it gets
the same review treatment. See "Next step" below.

## Where the project stands

- The project is transitioning from branch `V1.0.0` to `V1.1.0` (currently on `V1.1.0`, 8 commits ahead
  of `origin/V1.1.0`, not yet pushed). This session's confirmed work was spec-stage only for v1.1.0 — no
  `developer-lite` implementation work happened, and the user had not given explicit go-ahead to commit
  anything before the anomaly above was discovered.
- **Convention established this session**: version-suffixed spec files. `docs/spec.md` is now frozen as
  the permanent v1.0.0 baseline and never appended to again. New v1.1.0 content lives in
  `docs/spec.v1.1.0.md` (written, reviewed, and fixed this session via `designer-lite` — see below).
  Future version bumps repeat the pattern (`spec.v1.2.0.md`, etc.), extending to
  `docs/detailed-spec.v<version>.md` and version-labeled `docs/plans/<feature>/` content.
- `docs/spec.v1.1.0.md`: complete, reliable. 5 features (§1-§5), no open questions. A full
  consistency-review pass (`designer-lite`, report-only) ran and every finding was fixed in a follow-up
  pass — this file's content was fully vetted through this conversation's actual process.
- `docs/detailed-spec.v1.1.0.md`: **exists with substantial content (§0-§6) but was NOT produced through
  this conversation.** A structural skim shows it's plausible/well-formed — its section list matches
  `spec.v1.1.0.md`'s 5 features 1:1, it independently arrives at the same combined-schemaVersion-bump
  decision this session made explicitly, and it follows `detailed-spec.md`'s existing citation/style
  conventions closely. But "plausible on a skim" is not the same as reviewed — no consistency-review
  pass (the kind that already caught 7 real issues in `spec.v1.1.0.md`) has been run against it yet.

## The 5 confirmed v1.1.0 features (`docs/spec.v1.1.0.md` — this part is solid, unaffected by the anomaly)

1. **§1 — OS-Folder → Tree-Pane Drag-and-Drop Import**: drag `.md` files from Explorer onto an open
   tab's tree pane. Copy-only, `.md`-only (others silently skipped), in-app Rename/Overwrite/Skip prompt
   on filename collision. Skip-summary toast fires only when something was actually skipped.
2. **§2 — Per-Tab Last-Viewed-Memo and Scroll-Position Memory**: extends startup-only memo restore to
   every in-session tab switch. New per-tab `lastViewedMemo`; `AppSettings.lastOpened` narrowed to
   `{tabId}`. Also restores tree-pane scroll/selection (no new field needed for that part).
3. **§3 — Cross-Tab Quick Memo-Name Search ("Quick Open")**: `Ctrl+Shift+P`, substring match on memo
   names across tabs, reuses the existing full-text index, MRU-ordered via §2's `lastViewedMemo`.
4. **§4 — Cross-Tab Favorites**: new `favorite?: boolean` on `MetaTreeNode`, reuses Quick Open's index/
   ordering/Tab-Lock-exclusion/jump-to-result mechanics.
5. **§5 — Tab-Lock Auto-Relock on Inactivity**: single global inactivity timer (at most one session can
   be alive at a time), reuses existing save-flush + session-teardown logic, pauses while any modal
   dialog is open, configurable via Settings → 一般 (dropdown, 5/15/30/60 min + 無効, default 15).

**Dropped along the way**: an earlier "Cross-Tab Memo Clipboard (Copy/Cut/Paste)" feature was fully
designed then deleted after the user clarified the original request was already covered by existing
features (cross-tab DND for files, existing editor copy/paste for text). Lesson captured in
`feedback_confirm_interpretation_before_elaborating.md` (auto-memory).

## Cross-cutting decisions worth knowing

- **schemaVersion bumps are consolidated, not per-field**: one `TabMeta.schemaVersion` bump covers both
  `lastViewedMemo` (§2) and `favorite` (§4); one `AppSettings.schemaVersion` bump covers both the
  `lastOpened` narrowing (§2) and `tabLockAutoRelockMinutes` (§5) — not independent increments, since
  v1.1.0 ships as one unit and no real intermediate file state will exist.
- **`[spec §x]` citation discipline**: this notation means "cites `docs/spec.md` specifically." The
  review pass on `spec.v1.1.0.md` found and fixed two cases where `detailed-spec.md`-sourced content was
  mis-tagged as `[spec §x]`. `detailed-spec.v1.1.0.md` (per its own header, read this session) uses a
  three-way convention: `[spec §x]` / `[spec v1.1.0 §x]` / plain `` detailed-spec.md §x `` — worth
  verifying this distinction was actually applied correctly throughout, given the file's unknown
  provenance.

## Agent/config changes made this session (project + global) — these are solid, independent of the anomaly

- Project-level `.claude/agents/{designer,developer,reviewer}-lite.md`: document the version-suffixed
  convention in their "must check" steps.
- This project's `CLAUDE.md`: pointer sentence in "Documentation for This Repository" about future
  version bumps, citing `docs/README.md`.
- `docs/README.md`: File Structure list documents `spec.v1.1.0.md` and the convention.
- Global `~/.claude/agents/{designer,developer,reviewer}-lite.md`: each now reads
  `~/.claude/README-lite-workflow.md` as a required check (affects other projects too).
- Global `~/.claude/README-lite-workflow.md`: new "Version-Suffixed Specification Files (Optional
  Convention)" section — generic, explicitly optional, tells adopting projects to mirror it into their
  own `docs/README.md` and `CLAUDE.md`.
- Saved as a standing lesson: `feedback_defense_in_depth_for_conventions.md` (document new cross-cutting
  conventions at every layer, not just one — this was learned *because* single-layer documentation
  wasn't reliably picked up earlier in this same session).

## Uncommitted state (as of this handoff)

```
?? .claude/files/
?? "CLAUDE - コピー.md"
```

Both are **pre-existing, unrelated to this session's work** (present before this session started).

**Everything else this session touched is already committed** (see the anomaly note at the top) — 8
commits, branch `V1.1.0` is 8 commits ahead of `origin/V1.1.0`, **not pushed**. This is a marked change
from this project's usual pattern (per this file's previous version: `developer-lite`/`designer-lite`
leave changes uncommitted for the user to review and commit explicitly). Do not push without explicit
user instruction, and do not assume the commit history is final/correct without the user confirming what
happened.

## Next step

1. **Immediate**: run the same kind of report-only consistency-review pass used for `spec.v1.1.0.md`
   earlier this session (citation accuracy, cross-references to `spec.v1.1.0.md` sections, schemaVersion
   consolidation consistency, no stray content) on `docs/detailed-spec.v1.1.0.md` before treating it as
   reliable input for `PLAN.md` creation.
2. **After detailed-spec is confirmed reliable**: per the user's original instruction, new
   `docs/plans/<feature>/PLAN.md` content for v1.1.0 features should stay separate from any *existing*
   plan rather than being appended — but the exact separation format for the plans layer was
   deliberately left undecided (confirm with the user/`designer-lite` the first time an existing plan
   needs a version-specific change). None of the 5 v1.1.0 features currently modify an existing plan, so
   this may not come up yet.
3. Confirm whether to push the 8 local commits to `origin/V1.1.0`, or hold.
