# Session Handoff

> Written to carry context into a fresh conversation when the previous one has grown too large.
> Read this once at the start of the new session, then treat it as disposable — once its content is
> either acted on or absorbed back into `spec.md` / `DECISIONS.md` / `QA.md` / commits, this file can be
> deleted. It is not a permanent part of the documentation set (unlike `spec.md`, `DECISIONS.md`,
> `detailed-spec.md`, `QA.md`, which `docs/plans/README.md` and `CLAUDE.md` already index).

## Where the project stands

- The project is transitioning from branch `V1.0.0` to `V1.1.0` (currently on `V1.1.0`). This session's
  work was entirely **specification-stage** for v1.1.0 — no code implementation, no `developer-lite`
  work, and nothing has been committed yet.
- **New convention established this session**: version-suffixed spec files. `docs/spec.md` is now
  frozen as the permanent v1.0.0 baseline and is never appended to again. New/changed spec content for
  v1.1.0 lives in `docs/spec.v1.1.0.md` (new file, fully written this session, no open questions). Future
  version bumps repeat the same pattern (`spec.v1.2.0.md`, etc.), and the same separation is meant to
  extend to `docs/detailed-spec.v<version>.md` and version-labeled `docs/plans/<feature>/` content once
  those stages are reached — **not yet done for v1.1.0** (see Next step).
- `docs/spec.v1.1.0.md` is complete: 5 features (§1-§5), no items in its closing "Open Questions for
  User" section, and a full consistency-review pass (via `designer-lite`, report-only) already ran and
  every finding from it was fixed in a follow-up pass. Read this file's own Index first, as usual.

## The 5 confirmed v1.1.0 features (`docs/spec.v1.1.0.md`)

1. **§1 — OS-Folder → Tree-Pane Drag-and-Drop Import**: drag `.md` files from Windows Explorer onto an
   open tab's tree pane to import them. Copy-only (source untouched), `.md`-only (non-`.md` items
   silently skipped), filename collisions get an in-app Rename/Overwrite/Skip prompt (deliberate
   divergence from the baseline's silent auto-number convention). A skip-summary toast fires only when
   at least one item was skipped; full success stays silent.
2. **§2 — Per-Tab Last-Viewed-Memo and Scroll-Position Memory**: extends the existing startup-only
   "reopen last memo" behavior to also apply on every in-session tab switch. New per-tab
   `lastViewedMemo` field; `AppSettings.lastOpened` narrowed to `{tabId}` only (modifies `spec.md`
   §3.4). Also restores the tree pane's own scroll/selection to the restored node (no new field needed
   for that part — computed at restore time).
3. **§3 — Cross-Tab Quick Memo-Name Search ("Quick Open")**: `Ctrl+Shift+P` overlay, substring-matches
   memo names across all tabs, reuses the existing cross-tab full-text index (`spec.md` §2.4) rather
   than building a new one. Results ordered MRU-first using §2's `lastViewedMemo` as the recency signal
   (tie-break alphabetical).
4. **§4 — Cross-Tab Favorites**: new `favorite?: boolean` on `MetaTreeNode` (rides along inside the
   existing tree/lock-payload structure, no new Tab-Lock handling needed). Cross-tab list view reuses
   the same index as Quick Open, same MRU ordering rule, same Tab-Lock exclusion, same
   jump-to-result behavior.
5. **§5 — Tab-Lock Auto-Relock on Inactivity**: single global inactivity timer (since at most one Tab
   Lock session can ever be alive at a time, per the existing "discard on leaving the tab" rule) that
   auto-ends a session even without leaving the tab. Reuses the existing pre-lock save-flush rule and
   session-teardown logic verbatim — only adds a new trigger. Pauses (doesn't reset) while any modal
   dialog is open. New configurable `AppSettings.tabLockAutoRelockMinutes` field, Settings → 一般 tab,
   dropdown with 5/15/30/60-minute presets + 無効, default 15 minutes.

**Dropped along the way**: an earlier "§2 Cross-Tab Memo Clipboard (Copy/Cut/Paste)" feature was fully
designed (multiple `designer-lite` passes) then **deleted entirely** after the user clarified their
original request was ambiguous and actually already covered by existing features — moving whole memo
files between tabs already works via existing cross-tab drag-and-drop, and copying memo *text* already
works via the existing editor's OS-clipboard copy/paste. This was a real misstep (over-elaborating an
ambiguous request before confirming the basic interpretation) — see
`feedback_confirm_interpretation_before_elaborating.md` in the auto-memory system for the full lesson.
Everything currently in §1-§5 has since been re-confirmed against the user's actual intent.

## Cross-cutting decisions worth knowing before continuing

- **schemaVersion bumps are consolidated, not per-field**: `TabMeta.schemaVersion` bumps once for
  v1.1.0 covering both §2's `lastViewedMemo` and §4's `favorite` together; `AppSettings.schemaVersion`
  bumps once covering both §2's `lastOpened` narrowing and §5's `tabLockAutoRelockMinutes` together —
  explicitly, not three independent increments, since all three ship together and no real intermediate
  file state will ever exist.
- **Citation discipline matters in this file**: `[spec §x]` is a defined convention meaning "cites
  `docs/spec.md` specifically." The consistency-review pass found and fixed two cases where content
  actually sourced from `detailed-spec.md` had been mis-tagged as `[spec §x]` — worth double-checking
  citation accuracy again when writing `detailed-spec.v1.1.0.md`, since it will cite both `spec.md` and
  `spec.v1.1.0.md`.

## Agent/config changes made this session (project + global)

To make the version-suffixed-file convention durable (not dependent on the orchestrator re-explaining it
in every task prompt — this was a real, repeated problem this session before these fixes landed):

- **Project-level** `.claude/agents/{designer,developer,reviewer}-lite.md`: each now explicitly
  documents the version-suffixed-file convention in their "must check" steps.
- **This project's own `CLAUDE.md`**: "Documentation for This Repository" section now has a pointer
  sentence about future version bumps following this convention, citing `docs/README.md`.
- **`docs/README.md`**: File Structure list now documents `spec.v1.1.0.md` and the convention it starts.
- **Global** `~/.claude/agents/{designer,developer,reviewer}-lite.md` (affects *other* projects too, not
  just this one): each now has a line to also read `~/.claude/README-lite-workflow.md`.
- **Global** `~/.claude/README-lite-workflow.md`: new "Version-Suffixed Specification Files (Optional
  Convention)" section — generic description of the pattern, explicitly optional (not a default for new
  projects), with instructions for adopting projects to mirror it into their own `docs/README.md` and
  `CLAUDE.md`.
- This defense-in-depth setup (three layers: project `docs/README.md`, global shared doc, project
  `CLAUDE.md`) is itself saved as a standing lesson: `feedback_defense_in_depth_for_conventions.md`.

## Uncommitted state (as of this handoff)

Nothing from this session has been committed. Current working tree (`git status --short`):

```
 M .claude/agents/designer-lite.md
 M .claude/agents/developer-lite.md
 M .claude/agents/reviewer-lite.md
 M CLAUDE.md
 M docs/README.md
?? docs/spec.v1.1.0.md
```

`.claude/files/` and `"CLAUDE - コピー.md"` are also untracked but **pre-existing, unrelated to this
session's work** — do not assume they need handling here.

The user has not yet given go-ahead to commit. Per this repo's commit convention (one file per commit,
message describes that file's role/purpose), each modified/new file above should get its own commit once
confirmed.

## Next step

1. **Immediate**: create `docs/detailed-spec.v1.1.0.md` (elaborating `docs/spec.v1.1.0.md` to a level
   `developer-lite` can plan from, same relationship `detailed-spec.md` already has to `spec.md`) — this
   is what the user was about to start when this handoff was written. Follow the same
   version-suffixed-file convention: do not append into `docs/detailed-spec.md`.
2. **After that**: per the user's original instruction, `docs/plans/<feature>/PLAN.md` content
   introduced by v1.1.0 features should also stay separate from any existing plan rather than being
   appended — but the **exact separation format for the plans layer was deliberately left undecided**
   (a companion file vs. a version-labeled section; see `developer-lite.md`'s own note to confirm the
   format with the user/`designer-lite` the first time an *existing* plan needs a version-specific
   change, rather than inventing one). None of the 5 v1.1.0 features currently modify an existing plan
   (they're all brand-new feature areas), so this open format question may not even come up yet.
3. Get the user's go-ahead to commit the uncommitted state listed above.
