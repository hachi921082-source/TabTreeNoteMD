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
- **Not reachable via any shipped UI today** (no context-menu control toggles Edit Lock at all yet).
- Direction (live-enforce now vs. defer until an Edit-Lock-toggle UI ships) is tracked as an open
  question in [docs/QA.md](QA.md).

### In-memo replace "selection" mode missing

- **Found by**: `reviewer-lite`, reviewing `docs/plans/editor-save/PLAN.md`.
- **Symptom**: `spec.md` §2.4 lists replace modes as "single / all / selection / one-by-one"; the
  implemented `components/Search/SearchPanel.tsx` / `state/useSearch.ts` only has replace-one and
  replace-all. No "constrain to current selection" mode.
- Whether this was an intentional drop or needs to be built is tracked as an open question in
  [docs/QA.md](QA.md).
