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

## Real `<img>` Rendering + Drag-Resize-Handle in Live View: Scope and Mechanism

**Raised by**: `developer-lite`, mid-implementation of `image-handling/PLAN.md` Phase 2 (backend
command `image_save_pasted` is done; the frontend paste/toolbar-insert flow that writes
`![alt](img_xxxx.png)` Markdown text is straightforward and not blocked by this question).

**Context**: `docs/plans/image-handling/PLAN.md` §4 attributes "the single image's markup converts to
`<img src=".." width=".." height="..">`" to a **drag handle on the rendered image in live view**, and
its own header line assigns "the decoration layer that renders `<img>`/`![alt](url)` and provides the
resize handle" to `markdown-editing/PLAN.md`. But `markdown-editing/PLAN.md`'s own status line says the
opposite: its current `ImagePlaceholderWidget` (`src/editor/markdownDecorations.ts`) is an explicit
`<span>` stub (🖼 emoji + alt/src text, no real image bytes, no resize handle) that "becomes a real
rendered `<img>` once `image-handling/PLAN.md` lands." Each plan defers the concrete design to the
other — confirmed by reading both in full: neither contains a widget markup spec, drag mechanics,
handle position/size, aspect-ratio-lock behavior, or a minimum-size rule.

Beyond the missing UX spec, actually rendering a real `<img>` pointing at a file in an arbitrary
user-chosen tab folder (folders are picked via folder-picker at runtime, not fixed app-data paths)
needs infrastructure not mentioned in any plan. Confirmed by inspection: `src-tauri/tauri.conf.json`
currently has `"csp": null` and no `app.security.assetProtocol` scope configured at all; the only image
command listed in `project-architecture/PLAN.md`'s command inventory (§4) is `image_save_pasted` (write
path only, no read/serve command exists). Two candidate mechanisms exist, each a real
architecture/security tradeoff not addressed anywhere in `spec.md`/`detailed-spec.md`/`DECISIONS.md`:
(1) Tauri's asset protocol, which would need an `assetProtocol` scope config that somehow permits
arbitrary runtime-chosen directories (not a static glob known at build time) — this widens what the
webview can directly fetch off disk; or (2) a new IPC command that reads the image file and returns
bytes/base64 for the frontend to build a `data:` URL — avoids widening the asset-protocol scope, at the
cost of extra IPC overhead/memory for larger images, and would add a new command not currently in
`project-architecture/PLAN.md`'s inventory table.

**Question**: (a) Is building real `<img>` rendering + a drag-resize-handle interaction in scope for
`developer-lite` to design and implement now, as part of closing out `image-handling/PLAN.md` /
`markdown-editing/PLAN.md`'s cross-reference — or should the existing `ImagePlaceholderWidget` stub be
left as-is (no regression versus current behavior) and this tracked as an explicit follow-up in both
plans' "Open Items"? (b) If in scope now, which rendering mechanism should be used — Tauri asset-protocol
scope config (needs a decision on how to scope it for arbitrary runtime-chosen folders) or a new
base64/bytes-returning IPC command (needs a decision on the new command's name/shape and where perf
limits, if any, apply)? Whichever is chosen also needs the actual resize-handle UX decided (drag
mechanics, handle placement, aspect-ratio lock, minimum size).

**Needed**: A decision on scope timing (now vs. follow-up) and, if now, on the rendering mechanism, to
record in `DECISIONS.md` and reflect into `image-handling/PLAN.md` / `markdown-editing/PLAN.md`.
**Non-blocking**: `developer-lite` can complete and ship the rest of `image-handling/PLAN.md`'s
clearly-specified parts (backend command, paste/toolbar-insert flow) without this being resolved first;
the placeholder widget stays a non-regressing stub in the meantime.

---

_(Past questions and their resolutions are available via this file's git history and are permanently
recorded in `DECISIONS.md`.)_
