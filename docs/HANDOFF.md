# Session Handoff

> Written to carry context into a fresh conversation when the previous one has grown too large.
> Read this once at the start of the new session, then treat it as disposable — once its content is
> either acted on or absorbed back into `spec.md` / `DECISIONS.md` / `QA.md` / commits, this file can be
> deleted. It is not a permanent part of the documentation set (unlike `spec.md`, `DECISIONS.md`,
> `detailed-spec.md`, `QA.md`, which `docs/plans/README.md` and `CLAUDE.md` already index).

## Where the project stands

- `docs/spec.md`, `docs/detailed-spec.md`, `docs/DECISIONS.md`, `docs/QA.md` are all up to date with
  every decision made this session (see below). `QA.md` currently has **no open items**.
- `docs/plans/` now exists (created this session) with 12 independent per-feature `PLAN.md` files plus
  `docs/plans/README.md` as an index. Per `.claude/agents/README.md`'s workflow, this is the
  "implementation planning" stage output — **no implementation code exists yet** (`src/` has not been
  started). Every plan is marked "planning complete, not yet implemented."
- All 12 plans are currently **fully unblocked** — no plan has an open `QA.md`-dependent item.

## Decisions made this session (already reflected in the docs above — this is a summary, not the record)

This was a long session covering two phases:

**Phase 1 — creating the initial plan set**: `docs/plans/` was created from scratch (12 feature plans +
index), organized by feature name rather than dependency order (per user request), with a mandatory
whole-app integration-test policy recorded in `docs/plans/README.md` for after all features are
implemented.

**Phase 2 — a long design-review conversation that amended both `detailed-spec.md` and the plans**:

1. **Tab Lock crash-mid-lock recovery** (formerly `QA.md` Q1): resolved as Option B — explicit
   in-progress marker + deterministic resume/rollback. Fully specified in `detailed-spec.md` §5.9 and
   `tab-lock/PLAN.md`.
2. **UI-state persistence**: scroll position (persist) and toolbar position (persist) now firm
   requirements; search/replace history confirmed session-only. New fields:
   `MemoEditorSettings.scrollPosition`, `AppSettings.window.toolbarPosition`.
3. **Force-delete shortcut**: confirmed `Ctrl+Delete` (was previously just an example).
4. **Tab-lock placeholder display name**: confirmed as the 🔒 glyph alone, no text.
5. **`label.color`**: confirmed 5-choice palette adopted verbatim from the legacy app (None/Red `#e05a5a`/
   Blue `#4a90d9`/Green `#4caf50`/Yellow `#e0c040`). The right-click "Icon" submenu (`spec §2.2`) is this
   picker. `label.kind` confirmed to stay reserved/unimplemented for v1 (verified via legacy code that
   it was always a dead/unused field).
6. **Mouse-wheel tab switching**: removed from v1 scope entirely (`spec.md` §2.1 itself edited). Click
   and keyboard-shortcut switching unaffected.
7. **Markdown toolbar — fully redesigned and finalized**: fixed **8×2 = 16-slot grid** (up from an
   earlier 12-slot concept), icon set = **Lucide** (bundled locally). Bold/Italic are separate slots;
   Undo/Redo added as toolbar actions. A "rearrange mode" (16th slot, `GripVertical` icon) lets the user
   drag-reorder the 16 icons; OFF=click invokes action, ON=drag-only+Esc-exits+whole-toolbar tint. Icon
   order persists (`AppSettings.window.toolbarOrder`, new field). Full layout table, icon mapping, and
   mechanism are in `detailed-spec.md` §5.10 and `markdown-editing/PLAN.md` §4.
8. **New v1 feature — Table of Contents (TOC)**: triggered by the toolbar's "Update" slot only (manual,
   never live/background). Scans headings, GitHub-style slugifies them, writes a nested Markdown link
   list into a marker-delimited block (insert-or-replace in place). Adds a new same-document
   heading-anchor link type (`#slug`) to the Link domain — click re-scans current memo, scrolls via
   CodeMirror, silent no-op if no match. Fully specified in `spec.md` §2.8/§4.2, `detailed-spec.md`
   §4.6/§5.8/§5.10, `markdown-editing/PLAN.md` §8, `link/PLAN.md` §1/§5.
9. **Toolbar "Color"/"Font size" buttons** (text decoration via `<span>`, distinct from `label.color`):
   confirmed palettes adopted verbatim from the legacy app — Color: Red `#e53935`/Blue `#1e88e5`/
   Green `#43a047`/Orange `#fb8c00`/Purple `#8e24aa` (+ a "clear color" control); Font size: Small `12px`/
   Large `20px`/Extra-large `28px` (+ a "clear size" control).
10. **Default shortcuts**: all 16 toolbar actions now get a default keybinding in
    `window-operation/PLAN.md` §3's keymap (rebindable via the existing `AppSettings.shortcuts`
    mechanism — no new mechanism was needed).
11. A self-review pass confirmed no other recording gaps; `docs/plans/README.md`'s integration-test
    example list got a new "TOC + Tab Lock" example.

**Process note for future sessions**: partway through, the user asked that `DECISIONS.md`/`PLAN.md`
entries be written **concisely** — decision + concrete parameters only, skip rationale/rejected-
alternatives narrative unless it's load-bearing for a future implementation judgment call (this is
saved as a standing memory: `feedback_concise_docs.md`). Entries written before that point in this
session (e.g. the Tab Lock recovery entry, the label/Icon-submenu investigation entry) are still in the
older, more narrative style and were **not** retroactively rewritten in `DECISIONS.md`/`detailed-spec.md`
(low priority — rarely re-read); `docs/plans/tab-lock/PLAN.md` **was** trimmed of a few non-load-bearing
"why" sentences, since `PLAN.md` files are read often. Two rationale notes were deliberately kept in
`tab-lock/PLAN.md` (why the marker isn't encrypted; why the marker must hold the complete mapping) because
they prevent a plausible future "fix" that would reintroduce the exact bug they exist to close.

## Uncommitted state (as of this handoff)

Nothing from this session has been committed yet. Current working tree:

- Modified: `docs/spec.md`, `docs/detailed-spec.md`, `docs/DECISIONS.md`, `docs/QA.md`
- New (untracked): `docs/plans/` (12 `PLAN.md` files + `README.md`, 13 files total)

The user has not yet given the go-ahead to commit. Per this repo's commit convention (one file per
commit, message states that file's role/purpose), these files should each get their own commit once the
user confirms — the orchestrator asked for this confirmation multiple times this session without a
definitive yes/no yet.

`CLAUDE.ja.md` also has pre-existing uncommitted changes, unrelated to this session's work. It's a
Japanese mirror file; `CLAUDE.md`'s own "Language" section says not to create/maintain `.ja.md` mirrors
going forward. Whether to delete it (vs. leave it) has been asked but not yet answered.

## Next step

1. **Immediate**: get the user's go-ahead to commit the 4 modified docs + 13 new `docs/plans/` files
   (one file per commit), and a decision on `CLAUDE.ja.md`.
2. **After that**: per `.claude/agents/README.md`'s workflow, the next stage is **implementation**
   (`developer-lite` begins writing code in `src/`, following the finalized `PLAN.md` files) — likely
   starting with the two foundational plans (`project-architecture`, `persistence-fs`) that the other
   ten depend on. No implementation has started yet.
