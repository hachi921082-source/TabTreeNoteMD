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
> stale before it's even read. The prior version of this file described the v1.2.0 spec-drafting session
> (branch `V1.2.0`, `spec.v1.2.0.md` fully decided but uncommitted) — that work is now committed and
> pushed (see §2), and this file has been entirely rewritten for the `detailed-spec.v1.2.0.md` session
> that followed it. Don't assume a fresh HANDOFF.md is safe to skim past its own verification instruction.

## Index

1. Where the project stands
2. `docs/detailed-spec.v1.2.0.md` — created, self-checked, committed and pushed
3. Separate track: GitHub issue #190 (TOC/color-tag bug) — filed only, not started
4. Uncommitted state
5. Next step
6. Branch policy for v1.2.0 (carried forward from the prior HANDOFF.md, unchanged)

## 1. Where the project stands

- v1.1.0 is fully complete and merged into `V1.1.0` (unchanged since the prior HANDOFF.md).
- Current branch: **`V1.2.0`**, up to date with `origin/V1.2.0`. This session's work: (a) created
  `docs/detailed-spec.v1.2.0.md` via `designer-lite`, elaborating all nine items (§1–§9) of the
  already-decided `docs/spec.v1.2.0.md`; (b) `designer-lite` ran an independent self-check pass on its
  own output and fixed 4 real issues (1 factual error, 3 clarity/completeness gaps — see git log on
  `detailed-spec.v1.2.0.md` for detail, not repeated here); (c) a second-opinion review (external, not an
  in-repo agent) flagged one genuinely actionable point on §6.2 (preview zoom), which was folded in as a
  note that the 180%-zoom table/image visual check is a **manual, user-performed check**, not something
  `reviewer-lite` should adjudicate on its own (visual/subjective judgment call); (d) a stale line in
  `spec.v1.2.0.md`'s own header (still describing `detailed-spec.v1.2.0.md` as "a future file, not
  created by this pass") was corrected. **No `developer-lite` or `reviewer-lite` work has started** —
  same as the prior session, this one was still entirely spec/design-layer work.

## 2. `docs/detailed-spec.v1.2.0.md` — created, self-checked, committed and pushed

All of this session's doc changes are committed to `V1.2.0` and pushed to `origin/V1.2.0` (4 commits,
one file each, per `CLAUDE.md`'s commit convention — nothing left uncommitted from this pass):

- `docs/DECISIONS.md` — new entry recording the `Q-v1.2.0-1` resolution (bundling the `memo:`-branch
  link-encoding fix into §9's existing scope rather than deferring it).
- `docs/spec.v1.2.0.md` — §9 extended to cover both the `#slug` and `memo:` branches; Status/Scope/Index/
  §4.3 updated to match; stale header line fixed.
- `docs/detailed-spec.v1.2.0.md` (new file) — the full nine-item elaboration, self-checked.
- `docs/README.md` — added the missing File Structure entries for `spec.v1.2.0.md` and
  `detailed-spec.v1.2.0.md` (both already existed but were undocumented there).

`docs/QA.md` is back to its pristine "no open questions" state — `Q-v1.2.0-1` was resolved and removed
per the file's own convention (resolution lives in `DECISIONS.md`, not there).

## 3. Separate track: GitHub issue #190 (TOC/color-tag bug) — filed only, not started

Unrelated to the nine v1.2.0 items — surfaced during this session from an off-topic user question, not
part of `spec.v1.2.0.md`. **Not part of the v1.2.0 feature set**; do not fold it into any v1.2.0
`PLAN.md` unless the user says otherwise.

- **Bug**: a heading containing the app's own color/font-size `<span style="...">`/`<mark>` tag produces
  a garbled slug in `src/editor/tocGenerator.ts`'s `scanHeadings()` (the raw tag markup isn't stripped
  before slugification), corrupting both the "Update TOC" toolbar command's generated link and possibly
  (unverified — theory only, not live-tested) breaking the live-view click handler due to an
  unprecedented decoration-overlap between the color-span loop and a `Link` node's label range in
  `src/editor/markdownDecorations.ts`.
- **Filed**: https://github.com/hachi921082-source/TabTreeNoteMD/issues/190 — full root-cause writeup
  (confirmed via a throwaway `tsx` script exercising the real `scanHeadings`/`generateTocList` against an
  `EditorState`) and a suggested fix (reuse `decorationCommands.ts`'s existing `unwrapColorSpans()`-style
  stripping in `scanHeadings()`'s text extraction) are in the issue body — not repeated here.
- **User's explicit instruction**: "issueに起票しておいて。後で進めます" (file the issue, will proceed
  later) — **do not start a `fix/` branch or any implementation for this on your own initiative.** Wait
  for the user to say to proceed.

## 4. Uncommitted state

```
?? .claude/files/
?? "CLAUDE - コピー.md"
```

Both pre-existing and unrelated to this session — carried forward unverified from earlier sessions'
notes (the prior HANDOFF.md said the same thing). Still nobody has had a reason to touch either; still
worth a fresh check by whichever session first does.

## 5. Next step

Per `.claude/agents/README.md`'s step 3, the next stage is `developer-lite` reading `spec.v1.2.0.md` +
`detailed-spec.v1.2.0.md` and writing independent `docs/plans/<feature-name>/PLAN.md` files for each of
the nine v1.2.0 items (or however `developer-lite`/the user chooses to group them — nothing in this
session decided a specific feature-branch/PLAN-file grouping for v1.2.0 yet). **This has not been started
at all.** Ask the user before starting it, same as every other stage transition this project's workflow
uses.

## 6. Branch policy for v1.2.0 (carried forward from the prior HANDOFF.md, unchanged)

**`V1.2.0` development does NOT merge back into `V1.1.0`.** Per the user (stated in an earlier session):
"v1.1.0とは別にv1.2.0として開発を継続するのでv1.1.0へマージはしない". Whatever branch strategy v1.2.0
features end up using (feature branches off `V1.2.0`, or direct commits to `V1.2.0`), the merge target is
not `V1.1.0`. If a feature-branch-per-item flow is adopted for v1.2.0 (mirroring v1.1.0's
`feature/v1.1.0-<name>` pattern), confirm the actual merge target with the user before assuming it's
`V1.2.0` by default — this was stated once, in passing, not exhaustively specified.
