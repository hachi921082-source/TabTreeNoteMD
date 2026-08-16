# TabTreeNoteMD v1.1.0 Detailed Specification (detailed-spec.v1.1.0.md)

> Elaboration of [spec.v1.1.0.md](spec.v1.1.0.md) to a level usable by `developer-lite` when writing
> implementation plans (`docs/plans/<feature-name>/PLAN.md`) for the five v1.1.0 features. This document
> plays the same role for `spec.v1.1.0.md` that [detailed-spec.md](detailed-spec.md) plays for
> [spec.md](spec.md) — it does not introduce new product decisions; it translates and refines what is
> already confirmed. Where a genuine ambiguity requires the user's own judgment, it is **not** decided
> here — it is logged in [QA.md](QA.md) instead (see §6).
>
> **Citation convention used in this file** (parallels the one `spec.v1.1.0.md` itself introduced for
> citing `spec.md`):
> - `[spec §x]` — cites `docs/spec.md` (the frozen v1.0.0 baseline).
> - `[spec v1.1.0 §x]` — cites `docs/spec.v1.1.0.md` (the v1.1.0 feature additions this file elaborates).
> - `` `detailed-spec.md` §x `` (no brackets, matching how `spec.v1.1.0.md` itself cites that file) — cites
>   the baseline detailed specification, for mechanisms/conventions this file reuses rather than
>   redefines.
>
> Read `spec.v1.1.0.md` first; this document assumes familiarity with it and does not repeat the
> rationale already written inline there. Decision rationale, comparative analysis, and rejected
> alternatives belong in `DECISIONS.md`, not here.

---

## Index

Keep this in sync whenever a section is added, renamed, or moved below.

- **§0 How to Use This Document** — relationship to `spec.v1.1.0.md`, `spec.md`, and `detailed-spec.md`.
- **§1 Data Model Additions — Detailed**
  - §1.1 `TabMeta` Schema Addition (`lastViewedMemo`, `MetaTreeNode.favorite`, combined `schemaVersion` bump)
  - §1.2 `AppSettings` Schema Change (`lastOpened` narrowing, `tabLockAutoRelockMinutes`, combined `schemaVersion` bump)
  - §1.3 Full-Text Index Entry Extension (`favorite` field; no change needed for Quick Open's own matching)
- **§2 Backend Operation Inventory Additions** (extends `detailed-spec.md` §4)
  - §2.1 Import Domain (New)
  - §2.2 Tree/Memo Domain Additions (favorite toggle, last-viewed-memo write)
  - §2.3 Search Domain Additions (Quick Open query, favorites list query)
  - §2.4 Tab Lock Domain Additions (auto-relock trigger)
- **§3 Feature Behavior Details — Elaboration**
  - §3.1 OS-Folder → Tree-Pane Drag-and-Drop Import
  - §3.2 Per-Tab Last-Viewed-Memo and Scroll-Position Memory
  - §3.3 Cross-Tab Quick Memo-Name Search ("Quick Open")
  - §3.4 Cross-Tab Favorites
  - §3.5 Tab-Lock Auto-Relock on Inactivity
- **§4 Cross-Cutting Additions**
  - §4.1 `schemaVersion` / Migration Handling
  - §4.2 Error Handling Additions (extends `detailed-spec.md` §7)
- **§5 Deferred-to-Implementation Details (v1.1.0)** — implementation-discretion items, not ambiguities.
- **§6 Open Questions Requiring User Decision** — currently empty.

---

## 0. How to Use This Document

- `spec.v1.1.0.md` = authoritative "what is decided" for the five v1.1.0 features. This file = "how that
  decided behavior plays out precisely enough to plan and build it," exactly mirroring the relationship
  `detailed-spec.md` already has to `spec.md`. Neither file dictates Rust module layout, crate choice, or
  Tauri command naming — that stays `PLAN.md`-level, same as the baseline.
- Every mechanism already specified for the baseline (atomic write, self-write registry, sync order,
  conflict detection, reconciliation, the shared dialog component, etc.) is **reused verbatim** by the
  v1.1.0 features unless explicitly stated otherwise; this document cites the relevant `detailed-spec.md`
  section rather than re-deriving those mechanisms.
- Same "starting default" convention as `detailed-spec.md` §0: anywhere this document proposes a concrete
  value/behavior inside a range `spec.v1.1.0.md` left flexible (including its own explicit
  "implementation discretion" callouts), `developer-lite` may adjust it during planning as long as the
  observable behavior/guarantees in `spec.v1.1.0.md` are preserved. §5 below enumerates these explicitly.
- A small number of points below (flagged inline) are **non-arbitrary completions** of a rule
  `spec.v1.1.0.md` states the *purpose* of but does not spell out the exact trigger boundary for (e.g.
  §3.2's session-start restore). These are included as elaboration, not escalated to §6, because the
  completion follows directly from rationale `spec.v1.1.0.md` itself already gives — not from a new
  preference invented here. Each is called out explicitly so the user can object later if the completion
  doesn't match their intent.

---

## 1. Data Model Additions — Detailed

### 1.1 `TabMeta` Schema Addition

Elaborates `[spec v1.1.0 §2.2]` (`lastViewedMemo`) and `[spec v1.1.0 §4.1]` (`MetaTreeNode.favorite`).
Concrete diff against the unlocked-tab example already given in `detailed-spec.md` §3.3:

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
      "favorite": true,                  // NEW — optional boolean, sibling to "locked"/"label"/"editor"
      "children": []
    }
  ]
}
```

- `lastViewedMemo` is a plain `memoName` string (extension-less, matching `MetaTreeNode.memoName`
  convention, `detailed-spec.md` §3.3) or `null`. Absent is treated identically to `null` (never-viewed
  tab, or pre-upgrade file that predates this field).
- `favorite` is optional; absent and `false` are equivalent (mirrors how `locked?: boolean` and
  `label?: Label` are already treated as optional/absent-means-default, `[spec §3.4]`). Newly-created
  memos, newly-imported memos (§3.1 below), and reconciliation-appended entries (`detailed-spec.md` §3.5)
  all start with `favorite` absent.
- Mirrored onto the runtime-only `TreeNode` type the same way `[spec v1.1.0 §4.1]` itself states
  `locked?`/`label?` already are: `readonly favorite?: boolean`. (Note: `spec.md`'s own literal `TreeNode`
  TS snippet, `[spec §3.4]`, shows `label?` but not `locked?` — this document does not attempt to resolve
  that pre-existing discrepancy, which predates v1.1.0 and is outside this file's scope; `favorite`
  simply follows whatever `[spec v1.1.0 §4.1]` already committed to.)
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
  fully plaintext (unlocked) or fully inside the blob (locked) with zero new mechanism, per
  `[spec v1.1.0 §4.1]`'s own note.

### 1.2 `AppSettings` Schema Change

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
  it is simply not read for that field — no migration/strip step required (`[spec v1.1.0 §2.2]`, "no
  destructive migration is needed").
- `tabLockAutoRelockMinutes` absent (pre-upgrade file, or a schemaVersion-2 file that predates a user
  ever opening Settings) is treated as if it held the documented default, `15` — i.e. "absent" is not a
  third disabled/undefined state distinct from an explicit value; it resolves to the same default a
  first-ever launch would get (`[spec v1.1.0 §5.4]`, "Default value: 15 minutes"). `null` and `0` are
  both valid explicit "disabled" values once the field has been written at least once via Settings; which
  of the two the Settings UI writes on "無効" is `developer-lite`'s call (§5).
- Both changes ship together, covered by the **same single** `AppSettings.schemaVersion` bump
  (`[spec v1.1.0 §2.2]`, `[spec v1.1.0 §5.4]`) — see §4.1 below.

### 1.3 Full-Text Index Entry Extension

Elaborates `[spec v1.1.0 §4.3]`. The in-memory index entry shape (`detailed-spec.md` §5.4,
`{tabId, memoName, plaintext}`) gains one field:

```
{ tabId: string, memoName: string, plaintext: string, favorite: boolean }
```

- No new field is needed for Quick Open (§3.3 below) — it filters/matches on the existing `memoName`
  field already present in every entry.
- `favorite` is kept in sync by the same "in-app change patches index directly" path already specified
  for every other index field (`[spec §2.4]`, "Two paths for index differential update";
  `detailed-spec.md` §5.4, "Update triggers") — the favorite-toggle operation (§2.2 below) is simply one
  more in-app mutation that patches its own entry, no different in kind from a rename or a body save.
- Locked-tab exclusion applies to this extended entry exactly as it already does to the rest of the
  entry (`[spec v1.1.0 §3.4]`/`[spec v1.1.0 §4.4]`) — no separate exclusion logic for the new field.

---

## 2. Backend Operation Inventory Additions

Extends the domain list in `detailed-spec.md` §4. Same conventions as that section: logical operation
names, not Rust/Tauri signatures; naming is `PLAN.md`-level.

### 2.1 Import Domain (New)

Elaborates `[spec v1.1.0 §1]` in full.

- **Import dropped files** — input: the OS drag-drop payload (list of source file paths + drop
  coordinates, resolved by the frontend against the tree DOM to a target tab id and either "empty
  background" or "hovering node X," reusing whatever hit-testing the internal-DND "Move / reorder /
  re-parent memo" operation already performs, `detailed-spec.md` §4.2/§5.2 — no second hit-testing
  implementation). Runs the full sequence below and returns an aggregate summary
  `{ imported: number, skipped: number }` for the skip-summary toast (`[spec v1.1.0 §1.2]`).
  1. **Filter**: partition the payload into `.md` items and non-`.md` items (folders included, per
     `[spec v1.1.0 §1.1]`); every non-`.md` item is immediately counted as skipped, no further processing.
  2. **Placement resolution**: exactly one destination — root-level end, or end-of-children for the
     hovered node (`[spec v1.1.0 §1.4]`) — applies to the whole batch (a single drop has one drop point).
     Unlike internal DND, this feature has **only these two outcomes**; there is no third "insert
     before/after a specific sibling" outcome — the hover-zone reuse in `[spec v1.1.0 §1.4]` is only about
     which of these two zones (background vs. over-a-row) a given cursor position falls into, not about
     adopting internal DND's sibling-reorder zone semantics.
  3. **Per-item processing loop**, in whatever order the OS reports the payload
     (`[spec v1.1.0 §1.4]`, implementation discretion):
     a. Sanitize the source filename (extension stripped) using the same strict name-validator used for
        any brand-new tab/memo name (`[spec §5.11]`, `detailed-spec.md` §6.5); if it produced a
        replacement suggestion, use that suggestion **directly**, no confirmation prompt
        (`[spec v1.1.0 §1.2]`, "auto-applied rather than shown as a blocking prompt").
     b. Check the sanitized name for a collision against the **current** state of the target tab's tree —
        which includes both pre-existing memos and any memo already imported earlier in this same batch
        (e.g. two dropped files that sanitize to the same name collide with each other, not only with
        pre-existing memos; a `Rename`-resolved earlier item's new `name(1)` also counts as "existing" for
        this purpose). This scoping is a direct, mechanical consequence of `[spec v1.1.0 §1.3]`'s own
        "already exists in the target tab" collision rule applied item-by-item within one batch — not a
        new rule.
     c. **No collision** → import immediately (step 4).
     d. **Collision, and a batch-wide policy (`rename` / `overwrite` / `skip`) is already set** from an
        earlier "apply to all" choice in this same batch → apply it silently, no dialog for this item.
     e. **Collision, no batch-wide policy set** → show the collision dialog (`[spec v1.1.0 §1.3]`); the
        user's choice, and whether they also checked "apply to all remaining," is recorded and applied
        going forward in the loop. **Esc** stops the loop: this item and every remaining not-yet-processed
        item count as skipped (`[spec v1.1.0 §1.3]`); items already imported before the Esc are not rolled
        back.
  4. **Import a resolved item**: read the source file's bytes and decode them using the same
     BOM/UTF-8/Shift_JIS detection already specified for loading any memo body
     (`detailed-spec.md` §5.3, "Encoding detection order on load") — the decoded text becomes this new
     memo's canonical body, written out per the app's normal save-encoding rules
     (`[spec §3.4]` `AppSettings.encoding.defaultSave`, a global setting, not per-tab). EOL style is likewise detected from the source
     file and preserved on write (`detailed-spec.md` §5.3, "EOL handling"), same as any other load/save.
     Follows the existing sync-order convention (`detailed-spec.md` §6.3) adapted for a brand-new file
     rather than a rename: (i) in-memory tree update (post-sanitization/collision-resolution, already done
     in steps 3a–3e), (ii) the real file write — self-write-registered before it happens
     (`detailed-spec.md` §6.2) and atomically written (`detailed-spec.md` §6.1; for a brand-new target with
     nothing to preserve, this collapses to write-temp → fsync → rename-to-target, skipping the `.bak`
     generation-swap sub-steps, the same simplification any other "create new file" operation already
     needs), (iii) the metadata JSON save. New `MetaTreeNode` entries get the same default settings
     reconciliation already gives newly-appended entries (`detailed-spec.md` §3.5): `expanded: true`, no
     label, no per-memo editor override, `locked: false`, `favorite` absent. The affected index entry is
     patched immediately (`detailed-spec.md` §5.4), same as any other in-app change.
     - **Overwrite** case: same target filename and tree position/metadata are kept; only the body content
       is replaced (`[spec v1.1.0 §1.3]`) — steps (i)/(iii) above are skipped since nothing about the tree
       entry changes, only step (ii)'s file write happens (with self-write registration and atomic write
       as usual), plus an index-patch of just the `plaintext` field.
  5. **Metadata save batching**: whether step 4(iii) (or the Overwrite case's index patch) happens once
     per resolved item or is coalesced into a single metadata save at the end of the whole batch is
     implementation discretion (§5) — both preserve the same crash-safety guarantee, since any files
     already written to disk before a mid-batch crash are still recoverable via the ordinary §3.5
     reconciliation the next time the tab is opened (they simply get picked up as reconciliation-appended
     entries if the metadata write for them never landed).
  6. **Source-file-read failure** (e.g. the dragged file is locked/held open by another process at the
     moment of import): treated the same as `IO_LOCKED` — short retry per the existing Windows-lock retry
     policy (`detailed-spec.md` §3.7), then, if still failing, skip this item (counts toward the
     skip-summary tally) rather than aborting the whole batch. This is a starting-default extension of the
     existing retry policy to a new read-source context, not a new error code (§4.2 below).
  7. **Target-tab reachability preconditions**, both direct restatements of `[spec v1.1.0 §1.5]`: a
     locked tab with no active session has no tree pane to drop onto at all (same reasoning as
     `detailed-spec.md` §5.9's "nothing to drag from it," applied to dropping rather than dragging); a tab
     with an unresolved crash-recovery marker (`detailed-spec.md` §5.9, "Recovery on tab-folder access")
     is, by the same logic, showing its recovery dialog instead of an ordinary tree — not a reachable drop
     target either, by direct extension of the same "no tree pane surface" reasoning.
  8. **Locked-tab-with-active-session target**: proceeds exactly like a normal-tab import — each imported
     body is encrypted into the session's in-memory state and re-saved into the lock blob, filename
     obfuscated per `[spec §4.1 pt.1]` (`[spec v1.1.0 §1.5]`). The collision dialog, sanitization, and
     placement logic above are unaffected by session/lock status; only the final write path differs
     (encrypt-then-write instead of write).
  9. **A collision dialog from an earlier, still-unresolved drop already open on the same target tab**:
     per `[spec v1.1.0 §1.3]`'s existing "no other tree operation on the target tab is accepted" rule
     while a collision dialog is open, a second concurrent OS-file-drop onto the same tab while one is
     already pending is itself a tree operation on that tab and is likewise not accepted until the first
     dialog resolves — direct extension of the existing rule, not a new one.
- **Skip-summary toast**: rendered by the frontend once "Import dropped files" returns, using its
  `{imported, skipped}` result — fires only if `skipped > 0` (`[spec v1.1.0 §1.2]`). Exact copy/format
  (e.g. "2 imported, 1 skipped (not .md)") and toast duration/dismissal are implementation discretion (§5).

### 2.2 Tree/Memo Domain Additions

Extends `detailed-spec.md` §4.2.

- **Toggle favorite** — metadata-only mutation, same class as the existing "Toggle expand/collapse, set
  label, set per-memo editor override, toggle edit lock" bullet (`detailed-spec.md` §4.2) — this is
  simply one more item added to the same bullet. Flips `MetaTreeNode.favorite`, saves metadata JSON
  (`detailed-spec.md` §6.1), and patches the corresponding index entry's `favorite` field (§1.3 above).
  Same single-target-only precondition as the Edit-lock context-menu item — disabled in the UI when the
  current tree selection is not exactly one item (`detailed-spec.md` §5.2, "disabled when the current
  selection is not a single item").
- **Write last-viewed memo** — not a separate user-facing operation; a side effect folded into whatever
  operation already changes "which memo is open within a tab" or "which tab is active" (memo click,
  arrow-key nav, `memo:` link navigation, tab switch, Quick-Open/Favorites-list selection). Sets
  `TabMeta.lastViewedMemo` in the in-memory tree representation at the same moment the previously-open
  memo's `scrollPosition` is captured (§3.2 below), and rides along in whatever metadata save already
  happens at the existing tab-switch/memo-switch save-trigger cadence (`[spec §2.6]`,
  `detailed-spec.md` §5.6) — no new save-trigger category, per `[spec v1.1.0 §2.3]`.

### 2.3 Search Domain Additions

Extends `detailed-spec.md` §4.4.

- **Quick Open query** — input: query string (possibly empty). Filters the existing in-memory index
  (§1.3 above; `detailed-spec.md` §5.4) by substring match against `memoName`
  (`[spec v1.1.0 §3.2]`), sorted per the ordering rule in §3.3 below, capped at an implementation-chosen
  count (§5). Output: ordered list of `{ tabId, tabName, memoName, isLastViewed: boolean }`. Locked-tab
  exclusion is automatic (the candidate set is the index itself, `[spec v1.1.0 §3.4]`).
- **Get favorites list** — same output shape as Quick Open query, no query-string input; filters the
  index by `favorite === true` instead of a name-substring match, same ordering rule
  (`[spec v1.1.0 §4.3]`, "identical rule to §3.2... reused verbatim"). Locked-tab exclusion automatic,
  same reasoning (`[spec v1.1.0 §4.4]`).

### 2.4 Tab Lock Domain Additions

Extends `detailed-spec.md` §4.1/§5.9.

- **Auto-relock trigger** — not a new teardown operation; a new caller of the existing "Session end"
  routine already specified in `detailed-spec.md` §5.9 ("Unlocking (starting a session)," step 5), invoked
  by either (a) the inactivity timer elapsing, or (b) the app window losing OS focus while a session is
  active — see §3.5 below for the concrete trigger mechanics. Reuses the existing pre-teardown flush rule
  (`[spec §8]`) verbatim, per `[spec v1.1.0 §5.3]`.

---

## 3. Feature Behavior Details — Elaboration

### 3.1 OS-Folder → Tree-Pane Drag-and-Drop Import — Elaboration

Elaborates `[spec v1.1.0 §1]`; the concrete backend sequence is in §2.1 above. Remaining UI-level detail:

- **Drag-over visual feedback** (before drop): reusing whatever hover-highlight affordance internal DND
  already shows for a drag hovering a row/background (`detailed-spec.md` §5.2's DND handling; no separate
  visual language is specified for this feature) is the starting default — same reasoning as §2.1's
  hit-testing reuse. Exact styling is implementation discretion (§5).
- **Collision dialog**: built from the shared custom dialog component (`detailed-spec.md` §6.5), Esc
  behavior per §2.1 step 3e above. While open, this same dialog's "pauses" the Tab-Lock inactivity timer
  if the target tab currently holds the active locked session (§3.5 below) — this is the concrete case
  `[spec v1.1.0 §5.2]` names as motivating the pause rule.
- **No new `ErrorCode`s are introduced**: sanitization reuses the existing strict-validation path
  (`NAME_INVALID`-class handling, auto-applied rather than surfaced, §2.1 step 3a); source-read failures
  reuse `IO_LOCKED`-class retry-then-skip handling (§2.1 step 6, §4.2 below).

### 3.2 Per-Tab Last-Viewed-Memo and Scroll-Position Memory — Elaboration

Elaborates `[spec v1.1.0 §2]`. Data model already covered in §1.1 above. Restore algorithm, expressed as
a single function reused at both trigger points (`[spec v1.1.0 §2.2]`, "startup restore and mid-session
tab-switch restore become the same code path"):

**`restoreTabView(tabId)`**:
1. Load the tab's reconciled tree (`detailed-spec.md` §3.5) and its `TabMeta.lastViewedMemo`.
2. If `lastViewedMemo` is `null`/absent, **or** it names a memo that reconciliation dropped (deleted,
   renamed away): the editor pane shows no memo open, and the tree shows no row selected, for this tab —
   this is the concrete behavior "do nothing" resolves to, directly reusing the resolution already given
   for the analogous baseline startup case (`detailed-spec.md` §5.6, "if the referenced memo no longer
   exists after §3.5 reconciliation, do nothing"; `[spec v1.1.0 §2.3]`). Both the "never viewed yet" case
   and the "no longer exists" case collapse to this same end state.
3. Otherwise, open the named memo (`detailed-spec.md` §4.3, "Load memo body") and restore its
   `MemoEditorSettings.scrollPosition` (`detailed-spec.md` §3.3) exactly as an ordinary memo-switch
   already does.
4. **Tree-pane visual follow-through**: scroll the tree so the restored memo's node is visible and mark
   it selected/highlighted — computed fresh from the current tree structure at restore time, not a
   persisted field (`[spec v1.1.0 §2.3]`). Same end state as the existing "tree selection and scroll
   position auto-follow the open memo" rule (`[spec §2.2]`).

**Trigger points invoking `restoreTabView`** — no behavioral difference between them:
- App startup: resolve the target `tabId` from `AppSettings.lastOpened.tabId` first
  (`[spec v1.1.0 §2.2]`); if that tab no longer exists in `AppSettings.tabs`, do nothing at the tab level
  either, matching the existing baseline rule (`detailed-spec.md` §5.6).
- Any in-session tab switch — by click, by tab-switch keyboard shortcut, or as the side effect of
  selecting a Quick Open/Favorites-list result that targets a different tab (§3.3/§3.4 below) — all funnel
  through the same "switch active tab" code path, so all trigger the same function.

**Locked-tab interaction** (not explicitly itemized as a trigger in `[spec v1.1.0 §2.3]`'s own list —
included here as a non-arbitrary completion per §0's note, flagged for visibility):
- Switching **to** a locked tab with no active session: `restoreTabView`'s tree-restoration step (3–4) is
  not reachable yet — there is no tree to show until a session starts (`[spec §4.1]`); `lastViewedMemo`
  itself is unreadable pre-decryption regardless (§1.1 above).
- **Session start** (successful password entry) for that tab: this is the moment `lastViewedMemo` first
  becomes readable in this run. Applying `restoreTabView` at this moment too — even though it is not
  literally "switching to a tab" as `[spec v1.1.0 §2.3]`'s trigger list phrases it (the tab was already
  active before the password prompt) — is treated as the direct, intended completion of
  `[spec v1.1.0 §2.2]`'s own stated reason for storing this field inside the encrypted payload in the
  first place ("even the fact of which memo name was last viewed should not be readable without the
  password" only makes sense as a *readable-once-decrypted* field; there would be no reason to persist it
  at all if it were never meant to be restored once the password is given). If the user wants session
  start to instead always begin with nothing selected (e.g. as an extra deliberate step before revealing
  which memo was viewed last), that would need to be raised as an explicit change to
  `[spec v1.1.0 §2.3]` — not assumed here.

**Write side**: covered by §2.2 above — `lastViewedMemo` updates in-memory whenever the open
memo/tab changes, and is persisted at the existing save-trigger cadence, no new trigger category
(`[spec v1.1.0 §2.3]`).

### 3.3 Cross-Tab Quick Memo-Name Search ("Quick Open") — Elaboration

Elaborates `[spec v1.1.0 §3]`. Backend operation in §2.3 above.

- **Overlay component**: built from the shared custom dialog component (`detailed-spec.md` §6.5) — text
  input (autofocused on open) plus a result list below it, Esc-closable (`[spec v1.1.0 §3.5]`).
- **Trigger/shortcut**: default `Ctrl+Shift+P`, registered as an `AppSettings.shortcuts` entry
  (`[spec v1.1.0 §3.1]`); rebindable via the Settings → ショートカット tab
  (`docs/RESOLVED-ISSUES.md` #6, cited the same way `[spec v1.1.0 §5.4]` already cites it for that tab's
  existence).
- **Matching**: live-recomputed on every keystroke; substring match against `memoName` across the whole
  index (`[spec v1.1.0 §3.2]`). Case-sensitivity is not specified by `spec.v1.1.0.md`; **starting default:
  case-insensitive**, consistent with `detailed-spec.md` §5.1's own precedent of defaulting an unspecified
  name-matching case-sensitivity question the same way.
- **Empty-query behavior** (query string is empty/just opened): not specified by `[spec v1.1.0 §3.2]`
  whether this shows zero results or a full MRU-ordered browse list of every match candidate —
  implementation discretion (§5), not escalated, since either satisfies "live-filtered result list."
- **Ordering**: primary key — a result whose memo is currently its own tab's `lastViewedMemo` sorts first
  (value `0`) ahead of all others (value `1`); secondary key (tie-break) — alphabetical by display name.
  This single two-level sort implements `[spec v1.1.0 §3.2]`'s ordering rule exactly.
- **Result cap**: capped and scrollable; exact count is implementation discretion per
  `[spec v1.1.0 §3.2]`'s own "cosmetic minutiae" callout (§5).
- **Keyboard interaction inside the overlay**: Up/Down moves the highlighted result, Enter opens the
  highlighted result (or the top result if none is highlighted yet), Esc dismisses without acting
  (`[spec v1.1.0 §3.5]`); mouse click on a row also selects it. This completes "keyboard-driven overlay...
  live-filtered result list" (`[spec v1.1.0 §3.1]`) at the level of concrete key bindings — not itself
  spec-mandated wording, so treated as elaboration/starting default (§5), same as any other
  interaction-completion elsewhere in `detailed-spec.md` (e.g. its own §5.2 arrow-key-nav elaboration).
- **Selecting a result**: switches to the result's owning tab (invoking `restoreTabView`'s tab-switch path
  is *not* used here — the target memo is explicit, not derived from `lastViewedMemo`) and opens that
  specific memo directly, same end effect as manually clicking the tab then the tree row
  (`[spec v1.1.0 §3.5]`). This also updates that tab's `lastViewedMemo` per §2.2 above, same as any other
  way of opening a memo.

### 3.4 Cross-Tab Favorites — Elaboration

Elaborates `[spec v1.1.0 §4]`. Data model in §1.1 above; backend operations in §2.2/§2.3 above.

- **Toggle UI**: right-click context-menu entry, label flips "お気に入りに追加" / "お気に入りから削除"
  (`[spec v1.1.0 §4.2]`) — same two-state label-flip and same single-selection-only precondition already
  used for the Edit-lock menu item (`detailed-spec.md` §5.2), applied here verbatim as the concrete UI
  pattern to copy.
- **Cross-tab list view**: same two-column shape (display name + owning tab name) as Quick Open's result
  list (`[spec v1.1.0 §4.3]`). Whether this view literally reuses the same overlay component as Quick Open
  (same component, different data source and no live-typed query — a static `favorite === true` filter
  instead) or is built as a visually-matching but separate component is not mandated by
  `spec.v1.1.0.md` — only the two-column *shape* is spec-level; component reuse is implementation
  discretion (§5).
- **Ordering**: identical two-level sort to §3.3's Quick Open ordering, reused verbatim
  (`[spec v1.1.0 §4.3]`).
- **Trigger**: toolbar ★ button, always visible, no default shortcut (`[spec v1.1.0 §4.6]`); still
  bindable by the user via `AppSettings.shortcuts` like any action, which already supports an entry with
  no default key (absence of a key in the `Record<string, string>` means "unbound," consistent with how
  any other not-yet-bound action in that map already behaves).
- **Tab Lock exclusion**: automatic via the shared index (§1.3 above), no new logic
  (`[spec v1.1.0 §4.4]`).
- **Selecting an entry**: identical jump-to-tab-and-open behavior as Quick Open's own "Selecting a
  Result" (§3.3 above), reused verbatim (`[spec v1.1.0 §4.5]`).

### 3.5 Tab-Lock Auto-Relock on Inactivity — Elaboration

Elaborates `[spec v1.1.0 §5]`. Backend trigger in §2.4 above.

- **Timer lifecycle**: the single global inactivity timer is created the moment a Tab Lock session
  starts (`detailed-spec.md` §5.9, "Unlocking (starting a session)," step 3) and destroyed the moment that
  session ends, by whatever path (leaving the tab, permanent-unlock completion, delete-after-unlock, or
  now also inactivity-timeout or app-focus-loss, §2.4 above) — no timer exists while no session is active,
  consistent with `[spec v1.1.0 §5.1]`'s "at most one locked session can ever be alive at a time."
- **Reset ("activity") condition**: a single listener at the app-window level for keyboard/pointer events
  (candidate concrete event set: `keydown`, `pointerdown`, `pointermove` — exact enumeration is
  implementation discretion, §5, since `[spec v1.1.0 §5.2]` only says "any keyboard or pointer input")
  updates `lastActivityTimestamp` whenever it fires **and** a session is currently active **and** the
  timer is not currently paused (below). A continuous action like an internal-DND drag (which fires
  repeated `pointermove` events) naturally keeps resetting the timer for its whole duration, with no
  special-casing needed.
- **Pause condition**: the timer pauses (stops counting down, does not reset) while any modal-equivalent
  surface is open and awaiting input, and resumes from where it left off when the surface closes
  (`[spec v1.1.0 §5.2]`). Concretely, "modal-equivalent surface" is: any dialog built from the shared
  custom dialog component (`detailed-spec.md` §6.5) — which per that section already covers essentially
  every prompt/confirm/notice in the app, including §3.1's collision dialog (the case `[spec v1.1.0 §5.2]`
  itself names) — plus, as an elaboration extension not explicitly named by `spec.v1.1.0.md` but following
  the same "awaiting input, blocks the rest of the app" characteristic, the full Settings screen while
  open. Because this app shows only one tab's content at a time and a locked session can only be active
  while its own tab is the active tab, any such surface that could be open is necessarily in the context
  of that same tab — there is no cross-tab modal scenario to account for separately. A single shared
  open-count (not a plain boolean, so a still-open outer dialog keeps pausing even if an inner one closes
  first — relevant only if the app ever allows dialog stacking at all) gates the pause; exact mechanism is
  `developer-lite`'s call (§5).
- **On-timeout teardown**: fires the existing "Session end" routine (`detailed-spec.md` §5.9) verbatim,
  flush-then-teardown, no new teardown logic (`[spec v1.1.0 §5.3]`). If the flush step itself surfaces a
  conflict-resolution dialog (`detailed-spec.md` §6.4), that is no different from how any other trigger
  into "leaving the tab" teardown could already stall on a conflict dialog — not new to this feature, and
  irrelevant to the (already-fired, no-longer-counting-down) inactivity timer itself.
- **App-focus-loss trigger**: a Tauri window-focus-lost event (`WindowEvent::Focused(false)` or the
  frontend-side equivalent), while a session is active, immediately invokes the same teardown — not
  gated by, or interacting with, the inactivity timer/pause state at all; it is an independent,
  instantaneous second trigger into the same existing "leaving the tab" routine
  (`[spec v1.1.0 §5.2]`, broadening `[spec §4.1 pt.5]`).
- **Settings UI**: `tabLockAutoRelockMinutes` dropdown in the 一般 tab, choices 5 / 15 / 30 / 60 /
  無効 (`[spec v1.1.0 §5.4]`); persisted via the existing generic `AppSettingsPatch` deep-partial-merge
  mechanism (`detailed-spec.md` §3.2) — no new settings-persistence plumbing required. Exact row styling
  within the 一般 tab is implementation discretion (§5), per `[spec v1.1.0 §5.4]`'s own note.

---

## 4. Cross-Cutting Additions

### 4.1 `schemaVersion` / Migration Handling

Elaborates `[spec v1.1.0 §2.2]`'s consolidated-bump notes (also referenced from `[spec v1.1.0 §4.1]` and
`[spec v1.1.0 §5.4]`).

- `TabMeta.schemaVersion` increments once, covering both `lastViewedMemo` (§1.1) and `MetaTreeNode.favorite`
  (§1.1) together. `AppSettings.schemaVersion` increments once, covering both the `lastOpened` narrowing
  and `tabLockAutoRelockMinutes` (§1.2) together. Neither is split into two increments — no real
  intermediate file state with only one of each pair's fields will ever exist, since both ship in the same
  release (`[spec v1.1.0 §2.2]`).
- **No destructive migration logic is required for either bump.** Every new field is optional/nullable
  with a well-defined "absent" meaning (§1.1, §1.2 above); a pre-upgrade file simply reads with those
  fields absent, and the app's own writes from that point on include them. The `schemaVersion` bump exists
  for record-keeping (`[spec §3.2]`, "Record `schemaVersion` + `appVersion`"), not because a transform
  step is needed to make an old file readable.
- The concrete new integer value each `schemaVersion` bumps **to** depends on whatever value is currently
  shipping in the v1.0.0 codebase at the time v1.1.0 is implemented (i.e. "current value + 1"); this
  document does not hardcode a specific number, consistent with `detailed-spec.md` §3.2's own example
  values being illustrative rather than binding.

### 4.2 Error Handling Additions

Extends the table in `detailed-spec.md` §7. **No new `ErrorCode` values are introduced by any of the five
v1.1.0 features.** All error-shaped situations reuse an existing code:

| Situation | Reused code | Handling |
|---|---|---|
| Import: sanitization needed on a dropped filename | (internally reuses `NAME_INVALID`-class validation) | Auto-applied replacement, never surfaced to the user (§2.1 step 3a; `[spec v1.1.0 §1.2]`) |
| Import: collision on a dropped filename | (internally reuses `NAME_DUPLICATE`-class detection) | In-app collision dialog, not a blocking error (§2.1 step 3e; `[spec v1.1.0 §1.3]`) |
| Import: source file locked/unreadable at read time | `IO_LOCKED`-class retry-then-skip | Retried per `detailed-spec.md` §3.7, then skipped and tallied, batch continues (§2.1 step 6) |
| Tab-Lock: auto-relock's pre-teardown flush hits a save conflict | `CONFLICT` | Existing conflict-resolution dialog (`detailed-spec.md` §6.4), unrelated to the (already-fired) inactivity timer |

---

## 5. Deferred-to-Implementation Details (v1.1.0) — Not Ambiguities, No User Decision Needed

Mirrors the role of `detailed-spec.md` §9, scoped to the five v1.1.0 features.

- Order in which multiple simultaneously-dropped files are processed (§2.1; `[spec v1.1.0 §1.4]`, spec's
  own explicit discretion callout).
- Whether import's metadata-JSON save is coalesced once per batch or written once per resolved item
  (§2.1 step 5).
- Exact copy, format, duration, and dismissal behavior of the import skip-summary toast (§2.1).
- Exact drag-over visual affordance shown while an OS file hovers a tree row/background before drop
  (§3.1).
- Whether Quick Open's empty-query state shows zero results or a full MRU-ordered browse list (§3.3).
- Quick Open / Favorites result-list cap count (§3.3; `[spec v1.1.0 §3.2]`, spec's own explicit discretion
  callout).
- Case-sensitivity of Quick Open's substring match (§3.3 proposes case-insensitive as the starting
  default).
- Whether the Favorites cross-tab list view reuses Quick Open's overlay component or is a visually-matching
  separate component (§3.4).
- Exact enumeration of DOM/window event types counted as "activity" for the inactivity timer, and whether
  a boolean or counter is used to track the open-modal pause state (§3.5).
- Exact row styling of the `tabLockAutoRelockMinutes` dropdown within the Settings 一般 tab (§3.5;
  `[spec v1.1.0 §5.4]`, spec's own explicit discretion callout).
- Whether the Settings UI writes `null` or `0` when the user selects "無効" for `tabLockAutoRelockMinutes`
  (§1.2) — both are valid per the type, spec does not distinguish them.
- The concrete new integer value `TabMeta.schemaVersion`/`AppSettings.schemaVersion` bump to (§4.1).

---

## 6. Open Questions Requiring User Decision

None identified during this elaboration pass. Every point in §3 that required judgment beyond a direct
translation of `spec.v1.1.0.md` was either (a) resolved as a non-arbitrary completion of rationale
`spec.v1.1.0.md` already states, flagged inline where it occurs (§3.2's session-start restore is the one
example), or (b) listed in §5 as implementation discretion consistent with `spec.v1.1.0.md`'s own explicit
"implementation discretion" callouts. Any newly identified genuine spec-level ambiguity should be appended
here and to `QA.md`, following the same convention `detailed-spec.md` §10 already documents.
