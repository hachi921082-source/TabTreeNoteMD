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

## "Pin to Foreground" Tab Button: Observable Effect and Placement

**Raised by**: `developer-lite`, while implementing the `TabBar` frontend component
(`docs/plans/tab-management/PLAN.md` §4).

**Context**: `spec.md` §2.1, `detailed-spec.md` §4.1/§5.1, and `docs/plans/tab-management/PLAN.md` §4
all name a "📌 pin to foreground" button (session-only, not persisted) but none of them state what
actually happens when it's clicked. The old reference repo (`TabTreeNote`, read-only reference) has no
such feature at all — a grep for "pin" there only matches unrelated identifiers (`AppInfo`,
`setAppInfo`), so there is no legacy behavior to carry over.

Two sub-questions block implementation:

1. **Observable effect** — does clicking it (a) merely toggle a cosmetic pin icon/highlight on that tab
   with no other effect, (b) move/keep that tab pinned at a fixed position in the tab bar (e.g.
   leftmost, immune to drag-reordering), similar to browser tab-pinning conventions, or (c) something
   else?
2. **Placement (per-tab vs. single global button)** — `docs/plans/tab-management/PLAN.md` §4 calls it a
   "pin button per tab" (one instance embedded in each tab element), but `spec.md` §2.1 and
   `docs/plans/search-index/PLAN.md` §5 both describe it as sitting immediately to the left of the
   single, global, cross-tab search button in the "top bar" — which reads as one shared button, not one
   per tab. These two descriptions conflict and need to be reconciled before `TabBar` can be built
   correctly.

**Needed**: a concrete decision on both points so `spec.md`/`detailed-spec.md` can state the actual
behavior and placement, and `docs/plans/tab-management/PLAN.md` §4 can be corrected if its "per tab"
wording is wrong.

_(Past questions and their resolutions are available via this file's git history and are permanently
recorded in `DECISIONS.md`.)_
