# Open Questions for the User (QA.md)

> Questions raised by `designer-lite` (either while writing `detailed-spec.md`, or when answering
> `developer-lite`'s questions during implementation planning) that require the user's own judgment
> rather than a provisional decision. The user reviews this file and provides answers; answers are
> then reflected back into `spec.md` (and `DECISIONS.md` for rationale) through a normal
> user/`designer-lite` dialogue, not by `developer-lite` or `designer-lite` deciding unilaterally.
>
> **Convention for resolved items**: once the user answers a question here and the answer has been
> reflected into `spec.md` (confirmed statement) and `DECISIONS.md` (rationale + rejected
> alternatives), the item is **removed from this file** rather than marked "resolved" in place — this
> file's purpose is the list of *currently open* questions, so keeping it limited to that keeps it easy
> to scan. History of past questions and their resolutions is available via git history on this file,
> and the resolution itself lives permanently in `DECISIONS.md`.

---

## Table of Contents: Absolute vs. Relative Heading Nesting

**Raised by**: `reviewer-lite`, during review of the `table-of-contents` implementation
(`docs/plans/table-of-contents/PLAN.md`).

**Context**: `src/editor/tocGenerator.ts`'s `generateTocList` indents each heading by its absolute level
(`"  ".repeat(h.level - 1)`) — an `H2` is always nested one level under wherever the nearest preceding
`H1` would be, even if no `H1` exists in the document. `reviewer-lite` confirmed hands-on: in a document
that starts at `H2` (no `H1` at all), every generated TOC entry is indented one level, so the list has no
top-level (unindented) item at all — a visually "floating" TOC. `docs/plans/table-of-contents/PLAN.md`'s
own wording ("a `##` heading nests one level under the preceding `#` heading") is ambiguous between this
absolute-level reading and the more common GitHub-style convention of treating the document's *shallowest*
heading level as the flat top level regardless of its actual level number — so this isn't a spec
violation, just an underspecified case the plan didn't anticipate.

**Question**: For documents whose first/shallowest heading isn't `H1`, should the TOC's top level be (a)
kept as absolute heading level, as currently implemented (matches a literal reading of the plan, but
produces an all-indented list with no flat top item), or (b) relative to the document's shallowest actual
heading level (shallowest heading becomes the flat top level, GitHub-style)?

**Needed**: A decision to record in `docs/plans/table-of-contents/PLAN.md` and `DECISIONS.md`. If
"relative," a small follow-up fix to `generateTocList`. **Non-blocking**: no other implementation work
depends on this being resolved.

## Edit Lock: Live Enforcement for an Already-Open Memo

**Raised by**: `reviewer-lite`, during review of the editor-save implementation
(`docs/plans/editor-save/PLAN.md`), escalated via `designer-lite` per that plan's own "Open Items"
clause ("if `developer-lite` finds during implementation that this path *is* reachable in a way that
matters to the user... it should be escalated to `designer-lite`... rather than assumed").

**Context**: `editor_load` returns a one-time snapshot `edit_locked` flag (`docs/plans/editor-save/PLAN.md`
§2's `MemoContent` struct), and the frontend's `EditorPane` derives its read-only state from that
snapshot taken once at load time. `reviewer-lite` confirmed: if a memo's Edit Lock is toggled on while
that memo is the currently-open one in the editor, the editor does not become read-only immediately — a
user can keep typing, and Ctrl+S then surfaces a generic "unexpected error" toast (the backend does
correctly refuse the write, per `editor_save` step 1, so there is no data-loss/corruption risk — this is
a UX/live-update gap, not a safety gap).

Note on current reachability: there is presently no shipped UI control anywhere in the app that lets a
real user toggle Edit Lock at all — the backend command (`memo_set_edit_lock`) and a frontend hook
(`setEditLock` in `state/useTree.ts`) exist, but no context-menu item calls it (`docs/plans/memo-tree/PLAN.md`'s
confirmed right-click menu list — Rename/Delete/Create Memo/Create as Child/Icon/Properties — does not
include an Edit Lock toggle entry, and that plan's own "Open Items" says "None"). `reviewer-lite`
demonstrated this scenario via a direct backend call, not a UI path a real user can hit today. This will
most likely become reachable once some future work adds an actual Edit-Lock-toggle UI control.

**Question**: When Edit Lock changes while the affected memo is open in the editor, should the editor (a)
listen live (event push or poll) for the change and immediately flip to read-only, with a specific "this
memo is now locked" save-failure message instead of the generic error — or (b) is snapshot-at-load
acceptable to leave as-is for now, deferring a real fix until whatever future work actually adds a
UI control for toggling Edit Lock (at which point this question would naturally be revisited alongside
that control's own design)?

**Needed**: A decision on live-enforcement scope, to record in `DECISIONS.md`. If "add live enforcement
now," a concrete task for `developer-lite`. If "defer," a note of where this should be picked up again
(most likely: whatever plan eventually adds the Edit-Lock-toggle UI control itself, which does not yet
exist anywhere in the confirmed spec/plans). **Non-blocking**: no other implementation work depends on
this being resolved.

## In-Memo Replace "Selection" Mode Missing from Implementation

**Raised by**: `reviewer-lite`, during review of the editor-save implementation
(`docs/plans/editor-save/PLAN.md`).

**Context**: `spec.md` §2.4 lists in-memo replace modes as "single / all / selection / one-by-one"
(`[DESIGN §2.4]`, carried over from the legacy repository). The implemented
`components/Search/SearchPanel.tsx` / `state/useSearch.ts` only support replace-one and replace-all —
there is no "constrain replace to the current selection" mode. Unlike other implementation-discretion
resolutions in this plan, which are written up explicitly in `docs/plans/editor-save/PLAN.md`'s
"Implementation Notes," this gap is not documented anywhere, so it currently reads as an overlooked
scope item rather than a disclosed decision.

**Question**: Was "selection" replace mode intentionally dropped from v1 scope (and if so, why — worth
recording so it is not accidentally "rediscovered" as a bug later), or does it need to be added back as
a follow-up implementation task for `developer-lite`?

**Needed**: A decision so `spec.md` §2.4 can be corrected to match (either drop "selection" from the
listed modes, with rationale recorded in `DECISIONS.md`, or keep it and add a follow-up implementation
task). **Non-blocking**: no other implementation work depends on this being resolved.

---

_(Past questions and their resolutions are available via this file's git history and are permanently
recorded in `DECISIONS.md`.)_
