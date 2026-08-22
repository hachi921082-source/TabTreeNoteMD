# TabTreeNoteMD Decision History and Rationale

> `spec.md` follows the principle of recording only "what is currently decided" concisely, so detailed decision history, comparative studies, and rejected alternatives are consolidated here. This does not need to be read ordinarily; **refer to it only when you want to reconsider a decision**.

---

## Scroll Position: Document-Position-Based Anchor, Not Raw Pixel Offset (2026-08-01)

**Decision**: `MemoEditorSettings.scrollPosition` is a 1-indexed document line number (the line visible
at the top of the viewport), not a raw pixel/`scrollTop` value. Elaborated in `detailed-spec.md`
§3.3/§5.3, `plans/editor-save/PLAN.md` (save/restore mechanism, clamping).

## Code-Fence Awareness in Markdown Scanning (2026-08-01)

**Decision**: The Table of Contents heading scanner (`table-of-contents/PLAN.md`) and the full-text
search plaintext converter (`search-index/PLAN.md` §3) must recognize fenced code blocks via a real
fence-boundary scan, not a uniform line-by-line marker regex. Elaborated in `detailed-spec.md`
§5.4/§5.10.

## Overlapping Text-Styling Decorations: Implement as Composable Mark Decorations (2026-08-01)

**Decision**: Color-span styling and link click-affordance rendering (e.g. a `memo:` link inside a
`<span style="color:...">`-decorated range) must both be implemented as CodeMirror "mark" decorations,
not "widget" decorations. Added as required test coverage (item 5) in `markdown-editing/PLAN.md` §9,
elaborated in `detailed-spec.md` §5.10.

## Image `src` Allowlist: Reject `..` Path-Traversal Segments (2026-08-01)

**Decision**: The sanitization allowlist's `<img src>` rule additionally rejects any value containing a
`..` path segment. Elaborated in `detailed-spec.md` §5.10, `markdown-editing/PLAN.md` §5.

## Markdown Toolbar: Finalized 16-Slot Fixed Grid Layout (2026-08-01)

**Decision**: Toolbar is a fixed 8 columns × 2 rows = 16-slot grid, no blanks. Supersedes the earlier
6×2/12-slot and 6×2/12-with-gear concepts in `detailed-spec.md` §5.10 and `docs/plans/markdown-editing/PLAN.md` §4.
Bold and Italic are two separate actions (the earlier "combined single button" idea is dropped). Icon set:
Lucide (MIT), bundled locally, no CDN. Full slot layout, icon mapping, and the new rearrange-mode
mechanism (16th slot toggles reorder-by-drag mode; OFF = normal click-to-invoke; ON = drag-only, click
does nothing, Esc or re-click exits, whole-toolbar tint indicates the mode) are recorded in
`detailed-spec.md` §5.10. New persisted field `AppSettings.window.toolbarOrder` (16-element action-ID
array) added alongside the already-decided `toolbarPosition` field (see "UI-State Persistence
Decisions" below, unaffected by this change).

**Addendum (2026-08-01) — Color/Font-size button palettes and default shortcuts confirmed**: The
toolbar's Color button (`<span style="color:...">`, distinct from `label.color` in §3.3, which keeps
its own separate 5-choice palette) and Font-size button (`<span style="font-size:...">`) each get a
concrete closed palette adopted verbatim from the legacy app's `MarkdownToolbar.tsx`: Color — Red
`#e53935`, Blue `#1e88e5`, Green `#43a047`, Orange `#fb8c00`, Purple `#8e24aa`, plus a separate "clear
color" control; Font size — Small `12px`, Large `20px`, Extra-large `28px`, plus a separate "clear
size" control. Additionally, all 16 toolbar actions get a default keyboard shortcut registered in the
existing `AppSettings.shortcuts` system (no new mechanism); exact key combos are left to
`developer-lite`'s implementation discretion, same treatment as other deferred items in
`detailed-spec.md` §9. Full values and elaboration in `detailed-spec.md` §5.10.

**Superseded (2026-08-15)**: the 16-slot count, 8×2 grid, and 16-element `toolbarOrder` array described
above are historical — see "Markdown Toolbar: Expanded to 18-Slot Grid" below for current values
(18/9×2/18-element). Icon set, rearrange-mode mechanism, Color/Font-size palettes, and default-shortcut
policy in this entry are otherwise unchanged.

## Markdown Toolbar: Expanded to 18-Slot Grid; "Insert Hard Line Break" and "Clear Formatting" Actions Added (GitHub issue #176, 2026-08-15)

**Decision**: Grid grows from 8 columns × 2 rows (16 slots) to **9 columns × 2 rows = 18 slots**, no
blanks — continuing the "grow columns, keep 2 rows" pattern already used once before (6×2/12-slot →
8×2/16-slot, entry above). The extra column holds two new actions, one per row: **Insert hard line
break** (`hardBreak`, icon `CornerDownLeft`) and **Clear formatting** (`clearFormatting`, icon
`Eraser`). `rearrangeToggle` shifts from slot 16 to slot 18, remaining the fixed last slot; the other 15
pre-existing action IDs keep their prior row/column positions unchanged. `AppSettings.window.toolbarOrder`
becomes an 18-element array. Full grid table, icon mapping, and both new actions' exact
insertion/strip behavior — including Clear Formatting's content-vs-decoration scope (links and images
are preserved, not stripped) — are in `detailed-spec.md` §5.10.

**Rejected**: 6 columns × 3 rows. Matches the pre-16-slot column count but grows the floating panel's
height instead of width; no user preference was expressed for taller-vs-wider, and matching the
established "grow by column" pattern from the prior expansion was judged the lower-risk, more
consistent choice.

## New Feature: Table of Contents (TOC) (2026-08-01)

**Decision**: Added to v1 scope. Triggered only by the toolbar's "Update" button (manual, one-shot,
never auto/background). Generates a nested Markdown list of slugified heading links in a
marker-delimited block, inserted or replaced in place on click. Adds a new same-document
heading-anchor link type (`#slug`) to the Link domain. Fully elaborated in `spec.md` §4.2/§2.8 and
`detailed-spec.md` §5.10/§4.6/§5.8.

## Technology Stack: Adopting Tauri (2026-07-30)

**Decision**: Tauri (Rust backend + OS standard WebView2 frontend). Changed from the previous v1's Electron+TypeScript.

- **Goal = Reducing Size**: The previous Electron version had a distribution size of approximately 72–108 MB and memory usage of 100–300 MB. Since Tauri does not have its own Chromium and rides on WebView2, the application itself is a few MB to tens of MB, with significantly reduced memory usage. Startup delay caused by self-extraction of portable exe is also expected to be eliminated.
- **Securing Verification Means as Top Priority**: Since users lack programming knowledge and cannot verify through code review, "automatic testing by Claude itself" becomes the key safety net. Both Electron and Tauri are WebView (Chromium-based) and support automatic verification via CDP/WebDriver (the previous project's tester role has a track record of actually verifying via CDP).
- **Reasons for Rejecting Fully Native GUI (C#: WPF/WinUI3)**: The corresponding automatic UI verification tools (FlaUI/WinAppDriver, etc.) are not as mature as CDP and there is no track record. Given that users cannot read code, a verification method with weak tools is a high-risk choice. Additionally, the cost of recreating the decoration mechanism for Markdown live preview is significant.
- **Tauri preserves renderer assets**: The renderer (HTML/CSS/JS) including the decoration mechanism can be carried over largely as is.
- **Rejected Alternatives**: C# fully native (reasons above) / C#+WebView2 hybrid (dual IPC configuration between native and WebView parts becomes complex) / Electron continuation (contradicts the goal of reducing size).
- **Known Remaining Costs**: Rust has heavy compilation and cannot iterate as fast as TypeScript. A new automatic test environment for Tauri needs to be built.

## Editor Engine for Markdown Editing: CodeMirror 6 (2026-07-31)

**Decision**: **CodeMirror 6 + a custom-built decoration extension**, operating directly on the plain-text
buffer as source of truth, for the Markdown live-view editor (`spec.md` §4.2). Rejected: Milkdown/TipTap
(ProseMirror-based rich-document-model WYSIWYG libraries), Slate, and Draft.js. This resolves the open
question originally recorded as `QA.md` item #1.

- **Source-of-truth safety**: CodeMirror 6's buffer is the plain text itself — there is no parse/serialize
  round-trip on save. A ProseMirror-based library (Milkdown/TipTap) holds an internal document tree;
  saving means *regenerating* Markdown text from that tree via a serializer, not writing back the user's
  original characters with edits applied in place.
- **Custom syntax risk**: This app has several non-standard Markdown extensions with no native schema
  support in Milkdown/TipTap: the `memo:` link scheme (`[text](memo:TabName/MemoName)`), `<img src width
  height>` resize notation, and `<span style="color;font-size">` decoration notation (all from `spec.md`
  §4.2). Using a ProseMirror-based library would require hand-building both a custom parser (text→tree)
  AND a custom serializer (tree→text) for every one of these. Any gap in that custom work causes *silent*
  content mutation on save (reformatting, or literal loss of unsupported syntax) — a data-integrity risk,
  which is a more severe failure class for a note-taking app than a UI bug.
- **CodeMirror's known risk is a different, lower-severity class**: the legacy project's flagged defect
  area ("overlapping range edits, decoration widget click select" — cursor/selection mapping issues when
  decorations and hidden syntax markers occupy different visual space than their underlying text length,
  `spec.md` §8) is a rendering/interaction bug. It does not corrupt the saved file (the buffer is still
  correct plain text underneath); it is testable and fixable through careful decoration/click-handling
  implementation, unlike the silent-mutation risk above.
- **Frequent live/source switching**: The user wants to switch between live-view and raw-Markdown-source
  display frequently, not just occasionally, and additionally wanted a settings-level default for which
  mode a memo opens in (`spec.md` §3.4 `viewMode`). With CodeMirror 6, both modes operate on the exact
  same text buffer — switching is just toggling the decoration layer on/off, with zero conversion and
  zero risk. With a ProseMirror-based library, a "raw source" view would need either a second synced
  CodeMirror instance (re-running the tree↔text conversion, and its risks, on every single toggle rather
  than just at save time) or a library-specific source-mode plugin. Since the user wants to toggle often,
  this would mean hitting the round-trip risk far more frequently than just at save time.
- **Rejected Alternatives**:
  - **Milkdown / TipTap**: the only realistic representatives of "ready-made Markdown WYSIWYG library"
    considered, because they are the only ones that ship Markdown-aware parsing/serialization and
    toolbar infrastructure out of the box. Rejected for the reasons above (points 1–4).
  - **Slate**: a lower-level framework like ProseMirror, with no built-in Markdown import/export or
    toolbar. Choosing it would mean paying the same "build it yourself" cost as CodeMirror while *still*
    carrying the document-tree serialization risk described above — the worst of both approaches
    combined.
  - **Draft.js**: effectively unmaintained (Meta stopped active development years ago); not viable for a
    new project.

## Tab Deletion Semantics: Unregister-Only, No Folder-Trash Option (2026-08-01)

**Decision**: Removing a tab from `AppSettings.tabs` never touches, deletes, or trashes the underlying
folder or any files in it. It only unregisters the folder from the app's tab list (`spec.md` §2.1,
§4.1 pt.7). There is no "delete tab and folder" action anywhere in the app.

- **What was considered**: The legacy app (`TabTreeNote`, reference-only) offered two separate
  right-click actions on a tab: "remove tab, keep folder" (unregister-only) and "delete tab and
  folder" (unregister + send the folder to the OS trash).
- **What was rejected, and why**: Offering a folder-trash option at the tab level was rejected
  entirely, not just deprioritized.
  - **Data-loss risk disproportionate to convenience**: a tab is a pointer to an arbitrary,
    possibly pre-existing folder (`[spec §2.1]`, "drag-register existing folders") that may contain
    files the app doesn't manage or even know about (non-`.md` files, subfolders — `[spec §2.2]`
    excludes subdirectories from scanning but they can still exist on disk). Bulk-trashing an entire
    folder tree on a single tab-removal action risks destroying content the user did not intend to
    delete, for a convenience (skipping a manual Explorer delete) that doesn't justify the risk.
  - **Simplicity**: a folder-trash option would require designing the `TRASH_FAILED`-retry/fallback
    edge case (`[spec §5.8]`) at the tab level, in addition to the one that already exists for
    memo-level delete (`[spec §2.2]`). Dropping the option avoids that second edge-case surface
    entirely, leaving exactly one unambiguous action ("remove tab" = unregister) instead of two
    similarly-named, easily-confused actions.
- **What this does not affect**: memo-level deletion (`[spec §2.2]`) is unchanged and still goes to
  the OS trash by default, with the existing `TRASH_FAILED` → user-consent → direct-delete fallback
  (`[spec §5.8]`). Only the *tab* level dropped the trash option; individual memo files within a tab
  are still deletable (to trash) as before.
- **User-facing consequence**: to actually delete a registered folder's contents, the user must do so
  outside the app (e.g. via Windows Explorer) after removing the tab, or before/without ever
  registering it.

## Tab Lock: Crash-Mid-Lock Recovery via Explicit Marker + Resume/Rollback (2026-08-01)

**Decision**: **Option B** from `QA.md` item Q1 ("Tab Lock: crash window mid-locking leaves orphaned
encrypted files invisible to recovery"). Locking a tab (`detailed-spec.md` §5.9 "Locking a tab") now
writes an explicit in-progress marker (e.g. a sibling `.tabtreenote.lock-inprogress` file; exact format
left to `developer-lite`) *before* the per-file rename/encrypt loop begins. The app checks for this
marker whenever the tab's folder is next accessed and, if found, deterministically either (a) finishes
the incomplete lock by re-running the remaining renames, or (b) rolls it back by renaming the
already-token-named files back to their original names — so a crash or kill between the start of the
rename loop and the final lock-blob write (`detailed-spec.md` §5.9 "Locking a tab") never leaves memo
files as permanently unreadable, unmapped token blobs.

- **Why the gap was worth closing, despite the added cost**: Tab Lock exists specifically to protect
  data the user has deliberately flagged as important or private (`spec.md` §4.1). The general
  "memo body (`.md`) is never lost" guarantee (`spec.md` §3.2) does not cover this crash window (its
  reconciliation/flat-rebuild scan only looks at `.md` files, so a partially-locked tab's token files
  are invisible to it — `QA.md` Q1 background) — leaving it as an accepted risk would mean the one
  guarantee users rely on most is silently weaker for exactly the subset of data they most want
  protected. The user chose to close this gap entirely rather than document it as accepted risk.
- **Cost judged acceptable**: the recovery check is a single extra file-existence check at tab-folder
  access (startup or that specific tab's open, exact trigger point left to `detailed-spec.md`/
  `PLAN.md`), not a scan whose cost scales with memo count — so it does not meaningfully work against
  the app's general lightweight-performance goal (`spec.md` §6), even though it does add new design
  and engineering surface (a new marker file/format, a new recovery flow, and resume/rollback logic)
  beyond what was originally scoped for the lock operation.
- **Rejected Alternative — Option A (accept as risk, optionally reorder writes)**: Leaving this as an
  accepted, out-of-scope risk (parallel to the already-accepted `spec.md` §5.3 sync-order crash gap)
  was rejected because — unlike that general gap, which only risks a tree-position edit needing to be
  redone — this one risks memo bodies becoming permanently inaccessible, for the tab class the user
  cares most about protecting. The milder variant of Option A (write the lock blob's `fileMap` *before*
  the rename loop, so token names are anticipated rather than confirmed after the fact) was also
  rejected: it avoids needing a real recovery mechanism, but leaves the lock blob and the actual
  on-disk token names free to disagree after a mid-rename crash, which would require `developer-lite`
  to design an equally complex fallback-matching heuristic ("if the fileMap-listed token file is
  missing, check whether a `.md` file with the original name still exists") without the deterministic
  before/after certainty that an explicit progress marker gives for free.
- **Where elaborated**: `detailed-spec.md` §5.9 "Locking a tab" (marker-write step inserted before the
  rename loop) and a new "Recovery on tab-folder access" subsection (resume/rollback logic). `QA.md`
  item Q1 is resolved and removed from that file per its own resolved-item convention; this entry is
  its permanent record.

## Tab Lock: Permanent Unlock Operation Added (2026-08-08)

**Decision**: `spec.md` §4.1's former statement that "permanent unlock operations do not apply" (it
only ever meant nested/partial locks don't apply — a tab is fully locked or fully unlocked, never a
mix, which is unchanged) is superseded: a whole-tab **permanent unlock** is now a supported, explicit,
irreversible operation (`spec.md` §4.1 pt.9, `docs/tab-lock-detailed-spec.md` "Permanently unlocking a tab").
It fully decrypts a locked tab's memos, renames obfuscated token files back to their real names, and
replaces the tab's `lock` metadata with a plain `tree`, distinct from — and not a substitute for —
ending a session (which remains temporary and automatic on leaving the tab, unchanged).

- **Two entry points, both kept**: from within an active unlock session (reuses the session's already-
  verified key, no repeat password prompt) and directly from the locked/no-session state (its own
  password prompt, structurally the same decrypt-and-verify `tab_unlock` already performs). Both were
  kept rather than picking one, since they serve different real situations (already reviewing the tab's
  contents vs. deciding to strip protection without first opening a session) at no extra design cost —
  they converge on the same underlying operation.
- **Crash safety reuses the existing lock-direction marker design, mirrored**: a minimal in-progress
  marker (existence + format version only) plus per-entry on-disk state classification and a
  Finish/Restore-Lock choice, symmetric to `docs/plans/tab-lock/CRASH-RECOVERY.md`. The new marker
  deliberately does **not** duplicate the `fileMap`/`kdf` the lock-direction marker carries: the
  still-present `lock` blob (untouched until the very last file-system step) already is that
  authoritative record, and copying real memo names into a second plaintext marker file would
  needlessly re-expose already-hidden names during any crash window, unlike the lock-direction marker's
  `fileMap` (never secret, since the real filenames are already visible on disk when that marker is
  written).
- **Confirmation is a UI-level requirement, not just the password gate**: since the outcome is
  irreversible and cannot be reversed by re-locking with the same guarantees (the discarded key means a
  later lock only protects data going forward), an explicit confirmation step is required at both entry
  points, mirroring the existing confirm-before-destructive-action pattern for trash failure -> direct
  delete (`spec.md` §5.8).
- **Full-text search re-indexes immediately** rather than waiting for the next app restart, matching the
  in-app "patch the index directly" pattern already used for ordinary content changes
  (`search-index/PLAN.md` §1) — a deliberate contrast with the separately-accepted, unrelated gap where
  a newly-registered tab's pre-existing files aren't indexed until restart (`search-index/PLAN.md`'s own
  status note); that gap is not extended to this feature.
- **Where elaborated**: `spec.md` §4.1 pt.7 (revised) and pt.9 (new); `docs/tab-lock-detailed-spec.md`'s
  "Permanently unlocking a tab" and its "Recovery on tab-folder access (crash mid-permanent-unlock)"
  subsection (split out of `detailed-spec.md` §5.9). The actual `CRASH-RECOVERY.md`-equivalent document
  (exact marker filename/JSON shape, new Tauri command names, frontend dialog wiring) is left to
  `developer-lite`'s `PLAN.md` for this feature.

**Follow-up review (2026-08-08, same day)** closed several gaps found by symmetry with `tab_lock`'s own
existing behavior, plus three user decisions:
- By symmetry with `tab_lock` (no user input needed): the flow now includes `tab_lock`'s own
  open-unsaved-memo defensive check; an explicit requirement that the frontend serialize/block all other
  operations on the tab for the operation's duration (the same pre-existing exposure `tab_lock` itself
  has, now stated rather than left implicit); `.bak` companion cleanup when deleting a restored token
  file (same residue class as the already-fixed GitHub issue #18); a one-line "errors out if the tab
  isn't locked" precondition (mirrors `tab_unlock`); and an explicit statement that a pending
  permanent-unlock marker gets the same (i.e., no special) tab-bar treatment as a pending lock marker.
- **User decisions**: the confirmation dialog is a simple OK/Cancel with warning copy (not typed
  tab-name confirmation, not a checkbox) and must include a line recommending a folder-copy backup first
  (`spec.md` §6's existing "folder copy = backup" property); a same-named plaintext file unexpectedly
  already present at restore time is treated as an anomalous entry (aborted with an error, untouched) —
  reusing crash-recovery's existing `EntryState::Anomalous` framing, now stated as applying to the
  ordinary (non-crash) restore loop too, not only to crash recovery.

## UI-State Persistence Decisions: Scroll Position, Toolbar Position, Search History (2026-08-01)

Three previously-unaddressed or hedged "does this survive app restart?" questions were resolved
together during an ad-hoc review conversation, with different outcomes for each:

- **Per-memo scroll position (`detailed-spec.md` §5.3) — now persisted across app restart.**
  Previously phrased as session-only by default with restart-persistence merely "not prohibited."
  The user decided this should be a firm requirement: scroll position is stored per-memo in
  `MemoEditorSettings` (part of the tab's `.tabtreenote.json` tree metadata, `[spec §3.4]`), the
  same place other per-memo editor overrides (wrap, tab width, view mode, etc.) already live, so no
  new storage location was introduced — just a new field in an existing structure.
- **Markdown toolbar drag position (`detailed-spec.md` §5.10) — now persisted across app restart.**
  This was entirely unaddressed in `detailed-spec.md`; `developer-lite` had independently chosen
  session-only at the `PLAN.md` level (`docs/plans/markdown-editing/PLAN.md` §4) in the absence of
  spec guidance. The user decided it should persist. Placement: `AppSettings.window` (a top-level
  window UI field, not per-memo) rather than `MemoEditorSettings`, because the toolbar is a
  per-window UI element — its position is not tied to which memo is open, unlike scroll position,
  which is inherently per-memo (each memo has its own natural scroll range).
- **Search/replace history (`detailed-spec.md` §5.4) — confirmed session-only, finalized.**
  This was already `detailed-spec.md`'s "starting default" with an explicit escape hatch letting
  `developer-lite` persist it instead if convenient. The user confirmed the session-only default is
  final; the escape hatch is removed since there is no longer any ambiguity to resolve at
  implementation time.

**Why the split outcome (two persisted, one not)**: no single blanket rule was applied — each was a
distinct product-feel judgment by the user (scroll position and toolbar position are workspace-layout
state worth restoring on every launch; search/replace history is considered transient working-context
that's fine to lose between sessions), not a technical constraint.

**Where elaborated**: `detailed-spec.md` §3.2 (new `AppSettings.window.toolbarPosition` field in the
schema example), §3.3 (new `MemoEditorSettings.scrollPosition` field in the schema example), §5.3
(scroll-position bullet restated as decided), §5.4 (search-history bullet restated as decided), §5.10
(toolbar bullet gains a persistence sentence).

## Force-Delete Shortcut Key: Confirmed as `Ctrl+Delete` (2026-08-01)

**Decision**: `spec.md` §9.1 item 3's `Ctrl+Delete` — originally presented purely as an illustrative
example ("exact key confirmed at implementation") — is now the final, confirmed key binding for the
dedicated tree force-delete shortcut. No alternative binding was seriously considered during this
review; the user simply confirmed the existing example as-is rather than leaving it open.

**Where elaborated**: `detailed-spec.md` §5.2 (Memo Tree elaboration) states the binding directly
instead of citing it as an example; §9's deferred-to-implementation list no longer includes this item
(moved out since it is no longer a `developer-lite`-discretion item).

## Tab-Lock Placeholder Display Name: 🔒 Glyph Alone (2026-08-01)

**Decision**: the placeholder text shown in `AppSettings.tabs[i].name` and the tab bar for a locked
tab (`[spec §4.1]`, "tab name itself hidden ... shown as 🔒") is the 🔒 glyph by itself, with no
accompanying text (e.g. no "(locked)" or similar). `detailed-spec.md` §3.2 previously called this
"a UI-copy decision for implementation, not a spec concern" and left it fully open; the user decided
to pin it down rather than leave it to `developer-lite`'s taste, since it is a small, easily-decided
question and pinning it avoids inconsistent phrasing surfacing later (e.g. in error messages or
tooltips that also need to reference the locked tab's placeholder name).

**Where elaborated**: `detailed-spec.md` §3.2's placeholder-name bullet, restated as decided.

**Superseded 2026-08-22 — see "Locked Tab Distinguishing Label" below.** With several locked tabs at
once (GitHub issue #202), the bare glyph alone made every locked tab visually identical; the plain
"no text" default is now overridden by an optional, user-settable per-tab label. This entry is kept
for history.

## Locked Tab Distinguishing Label: User-Settable, Editable Anytime via Context Menu (2026-08-22)

**Decision**: supersedes the "🔒 Glyph Alone" decision above. Multiple locked tabs at once (GitHub
issue #202, reported after several test-fixture tabs ended up locked with no way to tell them apart
without unlocking each one) made the bare-glyph-only default impractical in practice. Rather than an
automatic, generated identifier, the label is entirely user-authored:

- A new tab right-click context-menu item (e.g. "ロック名を設定"/"Set lock label") opens a
  single-text-input prompt (reusing the existing `PromptDialog` pattern already used identically for
  lock/unlock/permanent-unlock in `TabBar.tsx`) to set or change the label, at any time — not only at
  lock-setup time.
- New persisted field on the tab record (e.g. `AppSettings.tabs[i].lockLabel: string | null`), keyed
  to the tab's internal `id` — stable regardless of the underlying folder's real name/path, and
  unaffected if that folder is later renamed (see "separately noted" gap recorded on issue #202 about
  external folder renames in general, which is unrelated to this).
- Until the user sets one, display falls back to the original bare 🔒 — this is an opt-in enhancement,
  not a new mandatory default for every locked tab.
- Must still never reference or hint at the real tab/folder name — the label is the user's own free
  text, entirely under their control (same privacy intent as the original decision).

**Rejected alternative — auto-generated sequential label ("LockTabA"/"LockTabB"/...)**: considered
first (assigned in lock order, permanently fixed per tab, never reused even after a tab's deletion —
to avoid a stale letter later being reassigned to an unrelated tab — continuing past "Z" via a
spreadsheet-column-style `AA`/`AB`/... scheme for effectively unlimited capacity). Rejected because
permanently skipping a deleted tab's letter (e.g. no tab ever being "B" again after the original "B"
is removed) was judged visually/conceptually unpleasant, and a generated letter carries no actual
memory hook to the tab's real content the way a user's own chosen label would.

**Where elaborated**: `spec.v1.2.0.md` §10 (user-facing decided behavior) and `detailed-spec.v1.2.0.md`
§10 (data model, command, and context-menu/display elaboration). GitHub issue #202 and its comment thread
still carry the full decision history (including the rejected alternative's own sub-decisions on
gap-handling and overflow past "Z"), but the decided behavior itself is now also reflected in the spec
files `developer-lite` works from.

## Icon Submenu Clarified as the `label.color` Picker; Concrete Palette Adopted from Legacy; `label.kind` Confirmed Reserved (2026-08-01)

**Background**: `spec.md` §2.2 confirms a right-click "Icon" submenu on tree memos, but neither
`spec.md` nor the original `detailed-spec.md` ever connected this submenu to a concrete mechanism.
Separately, `detailed-spec.md` §3.3's data model shows a `Label { color, kind }` shape but only
`label.kind` had elaboration text (reserved, unimplemented for v1); `label.color` had no enumerated
values anywhere, just an example (`"color": "blue"`) with no accompanying palette definition.

**Investigation**: during this review, the orchestrator hypothesized that the "Icon" submenu might be
the intended UI for setting `label.kind` (a memo "type" tag), which would have meant `label.kind`
staying unimplemented in v1 conflicts with an already-confirmed `spec.md` UI element. This was checked
against the legacy `TabTreeNote` repository's actual implementation
(`src/renderer/components/Tree/FileIcon.tsx`, `Tree.tsx`, reference-only per `CLAUDE.md`): the legacy
"Icon" submenu is entirely a `label.color` picker. It renders the identical document/lock-shaped SVG
outline for every choice, varying only stroke color, and its `onSelect` handler calls
`onRequestSetLabel(memoName, preset.color, lockToken)` — setting only `color`, never `kind`. The
legacy `Label.kind` field existed in `src/shared/models.ts`'s type definition but was never read or
written anywhere in the legacy UI; it was effectively dead/unused there too. The hypothesized conflict
does not exist.

**Decisions**:
1. **`spec.md` §2.2's "Icon" submenu is the `label.color` picker.** This connection is now made
   explicit in `detailed-spec.md` (previously the submenu and the `label.color` field were never tied
   together anywhere in the documentation, even though they were always the same feature in spirit).
2. **`label.color` palette — adopted verbatim from the legacy app**: exactly 5 choices, same names
   and hex values —
   `{ name: 'なし' (None), color: null }`, `{ name: '赤' (Red), color: '#e05a5a' }`,
   `{ name: '青' (Blue), color: '#4a90d9' }`, `{ name: '緑' (Green), color: '#4caf50' }`,
   `{ name: '黄' (Yellow), color: '#e0c040' }`. No new colors were added and no existing ones were
   dropped or changed — a straight carry-over, since the legacy palette was already a reasonable,
   already-battle-tested small fixed set and there was no stated reason to change it.
3. **`label.kind` remains reserved/unimplemented for v1 (`null` only)** — same conclusion as before,
   but now resting on a concrete finding (dead field in the legacy app, no UI ever built for it) rather
   than a speculative "not blocking" statement.

**Where elaborated**: `detailed-spec.md` §3.3 (palette table added near the `label.kind` bullet,
cross-referenced from the Icon-submenu mention) and §5.2 (Memo Tree elaboration, Icon submenu bullet
now names the palette and cross-references §3.3).

## Mouse-Wheel Tab Switching: Removed from v1 Scope (2026-08-01)

**Decision**: mouse-wheel-based active-tab switching, previously listed in `spec.md` §2.1 ("switch
via click / shortcut / mouse wheel"), is dropped entirely from v1 scope — not deferred, not left as a
`developer-lite`-discretion detail, genuinely removed as a feature. Click and keyboard-shortcut
switching are unaffected.

- **Rationale**: user preference; the feature was judged not needed. No elaborate technical or UX
  problem drove this — it was simply decided to be out of scope, and there is nothing to compare it
  against (no rejected alternative implementation approach; the feature itself is the thing dropped).
- **Scope classification**: this is a genuine `spec.md`-level feature-scope change (removing an item
  from the v1 core feature list touched by `spec.md` §1 Q4), not merely a `detailed-spec.md`
  elaboration detail — hence it is recorded here with the same rigor as any other scope-affecting
  decision, and `spec.md` §2.1 itself was edited (the "mouse wheel" clause removed from the "switch
  via click / shortcut / mouse wheel" sentence, leaving "switch via click / shortcut").
- **Where elaborated**: `detailed-spec.md` §5.1's now-inapplicable "mouse wheel over the tab bar
  cycles the active tab" elaboration bullet was removed entirely (nothing to elaborate for a feature
  that no longer exists).

## "Pin" Button: Global Window Always-On-Top Toggle, Not Per-Tab (2026-08-01)

**Decision**: the 📌 button (`spec.md` §2.1) toggles OS-level "always on top" for the entire app
window — session-only, never persisted. It is a single global button (not one per tab), placed in the
top toolbar immediately to the left of the search button (button order: pin, search, theme toggle).
Confirmed by the legacy repo's `TabBar.tsx` button order and `windowHandlers.ts`'s
`window:setAlwaysOnTop` → Electron `BrowserWindow.setAlwaysOnTop` (also session-only there). Elaborated
in `detailed-spec.md` §4.1, `plans/tab-management/PLAN.md` §4.

## Memo Identification Method: Filename = Memo Name (UUID Not Adopted)

The philosophy of following plain text (transparency, external tool compatibility, retains content even if metadata is lost) is prioritized. The adoption of internal IDs like UUID was rejected because the loss of correspondence tables would risk breaking `memo:` links and tree structures. As a trade-off, renaming = file renaming will break `memo:` links (accepted).

## History of AutoReName Discontinuation (Feature to Automatically Name Memos Based on First Non-empty Line)

In the old v1, it was a specification that "always ON with no way to disable it", so it was removed at user request (post-M7). It was confirmed not to be implemented in TabTreeNoteMD (2026-07-31, spec.md §2.2).

## Tab Lock Session Lifetime: Why "Discard Immediately When Leaving Tab"?

In the old v3's node lock, initially "not discarded when switching tabs", but there is a history of change to a more conservative standard of "discard immediately when leaving tabs" through the user's own reconsideration (there is little practical benefit in maintaining the decrypted state while not viewing, so priority is given to closing the risk of being away early). Tab lock simply follows this standard as well.

## Implementation Policy: No Legacy Code Reference — Build from Scratch (2026-08-01)

**Decision**: `developer-lite` does not read, copy, or adapt source code from the old repository
(`c:\Users\m9112\vscode2\TabTreeNote`) during implementation. Every feature is implemented from
`spec.md`/`detailed-spec.md`/the feature's own `PLAN.md` only — those are treated as sufficient and
self-contained. This supersedes the project's earlier "reference the old repo's code during
implementation" policy.

This is scoped to *source code* only. The old repository's specification documents remain the
historical origin of the product requirements already consolidated into `spec.md` (visible throughout
as `[DESIGN §x]`-style citations) — that provenance is unaffected. `CLAUDE.md`'s "reference only, do
not Edit/Write/commit" rule for that path also still applies regardless of this policy.

## Remove Unused Editor Default Settings: Display Mode Selector and Soft-Tab Checkbox (2026-08-08)

**Decision**: Removed two rows from Settings → Editor (GitHub issue #61):

- The "display mode" selector (`AppSettings.editorDefaults.viewMode`). The effective default is now
  hardcoded to `'live'` — no behavior change, since `'live'` was already the existing factory default.
  The per-memo live/source override and its runtime toggle are unaffected and remain fully functional.
- The "insert tab as spaces" checkbox (`AppSettings.editorDefaults.softTab`, and the corresponding
  per-memo `MemoEditorSettings.softTab` override field). Tab-key behavior is now hardcoded to always
  insert a literal tab character (`\t`). **Behavior change**: the prior factory default was
  `softTab: true` (spaces), so existing users who relied on the default will see Tab now insert a tab
  character instead of spaces. The right-click "Convert" tab↔space transform is unaffected (separate,
  on-demand feature).

Elaborated in `spec.md` §2.3/§3.4/§4.2 and `detailed-spec.md` §3.2/§3.3/§5.3/§5.10.

## Edit Lock: Live Enforcement Added for an Already-Open Memo (GitHub issue #12, 2026-08-08)

**Original decision**: `editor_load`'s `edit_locked` flag was kept as a one-time load-time snapshot
(not changed to a polling/subscription model). Live-updating an already-open editor when Edit Lock is
toggled was deferred as a UX-only gap — the backend already refuses the write when locked (`editor_save`
step 1), so there was no data-loss/corruption risk either way — and tracked as follow-up GitHub issue #12.

**Follow-up (issue #12)**: Implemented. `EditorPane.tsx`'s `readOnly` now also reads the live tree
state's `locked` flag for the open memo (`App.tsx`, via `locateNode`), which `useTree.ts`'s `setEditLock`
already keeps in sync on every successful toggle — no backend/IPC change needed. Toggling Edit Lock on an
open memo now takes effect immediately, mirroring the legacy app's equivalent live check. Elaborated in
`spec.md` §2.2.

## In-Memo Replace "Selection" Mode: Dropped from Scope (GitHub issue #13, 2026-08-08)

**Decision**: Dropped the "constrain replace to the current selection" mode listed in `spec.md` §2.4
("single / all / selection / one-by-one"). Only replace-one and replace-all are implemented; `spec.md`
§2.4 corrected to match. Reason: unlike the other three modes, this one can't reuse
`@codemirror/search`'s ready-made `replaceNext`/`replaceAll` commands — it needs custom range-scoped
match-finding and boundary-tracking across incremental replaces, a moderate implementation cost judged
not worth it for a niche power-user feature.

## Markdown Preview: Pane-Mode Reached via Single 3-State Cycling Toggle (2026-08-08)

**Decision**: `docs/MDEditor-spec.md`'s pane-mode toggle (formerly described as a binary "preview
show/hide toggle") is a single button cycling `edit → split → preview → edit → ...`, replacing
`MemoEditorSettings.previewVisible: boolean | null` with `MemoEditorSettings.paneMode: "edit" | "split" |
"preview" | null` (and the matching `AppSettings.editorDefaults.paneMode`). The pre-existing live/source
toggle stays a separate, orthogonal control; it's now also added to the auto-reveal principle
(`docs/MDEditor-spec.md` §2), auto-switching pane-mode to `split` if clicked while in `preview`.

**Rejected alternatives**: (a) two independent boolean toggles (edit-visible + preview-visible) —
needs a second new toolbar-header button beyond the two already planned (pane-mode, PDF export) and
duplicates state-machine complexity for little benefit; (b) deferring "preview only" entirely for this
pass — rejected because showing preview-only after finishing editing was the original motivating request
for this whole feature.

## Real `<img>` Rendering via Byte-Read IPC Command + Blob-URL Cache (2026-08-08)

**Decision**: Live-view/preview/PDF-export image rendering reads image bytes through a new backend
command, `image_read_bytes` (returns raw bytes via `tauri::ipc::Response`, not base64/JSON), and the
frontend builds and caches a `Blob` URL per image path (shared path-keyed cache across the three
consumers). Elaborated in `docs/MDEditor-spec.md` §5 and `docs/plans/markdown/image-handling/PLAN.md`.

**Rejected alternative**: widening the Tauri `assetProtocol` scope to permit arbitrary runtime-chosen
tab folders (folders are picked via folder-picker at runtime, not a static build-time glob) — rejected
because it would weaken this app's existing conservative security posture (no `assetProtocol` scope
configured today) more broadly than necessary just to serve images.

## Drag-Resize-Handle for Rendered Images in Live View — Deferred (2026-08-09)

**Decision**: The drag-resize-handle interaction (dragging a handle on a rendered image in live view
to rewrite `![alt](path)` → `<img src width height>`) is deferred, not rejected — tracked as a future
follow-up, not scheduled now.

**Rationale**: This was already flagged as the highest-risk remaining piece of the original
Markdown-preview/image-rendering feature work (~6/10 risk vs. ≤4/10 for the features implemented in
this pass), specifically because it involves CodeMirror decoration/widget click+drag handling. That
assessment was borne out during implementation: two critical CodeMirror-transaction-safety bugs (raw
DOM mutation outside a transaction misread by CM6's `DOMObserver` as a user edit, corrupting the
document) were found and fixed in the closely-related image-widget code during this same effort,
reinforcing that this risk class deserves its own dedicated design pass rather than being rushed in
now.

**Note**: When this is eventually picked up, it will need its own UX design (drag mechanics, handle
placement, aspect-ratio lock, minimum size) before implementation — starting with a `designer-lite`
consultation, same as any other feature.

## Table of Contents: Relative Heading-Nesting Indentation (GitHub issue #84, 2026-08-13)

**Decision**: `generateTocList` (`src/editor/tocGenerator.ts`) indents each TOC entry relative to the
document's own shallowest heading level, not absolute heading level — the shallowest heading becomes the
TOC's flat top-level item regardless of its actual `#`-count, GitHub-style. A document that already
starts at `#`/H1 is unaffected (identical output to the prior absolute-indentation behavior).

**Rejected alternative**: keeping absolute-level indentation (the original v1 implementation) — rejected
because a document whose shallowest heading isn't `#` then produces a TOC with no flat top-level item at
all, which reads as visually broken.

## Quick Open Default Shortcut: `Ctrl+O` (2026-08-16)

**Background**: `spec.v1.1.0.md` §3.1 originally confirmed `Ctrl+Shift+P` as Quick Open's default shortcut,
reasoning only against `docs/plans/window-operation/PLAN.md`'s original 16-action shortcut table (ruling
out `Ctrl+P`/`Ctrl+K`). It did not check the app's actual current default keymap,
`src/shared/shortcuts.ts`'s `DEFAULT_SHORTCUTS`, where `Ctrl+Shift+P` was already bound to
`markdown.paneModeCycle` (added later by `docs/plans/markdown/markdown-preview/PLAN.md` §6.3, a plan that
postdates `window-operation/PLAN.md`'s table). This was a real, live default-shortcut collision between two
v1.1.0/v1.0.0 features, raised by `developer-lite` in `docs/QA.md` during `docs/plans/v1.1.0/quick-open/
PLAN.md` planning.

**Decision**: Quick Open's default shortcut is `Ctrl+O`, verified as unused in `DEFAULT_SHORTCUTS` as of
this writing (checked directly against `src/shared/shortcuts.ts`, not just `window-operation/PLAN.md`'s
table). `markdown.paneModeCycle`'s existing `Ctrl+Shift+P` binding is left untouched. `Ctrl+O` is also a
reasonably mnemonic choice ("Open") for a feature whose purpose is to jump-open a memo, echoing the same
mnemonic (if not identical key) VS Code's own "Quick Open" uses. Reflected in `spec.v1.1.0.md` §3.1 and
`detailed-spec.v1.1.0.md` §3.3.

**Rejected alternative**: reassigning `markdown.paneModeCycle`'s default away from `Ctrl+Shift+P` to free
it up for Quick Open — rejected because that binding is already implemented/shipped in v1.0.0, and
reassigning it would break existing users' muscle memory for no benefit over simply picking a different,
unused combo for the newer feature instead.

**Process note**: this collision existed because the original conflict-check only cross-referenced
`window-operation/PLAN.md`'s shortcut table, which is a point-in-time snapshot that predates
later-added bindings (`markdown.paneModeCycle`, `markdown.pdfExport`, etc.). Future shortcut
conflict-checks during spec/detailed-spec work should verify directly against the live
`src/shared/shortcuts.ts` `DEFAULT_SHORTCUTS` map, not only `window-operation/PLAN.md`'s original table,
to avoid this recurring as more shortcuts get added by later plans.

## Tab-Lock Auto-Relock Scope: Foreground Active Tab Only, Backgrounded Sessions Explicitly Out of Scope (2026-08-16)

**Background**: `spec.v1.1.0.md` §5.1 originally justified a single global inactivity timer on the premise
that "a session is already discarded the instant the user leaves its tab (`[spec §4.1 pt.5]`)," so "at
most one locked session can ever be alive at a time." That premise is factually false for the
already-implemented, frozen v1.0.0 Tab Lock feature: `docs/plans/tab-lock/PLAN.md`'s "Implementation
Notes" and the shipped `src/state/useTabLock.ts` both confirm Tab Lock supports more than one
concurrently-active session — e.g. a cross-tab drag-and-drop onto a locked tab unlocks that tab in place
(password entered at drop time) without the drag-source tab ever losing focus, so the dropped-onto tab's
session can stay alive in the background indefinitely, since it never itself becomes the active tab and so
never triggers the "leaving the tab" teardown. `developer-lite` raised this in `docs/QA.md` during
`docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md` planning, since it blocks the timer-scope architecture
decision (single global vs. per-session-aware).

**Decision**: Auto-Relock's inactivity timer covers the foreground active tab's session only, exactly as
`spec.v1.1.0.md` §5.1 already literally stated — no change to the feature's actual behavior or
architecture (still a single global timer, not a per-session one). What changed is that the spec now states
this scope limitation **explicitly** as a deliberate, accepted boundary rather than resting on the false
"at most one session alive" premise: a backgrounded, drop-created Tab-Lock session has no lifecycle
end-trigger at all today (not "leaving the tab" — it was never entered — and, with this scope, not
Auto-Relock's timer either), and this feature knowingly does not close that gap. Reflected in
`spec.v1.1.0.md` §5.1 ("Deliberately does not cover backgrounded sessions") and
`detailed-spec.v1.1.0.md` §3.5 (timer lifecycle bullet).

**Rejected alternative**: widening Auto-Relock to also watch backgrounded/drop-created sessions (e.g. a
per-session timer, or extending the single global timer's reach to every live session, not just the
foreground one) — presented to the user as an option alongside the narrower scope; declined to keep this
feature's scope small, treating the backgrounded-session gap as a pre-existing condition of Tab Lock's
multi-session model that is unrelated to and not newly introduced by Auto-Relock, not something this
feature is obligated to fix as a side effect. Closing that gap, if ever pursued, remains available as its
own separate future feature.

## Preview-Pane `memo:` Link Non-ASCII Name Encoding Bug: Bundled into v1.2.0's §9 Fix, Not Deferred (2026-08-20)

**Background**: `spec.v1.2.0.md` §9 (elaborating `[spec §4.2]`/`[spec §2.8]`) fixes a bug where
markdown-it's `normalizeLink` rule percent-encodes a rendered `<a href>` in the preview pane, while the
corresponding lookup target (a heading's `id`, set directly from `scanHeadings()`'s raw slug) is never
encoded — breaking resolution for any non-ASCII (in practice, almost any Japanese) `#slug` anchor. The
originally-decided fix (`decodeURIComponent` in `previewInteractions.ts`'s `handleAnchorClick`) was
scoped only to that function's `#`-prefixed branch. While elaborating §4 (`memo:TabName/MemoName#slug`
cross-tab anchor links) and §9 into `detailed-spec.v1.2.0.md`, `designer-lite` re-ran §9's own root-cause
investigation (the same `mdurl`/`markdown-it` package inspection technique) against a `memo:` target
instead of a bare `#slug` target, and confirmed the **identical** encoding defect also corrupts
`memo:TabName/MemoName` hrefs whenever the tab name or memo name — not just an anchor slug — contains a
non-ASCII character (empirically: `mdurl.encode(mdurl.format(mdurl.parse('memo:Work/日記#見出し')))` →
`memo:Work/%E6%97%A5%E8%A8%98#%E8%A6%8B%E5%87%BA%E3%81%97`). This is a **pre-existing bug in today's
baseline `memo:` cross-tab links** (`[spec §2.8]`), not introduced by v1.2.0's §4 — but it silently breaks
§4's new feature for any Japanese-named tab/memo when clicked from the preview or split pane, since the
memo-target half of a `memo:TabName/MemoName#slug` link would fail to resolve regardless of whether the
anchor half is even present. Live view is unaffected in both the original `#slug` case and this adjacent
`memo:` case, for the same reason: live view's link/anchor detection reads raw CodeMirror source text
directly and never passes through markdown-it's HTML-rendering pipeline, so no encoding is ever
introduced there. Since this was a genuinely new finding not decided or delegated anywhere in
`spec.v1.2.0.md`, it was escalated to `QA.md` as `Q-v1.2.0-1` rather than silently patched in during the
detailed-spec pass, per this project's process.

**Decision**: fix the `memo:`-prefixed branch of `handleAnchorClick` (`src/editor/preview/
previewInteractions.ts`) with the identical `decodeURIComponent` treatment already decided for the
`#`-prefixed branch, **in the same v1.2.0 implementation pass** — not deferred to a later issue. Reasons
for bundling now rather than later: (1) it is the same fix, on the same file, following the same
already-decided pattern — no new design work, just applying an already-approved technique to a second,
adjacent branch of the same function; (2) it is a **correctness prerequisite** for §4 (the new
`memo:TabName/MemoName#slug` cross-tab anchor feature) actually working on Japanese-named tabs/memos in
`preview`/`split` mode — shipping §4 without this fix would mean the feature silently fails for the
majority of this app's own real-world tab/memo names (a Japanese-first personal note app), which would be
a poor first impression of a brand-new feature; (3) the fix is small and low-risk (one additional
`decodeURIComponent` call site, isolated to preview-pane click handling, no schema/data-model change).
Reflected in `spec.v1.2.0.md` §9 (bundled-follow-up paragraph) and `detailed-spec.v1.2.0.md` §4.4/§9/§12.

**Rejected alternative**: track the `memo:`-branch fix as a separate, later issue/patch (`developer-lite`'s
original framing when escalating `Q-v1.2.0-1`, presented as one of two options) — rejected specifically
because of point (2) above: unlike most "adjacent gap, could be deferred" findings, this one is not
merely adjacent to §4, it is load-bearing for §4's own stated purpose in the common case for this app's
actual users, so deferring it would ship a headline v1.2.0 feature in a state that looks broken for most
realistic inputs rather than genuinely deferring an unrelated concern.

## i18n Mechanism: Hand-Rolled React Context, Not `react-i18next` Package (2026-08-20)

**Decision**: v1.2.0's UI-internationalization mechanism (`[spec v1.2.0 §1.4]`, which delegated the exact
library/mechanism to the detailed-spec pass) is a small hand-rolled React Context + `useT()` hook
(`src/i18n/`), not the `react-i18next` package itself.

**Rationale**: `[spec v1.2.0 §1.4]` itself frames the request as "a `react-i18next`-**like** mechanism,"
not the package by name. This app's architecture doc (`docs/plans/project-architecture/PLAN.md`) states
the app deliberately uses "plain hooks... no external state library," confirmed by inspection: every
other cross-cutting piece of app state (`useSettings.ts`, `useTree.ts`, `useTabs.ts`, etc.) is a plain
hook, and `package.json` has zero state-management dependencies today (`redux`/`zustand`/`jotai` all
absent). Adopting `react-i18next` would be the first external state-management dependency in the app,
contradicting that established architectural choice, for a need (runtime string lookup + re-render on
language change) a ~40-line Context/hook fully covers.

**Where elaborated**: `detailed-spec.v1.2.0.md` §1.1.

## Preview-Pane Zoom CSS Mechanism: `zoom` Property, Not `transform: scale()` (2026-08-20)

**Decision**: v1.2.0's preview-pane zoom (`[spec v1.2.0 §6]`, which specified the range/steps/scope but
not the CSS mechanism) is implemented with the CSS `zoom` property (e.g. `style={{ zoom: "${percent}%" }}`
on `PreviewPane.tsx`'s content element), not `transform: scale(...)`.

**Rationale**: `zoom` triggers real layout reflow at the new size (text rewraps, images/tables resize
within their containers at the new effective width) — matching `[spec v1.2.0 §6]`'s "affects... text
size, images, tables" description of a genuine content-size change, not merely a visual magnification.
`transform: scale()` does not reflow layout — it visually magnifies a box whose layout box stays at its
original size, clipping content past 100% or leaving dead space below 100%, requiring extra width/height
compensation math `zoom` needs none of. `zoom` is a long-standing Chromium/WebView2-native CSS property
(not W3C standard, but universally supported in every Chromium-based engine — this app's one and only
rendering target per its Tauri+WebView2 architecture, so no cross-browser-compatibility concern applies).
It also interacts correctly, with no extra work, with `[MDEditor-spec §4]`'s existing wide-table
horizontal-scroll rule and the preview's `max-width: 100%` image rule, since `zoom` affects the box's own
effective CSS pixel size for every descendant's percentage/auto calculations.

**Caveat**: this interaction is argued from code/CSS semantics only, not visually confirmed — whether the
table horizontal-scroll container and image `max-width` actually look right at the range's upper bound
(180%) in a real running preview pane is left as a manual check for the user during/after implementation.

**Where elaborated**: `detailed-spec.v1.2.0.md` §6.2.
