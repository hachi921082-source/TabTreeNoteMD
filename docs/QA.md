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

---

_(Past questions and their resolutions are available via this file's git history and are permanently
recorded in `DECISIONS.md`.)_
