# TabTreeNoteMD v1.1.0 Specification Additions (spec.v1.1.0.md)

> This is the first file in a new convention: **features added after the v1.0.0 baseline get their
> own version-suffixed spec file** instead of being appended into `spec.md`. `spec.md` stays frozen as
> the v1.0.0 baseline and is never edited for post-v1.0.0 feature work. This file plays the same role
> for v1.1.0 that `spec.md` plays for the baseline — it is the authoritative "what is decided" record
> for the v1.1.0-only additions/changes below, consumed the same way by `designer-lite` (when writing
> `docs/detailed-spec.v1.1.0.md`, a future file, not created by this pass) and downstream by
> `developer-lite` (`docs/plans/<feature-name>/PLAN.md`). Every item below that builds on or modifies
> existing baseline behavior cites the relevant `spec.md` section as `[spec §x]`; read `spec.md` first,
> this file assumes familiarity with it and does not repeat baseline behavior it doesn't change.
> Decision rationale, comparative analysis, and rejected alternatives for the choices below belong in
> `DECISIONS.md`, not here (same split as `spec.md`/`DECISIONS.md`). As of 2026-08-15.

---

## Index

Keep this in sync whenever a section is added, renamed, or moved below.

- **§0 Scope** — relationship to `spec.md` and to the version-suffixed-spec-file convention this file starts.
- **§1 OS-Folder → Tree-Pane Drag-and-Drop Import (New)** — dragging `.md` files from Windows Explorer onto an already-open tab's tree pane to import them as memos.
  - §1.1 Trigger, Accepted Sources, Copy-Only Semantics
  - §1.2 File-Type Filtering and Rejection Behavior
  - §1.3 Filename Collision Resolution (In-App Prompt)
  - §1.4 Drop-Target Placement in the Tree
  - §1.5 Cross-Cutting Tie-Ins (atomic write, self-write registry, index update, Tab Lock)
- **§2 Per-Tab Last-Viewed-Memo and Scroll-Position Memory (New)** — extends startup auto-open to also apply per-tab on every in-session tab switch.
  - §2.1 Current Gap
  - §2.2 Data Model Changes
  - §2.3 Restore Behavior (Tab Switch and Startup)
- **§3 Cross-Tab Quick Memo-Name Search ("Quick Open") (New)** — keyboard-driven overlay to jump-open a
  memo by filename across all tabs, distinct from existing in-memo/cross-tab content search.
  - §3.1 Trigger and Relationship to Existing Content Search
  - §3.2 Matching and Result List
  - §3.3 Backing Data Source (Reuses Existing Index)
  - §3.4 Tab Lock Exclusion
  - §3.5 Selecting a Result and Dismissal
- **§4 Cross-Tab Favorites (New)** — manually-curated, persistent shortlist of memos
  viewable across all tabs at once, distinct from §3's search-driven, ephemeral Quick Open.
  - §4.1 Data Model
  - §4.2 Toggle UI
  - §4.3 Cross-Tab List View
  - §4.4 Tab Lock Exclusion
  - §4.5 Selecting an Entry
  - §4.6 Trigger to Open the List
- **§5 Tab-Lock Auto-Relock on Inactivity (New)** — a single global inactivity timer that auto-discards
  an active Tab Lock session even without leaving the tab, plus a Settings UI to configure the timeout.
  - §5.1 Scope of the Timer
  - §5.2 Activity Definition and Relationship to `[spec §4.1 pt.5]`
  - §5.3 On-Timeout Behavior
  - §5.4 Configurable Timeout — New Settings UI
- **Open Questions for User**

---

## 0. Scope

- `spec.md` remains the frozen v1.0.0 baseline; it is not edited by this file or by any future
  version-suffixed spec file. This file (`spec.v1.1.0.md`) records only what changed or was added for
  v1.1.0. A later `v1.2.0` (etc.) feature set gets its own `spec.v1.2.0.md`, following the same pattern,
  each citing back to `spec.md` and to earlier version files as needed.
- The five features below are additive to `spec.md`, in particular §2.1/§2.2 (tab and tree management),
  §2.4 (search), §2.6 (persistence/startup), §2.7 (window/operation), §3.4 (type definitions), §4.1 (Tab
  Lock), §8 (Post-M10 fixes), and §9 (Settings screen layout), among other sections cited inline
  throughout (e.g. §2.8, §3.1, §3.2, §5.2, §5.4, §5.8, §5.11, §7) — this list is representative, not
  exhaustive; none of the additions replace or contradict baseline behavior except where explicitly
  noted as a modification (§2.2 below, `AppSettings.lastOpened`; §5.2 below, broadening
  `[spec §4.1 pt.5]`'s "leaving the tab" trigger to also include OS-level app-focus loss).
- `docs/README.md`'s File Structure list documents this convention; see that file for how
  `spec.v1.1.0.md` relates to `detailed-spec.md`, `DECISIONS.md`, `QA.md`, and `plans/`.

---

## 1. OS-Folder → Tree-Pane Drag-and-Drop Import (New)

A third drag source for the tree pane, distinct from the two already specified in the baseline:
internal DND within/between tabs (`[spec §2.2]`) and OS-folder drag-registration onto the tab bar to
create a new tab (`[spec §2.1]`, "drag-register existing folders"). This feature is: dragging one or
more `.md` files from Windows Explorer directly onto the **tree pane of an already-open tab**, to
import them as memos into that tab.

### 1.1 Trigger, Accepted Sources, Copy-Only Semantics

- Trigger: OS-level file drag from Explorer (or any other application that produces a standard OS file
  drag payload) released over the tree pane of the currently active tab. This is unrelated to the
  tab-bar-level "drag a folder to register a new tab" gesture in `[spec §2.1]` — that gesture targets
  the tab bar and registers a new tab; this feature targets the tree pane of a tab that is already
  open and registered.
- **Copy only**: source files are read and their content duplicated into the target tab; the original
  files at the drag source are never modified, moved, or deleted. This is a deliberate, explicit
  requirement (not a default subject to later change) — there is no "import and delete source" mode.
- **Folders are not accepted as drop payload items**: only individual `.md` files dropped directly are
  processed. A dropped folder is treated the same as any other non-`.md` payload item (§1.2) — silently
  skipped, not recursed into. This follows directly from the baseline's explicit exclusion of recursive
  subdirectory scanning (`[spec §2.2]`, "Scan only `.md` files directly under the folder... subdirectories
  excluded"; `[spec §10]`, "recursive subdirectory scan... " listed as out of scope) — accepting a
  dropped folder and walking its contents would be exactly the recursive-scan behavior the baseline
  already declined to support, so it is declined here too for consistency.

### 1.2 File-Type Filtering and Rejection Behavior

- **Only `.md` files are imported.** Any other item in the same drop payload (non-`.md` file, folder,
  shortcut, etc.) is **silently skipped at the individual-item level** — no error dialog, no blocking
  per-item prompt. (A summary-level notification still fires for the drop as a whole — see
  "Skip-summary notification" below; that is distinct from a per-item interruption and does not block
  anything.)
  - Precedent: the baseline already treats "not a direct-child `.md` file" as something the tree
    scanner simply ignores rather than surfaces as an error (`[spec §2.2]`, tree scan targets `.md`
    files only; `[spec §3.1]`, "only direct-child `.md` files targeted"). The app's general pattern for
    non-actionable input elsewhere is also silent no-op, not an error dialog (`[spec §2.8]`, unresolved
    `memo:` links "do nothing"; `[spec §4.2]`, unmatched `#slug` anchor is "a silent no-op"). Rejecting
    a non-`.md` drop item the same way is consistent with both precedents.
  - If the entire drop payload contains zero `.md` files, no memo is imported — but this is not a fully
    silent no-op like the zero-payload-item case might suggest: every item in the payload was skipped,
    so the skip-summary toast below still fires (e.g. "0 imported, 3 skipped (not .md)"), reporting that
    nothing was importable rather than giving no feedback at all.
  - This is a **partial-accept** policy for mixed selections: valid `.md` files in the drop are still
    imported even if other items in the same drop are skipped.
- **Skip-summary notification**: if the drop resulted in **at least one skipped item** — a non-`.md`
  payload item filtered out here, or a file skipped via §1.3's collision dialog (explicit Skip choice,
  "apply to all: skip", or Esc-cancelling the remaining batch) — the app shows a single, lightweight,
  non-blocking toast/notification after the whole drop operation finishes, summarizing counts, e.g.
  "2 imported, 1 skipped (not .md)" or "3 imported, 1 skipped (collision)". A drop with **zero** skips
  (every dropped `.md` file imported cleanly, no non-`.md` items present in the payload, no collisions
  encountered) stays **completely silent** — no toast. This rule is shared between this section and
  §1.3 (collision-driven skips); it is stated once here and cross-referenced from §1.3.
- Source filenames become the new memos' display names (`[spec §3.1]`, "memo identification = filename
  ... Display name and filename always identical"), extension stripped, subject to the same forbidden-
  character/reserved-name sanitization as any other name (`[spec §3.1]`, `[spec §5.11]`). Since this is
  a non-interactive drop (no per-file rename step), an invalid-character replacement suggestion is
  **auto-applied** rather than shown as a blocking prompt (adapting `[spec §5.11]`'s "real-time
  validation... replacement suggestions" to a context with no interactive text field to validate
  against) — this case is expected to be rare, since a dragged file already has a valid Windows
  filename in nearly all cases.

### 1.3 Filename Collision Resolution (In-App Prompt)

- If a memo with the same (post-sanitization) filename already exists in the target tab, the app
  **asks the user** how to resolve it, via an **in-app, non-native, Esc-closable dialog**
  (`[spec §7]`, "Do not use native blocking `window.confirm()`..."; "Dialogs escapable by Esc key
  (except external change conflict dialog...)" — this dialog is not that exemption, so it must be
  Esc-closable). This is a **deliberate divergence** from the baseline's "creation" convention of
  silently auto-numbering past a duplicate name (`name` → `name(1)` → `name(2)`, `[spec §5.8]`,
  `[spec §5.11]`, GitHub issue #97): for this feature, a prompt is wanted instead of silent
  auto-numbering.
- Offered choices, per colliding filename:
  - **Rename** — import under the next free auto-numbered name (`name(1)`, `name(2)`, ...), i.e. the
    baseline auto-number behavior, now offered as an explicit opt-in choice rather than the silent
    default.
  - **Overwrite** — replace the existing memo's body with the dropped file's content (same target
    filename, same tree position/metadata retained — only the body changes).
  - **Skip** — do not import this file; leave the existing memo untouched.
  - **Apply to all remaining conflicts in this drop** (checkbox/toggle alongside the three choices) —
    avoids re-prompting once per colliding file in a multi-file drop; applies the chosen action to every
    subsequent collision in the same drop operation, not to future unrelated drops.
- Pressing **Esc** cancels the entire remaining import batch (every file not yet resolved is treated as
  skipped; files already imported before the collision prompt appeared are not rolled back).
- While the collision dialog is open, no other tree operation on the target tab is accepted (same
  input-lock-during-resolution pattern already used for the external-edit-conflict dialog,
  `[spec §5.8]`).
- **Skip notification**: every explicit **Skip** choice, every file skipped via "apply to all: skip",
  and every file left unresolved by an **Esc**-cancel counts toward the skip tally in the summary toast
  described in §1.2 ("Skip-summary notification"). The toast fires once, after the whole drop operation
  (including any collision resolution) finishes — not once per collision — and is suppressed entirely
  if nothing was skipped (e.g. every collision was resolved via Rename or Overwrite).

### 1.4 Drop-Target Placement in the Tree

Decided by direct analogy with the two placement outcomes internal DND already supports
(`[spec §2.2]`, "Reorder by DND, change hierarchy") and with the two explicit default-position rules
already given for memo creation (`[spec §2.2]`, "Create Memo" → "always append to end"; "Create as
Child" → append as last child):

- **Dropped on empty tree background (no node under the cursor)**: imported memo(s) land at the
  **root level, appended to the end** of the tab's top-level list — same default position as
  right-click "Create Memo".
- **Dropped while hovering an existing tree node**: imported memo(s) land as **children of that node,
  appended to the end of its existing children** — same default position as right-click "Create as
  Child". (There is no separate "folder-like node" type in this app's tree — any memo node can have
  children — so "hovering a node" always means "reparent under this memo," mirroring how internal DND
  reparenting already works against the same node type.)
- Exact hover-zone pixel geometry (e.g. distinguishing a "reorder as sibling before/after" zone from a
  "reparent as child" zone within a row, if internal DND's implementation makes that distinction) reuses
  whatever internal DND already implements for the same visual affordance — this is an implementation
  detail, not a new spec-level rule, since the drag source (OS file vs. internal tree node) doesn't
  change what a given drop position on a row means.
- Order of multiple simultaneously-dropped files: appended in whatever order the OS reports the drop
  payload — implementation discretion (mirrors the general pattern of "spec doesn't mandate a specific
  answer for cosmetic ordering minutiae," `detailed-spec.md` §9).

### 1.5 Cross-Cutting Tie-Ins

- Each successfully imported memo is written via the same atomic-write path as any other memo body
  save (`[spec §5.2]`), registered in the self-write reservation table before the write so the file
  watcher doesn't misreport it as an external change (`[spec §5.4]`, "self-write reservation table"),
  and immediately patched into the full-text search index the same way an in-app save/create already is
  (`[spec §2.4]`, "in-app changes... update index directly on save completion").
- **Target tab is locked, no active session**: not a reachable state for this feature — a locked
  tab with no session hides its tree entirely (`[spec §4.1]`), so there is no tree-pane surface to drop
  onto in the first place (same reasoning as `detailed-spec.md` §5.9's "nothing to drag from it" for the
  DND case, applied here to dropping *onto* rather than dragging *from*).
- **Target tab is locked, active unlock session**: the drop proceeds exactly like a drop onto a normal
  unlocked tab — the imported plaintext memo body is immediately encrypted into the session's in-memory
  state and re-saved into the lock blob, filename obfuscated per `[spec §4.1 pt.1]` (mirrors the
  existing "dropping onto a locked tab that already has an active session... proceeds exactly like
  dropping onto a normal unlocked tab," from the cross-tab-DND rule in `detailed-spec.md` §5.9, applied
  here by analogy since this is structurally the same "content arriving into an active session" event,
  just from an OS drag instead of another tab's tree).

---

## 2. Per-Tab Last-Viewed-Memo and Scroll-Position Memory (New)

### 2.1 Current Gap

- The baseline restores the single most-recently-viewed memo **at app startup only**, via
  `AppSettings.lastOpened: { tabId: string; memoName: string } | null` (`[spec §2.6]`, "On startup
  automatically open the last-opened memo"; `[spec §3.4]` type definition). This is a single,
  app-level (not per-tab) pointer.
- **Gap**: switching tabs during a single running session does not restore that tab's own
  last-viewed memo — this behavior is not specified anywhere in `spec.md` or `detailed-spec.md`, and
  is therefore an **implementation-observed gap**, not a documented default to preserve. (Per-tab tree
  state that *is* already specified and already works correctly per-tab needs no change here: expand/
  collapse state and labels are already stored per-node inside each tab's own metadata,
  `[spec §3.2]`/`[spec §3.4]` `MetaTreeNode.expanded`, and are unaffected by this feature.)

### 2.2 Data Model Changes

- **New per-tab field, `lastViewedMemo: string | null`**, added to each tab's own metadata
  (`[spec §3.2]`, `.tabtreenote.json`) — for an **unlocked tab**, as a new top-level field sibling to
  `tree` in the `TabMeta` shape (`[spec §3.4]`); for a **locked tab**, inside the encrypted lock payload
  (the `realTabName`/`tree`/`fileMap` structure described in `detailed-spec.md` §3.4) rather than as a
  plaintext field outside the ciphertext — this follows the same reasoning already applied to
  `realTabName` and the real tree structure in that payload (`[spec §4.1]`): even the fact of *which*
  memo name was last viewed should not be readable without the password.
  - This is a schema change to `TabMeta` and bumps `TabMeta.schemaVersion` per the existing convention
    (`[spec §3.2]`, "Record `schemaVersion` + `appVersion`"). §4.1 below also adds a new `TabMeta`-nested
    field (`MetaTreeNode.favorite`); since both additions ship together in the same v1.1.0 release, they
    are covered by a **single** `TabMeta.schemaVersion` bump, not two separate increments — there is no
    real intermediate save-file state where only one of the two fields exists, so no migration logic
    needs to handle that non-occurring case. See §4.1 for the cross-reference.
- **`AppSettings.lastOpened` is narrowed from `{ tabId: string; memoName: string } | null` to
  `{ tabId: string } | null`** (**modifies** `[spec §3.4]`'s `AppSettings` type) — the `memoName` field
  becomes redundant once every tab tracks its own `lastViewedMemo`, and keeping both risks the two
  falling out of sync (e.g. a global pointer that disagrees with the tab's own record of what was last
  open in it). Startup auto-open now works as: open the tab named by `AppSettings.lastOpened.tabId`,
  then within that tab, apply the same restore logic §2.3 describes for an ordinary in-session tab
  switch — i.e. startup restore and mid-session tab-switch restore become the same code path, driven by
  the per-tab field, rather than two divergent mechanisms.
  - This is a schema change to the App Settings JSON and bumps `AppSettings.schemaVersion` per the
    existing convention (`[spec §3.2]`, "Record `schemaVersion` + `appVersion`"). §5.4 below also adds a
    new `AppSettings` field (`tabLockAutoRelockMinutes`); since both changes ship together in the same
    v1.1.0 release, they are covered by a **single** `AppSettings.schemaVersion` bump, not two separate
    increments — there is no real intermediate save-file state where only one of the two changes is
    present, so no migration logic needs to handle that non-occurring case. See §5.4 for the
    cross-reference. No destructive migration is needed: a pre-upgrade `lastOpened.memoName` value is
    simply not read going forward (worst case on the first post-upgrade launch, the target tab opens with
    no memo pre-selected, exactly like any tab whose own `lastViewedMemo` is still `null`; normal per-tab
    tracking takes over immediately after).
- **"Location" = scroll position only, reusing the existing field** — cursor/selection-position
  restoration is explicitly **not** added by this feature. This reuses `MemoEditorSettings.scrollPosition`
  exactly as already specified (`detailed-spec.md` §3.3: an integer, 1-indexed document line number,
  not a raw pixel offset, already persisted per-memo and already confirmed to survive app restarts per
  `DECISIONS.md`, "Scroll Position: Document-Position-Based Anchor..."; "UI-State Persistence
  Decisions"). That field already exists and is already per-memo (hence implicitly per-tab, since a
  memo belongs to exactly one tab) — the only change this feature makes is **wiring it into the
  tab-switch code path**, which today only reads/writes it on memo-switch-within-a-tab, not on
  switching tabs. No new field is introduced for scroll position. Cursor/selection position has no
  existing field or persistence mechanism anywhere in the baseline, and adding one is out of scope here
  — restoring the exact cursor/selection on tab switch is left for a future feature if wanted.

### 2.3 Restore Behavior (Tab Switch and Startup)

- **Update trigger**: `lastViewedMemo` (and, transitively, that memo's own `scrollPosition`) is written
  at the same trigger cadence already used for autosave (`[spec §2.6]`: tab switch, memo switch,
  minimize, exit, periodic autosave debounce) — no new save-trigger category is introduced; this is
  additional state saved alongside a write that was already going to happen at those points.
- **On switching to a tab** (mid-session, or as the startup-selected tab per §2.2): if
  `lastViewedMemo` names a memo that still exists in that tab's tree after reconciliation
  (reconciliation rules, `detailed-spec.md` §3.5), open it and restore its
  `scrollPosition`. If the named memo no longer exists (deleted, renamed away, or reconciliation
  dropped it), **do nothing** — no error, no fallback memo auto-selected — reusing the exact rule
  already specified for the startup case (`[spec §2.6]`, "If tab/memo is deleted, do nothing";
  `detailed-spec.md` §5.6, "if the referenced memo no longer exists after §3.5 reconciliation, do
  nothing").
- **Tree pane visual restoration**: whenever `lastViewedMemo` is restored per the point above, the tree
  pane itself also scrolls so that memo's node is visible, and that node becomes the tree's
  selected/highlighted row — the same end visual state as if the user had just clicked that row
  manually. This is a **derived UI action, not a new persisted field**: unlike the memo's own
  `scrollPosition` (which must be persisted because it cannot be recovered from anything else), "scroll
  the tree to reveal node X and select it" is fully computable at restore time from the tree structure
  itself plus the `lastViewedMemo` name already being restored — no additional schema field is
  introduced for this.
- **Tab never viewed yet** (freshly created/registered, `lastViewedMemo` still `null`/absent):
  switching to it shows no memo pre-selected, same as today's baseline default for an empty/unopened
  tab — no new default is introduced for this case.

---

## 3. Cross-Tab Quick Memo-Name Search ("Quick Open") (New)

A fast, keyboard-driven alternative to the existing two-step "pick a tab, then find and click the memo
in that tab's tree" navigation path. As tab/memo count grows, this avoids having to visually scan a
tree every time to open a known memo.

### 3.1 Trigger and Relationship to Existing Content Search

- **Trigger**: a shortcut opens a small overlay containing a text input with a live-filtered result list
  displayed below it.
- **Default shortcut: `Ctrl+O`.** Verified against the actual current default keymap
  (`src/shared/shortcuts.ts`'s `DEFAULT_SHORTCUTS`), not just `docs/plans/window-operation/PLAN.md`'s
  original table — the table alone misses bindings added by later plans, e.g. `markdown.paneModeCycle`'s
  `Ctrl+Shift+P` (`docs/plans/markdown/markdown-preview/PLAN.md` §6.3) and `markdown.pdfExport`'s `Ctrl+P`
  (`docs/plans/markdown/pdf-export/PLAN.md` §4), both added after that table was written. Not `Ctrl+P` —
  taken by PDF export, also colliding with WebView2/Chromium's native browser-print shortcut
  (`docs/MDEditor-spec.md`, "`Ctrl+P` native-print conflict"). Not `Ctrl+K` — taken by link insert. Not
  `Ctrl+Shift+P` — already bound to `markdown.paneModeCycle` (the 3-state edit/split/preview pane-mode
  cycle), an already-implemented/shipped v1.0.0 default this feature does not reassign, to avoid disturbing
  existing users' muscle memory. `Ctrl+O` is unused in `DEFAULT_SHORTCUTS` as of this writing, and is a
  reasonably mnemonic choice ("Open") for a feature whose purpose is to jump-open a memo. Like every other
  shortcut in this app, `Ctrl+O` is a **default, not hardcoded** — it is recorded in and overridable via
  `AppSettings.shortcuts: Record<string, string>` (`[spec §3.4]`), the same configurable-shortcut mechanism
  cited for window/operation shortcuts generally (`[spec §2.7]`).
- **Distinct from existing content search**: this is a different search surface from the in-memo/
  cross-tab-index **content** search already specified (`[spec §2.4]`), which searches memo *body text*.
  This feature searches memo *filenames/display-names* only, across all tabs, purely to jump-open a
  memo — it does not search or match against body content at all. The two are not alternate entry
  points into the same feature; they serve different purposes (content search vs. navigation/quick-open),
  the same distinction VS Code draws between "Quick Open" (`Ctrl+P`, matches filenames) and "Find in
  Files" (matches content). This feature reuses `[spec §2.4]`'s existing infrastructure as its data
  source (§3.3 below), but the UI surface, trigger, and matching target are new and separate from the
  content-search dialog.

### 3.2 Matching and Result List

- **Matching**: incremental/live filter as the user types, matched against memo display names
  (`[spec §3.1]`, "Display name and filename always identical") across **all tabs**, not just the active
  one. **Match style: substring match** — `[spec §2.4]`/`detailed-spec.md` §5.4 do not specify a
  fuzzy/subsequence matching style for the app's existing content search either (only regex support as
  an explicit opt-in mode for that separate feature), so this feature adopts plain substring match as
  the simple, consistent default; the query need not match at the start of the name.
  - **Result list contents**: each candidate row shows the memo's display name and its owning tab's
    name. This is necessary because matches span tabs, and while tab names must be unique
    (`[spec §2.1]`), the spec places no uniqueness constraint on memo names across different tabs
    (`[spec §2.1]` only states tab names must be unique; `[spec §3.1]`'s "no duplicate names" rule is
    scoped to "same namespace," i.e. within one tab) — so the same memo display name could legitimately
    appear under two different tabs, and the tab name disambiguates which one a given row refers to.
  - **Result count / scrolling**: capped and scrollable if numerous. Spec does not mandate an exact cap
    number — implementation discretion, the same "cosmetic minutiae" pattern already used elsewhere in
    this file (§1.4, "Order of multiple simultaneously-dropped files... implementation discretion").
- **Ordering**: results are ordered by recency of last-viewed, most-recent first, reusing §2's per-tab
  `lastViewedMemo` field (§2.2) as the recency signal — no new tracking mechanism (e.g. a full per-memo
  last-viewed timestamp history) is introduced for this feature. Concretely: a matching memo that is
  currently its own tab's `lastViewedMemo` sorts ahead of matching memos that are not. Because
  `lastViewedMemo` is a single value per tab rather than a full history, it identifies at most one
  "most recent" memo per tab, not a complete recency ranking across every memo — so for both (a) the
  remainder of the results (matches that are not currently any tab's `lastViewedMemo`) and (b) breaking
  ties among multiple matches that are each currently their own tab's `lastViewedMemo` (there is no
  cross-tab timestamp to say which of those was viewed more recently than another), the tie-break is
  **alphabetical by display name** — chosen as the simplest, most predictable default, for the same
  reason plain substring match (not fuzzy matching) was chosen in "Matching" above: there is no existing
  baseline convention this feature needs to diverge from for ordering.

### 3.3 Backing Data Source (Reuses Existing Index)

- This feature **reuses the existing cross-tab full-text search index** (`[spec §2.4]`, "cross-tab
  full-text index"; `detailed-spec.md` §5.4) as its sole data source. That index already enumerates
  `{tabId, memoName, plaintext}` for every memo across every (unlocked-eligible) tab
  (`detailed-spec.md` §5.4, "Index build (startup)"); this feature filters that same candidate set by
  the `memoName` field instead of by the `plaintext` field.
- **No new index or data structure is introduced.** This is a new UI surface (the Quick Open overlay)
  layered over already-indexed data, not a separate name-indexing mechanism — it is kept in sync by the
  exact same "in-app change patches index directly / external change via file monitoring" update paths
  already specified for the full-text index (`[spec §2.4]`, "Two paths for index differential update").

### 3.4 Tab Lock Exclusion

- Locked tabs are excluded from Quick Open results under the **same rule the full-text index already
  applies to itself, applied verbatim**: "Whether locked or in unlock session, that tab's name and body
  are always excluded from full-text search index" (`[spec §2.4]`; `[spec §4.1 pt.6]`;
  `detailed-spec.md` §5.4, "Locked tabs never contribute to the index, whether locked or in an active
  unlock session... with no exception for the session case"). Since this feature's candidate set is
  §3.3's index unmodified, this exclusion falls out automatically — no separate Tab-Lock-awareness logic
  is introduced for Quick Open specifically.

### 3.5 Selecting a Result and Dismissal

- **Selecting a result**: switches to that memo's owning tab (if it is not already the active tab) and
  opens the memo — the same end effect as manually clicking the tab and then the tree row. This is a
  shortcut to an existing navigation outcome; it introduces no new open/switch behavior beyond what
  clicking already does.
- **Dismissal**: pressing **Esc** closes the overlay without acting (no tab switch, no memo opened),
  consistent with the app's general Esc-cancels convention (`[spec §7]`, "Dialogs escapable by Esc key
  (except external change conflict dialog...)").

---

## 4. Cross-Tab Favorites (New)

Distinct from §3's Quick Open (search-driven, ephemeral — type a query, jump, overlay closes): this
feature is a manually-curated, persistent shortlist of specific memos the user returns to often,
viewable across all tabs at once without typing anything.

### 4.1 Data Model

- **New per-memo flag, `favorite?: boolean`**, added to `MetaTreeNode` (`[spec §3.4]`) — following the
  **exact same existing pattern** as the already-specified `locked?`/`label?` fields on that same type
  (`[spec §3.4]`; row-affordance precedent for those fields, `[spec §2.2]`). Mirrored onto the
  runtime-only `TreeNode` type the same way `locked?`/`label?` already are (`[spec §3.4]`), since the UI
  needs to read it at render time.
  - `MetaTreeNode` lives inside `TabMeta` (`[spec §3.4]`), so this is a `TabMeta` schema change. Per
    §2.2's cross-reference above (`TabMeta.lastViewedMemo`), it is covered by the **same single**
    `TabMeta.schemaVersion` bump as that field, not a separate increment — both additions ship together
    in v1.1.0, so no intermediate save-file state with only one of the two fields ever needs migration
    logic.
- **No new Tab-Lock handling needs to be invented.** For a locked tab, the entire tree structure —
  including every `MetaTreeNode` field — already lives inside the encrypted `lock` blob (`[spec §4.1]`
  pt.2; `detailed-spec.md` §3.4, the `realTabName`/`tree`/`fileMap` structure). A locked tab's `favorite`
  flags are therefore automatically protected the same way `locked?`/`label?` already are, with zero new
  mechanism.
  - **Contrast with §2**: unlike §2.2's `lastViewedMemo`, which needed its own fresh placement decision
    (plaintext `TabMeta` field vs. inside the lock blob) because it was a *new top-level field* sibling
    to `tree`, `favorite` is not a new top-level field — it rides along inside the existing per-node tree
    structure that is already either fully plaintext (unlocked tab) or fully inside the blob (locked
    tab), so no analogous placement decision is needed here.

### 4.2 Toggle UI

- Toggled via a **right-click context menu entry** ("お気に入りに追加" / "お気に入りから削除", label
  flips with current state) — the same treatment already given to the Edit Lock toggle (GitHub issue
  #64, `docs/RESOLVED-ISSUES.md`) — rather than an always-visible row icon. Chosen over a persistent star
  icon because the tree row's icon/affordance area is already occupied by the label-colored `FileIcon`
  and the bold-red edit-lock name styling (`docs/RESOLVED-ISSUES.md` #7, which replaced an earlier
  label-color-dot affordance for exactly this reason); adding another always-on glyph there risks
  crowding an area already redesigned once to reduce clutter. A context-menu entry keeps the same
  **interaction cost as toggling Edit Lock** (one right-click, one menu item), per direct instruction.

### 4.3 Cross-Tab List View

- A new panel/overlay lists every favorited memo across all tabs. **Same two-column result shape as
  §3.2's Quick Open results** (this file, §3.2): memo display name + owning tab name, for the same
  reason given there — memo names are not guaranteed unique across different tabs (`[spec §2.1]`,
  `[spec §3.1]`).
- **Reuses the existing cross-tab full-text search index; adds no new indexing/data structure** — the
  same reuse approach as §3.3 of this file ("Backing Data Source (Reuses Existing Index)"). The index's
  per-memo entries (`{tabId, memoName, plaintext}`, `detailed-spec.md` §5.4) are extended with a
  `favorite: boolean` field, kept in sync by the exact same "in-app change patches index directly /
  external change via file monitoring" update paths already specified (`[spec §2.4]`). This view is only
  a differently-filtered read over that same index (`favorite === true`, instead of §3's name-substring
  match or the baseline's content match) — no separate favorites-tracking data structure is introduced.
- **Ordering**: identical rule to §3.2 of this file ("Ordering") — reused verbatim, not re-derived:
  favorites matching their tab's current `lastViewedMemo` rank first; tie-break alphabetically by
  display name, identical to §3.2's rule.

### 4.4 Tab Lock Exclusion

- Identical rule to §3.4 of this file / `[spec §2.4]` — applied **verbatim**, no new logic: a locked
  tab's memos (locked, or in an active unlock session with no exception for the session case) never
  appear in the index, so they never appear in the favorites list either, for the same reason they never
  appear in Quick Open results.

### 4.5 Selecting an Entry

- Same jump-to-tab-and-open behavior already specified in §3.5 of this file ("Selecting a Result and
  Dismissal") — reused verbatim, not re-described here.

### 4.6 Trigger to Open the List

- A **toolbar button** (e.g. a ★ icon) in the top toolbar, mirroring the existing 📌 always-on-top
  toggle button's own treatment (`[spec §2.1]`, "sits in the top toolbar immediately to the left of the
  search button") — a single always-visible click target, no default keyboard shortcut assigned. This
  avoids picking a new default combo in an already-crowded shortcut space (`Ctrl+P` taken by PDF export,
  `Ctrl+K` taken by link insert, `Ctrl+Shift+P` taken by `markdown.paneModeCycle`, `Ctrl+O` taken by Quick
  Open per this file §3.1) without ruling one out later: like every
  other action in this app, a shortcut could still be bound to it via `AppSettings.shortcuts`
  (`[spec §3.4]`) if the user configures one themselves, even though no default is preassigned.

---

## 5. Tab-Lock Auto-Relock on Inactivity (New)

Currently a Tab Lock session only ends when the user leaves the tab (`[spec §4.1 pt.5]`, "Immediately
discarded when leaving tab"). Gap: if the user stays on the same locked/session tab but walks away from
the keyboard, the session stays decrypted/open indefinitely. This feature adds an inactivity timeout
that auto-discards the session even without leaving the tab, plus a Settings UI to configure the
timeout duration.

### 5.1 Scope of the Timer

- **Scope: the foreground active tab only.** This feature adds a **single global inactivity timer, not
  one per tab** — it tracks and can discard only whichever Tab-Lock session (if any) belongs to the tab
  that is currently active, resetting on activity within that tab (§5.2) and tearing that one session down
  on timeout (§5.3). This is what keeps the feature's scope small: no per-tab/per-session timer
  bookkeeping, no need to reconcile multiple simultaneous timers.
- **Deliberately does not cover backgrounded sessions.** Tab Lock's actual multi-session model allows more
  than one session to be alive at once — e.g. a cross-tab drag-and-drop onto a locked tab unlocks that tab
  in place (password entered at drop time) without the drag-source tab ever losing focus, so the
  dropped-onto tab's session can stay alive in the background, never itself becoming the active tab, until
  the user eventually visits and leaves it or the app exits. This feature's single global timer does not
  reach that backgrounded session; it watches only the foreground active tab. This is a deliberate, accepted
  scope boundary decided with the user, not an oversight: the backgrounded-session case is a pre-existing
  condition of Tab Lock's multi-session model, unrelated to and not newly introduced by this feature, and it
  is knowingly left unaddressed here to keep this feature's scope small (see `DECISIONS.md`). A backgrounded
  session remains exactly as exposed after this feature ships as it was before it.

### 5.2 Activity Definition and Relationship to `[spec §4.1 pt.5]`

- **Resets the timer**: any keyboard or pointer input within the app window while a locked-session tab
  is the active tab.
- **Pauses while a modal dialog is open**: the inactivity timer **pauses** (stops counting down, does not
  reset) whenever any modal dialog belonging to this app is open and awaiting user input, and **resumes**
  counting from where it left off once the dialog closes. This is a general rule, not limited to one
  dialog — the concrete case that surfaced it is §1.3's filename-collision-resolution dialog, which can
  legitimately stay open for a while awaiting the user's Rename/Overwrite/Skip choice and, per §1.3,
  already blocks other tree operations on the target tab while open, but does not itself touch keyboard/
  mouse activity-tracking. Without this rule, a user mid-decision on that dialog (or any other modal
  dialog) could be auto-relocked out from under an in-progress, unrelated interaction.
- **Leaving the tab already ends the session regardless of this timer** (`[spec §4.1 pt.5]`), via
  switching to another tab. **This feature additionally treats OS-level app-focus loss (the app window
  losing focus to another application) as also counting as "leaving the tab"** for pt.5's purposes — a
  small, explicit broadening of pt.5's trigger condition (pt.5 as written names only "select another
  tab"), decided here rather than left ambiguous. Chosen as the more conservative of the two options: a
  backgrounded window with a live decrypted session is exactly the "walked away from the keyboard" risk
  this whole feature exists to close, and losing focus to a different application is a stronger, more
  immediate signal than mere keyboard/pointer silence — it should not have to wait for even the shortest
  configured timeout. This reuses pt.5's teardown outright (§5.3 below); it is only a second trigger into
  the same existing rule, not a new rule.

### 5.3 On-Timeout Behavior

Reuses existing mechanisms verbatim; no new teardown logic is introduced.

- **Flush first**: any pending unsaved edit is flushed exactly like the existing "auto-save-confirm
  before lock" rule already requires (`[spec §8]`, "If memo target/descendants being edited during tab
  lock, auto-save-confirm → close before proceeding to lock").
- **Teardown**: the actual session discard then reuses the exact same routine already specified for
  "leaving the tab" (`[spec §4.1 pt.5]`). This feature is only a **new trigger** (the inactivity timer,
  or the app-focus-loss case in §5.2) into that existing teardown — not new teardown logic.

### 5.4 Configurable Timeout — New Settings UI

- **New `AppSettings` field**: `tabLockAutoRelockMinutes: number | null` (`null` — or `0` — means
  disabled/never). Schema addition to `AppSettings`. Per §2.2's cross-reference above
  (`AppSettings.lastOpened` narrowing), it is covered by the **same single** `AppSettings.schemaVersion`
  bump as that change, not a separate increment — both changes ship together in v1.1.0, so no
  intermediate save-file state with only one of the two changes ever needs migration logic. See §2.2 for
  the underlying convention citation (`[spec §3.2]`, "Record `schemaVersion` + `appVersion`").
- **Location: the 一般 (General) tab** of the Settings screen, per direct user instruction. The Settings
  screen's general/editor split is originally specified at `[spec §9]` ("Settings screen: checkbox/
  dropdown vertical margins, general/editor 2-tab layout, Esc support"); per `docs/RESOLVED-ISSUES.md`
  #6, Shortcuts was since split out into its own tab, so the screen is now three tabs
  (一般/ショートカット/エディタ) — the new row goes in 一般.
- **Control type**: a **dropdown with preset durations** — 5 / 15 / 30 / 60 minutes, plus a "無効"
  (never/disabled) option — rather than freeform number entry. Neither `detailed-spec.md` nor
  `docs/plans/window-operation/PLAN.md` documents an explicit row-style convention for the 一般 tab, so
  this is the simple default called for; freeform entry is avoidable complexity (input validation, unit
  ambiguity) for a setting with only a handful of sensible values. Adjustable by `developer-lite` during
  implementation planning if a better-established row-style precedent turns up there.
- **Default value: 15 minutes** — a conservative, security-reasonable default (in line with §5.2's
  conservative call on app-focus loss), consistent with common inactivity-lock defaults elsewhere, rather
  than defaulting to disabled.

---

## Open Questions for User

None.
