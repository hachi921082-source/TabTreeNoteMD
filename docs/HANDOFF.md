# Session Handoff

> Written to carry context into a fresh conversation when the previous one has grown too large.
> Read this once at the start of the new session, then treat it as disposable — once its content is
> either acted on or absorbed back into `spec.md` / `DECISIONS.md` / `QA.md` / commits, this file can be
> deleted. It is not a permanent part of the documentation set (unlike `spec.md`, `DECISIONS.md`,
> `detailed-spec.md`, `QA.md`, which docs/README.md already indexes).

## Where the project stands

- `docs/spec.md` is the confirmed baseline. It was amended this session (see below).
- `docs/detailed-spec.md` has been created by `designer-lite`, self-reviewed, and fixed based on that
  review. It has also been shown to two external reviewers (ChatGPT/GPT and Gemini) outside this
  workflow; the user asked for Claude's independent assessment of both reviews (not a fix pass) —
  that assessment was given in conversation and is not duplicated into any doc. Net effect: no doc
  changes resulted from either external review, since neither surfaced anything that changed the
  outcome of "safe to proceed."
- `docs/QA.md` currently has **no open items**.

## Decisions made this session (already reflected in `docs/spec.md` / `docs/DECISIONS.md`)

1. **Markdown editor engine: CodeMirror 6 + a custom-built decoration extension** (not
   Milkdown/TipTap/Slate/Draft.js). Rationale and rejected alternatives are in `DECISIONS.md`
   ("Editor Engine for Markdown Editing").
2. **`viewMode: 'live' | 'source'`** added to `AppSettings.editorDefaults` and `MemoEditorSettings`
   (per-memo override falls back to the global default), on top of the pre-existing always-available
   runtime toggle.
3. **Tab deletion is unregister-only** — there is no "delete tab and folder" option (the legacy app had
   one; it was deliberately dropped for this rebuild). Removing a tab never touches the underlying
   folder or its files. Rationale is in `DECISIONS.md` ("Tab Deletion Semantics: Unregister-Only, No
   Folder-Trash Option").

## Uncommitted state (as of this handoff)

Nothing from this session has been committed yet. Current working tree:

- Modified: `docs/spec.md`, `docs/DECISIONS.md`
- New (untracked): `docs/detailed-spec.md`, `docs/QA.md`

The user has not yet given the go-ahead to commit. Per this repo's commit convention (one file per
commit, message states that file's role/purpose), these four files should each get their own commit
once the user confirms.

`CLAUDE.md` also has pre-existing uncommitted changes (Commit Convention / Language sections) that
predate this session — unrelated, but still uncommitted; do not assume they came from this handoff's
work.

## Next step

Per the workflow in `.claude/agents/README.md`, the next stage is **implementation planning**:
`developer-lite` reads `docs/spec.md` and `docs/detailed-spec.md` and creates
`docs/plans/<feature-name>/PLAN.md` for each feature, escalating to `designer-lite` (which escalates to
`docs/QA.md` if needed) for anything requiring the user's own judgment. No `docs/plans/` directory
exists yet — this would be the first feature plan created.
