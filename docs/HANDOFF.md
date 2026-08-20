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
> stale before it's even read. The prior version of this file described the initial
> `detailed-spec.v1.2.0.md` creation session — that work has since been through two additional review
> passes (see §2) and this file has been rewritten to reflect the current, fully-reviewed state. Don't
> assume a fresh HANDOFF.md is safe to skim past its own verification instruction.

## Index

1. Where the project stands
2. `docs/detailed-spec.v1.2.0.md` — created, reviewed twice, committed and pushed
3. Separate track: GitHub issue #190 (TOC/color-tag bug) — filed only, not started
4. Uncommitted state
5. Next step
6. Branch policy for v1.2.0 (carried forward, unchanged)

## 1. Where the project stands

- v1.1.0 is fully complete and merged into `V1.1.0` (unchanged since the prior HANDOFF.md).
- Current branch: **`V1.2.0`**, up to date with `origin/V1.2.0`. `docs/spec.v1.2.0.md` (9 items, §1–§9,
  fully decided) and `docs/detailed-spec.v1.2.0.md` (full implementation-planning-level elaboration of
  those 9 items) both exist, are committed, and have now been through **two separate review passes** on
  top of the initial `designer-lite` draft (see §2 for what each pass covered and found). **No
  `developer-lite` or `reviewer-lite` work has started** — this has all been spec/design-layer work.

## 2. `docs/detailed-spec.v1.2.0.md` — created, reviewed twice, committed and pushed

All doc changes are committed to `V1.2.0` and pushed to `origin/V1.2.0` (one file per commit, per
`CLAUDE.md`'s convention). Three passes so far, each with a different focus — read this section before
assuming another blanket review pass is still needed; two of the three obvious angles are already covered:

1. **Initial draft** (`designer-lite`): elaborated `spec.v1.2.0.md`'s 9 items to implementation-planning
   depth, grounded in direct `src/`/`src-tauri/` inspection. Also resolved `docs/QA.md`'s `Q-v1.2.0-1`
   (a `memo:`-branch link-encoding bug found while elaborating §4/§9, bundled into §9's fix scope per
   user decision — rationale in `DECISIONS.md`).
2. **Accuracy/structure self-check** (same `designer-lite` instance, own output): re-verified file/
   function/line citations against real source, Index-vs-heading consistency, citation-convention
   compliance. Found and fixed 4 issues: 1 factual error (wrong prop name in a §7 code example), 1
   garbled sentence (§1.2), 1 under-verified/hedged claim tightened with confirmed specifics (§2.1
   KaTeX×DOMPurify), 1 completeness gap noted (§5.2's interaction with issue #28's cursor-position
   persistence).
3. **Unspecified-UI / spec-omission / contradiction check** (explicitly requested by the user as a
   distinct pass, since pass 2 above didn't target these three dimensions): found **no** remaining
   unspecified UI needing `QA.md` escalation, but found and fixed two real spec omissions and one real
   internal contradiction:
   - `[spec v1.2.0 §2.5]`'s wide-formula horizontal-scroll requirement was never elaborated (only asset
     bundling was) — fixed.
   - `[spec v1.2.0 §4.4]` (Tab Lock / unresolved-anchor cases) had **zero** elaboration anywhere — its
     section number had been silently repurposed for the bundled `memo:`-branch bug fix during pass 1's
     QA resolution, displacing the original topic entirely. Fixed by adding a new §4.5 (kept the §4.4
     numbering as-is rather than renumbering, to avoid breaking existing cross-references from §9/§12/
     `DECISIONS.md`/`spec.v1.2.0.md` itself) — confirmed via direct Rust (`link.rs`) and TS (`links.ts`)
     inspection that all three of spec's requirements are satisfied automatically by already-existing,
     unmodified code, with a test-coverage recommendation flagged for `developer-lite` since "no new code
     needed" cases are easy to silently regress later.
   - §4.3's explanation of why the anchor-jump doesn't need its own `decodeURIComponent` call was stale
     (written before the §9.2 `memo:`-branch bundling, only accounted for one of the two entry paths) —
     fixed to correctly trace both paths.
   - **User verified this pass's two omission fixes directly** (read the actual spec/detailed-spec text
     side by side, not just trusting the agent's self-report) and confirmed both are genuinely resolved,
     not superficial patches.

**Commits on `V1.2.0`, in order** (all pushed): `docs/DECISIONS.md` → `docs/spec.v1.2.0.md` →
`docs/detailed-spec.v1.2.0.md` (new file, pass 1+2 combined) → `docs/README.md` → `docs/HANDOFF.md` (the
prior version of this file) → `docs/detailed-spec.v1.2.0.md` (pass 3's fixes, latest commit `19fe524`).

`docs/QA.md` is back to its pristine "no open questions" state.

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
  and a suggested fix are in the issue body — not repeated here.
- **User's explicit instruction**: "issueに起票しておいて。後で進めます" (file the issue, will proceed
  later) — **do not start a `fix/` branch or any implementation for this on your own initiative.** Wait
  for the user to say to proceed.

## 4. Uncommitted state

```
?? .claude/files/
?? "CLAUDE - コピー.md"
```

Both pre-existing and unrelated to this session — carried forward unverified from earlier sessions'
notes (every prior HANDOFF.md said the same thing). Still nobody has had a reason to touch either; still
worth a fresh check by whichever session first does.

## 5. Next step

Per `.claude/agents/README.md`'s step 3, the next stage is `developer-lite` reading `spec.v1.2.0.md` +
`detailed-spec.v1.2.0.md` and writing independent `docs/plans/<feature-name>/PLAN.md` files for each of
the nine v1.2.0 items (or however `developer-lite`/the user chooses to group them — nothing decided a
specific feature-branch/PLAN-file grouping for v1.2.0 yet). **This has not been started at all.** Given
this document now sits at three review passes deep with two of them finding real (if not huge) issues
each time, a fourth blanket "review it again" pass has diminishing returns — if the user wants more
scrutiny before `developer-lite` starts, a more targeted angle (not yet tried: an actual `tauri dev` +
CDP live run of a representative subset, similar to how `table-of-contents/PLAN.md` and other past
features were verified) would likely surface different issues than another desk-review pass. Ask the
user before starting any of this, same as every other stage transition this project's workflow uses.

## 6. Branch policy for v1.2.0 (carried forward, unchanged)

**`V1.2.0` development does NOT merge back into `V1.1.0`.** Per the user (stated in an earlier session):
"v1.1.0とは別にv1.2.0として開発を継続するのでv1.1.0へマージはしない". Whatever branch strategy v1.2.0
features end up using (feature branches off `V1.2.0`, or direct commits to `V1.2.0`), the merge target is
not `V1.1.0`. If a feature-branch-per-item flow is adopted for v1.2.0 (mirroring v1.1.0's
`feature/v1.1.0-<name>` pattern), confirm the actual merge target with the user before assuming it's
`V1.2.0` by default — this was stated once, in passing, not exhaustively specified.
