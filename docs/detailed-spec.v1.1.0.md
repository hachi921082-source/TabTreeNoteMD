# TabTreeNoteMD v1.1.0 Detailed Specification (detailed-spec.v1.1.0.md)

> Elaboration of [spec.v1.1.0.md](spec.v1.1.0.md) to a level usable by `developer-lite` when writing
> implementation plans (`docs/plans/<feature-name>/PLAN.md`) for the five v1.1.0 features. This document
> plays the same role for `spec.v1.1.0.md` that [detailed-spec.md](detailed-spec.md) plays for
> [spec.md](spec.md) — it does not introduce new product decisions on its own; it translates and refines
> what is already confirmed, either directly in `spec.v1.1.0.md` or through the dedicated Q&A round this
> document's content was settled through (16 decisions, summarized inline at the point each applies).
> Where a genuine ambiguity remains unresolved, it is **not** decided here — it is logged in
> [QA.md](QA.md) instead (see §6).
>
> **Citation convention** (matches `detailed-spec.md`'s own use of `[spec §x]`, extended with one more
> prefix for the version-suffixed spec file this document elaborates):
> - `[spec §x]` — cites `docs/spec.md` (the frozen v1.0.0 baseline).
> - `[spec v1.1.0 §x]` — cites `docs/spec.v1.1.0.md` (the v1.1.0 feature additions this file elaborates).
> - `` `detailed-spec.md` §x `` (no brackets) — cites the baseline detailed specification, for mechanisms
>   and conventions this file reuses rather than redefines.
>
> Read `spec.v1.1.0.md` first; this document assumes familiarity with it and does not repeat the
> rationale already written inline there. Decision rationale, comparative analysis, and rejected
> alternatives belong in `DECISIONS.md`, not here — this document states outcomes and the mechanics of
> applying them.

---

## Index

Keep this in sync whenever a section is added, renamed, or moved below.

- **§0 How to Use This Document** — relationship to `spec.v1.1.0.md`, `spec.md`, and `detailed-spec.md`; starting-default convention.
- **§1 Data Model Additions**
  - §1.1 `TabMeta` Schema Additions (`lastViewedMemo`, `MetaTreeNode.favorite`, combined `schemaVersion` bump)
  - §1.2 `AppSettings` Schema Changes (`lastOpened` narrowing, `tabLockAutoRelockMinutes`, combined `schemaVersion` bump)
  - §1.3 Full-Text Index Entry Extension (`favorite` field)
  - §1.4 New Constants (auto-relock warning lead time)
- **§2 Backend Operation Inventory Additions** (extends `detailed-spec.md` §4)
  - §2.1 Import Domain (New)
  - §2.2 Tree/Memo Domain Additions (favorite toggle — two surfaces, tree right-click and Quick Open star; last-viewed-memo write)
  - §2.3 Search Domain Additions (Quick Open query, favorites list query)
  - §2.4 Tab Lock Domain Additions (auto-relock trigger, pre-timeout warning)
- **§3 Feature Behavior Details — Elaboration**
  - §3.1 OS-Folder → Tree-Pane Drag-and-Drop Import
  - §3.2 Per-Tab Last-Viewed-Memo and Scroll-Position Memory
  - §3.3 Cross-Tab Quick Open
  - §3.4 Cross-Tab Favorites
  - §3.5 Tab-Lock Auto-Relock on Inactivity
- **§4 Cross-Cutting Additions**
  - §4.1 `schemaVersion` / Migration Handling
  - §4.2 Error Handling Additions (extends `detailed-spec.md` §7)
- **§5 Deferred-to-Implementation Details (v1.1.0)** — implementation-discretion items, not ambiguities.
- **§6 Open Questions Requiring User Decision** — currently empty.

---

## 0. How to Use This Document

- `spec.v1.1.0.md` = authoritative "what is decided" for the five v1.1.0 features. This document = "how
  that decided behavior plays out precisely enough to plan and build it," the same relationship
  `detailed-spec.md` already has to `spec.md`. Neither document dictates Rust module layout, crate
  choice, or Tauri command naming — that stays `PLAN.md`-level, same as the baseline.
- Every mechanism already specified for the baseline (atomic write, self-write registry, sync order,
  conflict detection, reconciliation, the shared dialog component, etc.) is **reused verbatim** by the
  v1.1.0 features unless explicitly stated otherwise; this document cites the relevant `detailed-spec.md`
  section rather than re-deriving those mechanisms.
- Same "starting default" convention as `detailed-spec.md` §0: anywhere this document proposes a concrete
  value/behavior inside a range `spec.v1.1.0.md` left open, `developer-lite` may adjust it during planning
  as long as the observable behavior/guarantees this document and `spec.v1.1.0.md` describe are preserved.
  §5 below enumerates the items intentionally left open this way.
- This document's content was settled through a dedicated Q&A round with the user, covering points
  `spec.v1.1.0.md` itself left open (12 initial questions, 4 follow-ups after the first round of answers —
  16 decisions total). Each decision is stated inline at the point it applies, with a brief note of what
  question it resolves, so the connection back to the user's own choice stays traceable.
- A small number of points below go one step further than a direct restatement of a user decision —
  necessary mechanical consequences of applying it (e.g. §3.2's finding that a locked tab's
  `lastViewedMemo` is, as a direct effect of one of the 16 decisions, never auto-restored in v1.1.0). These
  are marked inline as extensions beyond `spec.v1.1.0.md`'s literal text, not escalated to §6, because they
  follow directly from something already decided rather than introducing a new preference — but they are
  called out explicitly so the user can object later if a given extension doesn't match their intent. (An
  earlier draft of this document also proposed extending Q4-3's Quick-Open-only star click-to-toggle
  pattern to the Favorites list view's own star — the user reviewed and declined that extension, so §2.2/
  §3.4 as written keep the Favorites list view's star read-only, per the existing right-click-only
  mechanism in `[spec v1.1.0 §4.2]`.)

---

## 1. Data Model Additions

### 1.1 `TabMeta` Schema Additions

Elaborates `[spec v1.1.0 §2.2]` (`lastViewedMemo`) and `[spec v1.1.0 §4.1]` (`MetaTreeNode.favorite`).
Concrete diff against the unlocked-tab example in `detailed-spec.md` §3.3:

```jsonc
{
  "schemaVersion": 2,                    // bumped once, combined for both additions below (§4.1)
  "appVersion": "0.1.0",
  "lastViewedMemo": "Shopping list",     // NEW — string | null, top-level sibling to "tree"
  "tree": [
    {
      "memoName": "Shopping list",
      "expanded": true,
      "label": { "color": "#4a90d9", "kind": null },
      "editor": { "wrap": "window", "tabWidth": null, "showLineNumbers": null, "fontSize": null, "viewMode": null, "scrollPosition": 340 },
      "locked": false,
      "favorite": true,                  // NEW — optional boolean, sibling to "locked"/"label"
      "children": []
    }
  ]
}
```

- `lastViewedMemo` is a plain `memoName` string (extension-less, matching the `MetaTreeNode.memoName`
  convention, `detailed-spec.md` §3.3) or `null`. Absent is treated identically to `null` — a never-viewed
  tab, or a pre-upgrade file that predates this field.
- `favorite` is optional; absent and `false` are equivalent (mirrors how `locked?: boolean` and `label?:
  Label` are already treated as optional/absent-means-default, `[spec §3.4]`). Newly-created memos,
  newly-imported memos (§3.1 below), and reconciliation-appended entries (`detailed-spec.md` §3.5) all
  start with `favorite` absent.
- Mirrored onto the runtime-only `TreeNode` type the same way `[spec v1.1.0 §4.1]` states `locked?`/
  `label?` already are: `readonly favorite?: boolean`.
- **Locked-tab placement**: for a locked tab, `lastViewedMemo` lives inside the decrypted lock payload
  (the `realTabName`/`tree`/`fileMap` structure, `detailed-spec.md` §3.4), not as a plaintext sibling of
  the `lock` field in `.tabtreenote.json` — per `[spec v1.1.0 §2.2]`'s own placement rule. Concrete diff
  against the decrypted-payload example in `detailed-spec.md` §3.4:

```jsonc
{
  "realTabName": "Private",
  "lastViewedMemo": "Diary 2026-07",     // NEW — string | null, plaintext only inside the decrypted payload
  "tree": [ /* MetaTreeNode[], favorite mirrored the same as the unlocked case above */ ],
  "fileMap": [ { "memoName": "Diary 2026-07", "token": "9f2c7a1e4b3d0f8c6a5e2d1b0c9a8f7e" } ]
}
```

  `favorite` needs no analogous placement decision — it already rides inside `tree`, which is either
  fully plaintext (unlocked) or fully inside the blob (locked), with no new mechanism, per
  `[spec v1.1.0 §4.1]`'s own note.
- **Write side, both fields**: covered in §2.2 below. Neither field introduces a new save trigger; both
  ride along at the existing save-trigger cadence (`[spec §2.6]`, `detailed-spec.md` §5.6).

### 1.2 `AppSettings` Schema Changes

Elaborates `[spec v1.1.0 §2.2]` (`lastOpened` narrowing) and `[spec v1.1.0 §5.4]`
(`tabLockAutoRelockMinutes`). Concrete diff against the example in `detailed-spec.md` §3.2:

```jsonc
{
  "schemaVersion": 2,                          // bumped once, combined for both changes below (§4.1)
  "appVersion": "0.1.0",
  // ...
  "lastOpened": { "tabId": "t1" },             // CHANGED — "memoName" field removed
  // ...
  "tabLockAutoRelockMinutes": 15,              // NEW — number | null (0 also treated as disabled)
  // ...
}
```

- `lastOpened.memoName` is no longer written by the app going forward. A pre-upgrade file that still has
  it simply has that field ignored on read — no migration/strip step required
  (`[spec v1.1.0 §2.2]`, "no destructive migration is needed").
- `tabLockAutoRelockMinutes` absent (pre-upgrade file, or a schemaVersion-2 file that predates the user
  ever opening Settings) resolves to the documented default, `15` (`[spec v1.1.0 §5.4]`) — i.e. "absent"
  is not a third disabled/undefined state distinct from an explicit value. `null` and `0` are both valid
  explicit "disabled" values once the field has been written at least once via Settings; which of the two
  the Settings UI writes for "無効" is `developer-lite`'s call (§5).
- Both changes ship together, covered by the **same single** `AppSettings.schemaVersion` bump
  (`[spec v1.1.0 §2.2]`, `[spec v1.1.0 §5.4]`) — see §4.1 below.

### 1.3 Full-Text Index Entry Extension

Elaborates `[spec v1.1.0 §4.3]`. The in-memory index entry shape (`detailed-spec.md` §5.4,
`{tabId, memoName, plaintext}`) gains one field:

```
{ tabId: string, memoName: string, plaintext: string, favorite: boolean }
```

- No new field is needed for Quick Open's own matching (§3.3 below) — it filters/matches on the existing
  `memoName` field already present in every entry; it only *reads* the new `favorite` field, to render the
  star. In Quick Open (§3.3 below) that star is clickable, per Q4-3. In the Favorites list view (§3.4
  below) that same field renders a read-only star instead, tracing not to Q4-3 (which was Quick-Open-only)
  but to `[spec v1.1.0 §4.3]`'s own two-column-shape requirement, extended for visual consistency with
  Quick Open's row shape (see §3.4 below for the read-only-vs-clickable distinction itself).
- `favorite` is kept in sync by the same "in-app change patches index directly" path already specified for
  every other index field (`[spec §2.4]`, "Two paths for index differential update";
  `detailed-spec.md` §5.4, "Update triggers") — toggling favorite (§2.2 below) is one more in-app mutation
  that patches its own entry, no different in kind from a rename or a body save.
- Locked-tab exclusion applies to this extended entry exactly as it already does to the rest of the entry
  (`[spec v1.1.0 §3.4]`/`[spec v1.1.0 §4.4]`) — no separate exclusion logic for the new field.

### 1.4 New Constants

Extends `detailed-spec.md` §3.7's Constants Inventory.

| Constant | Value | Source |
|---|---|---|
| Tab-Lock auto-relock warning lead time | 30 seconds (fixed, not configurable, not proportional to the configured timeout) | User decision, §3.5 below (Q5-2-A) |
| Tab-Lock auto-relock configurable presets | 5 / 15 / 30 / 60 minutes, plus 無効 (disabled) | `[spec v1.1.0 §5.4]` |
| Tab-Lock auto-relock default | 15 minutes | `[spec v1.1.0 §5.4]` |

The 30-second warning lead time is always shorter than every configurable preset (minimum preset is 5
minutes = 300 seconds), so the warning window never needs to be clamped or special-cased for a short
timeout value.

---

## 2. Backend Operation Inventory Additions

Extends the domain list in `detailed-spec.md` §4. Same conventions as that section: logical operation
names, not Rust/Tauri signatures — naming is `PLAN.md`-level.

### 2.1 Import Domain (New)

Elaborates `[spec v1.1.0 §1]` in full.

- **Import dropped files** — input: the OS drag-drop payload (list of source file paths + drop
  coordinates, resolved by the frontend against the tree DOM to a target tab id and either "empty
  background" or "hovering node X," reusing whatever hit-testing the internal-DND "Move / reorder /
  re-parent memo" operation (`detailed-spec.md` §4.2) already implements for the same visual affordance —
  no second hit-testing implementation is defined here, same reasoning `[spec v1.1.0 §1.4]` itself already
  gives for reusing internal DND's hover-zone handling). Runs the sequence below and returns an aggregate
  summary `{ imported: number, skipped: number }` for the skip-summary toast (`[spec v1.1.0 §1.2]`).
  1. **Filter**: partition the payload into `.md` items and non-`.md` items (folders included, per
     `[spec v1.1.0 §1.1]`); every non-`.md` item is immediately counted as skipped, no further processing.
  2. **Placement resolution**: exactly one destination for the whole batch — root-level end, or
     end-of-children for the hovered node (`[spec v1.1.0 §1.4]`); a single drop has one drop point. There
     is no third "insert before/after a specific sibling" outcome for this feature.
  3. **Per-item processing loop**, in whatever order the OS reports the payload
     (`[spec v1.1.0 §1.4]`, implementation discretion):
     a. Sanitize the source filename (extension stripped) using the same strict name-validator used for
        any brand-new tab/memo name (`[spec §5.11]`, `detailed-spec.md` §6.5); a produced replacement
        suggestion is applied directly, no confirmation prompt (`[spec v1.1.0 §1.2]`).
     b. Check the sanitized name for a collision against the current state of the target tab's tree —
        pre-existing memos plus any memo already imported earlier in this same batch (two dropped files
        that sanitize to the same name collide with each other too; a `Rename`-resolved earlier item's new
        `name(1)` also counts as "existing" for this purpose). Direct, mechanical consequence of
        `[spec v1.1.0 §1.3]`'s "already exists in the target tab" collision rule applied item-by-item.
     c. **No collision** → import immediately (step 4).
     d. **Collision, target is an existing pre-existing memo with `MetaTreeNode.locked === true`
        (Edit Lock)** — **Overwrite is not offered for this item** (user decision, resolving a gap
        `[spec v1.1.0 §1.3]` left open: Edit Lock is defined as "text editing only prohibited"
        (`[spec §2.2]`), and Overwrite is a body-content replacement, i.e. the same class of operation Edit
        Lock exists to block). Concretely: if the per-item collision dialog is shown for this item, its
        Overwrite choice is rendered disabled (greyed out) so it cannot be selected; only Rename / Skip are
        selectable — Q1-1's answer settles "disabled," not the specific presentation (e.g. whether a
        tooltip explains why; that presentation detail is implementation discretion, §5). If this item is instead
        auto-resolved by an "apply to all: Overwrite" batch policy set earlier in the same drop (no
        per-item dialog shown for it), it is silently force-converted to **Skip** instead — this item's
        skip is counted toward the skip-summary tally (§1.2) the same as any other skip. Rename and Skip
        both remain fully available for an edit-locked collision target either way, since neither touches
        the locked memo's existing body.
     e. **Collision, and a batch-wide policy (`rename` / `overwrite` / `skip`) other than the
        edit-lock-forced case above is already set** from an earlier "apply to all" choice in this batch →
        apply it silently, no dialog for this item.
     f. **Collision, no batch-wide policy set (and not the edit-lock-forced case)** → show the collision
        dialog (`[spec v1.1.0 §1.3]`); the user's choice, and whether "apply to all remaining" was also
        checked, is recorded and applied going forward in the loop. **Esc** stops the loop: this item and
        every remaining not-yet-processed item count as skipped; items already imported before the Esc are
        not rolled back.
  4. **Import a resolved item**: read the source file's bytes and decode using the same BOM/UTF-8/
     Shift_JIS detection already specified for loading any memo body (`detailed-spec.md` §5.3, "Encoding
     detection order on load"); the decoded text becomes the new memo's canonical body, written per the
     app's normal save-encoding rule (`[spec §3.4]` `AppSettings.encoding.defaultSave`, a global setting).
     EOL style is detected from the source file and preserved on write (`detailed-spec.md` §5.3, "EOL
     handling"), same as any other load/save. Sync order (`detailed-spec.md` §6.3), adapted for a
     brand-new file: (i) in-memory tree update (post-sanitization/collision-resolution, already done in
     3a–3f), (ii) the real file write — self-write-registered before it happens (`detailed-spec.md` §6.2)
     and atomically written (`detailed-spec.md` §6.1; a brand-new target with nothing to preserve
     collapses to write-temp → fsync → rename-to-target, skipping the `.bak` generation-swap sub-steps),
     (iii) the metadata JSON save. New `MetaTreeNode` entries get the same default settings reconciliation
     already gives newly-appended entries (`detailed-spec.md` §3.5): `expanded: true`, no label, no
     per-memo editor override, `locked: false`, `favorite` absent. The affected index entry is patched
     immediately (`detailed-spec.md` §5.4), same as any other in-app change.
     - **Overwrite** case (never reachable for an edit-locked target per step 3d above): same target
       filename and tree position/metadata are kept; only the body content is replaced
       (`[spec v1.1.0 §1.3]`) — steps (i)/(iii) above are skipped since nothing about the tree entry
       changes, only step (ii)'s file write happens (with self-write registration and atomic write as
       usual), plus an index-patch of just the `plaintext` field.
  5. **Metadata save batching**: whether step 4(iii) (or the Overwrite case's index patch) happens once
     per resolved item or is coalesced into a single metadata save at the end of the batch is
     implementation discretion (§5) — both preserve the same crash-safety guarantee, since files already
     written to disk before a mid-batch crash are recoverable via the ordinary §3.5 reconciliation the
     next time the tab is opened.
  6. **Source-file-read failure** (e.g. the dragged file is locked/held open by another process at the
     moment of import): treated as `IO_LOCKED` — short retry per the existing Windows-lock retry policy
     (`detailed-spec.md` §3.7), then, if still failing, skip this item (counts toward the skip-summary
     tally) rather than aborting the whole batch.
  7. **Target-tab reachability preconditions**, both restatements of `[spec v1.1.0 §1.5]`: a locked tab
     with no active session has no tree pane to drop onto at all (same reasoning as
     `detailed-spec.md` §5.9's "nothing to drag from it," applied to dropping rather than dragging); a tab
     with an unresolved crash-recovery marker (`detailed-spec.md` §5.9, "Recovery on tab-folder access")
     is showing its recovery dialog instead of an ordinary tree, so it is not a reachable drop target
     either, by the same "no tree pane surface" reasoning.
  8. **Locked-tab-with-active-session target**: proceeds exactly like a normal-tab import — each imported
     body is encrypted into the session's in-memory state and re-saved into the lock blob, filename
     obfuscated per `[spec §4.1 pt.1]` (`[spec v1.1.0 §1.5]`). Steps 3–5 above are unaffected by
     session/lock status; only the final write path differs (encrypt-then-write instead of write). Step
     3d's Edit-Lock check is unaffected too — `MetaTreeNode.locked` is orthogonal to Tab Lock.
  9. **A collision dialog from an earlier, still-unresolved drop already open on the same target tab**:
     per `[spec v1.1.0 §1.3]`'s existing "no other tree operation on the target tab is accepted" rule while
     a collision dialog is open, a second concurrent OS-file-drop onto the same tab while one is already
     pending is itself a tree operation on that tab and is likewise not accepted until the first dialog
     resolves.
- **No change to editor/tree selection state after import** (user decision, Q1-2 — resolving a gap
  `[spec v1.1.0 §2.3]`'s `lastViewedMemo`-update-trigger list left open, since "import" is not among the
  triggers it enumerates): whatever memo was open in the editor, and whatever row was selected/scrolled-to
  in the tree, before the import started remains exactly as it was afterward — no imported memo is
  auto-opened, no tree auto-scroll/auto-select happens as a result of the import itself. §2.2 below states
  the direct consequence: "Write last-viewed memo" is **not** invoked by this operation.
- **Skip-summary toast**: rendered by the frontend once "Import dropped files" returns, using its
  `{imported, skipped}` result — fires only if `skipped > 0` (`[spec v1.1.0 §1.2]`). Exact copy/format and
  toast duration/dismissal are implementation discretion (§5).

### 2.2 Tree/Memo Domain Additions

Extends `detailed-spec.md` §4.2.

- **Toggle favorite** — metadata-only mutation, same class as the existing "Toggle expand/collapse, set
  label, set per-memo editor override, toggle edit lock" bullet (`detailed-spec.md` §4.2). Flips
  `MetaTreeNode.favorite`, saves metadata JSON (`detailed-spec.md` §6.1), and patches the corresponding
  index entry's `favorite` field (§1.3 above). Callable from **two** UI surfaces:
  1. The tree's right-click context-menu entry (`[spec v1.1.0 §4.2]`) — single-target-only precondition,
     same as the Edit-Lock menu item (disabled when the current tree selection is not exactly one item,
     `detailed-spec.md` §5.2). This is the **only** way to remove an entry from the Favorites list view
     itself (§3.4 below) — the list view's own star glyph is read-only display, not a click target (user
     decision, confirmed after review: the click-to-toggle star pattern established by Q4-3 is scoped to
     Quick Open only and does not extend to the Favorites list view).
  2. The star icon on a Quick Open result row (§3.3 below; user decision, Q4-3) — target is the specific
     row clicked, unambiguous, no "current selection" precondition applies.
  Both surfaces invoke the same underlying operation; only how the `{tabId, memoName}` target is identified
  differs (current tree selection vs. the specific clicked row).
- **Write last-viewed memo** — not a separate user-facing operation; a side effect folded into whatever
  operation already changes "which memo is open within a tab" or "which tab is active": memo click,
  arrow-key nav, `memo:` link navigation, tab switch (§3.2 below), and Quick-Open/Favorites-list selection
  (§3.3/§3.4 below). For Quick Open specifically (§3.3 below), this is distinct from the star-click toggle
  above — selecting the *row itself* triggers this, clicking its star does not; the Favorites list (§3.4
  below) has no star click target at all (per the point above), so this row-vs-star distinction is only
  meaningful for §3.3. Sets `TabMeta.lastViewedMemo` in the in-memory tree representation at the same moment the
  previously-open memo's `scrollPosition` is captured (§3.2 below), and rides along in whatever metadata
  save already happens at the existing tab-switch/memo-switch save-trigger cadence (`[spec §2.6]`,
  `detailed-spec.md` §5.6) — no new save-trigger category, per `[spec v1.1.0 §2.3]`.
  - **Not triggered by Import** (§2.1 above, Q1-2) — the imported memo(s) do not become the tab's
    `lastViewedMemo`.
  - **Not triggered by a star-click favorite toggle** (§3.3 — the only surface with one, per the point
    above) — toggling a favorite does not change which memo is considered "last viewed" in any tab.

### 2.3 Search Domain Additions

Extends `detailed-spec.md` §4.4.

- **Quick Open query** — input: query string, **may be empty**. Filters the existing in-memory index (§1.3
  above; `detailed-spec.md` §5.4):
  - **Non-empty query**: case-insensitive substring match against `memoName` (`[spec v1.1.0 §3.2]`; user
    decision, Q3-2, resolving that same section's unspecified case-sensitivity).
  - **Empty query**: no filtering — every index entry is a candidate (user decision, Q3-1: the overlay
    shows the full ordering-rule-sorted list even before typing, not an empty result list).
  - Both cases sorted per the ordering rule in §3.3 below, capped at an implementation-chosen count (§5).
  - Output: ordered list of `{ tabId, tabName, memoName, isLastViewed: boolean, isFavorite: boolean }` —
    `isFavorite` is the read side supporting the star icon and its click-to-toggle behavior (§2.2 above,
    Q4-3).
  - Locked-tab exclusion is automatic (the candidate set is the index itself, `[spec v1.1.0 §3.4]`).
- **Get favorites list** — same output shape as Quick Open query (including `isFavorite`, which is always
  `true` for every row here), no query-string input; filters the index by `favorite === true` instead of a
  name-substring match, same ordering rule (`[spec v1.1.0 §4.3]`). Locked-tab exclusion automatic, same
  reasoning (`[spec v1.1.0 §4.4]`).

### 2.4 Tab Lock Domain Additions

Extends `detailed-spec.md` §4.1/§5.9.

- **Auto-relock trigger** — not a new teardown operation; a new caller of the existing "Session end"
  routine already specified in `detailed-spec.md` §5.9 ("Unlocking (starting a session)," step 5), invoked
  by either (a) the inactivity timer elapsing, or (b) the app window losing OS focus while a session is
  active — see §3.5 below for the concrete trigger mechanics. Reuses the existing pre-teardown flush rule
  (`[spec §8]`) verbatim, per `[spec v1.1.0 §5.3]`.
- **Pre-timeout warning** (new sub-state, user decision Q5-2) — a **non-blocking, non-modal** notice shown
  when the inactivity timer reaches (configured-timeout − 30s) with no activity since. Explicitly **not**
  built from the shared modal dialog component (`detailed-spec.md` §6.5) — if it were, showing it would
  itself pause the very timer it exists to warn about, per `[spec v1.1.0 §5.2]`'s "pauses while a modal
  dialog is open" rule, which would prevent the warned-about timeout from ever being reached. Cancelled by
  the same ordinary keyboard/pointer activity that resets the main timer (user decision, Q5-2-B — no
  separate explicit "stay unlocked" action is needed); when the warning is cancelled this way, it hides and
  the main timer resets to its full configured duration, same as any other activity-reset.
- **Auto-relock toast** (user decision, Q5-3) — fires **only** when the session end was caused by the
  inactivity timer elapsing (path (a) above); **does not** fire when caused by app-focus loss (path (b)) —
  a focus-loss-triggered session end stays indistinguishable from an ordinary tab-switch-triggered session
  end, with no toast at all.

---

## 3. Feature Behavior Details — Elaboration

### 3.1 OS-Folder → Tree-Pane Drag-and-Drop Import — Elaboration

Elaborates `[spec v1.1.0 §1]`; the concrete backend sequence is in §2.1 above. Remaining UI-level detail:

- **Drag-over visual feedback** (before drop): reuses whatever hover-highlight affordance internal DND
  already shows for a drag hovering a row/background — not a specific documented visual language elsewhere
  in `detailed-spec.md`, same "implementation detail, not a new spec-level rule" treatment
  `[spec v1.1.0 §1.4]` itself gives to hover-zone geometry; exact styling is implementation discretion (§5).
- **Collision dialog**: built from the shared custom dialog component (`detailed-spec.md` §6.5), Esc
  behavior per §2.1 step 3f above. Its Overwrite control is conditionally disabled per §2.1 step 3d (Q1-1)
  when the colliding target is edit-locked. While open, this dialog "pauses" the Tab-Lock inactivity timer
  if the target tab currently holds the active locked session (§3.5 below) — the concrete case
  `[spec v1.1.0 §5.2]` names as motivating the pause rule.
- **No editor/tree selection change as a result of import** (§2.1 above, Q1-2) — the frontend must not
  auto-open or auto-select any imported memo, even the last one processed in a batch.
- **No new `ErrorCode`s are introduced**: sanitization reuses the existing strict-validation path
  (`NAME_INVALID`-class handling, auto-applied rather than surfaced, §2.1 step 3a); source-read failures
  reuse `IO_LOCKED`-class retry-then-skip handling (§2.1 step 6, §4.2 below); the edit-lock-blocked
  Overwrite case (Q1-1) is a UI-level restriction (disabled control, or silent force-to-skip), not a new
  error code — the underlying Edit Lock enforcement it relies on is the same one already governing direct
  body edits, with no new `ErrorCode` needed for this new call site.

### 3.2 Per-Tab Last-Viewed-Memo and Scroll-Position Memory — Elaboration

Elaborates `[spec v1.1.0 §2]`. Data model in §1.1 above.

**`restoreTabView(tabId)`** — the single function invoked at both trigger points
(`[spec v1.1.0 §2.2]`, "startup restore and mid-session tab-switch restore become the same code path"):
1. Load the tab's reconciled tree (`detailed-spec.md` §3.5) and its `TabMeta.lastViewedMemo`.
2. If `lastViewedMemo` is `null`/absent, **or** it names a memo that reconciliation dropped (deleted,
   renamed away): the editor pane shows no memo open, and the tree shows no row selected, for this tab —
   reusing the exact rule already specified for the analogous baseline startup case
   (`detailed-spec.md` §5.6, "if the referenced memo no longer exists after §3.5 reconciliation, do
   nothing"; `[spec v1.1.0 §2.3]`). Both "never viewed yet" and "no longer exists" collapse to this same
   end state.
3. Otherwise, open the named memo (`detailed-spec.md` §4.3, "Load memo body") and restore its
   `MemoEditorSettings.scrollPosition` (`detailed-spec.md` §3.3) exactly as an ordinary memo-switch already
   does.
4. **Tree-pane visual follow-through**: scroll the tree so the restored memo's node is visible and mark it
   selected/highlighted — computed fresh from the current tree structure at restore time, not a persisted
   field (`[spec v1.1.0 §2.3]`).

**Trigger points invoking `restoreTabView`**:
- App startup: resolve the target `tabId` from `AppSettings.lastOpened.tabId` first
  (`[spec v1.1.0 §2.2]`); if that tab no longer exists in `AppSettings.tabs`, do nothing at the tab level
  either, matching `detailed-spec.md` §5.6.
- An ordinary in-session tab switch (click, tab-switch keyboard shortcut) to a tab that is **not** in the
  middle of starting a Tab-Lock session (see below).

**Explicitly excluded from `restoreTabView`** (both are user decisions from the Q&A round, not literal
restatements of `spec.v1.1.0.md`'s own trigger list, which is silent on both):
- **Tab-Lock session start** (password entry succeeds): the tab does **not** auto-restore
  `lastViewedMemo` at this moment (user decision, Q2-1). The session begins with the editor showing no
  memo open and the tree showing no row selected, exactly like case 2 above, regardless of what
  `lastViewedMemo` actually holds — the user must click a row to view anything, even on the very first
  moment content becomes decryptable in that run.
  - **Consequence worth stating explicitly, not itself a new decision**: because a Tab-Lock session
    always ends the instant its tab is left (`[spec §4.1 pt.5]`) and a locked tab has no visible tree while
    no session is active, the *only* two moments a locked tab's tree is ever shown are "just after a
    session starts" (excluded above) and "mid-session, switching to some other already-open tab and back"
    — which cannot happen, because leaving the tab is exactly what ends the session. In practice, this
    means a locked tab's `lastViewedMemo` is written every time the open memo within an active session
    changes (§2.2 above), but is **never** read back automatically anywhere in v1.1.0 — its only purpose
    at present is data-model symmetry with the unlocked case and potential future use, not an active
    restore path. This is a direct arithmetic consequence of Q2-1's answer combined with the existing
    `[spec §4.1 pt.5]` session-lifetime rule, not an independent choice made in this document.
- **Selecting a Quick Open or Favorites-list result that targets a different tab** (§3.3/§3.4 below): this
  switches to that tab, but does **not** invoke `restoreTabView` for it — instead it opens the *specific*
  memo the user selected directly, and that becomes the tab's new `lastViewedMemo` (user decision, Q2-2,
  confirming the intended precedence between `[spec v1.1.0 §2.3]`'s "tab switch restores `lastViewedMemo`"
  rule and `[spec v1.1.0 §3.5]`/`[spec v1.1.0 §4.5]`'s "selecting a result opens that specific memo" rule
  for the moment both would otherwise apply to the same tab switch — not to be confused with *this*
  document's own §3.5, which covers Tab-Lock Auto-Relock, an unrelated section).

### 3.3 Cross-Tab Quick Open — Elaboration

Elaborates `[spec v1.1.0 §3]`. Backend operation in §2.3 above.

- **Overlay component**: built from the shared custom dialog component (`detailed-spec.md` §6.5) — text
  input (autofocused on open) plus a result list below it, Esc-closable (`[spec v1.1.0 §3.5]`).
- **Trigger/shortcut**: default `Ctrl+Shift+P`, registered as an `AppSettings.shortcuts` entry
  (`[spec v1.1.0 §3.1]`); rebindable via the Settings → ショートカット tab.
- **Initial state on open, before typing**: shows the full candidate list (every index entry), sorted per
  the ordering rule below — not an empty result list (user decision, Q3-1).
- **Matching once the user types**: live-recomputed on every keystroke; case-insensitive substring match
  against `memoName` across the whole index (`[spec v1.1.0 §3.2]`; user decision, Q3-2).
- **Ordering** (applies identically to the empty-query full list and to any filtered result set): a result
  whose memo is currently its own tab's `lastViewedMemo` sorts first (value `0`) ahead of all others (value
  `1`); tie-break — alphabetical by display name. Implements `[spec v1.1.0 §3.2]`'s ordering rule exactly.
- **Result cap**: capped and scrollable; exact count is implementation discretion (§5), per
  `[spec v1.1.0 §3.2]`'s own "cosmetic minutiae" callout.
- **Favorite star per row** (user decision, Q4-3): each result row shows a star glyph reflecting
  `isFavorite` (§2.3 above). **Clicking the star directly toggles that memo's favorite status** (invokes
  "Toggle favorite," §2.2 above) — it does **not** also select/navigate to the result (the star is a
  distinct click target from the rest of the row). After toggling, the overlay stays open and the row
  updates its star glyph in place; the result list's order is unaffected, since ordering depends on
  `lastViewedMemo`-match and alphabetical name, never on `favorite` (§3.2's ordering rule is unchanged by
  this feature).
- **Keyboard interaction inside the overlay**: Up/Down moves the highlighted result, Enter opens the
  highlighted result (or the top result if none is highlighted yet — meaningful now that the list is never
  empty on open, per Q3-1), Esc dismisses without acting (`[spec v1.1.0 §3.5]`); mouse click on a row (
  outside the star) also selects it.
- **Selecting a result** (clicking the row, or Enter): switches to the result's owning tab and opens that
  specific memo directly — this is the "selecting a result" path from §3.2 above (Q2-2): it does not go
  through `restoreTabView` for the target tab, and it does update that tab's `lastViewedMemo` to the
  selected memo, same as any other way of opening a memo. Same end effect as manually clicking the tab
  then the tree row (`[spec v1.1.0 §3.5]`).
- **Dismissal**: Esc closes the overlay without acting (no tab switch, no memo opened, no pending star
  toggle undone — a star toggle already committed before Esc stays committed), consistent with
  `[spec §7]`'s general Esc-cancels convention.

### 3.4 Cross-Tab Favorites — Elaboration

Elaborates `[spec v1.1.0 §4]`. Data model in §1.1 above; backend operations in §2.2/§2.3 above.

- **Toggle UI, tree surface**: right-click context-menu entry, label flips "お気に入りに追加" /
  "お気に入りから削除" (`[spec v1.1.0 §4.2]`) — same two-state label-flip and single-selection-only
  precondition already used for the Edit-Lock menu item (`detailed-spec.md` §5.2).
- **No list-view star click-to-toggle**: unlike Quick Open (§3.3 above), the Favorites list view's star
  glyph is **read-only display**, not a click target (user decision, confirmed after review — reverting an
  earlier draft of this document that had extended Q4-3's click-to-toggle pattern here; Q4-3 was scoped to
  Quick Open only). Removing an entry from the Favorites list stays a right-click-only operation, via the
  same tree context-menu toggle described above (`[spec v1.1.0 §4.2]`) — there is no additional removal
  affordance inside the list view itself. A user viewing the Favorites list who wants to unfavorite an
  entry switches to that memo's tab and uses the tree's right-click menu, same as toggling any other
  favorite.
- **Cross-tab list view**: same two-column shape (display name + owning tab name) as Quick Open's result
  list (`[spec v1.1.0 §4.3]`), now understood to include the same star-glyph column as Quick Open for
  visual consistency (both surfaces show the same information: name, owning tab, favorite state) — but
  here the star is purely informational (every row's star is trivially "on," since this list is already
  filtered to `favorite === true`), not interactive, per the point above. Whether this view literally reuses
  the same overlay component as Quick Open (same component, static `favorite === true` filter and no live
  query, instead of a live-typed substring filter) or is a visually-matching but separate component is
  implementation discretion (§5) — only the two-column-plus-star *shape* is spec-level.
- **Ordering**: the fixed rule from §3.3 above (own-tab `lastViewedMemo` match first, then alphabetical) —
  reused verbatim, **no manual reorder or pin** (user decision, Q4-1: the fixed rule stays as-is, no
  drag-reorder UI or persisted custom order is added).
- **No count cap** (user decision, Q4-2): the Favorites list has no maximum number of entries; every
  favorited memo across every eligible tab appears, subject only to the same locked-tab exclusion as
  everywhere else in this feature.
- **Trigger**: toolbar ★ button, always visible, no default shortcut (`[spec v1.1.0 §4.6]`); still bindable
  via `AppSettings.shortcuts` like any action.
- **Tab Lock exclusion**: automatic via the shared index (§1.3 above), no new logic
  (`[spec v1.1.0 §4.4]`).
- **Selecting an entry** (clicking the row — the star is not a separate click target here, unlike Quick
  Open, since it is read-only): identical jump-to-tab-and-open behavior as Quick Open's own "Selecting a
  result" (§3.3 above) — same `restoreTabView`-bypass and `lastViewedMemo`
  update, reused verbatim (`[spec v1.1.0 §4.5]`, §3.2 above).

### 3.5 Tab-Lock Auto-Relock on Inactivity — Elaboration

Elaborates `[spec v1.1.0 §5]`. Backend triggers in §2.4 above.

- **Timer lifecycle**: the single global inactivity timer is created the moment a Tab Lock session starts
  (`detailed-spec.md` §5.9, "Unlocking (starting a session)," step 3) and destroyed the moment that session
  ends, by whatever path (leaving the tab, permanent-unlock completion, delete-after-unlock, or now also
  inactivity-timeout or app-focus-loss, §2.4 above) — no timer exists while no session is active, per
  `[spec v1.1.0 §5.1]`'s "at most one locked session can ever be alive at a time."
- **Reset ("activity") condition**: keyboard/pointer input at the app-window level, while a session is
  active and the timer is not currently paused, updates the last-activity timestamp and — if the pre-
  timeout warning (below) is currently showing — cancels/hides it (user decision, Q5-2-B).
- **Pause condition** (elaborates `[spec v1.1.0 §5.2]`): the timer pauses (stops counting down, does not
  reset) while:
  - any dialog built from the shared custom dialog component (`detailed-spec.md` §6.5) is open — including
    §3.1's collision dialog, the case `[spec v1.1.0 §5.2]` itself names — and
  - the **Settings screen** is open (user decision, Q5-1 — resolving a gap `[spec v1.1.0 §5.2]` left
    open: it names "any modal dialog" but does not explicitly say whether the Settings screen counts;
    confirmed here that it does, treated the same as a modal dialog for this purpose).
  Resumes counting from where it left off once the dialog/Settings screen closes. Because this app shows
  only one tab's content at a time and a locked session can only be active while its own tab is the active
  tab, any such surface that could be open is necessarily in the context of that same tab.
  - **The pre-timeout warning itself is explicitly excluded from this pause list** (§2.4 above) — it is not
    a modal dialog and does not pause the timer, by design, since it exists to warn about the same timer
    continuing to run.
- **On-timeout teardown**: fires the existing "Session end" routine (`detailed-spec.md` §5.9) verbatim,
  flush-then-teardown, no new teardown logic (`[spec v1.1.0 §5.3]`). If the flush step itself surfaces a
  conflict-resolution dialog (`detailed-spec.md` §6.4), that is no different from how any other trigger
  into "leaving the tab" teardown could already stall on a conflict dialog.
- **Pre-timeout warning** (§2.4 above; user decision, Q5-2): appears when the timer reaches
  (configured-timeout − 30s) with no activity since (§1.4 above, fixed 30-second lead, not proportional —
  user decision, Q5-2-A). Non-blocking, ambient — rendered as a toast/banner, not a dialog (see the "pause
  condition" bullet above for why this matters). Any ordinary activity while it is showing cancels it and
  resets the main timer to its full configured duration (Q5-2-B) — there is no separate "Stay unlocked"
  button distinct from ordinary activity detection. Exact visual treatment (e.g. whether a live numeric
  countdown is shown, or a static message) is implementation discretion (§5).
- **App-focus-loss trigger**: a Tauri window-focus-lost event, while a session is active, immediately
  invokes the same teardown — independent of, and not gated by, the inactivity timer/pause/warning state at
  all (`[spec v1.1.0 §5.2]`, broadening `[spec §4.1 pt.5]`). Because this trigger is instantaneous, the
  30-second warning never applies to it — there is no window of time in which the app is both losing focus
  and still displayed for a warning to be seen.
- **Auto-relock toast** (§2.4 above; user decision, Q5-3): shown **only** after a teardown caused by the
  inactivity timer elapsing — distinguishing text such as "非アクティブのため自動的にロックされました" (
  exact copy is implementation discretion, §5) from the app's other, silent session-end paths. **Not**
  shown after an app-focus-loss-triggered teardown, which stays silent/indistinguishable from any other
  ordinary session end (leaving the tab, etc.).
- **Settings UI**: `tabLockAutoRelockMinutes` dropdown in the 一般 tab, choices 5 / 15 / 30 / 60 / 無効
  (`[spec v1.1.0 §5.4]`); persisted via the existing generic `AppSettingsPatch` deep-partial-merge
  mechanism (`detailed-spec.md` §3.2) — no new settings-persistence plumbing required. Exact row styling
  within the 一般 tab is implementation discretion (§5).

---

## 4. Cross-Cutting Additions

### 4.1 `schemaVersion` / Migration Handling

Elaborates `[spec v1.1.0 §2.2]`'s consolidated-bump notes (also referenced from `[spec v1.1.0 §4.1]` and
`[spec v1.1.0 §5.4]`).

- `TabMeta.schemaVersion` increments once, covering both `lastViewedMemo` (§1.1) and
  `MetaTreeNode.favorite` (§1.1) together. `AppSettings.schemaVersion` increments once, covering both the
  `lastOpened` narrowing and `tabLockAutoRelockMinutes` (§1.2) together. Neither is split into two
  increments — no real intermediate file state with only one of each pair's fields will ever exist, since
  both ship in the same release (`[spec v1.1.0 §2.2]`).
- **No destructive migration logic is required for either bump.** Every new field is optional/nullable
  with a well-defined "absent" meaning (§1.1, §1.2 above); a pre-upgrade file simply reads with those
  fields absent, and the app's own writes from that point on include them. None of the 16 Q&A decisions in
  §2–§3 above introduce any additional schema field beyond the two already covered by these bumps — every
  decision in this document is purely behavioral (interaction rules, trigger scoping, UI affordances), not
  a new persisted field.
- The concrete new integer value each `schemaVersion` bumps **to** depends on whatever value is currently
  shipping in the v1.0.0 codebase at the time v1.1.0 is implemented; this document does not hardcode a
  specific number, consistent with `detailed-spec.md` §3.7 already listing the `schemaVersion` starting
  value as a constant with no numeric spec, to be fixed by `developer-lite`.

### 4.2 Error Handling Additions

Extends the table in `detailed-spec.md` §7. **No new `ErrorCode` values are introduced by any of the five
v1.1.0 features.** All error-shaped situations reuse an existing code:

| Situation | Reused code | Handling |
|---|---|---|
| Import: sanitization needed on a dropped filename | (internally reuses `NAME_INVALID`-class validation) | Auto-applied replacement, never surfaced to the user (§2.1 step 3a; `[spec v1.1.0 §1.2]`) |
| Import: collision on a dropped filename | (internally reuses `NAME_DUPLICATE`-class detection) | In-app collision dialog, not a blocking error (§2.1 step 3f; `[spec v1.1.0 §1.3]`) |
| Import: collision target is Edit-Locked | (no new code — UI-level restriction) | Overwrite disabled in the dialog, or silently force-converted to Skip in a batch-applied policy (§2.1 step 3d; Q1-1) |
| Import: source file locked/unreadable at read time | `IO_LOCKED`-class retry-then-skip | Retried per `detailed-spec.md` §3.7, then skipped and tallied, batch continues (§2.1 step 6) |
| Tab-Lock: auto-relock's pre-teardown flush hits a save conflict | `CONFLICT` | Existing conflict-resolution dialog (`detailed-spec.md` §6.4), unrelated to the (already-fired) inactivity timer |

---

## 5. Deferred-to-Implementation Details (v1.1.0) — Not Ambiguities, No User Decision Needed

Mirrors the role of `detailed-spec.md` §9, scoped to the five v1.1.0 features.

- Order in which multiple simultaneously-dropped files are processed (§2.1; `[spec v1.1.0 §1.4]`, spec's
  own explicit discretion callout).
- Whether import's metadata-JSON save is coalesced once per batch or written once per resolved item
  (§2.1 step 5).
- Exact copy, format, duration, and dismissal behavior of the import skip-summary toast (§2.1, §3.1).
- Exact drag-over visual affordance shown while an OS file hovers a tree row/background before drop
  (§3.1).
- Whether the disabled Overwrite control on an edit-locked collision target shows a tooltip (and its exact
  copy, if so) explaining why it's disabled — Q1-1 settles only that the control is disabled, not this
  presentation detail (§2.1 step 3d).
- Quick Open / Favorites result-list cap count (§3.3; `[spec v1.1.0 §3.2]`, spec's own explicit discretion
  callout).
- Whether the Favorites cross-tab list view reuses Quick Open's overlay component or is a visually-matching
  separate component (§3.4).
- Exact enumeration of DOM/window event types counted as "activity" for the inactivity timer, and whether a
  boolean or counter is used to track the open-modal/open-Settings pause state (§3.5).
- Exact visual treatment of the pre-timeout warning (e.g. static text vs. a live numeric countdown) (§3.5).
- Exact copy text of the auto-relock toast (§3.5).
- Exact row styling of the `tabLockAutoRelockMinutes` dropdown within the Settings 一般 tab (§3.5;
  `[spec v1.1.0 §5.4]`, spec's own explicit discretion callout).
- Whether the Settings UI writes `null` or `0` when the user selects "無効" for `tabLockAutoRelockMinutes`
  (§1.2) — both are valid per the type, spec does not distinguish them.
- The concrete new integer value `TabMeta.schemaVersion`/`AppSettings.schemaVersion` bump to (§4.1).

---

## 6. Open Questions Requiring User Decision

None identified. All ambiguities `spec.v1.1.0.md` left open for these five features were resolved through
the dedicated Q&A round with the user described in §0 (12 initial questions plus 4 follow-ups), cited
inline throughout §2–§3 above at the point each applies, by question label (Q1-1, Q1-2, Q2-1, Q2-2, Q3-1,
Q3-2, Q4-1, Q4-2, Q4-3, Q5-1, Q5-2-A, Q5-2-B, Q5-3). The remaining unresolved-looking points in §3 above are
extensions/consequences flagged inline as such, not open questions — each states plainly what it follows
from and invites the user to object if the extension doesn't match their intent, per §0's note. Any newly
identified genuine spec-level ambiguity should be appended here and to `QA.md`, following the same
convention `detailed-spec.md` §10 already documents.
