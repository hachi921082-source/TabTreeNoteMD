# PLAN: Table of Contents (TOC)

> Status: implemented (`src/editor/tocGenerator.ts`; `src/editor/links.ts`'s
> `resolveAndScrollToHeadingAnchor`; wired into `src/components/Editor/Toolbar/MarkdownToolbar.tsx`'s
> "Update" slot, replacing `decorationCommands.ts`'s former `tocUpdateStub`, which was removed). Depends
> on `project-architecture/PLAN.md`. Triggered from `markdown-editing/PLAN.md` §4's toolbar (row 1, slot
> 8, "Update"/`RefreshCw`), but specified here as its own plan because it is referenced from multiple
> other plans: `markdown-editing/PLAN.md` (decoration rendering of `#slug` links, `tocGenerator.ts`
> module placement), `link/PLAN.md` §1/§5 (`#slug` click-resolution shares this file's slugification
> rule), and `tab-lock/PLAN.md` (operates identically on a decrypted in-session memo buffer, no
> special-casing needed).

Sources: `docs/spec.md` §2.8, §4.2; `docs/detailed-spec.md` §4.6, §5.8, §5.10; `docs/DECISIONS.md` "New
Feature: Table of Contents (TOC)", "Code-Fence Awareness in Markdown Scanning".

## Implementation Notes (resolutions of this plan's discretion points)

- **Heading scan reuses the shared syntax tree**: `scanHeadings(state)` calls `syntaxTree(state)`
  (`@codemirror/language`) directly — the same CommonMark+GFM tree `markdownDecorations.ts` builds from
  `codeMirrorSetup.ts`'s always-on language extension — and matches `ATXHeading[1-6]` node names. A `#`
  inside a fenced code block is tokenized as plain code text, never one of these node types, so it's
  excluded with no special-casing, satisfying `DECISIONS.md` "Code-Fence Awareness in Markdown Scanning"
  by construction rather than by a parallel fence-detection pass.
- **Slugification** (`slugifyHeading`): lowercase, Unicode-aware punctuation stripping (`\p{L}`/`\p{N}`,
  so Japanese/other non-Latin heading text slugifies sensibly instead of collapsing to nothing), spaces
  to hyphens. One addition beyond the plan's literal wording: a heading with no word/number characters
  at all (e.g. a heading that's only punctuation) would otherwise slugify to an empty string, which
  isn't a usable `#` anchor — falls back to the fixed placeholder `"section"` in that case. Duplicate
  slugs get `-1`, `-2`, ... suffixes as specified; both `tocUpdateCommand` (generation) and
  `resolveAndScrollToHeadingAnchor` (resolution) call this same `scanHeadings`/`slugifyHeading` pair, so
  the two can never disagree.
- **Nesting depth is relative to the document's own shallowest heading level** (`generateTocList`),
  GitHub-style: a document whose shallowest heading is `##` gets that heading flat at the TOC's top
  level, not indented one level as if a missing `#` preceded it. A document that already starts at `#`
  is unaffected. Resolved via `docs/QA.md` → `DECISIONS.md` "Table of Contents: Relative Heading-Nesting
  Indentation" (GitHub issue #84); originally shipped as absolute-level nesting, since revised.
- **Marker text**: `<!-- TOC start -->` / `<!-- TOC end -->`, exactly as the plan's own example — no
  reason to deviate.
- **Bug found and fixed during self-verification**: the first implementation inserted the new
  marker-delimited block directly at the cursor position with no surrounding-newline guard. When the
  cursor sat at a position not already bounded by a line break (e.g. position 0 in a document that
  starts with a heading), the closing `<!-- TOC end -->` marker glued onto the same line as the
  following text (`<!-- TOC end --># Title`), which silently broke that line's own Markdown syntax (the
  heading was no longer parsed as a heading at all — confirmed by a throwaway `tsx` smoke script
  exercising `scanHeadings`/`tocUpdateCommand` directly against `EditorState`, not committed). Fixed by
  `needsNewlineBefore`/`needsNewlineAfter` in `tocGenerator.ts`, which prepend/append a `\n` only when
  the insertion point isn't already adjacent to a line break or a document edge. Re-verified via the
  same smoke script: insert-then-edit-heading-then-replace round-trips correctly, and the code-fenced
  `#` is excluded from every scan.
- **`#slug` click resolution** (`links.ts`'s `resolveAndScrollToHeadingAnchor`): re-scans headings fresh
  via `scanHeadings(view.state)` at click time (never cached from the last "Update" run, per
  `[detailed-spec §4.6, §5.8]`), moves the cursor to the matching heading's line start, and scrolls it
  to the top of the viewport via `EditorView.scrollIntoView`. No match is a silent no-op — indistinguishable from this app's other unresolved-link cases, no error UI.
- **Not yet exercised via a real `EditorView`/WebView2**: verification above used `EditorState`
  transactions directly (a throwaway Node/`tsx` script), not `tauri dev` + CDP, since this pass was
  scoped to the generator/resolver logic itself; the toolbar wiring (`MarkdownToolbar.tsx`) was only
  typechecked/built, not click-tested interactively. Worth a `reviewer-lite` pass clicking "Update" in a
  real running memo, including the cross-feature paths `docs/plans/README.md` calls out (Tab Lock,
  `#slug` resolution after a lock/unlock cycle).

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
