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

## Drag-Resize-Handle for Rendered Images in Live View

**Raised by**: `developer-lite`, mid-implementation of `image-handling/PLAN.md` Phase 2 (backend
command `image_save_pasted` is done; the frontend paste/toolbar-insert flow that writes
`![alt](img_xxxx.png)` Markdown text is straightforward and not blocked by this question). The
companion question about real `<img>` rendering scope/mechanism (asset-protocol vs. byte-read IPC
command) that was originally bundled with this one has since been resolved and shipped — see
`DECISIONS.md`'s "Real `<img>` Rendering via Byte-Read IPC Command + Blob-URL Cache" entry,
`docs/MDEditor-spec.md` §5, and `docs/plans/markdown/image-handling/PLAN.md`.

**Context**: `docs/plans/image-handling/PLAN.md` §4 attributes "the single image's markup converts to
`<img src=".." width=".." height="..">`" to a **drag handle on the rendered image in live view**, but
neither `image-handling/PLAN.md` nor `markdown-editing/PLAN.md` contains a widget markup spec, drag
mechanics, handle position/size, aspect-ratio-lock behavior, or a minimum-size rule for this
interaction — it remains undesigned even though the underlying real `<img>` rendering it would attach
to is now implemented.

**Question**: Is building the drag-resize-handle interaction in scope for `developer-lite` to design
and implement now, or should it be tracked as an explicit follow-up in both plans' "Open Items" (no
regression versus current behavior, which has no resize handle)? If in scope now, the actual
resize-handle UX needs deciding: drag mechanics, handle placement, aspect-ratio lock, minimum size.

**Needed**: A decision on scope timing (now vs. follow-up) and, if now, on the resize-handle UX, to
record in `DECISIONS.md` and reflect into `image-handling/PLAN.md` / `markdown-editing/PLAN.md`.
**Non-blocking**: `developer-lite` can complete and ship the rest of `image-handling/PLAN.md`'s
clearly-specified parts without this being resolved first; images render without a resize handle in
the meantime.

---

_(Past questions and their resolutions are available via this file's git history and are permanently
recorded in `DECISIONS.md`.)_
