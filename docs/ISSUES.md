# Known Issues (ISSUES.md)

> Lightweight tracker for minor bugs, gaps, or rough edges found during implementation/review that do
> **not** need the user's judgment call to even understand (unlike [docs/QA.md](QA.md), which is for
> open questions that need the user to decide something before work can proceed). Entries here are
> meant to accumulate and be cleaned up together in a batch, rather than being triaged one at a time as
> they're found.
>
> **Convention for resolved items**: once an entry is fixed, remove it from this file (history remains
> in git). If an entry turns out to actually need a user decision rather than just a fix, move it to
> `docs/QA.md` instead of leaving it here.

---

## Open

### Edit Lock: live enforcement for an already-open memo

- **Found by**: `reviewer-lite`, reviewing `docs/plans/editor-save/PLAN.md`.
- **Symptom**: `editor_load`'s `edit_locked` flag is a load-time snapshot; if Edit Lock is toggled on
  while the memo is already open in the editor, the editor doesn't go read-only immediately — typing
  still works, and a save attempt then surfaces a generic error instead of being blocked client-side.
  No data-loss risk (backend correctly refuses the write).
- **Now reachable**: issue #64 (PR #67) added an edit-lock/unlock context-menu item, so this can be
  reproduced by locking a memo while it's already open in the editor.
- Direction (live-enforce vs. some other mitigation) is tracked as GitHub issue #12.
