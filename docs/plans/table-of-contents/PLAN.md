# PLAN: Table of Contents (TOC)

> Status: planning complete, not yet implemented. Depends on `project-architecture/PLAN.md`. Triggered
> from `markdown-editing/PLAN.md` §4's toolbar (row 1, slot 8, "Update"/`RefreshCw`), but specified here
> as its own plan because it is referenced from multiple other plans: `markdown-editing/PLAN.md`
> (decoration rendering of `#slug` links, `tocGenerator.ts` module placement), `link/PLAN.md` §1/§5
> (`#slug` click-resolution shares this file's slugification rule), and `tab-lock/PLAN.md` (operates
> identically on a decrypted in-session memo buffer, no special-casing needed).

Sources: `docs/spec.md` §2.8, §4.2; `docs/detailed-spec.md` §4.6, §5.8, §5.10; `docs/DECISIONS.md` "New
Feature: Table of Contents (TOC)", "Code-Fence Awareness in Markdown Scanning".

---

## 1. Scope

New v1 feature, triggered only by `markdown-editing/PLAN.md`'s toolbar Update slot — manual, never
automatic/background/live, no heading-change watcher of any kind. Implemented in `tocGenerator.ts`
(`markdown-editing/PLAN.md` §2's module layout), invoked as a single CodeMirror transaction (one undo
step, same treatment as other toolbar actions, `[detailed-spec §4.3]`).

## 2. Generation Steps

- **Heading scan**: on click, scan the buffer for Markdown headings (`#` through `######`), collecting
  level + text, in document order. **Must reuse the same Markdown-aware parse (respecting
  fenced-code-block boundaries) that `markdownDecorations.ts` already builds for rendering, not an
  independent line-by-line `#`-regex** (`DECISIONS.md` "Code-Fence Awareness in Markdown Scanning") —
  a `#` inside a fenced code block is not a heading under CommonMark and must not produce a TOC entry.
- **Slugification**: GitHub-style (lowercase, spaces → hyphens, strip non-word punctuation). Duplicate
  slugs in the same document: first occurrence keeps the bare slug, later ones get `-1`, `-2`, ... —
  this exact rule is shared with `#slug` click resolution (§4 below) so generation and resolution never
  disagree.
- **Block generation**: nested Markdown list of `[heading text](#slug)`, nesting depth follows heading
  level.
- **Marker delimiters**: HTML comments, e.g. `<!-- TOC start -->` / `<!-- TOC end -->` (exact text is
  this plan's call — any text is fine as long as it renders inert in live view, which HTML comments
  already do under CommonMark with no new sanitization-allowlist entry needed).
- **Insert vs. replace**: scan the document for an existing marker pair.
  - Found → replace the content between the markers with the freshly generated list.
  - Not found → insert a new marker-delimited block at the cursor.

## 3. Persistence and Scope

- **Persistence**: none beyond the memo body itself — the TOC block is ordinary saved Markdown text, no
  metadata field.
- **Scope**: self-contained to the current memo; no cross-memo/cross-tab behavior. Operates purely on
  the already-loaded CodeMirror buffer, so it composes with `tab-lock/PLAN.md`'s decrypted-in-session
  memos without any special-casing.

## 4. `#slug` Link Rendering and Resolution

`#slug` link **rendering** (making the generated links clickable in the decoration layer) is
`markdownDecorations.ts`'s job (`markdown-editing/PLAN.md` §2), same as any other Markdown link. Click
**resolution** and the scroll-to-heading mechanism are specified in `link/PLAN.md` §5 (re-scans the
current memo's headings fresh at click time using this file's §2 heading-scan/slugification rules, not
cached from the last "Update" run — a link still resolves correctly even if headings changed since the
last "Update" click). No match is a silent no-op.

## Open Items

None. Engine choice, sanitization, and the feature itself are all confirmed in `spec.md`/`DECISIONS.md`;
this plan only operationalizes them.
