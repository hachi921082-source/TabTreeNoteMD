# PLAN: Memo Tree (Left Pane)

> Status: implemented (backend commands in `src-tauri/src/commands/tree.rs` +
> `src-tauri/src/model/tree.rs`, frontend `components/Tree/`). The parts this file itself deferred
> to `tab-lock/PLAN.md` — the crash-mid-lock recovery dialog, and the password-at-drop-time flow
> for a cross-tab drop onto a locked target tab — are now implemented too, by that plan's own
> implementation pass (`components/dialogs/RecoveryDialog.tsx`, `commands/tab_lock.rs`'s
> `move_cross_tab_*` functions). Depends on `project-architecture/PLAN.md` and
> `persistence-fs/PLAN.md`. Cross-tab DND involving a **locked** target tab is specified in
> `tab-lock/PLAN.md` §"Cross-tab DND" — this file covers the tab-to-tab case only for two
> already-unlocked (or same-tab) endpoints, and deferred to that file when the drop target is
> locked. `tree_open`'s crash-mid-lock recovery check (§2 step 0 below) also now fully implements
> `tab-lock/CRASH-RECOVERY.md`'s trigger point (previously just a detection seam).

Sources: `docs/spec.md` §2.2, §3.1–3.2, §5.3; `docs/detailed-spec.md` §3.5, §4.2, §5.2, §6.3.

## Implementation Notes (resolutions of this plan's discretion points)

- `NodePath = String` (the `memoName` itself) — sufficient since a `.md` file's name is already
  unique within a tab (flat folder, no nested subfolders).
- `TreeNode` implemented as a Rust type alias for `MetaTreeNode` (§1's explicitly-permitted option).
- Reconciliation: a dropped metadata entry's still-real children are promoted to that level rather
  than dropped too (§2 rule wasn't explicit about children of a dropped entry; resolved in favor of
  "memo body is never lost," `[spec §3.2]`).
- Every mutating command re-reconciles the tree from disk on entry and unconditionally re-saves on
  exit, so `tree_open`'s "reconciled but not force-written" correction (§2 step 5) is picked up by
  whichever mutating command runs next — no separate dirty-tracking store was needed.
- `memo_move`'s cross-tab return shape: `MemoMoveResult { source: TreeSnapshot, target?:
  TreeSnapshot }`. The frontend only ever applies `source` — since only one tab's tree is held in
  memory at a time (the active one), the other tab's updated tree is re-fetched fresh via
  `tree_open` whenever the user switches to it.
- `memo_move`'s locked-target-tab seam: reuses the same `resolve_tab` locked-tab rejection already
  used by every other mutating command, which returns `TabLocked` before touching anything — this
  is the hand-off point `tab-lock/PLAN.md`'s drop-time password flow should intercept.
- search-index integration is stubbed as two narrow, clearly `TODO(search-index)`-marked no-op
  functions (`touch_search_index_upsert`/`_remove`) called from every content-changing command.
- Frontend "Icon" submenu implemented as a click-to-expand accordion inside a newly
  submenu-capable shared `ContextMenu` component, not a hover flyout.
- DND is pointer-based throughout (within-tree reorder/reparent and cross-tab drag onto a
  `TabBar` tab, identified via a `data-ttn-tab-id` attribute + `elementFromPoint` hit-testing),
  consistent with `tab-management/PLAN.md`'s existing avoidance of HTML5 DnD (conflicts with
  `dragDropEnabled` on Windows).
- Ctrl+Delete skips the confirm dialog (goes straight to trash, same as any right-click "Delete");
  only a `TRASH_FAILED` response triggers the direct-delete consent dialog either way.

---

## 1. Data Structures

Already fully specified in `[spec §3.4]` and `[detailed-spec §3.3]`: `MetaTreeNode` (persisted shape,
in `.tabtreenote.json`'s `tree` field) vs. `TreeNode` (runtime/reconciled shape returned to the
frontend). Rust: `model/tree.rs` defines both as `#[derive(Serialize, Deserialize)]` structs; `TreeNode`
additionally carries nothing `MetaTreeNode` doesn't (it's a read-only reconciled view, not a superset)
— the split exists so the frontend's TS type can be `readonly`-flagged and structurally distinct from
what gets serialized back on save, not because the Rust representation needs two structs. `developer-lite`
may implement `TreeNode` as a Rust type alias or a thin wrapper if that avoids duplication, as long as
the two remain intentionally-covariant per `[spec §3.4]`'s comment ("Return type of `tab:openTree`").

## 2. Reconciliation — `tree_open(tab_id: String) -> AppResult<TreeSnapshot>`

```rust
struct TreeSnapshot { nodes: Vec<TreeNode>, tab_name: String, locked: bool }
```

Elaborates `[detailed-spec §3.5]`. Steps:

0. **Before anything else**: check whether `.tabtreenote.lock-inprogress` exists in the tab's folder
   (crash-mid-lock recovery marker, `tab-lock/CRASH-RECOVERY.md`). If present, skip steps 1–5 below and
   return the `recovery_pending` state that file specifies instead.
1. If the tab is locked with no active session, return a snapshot indicating `locked: true` with
   `nodes: []` (frontend renders the locked-tab empty/prompt state — actual content only comes from
   `tab_unlock`, see `tab-lock/PLAN.md`).
2. Otherwise, read the persisted `MetaTreeNode[]` from `.tabtreenote.json` (or the in-session decrypted
   payload's `tree` if locked-with-session — `tab-lock/PLAN.md` supplies this via the session store
   rather than this function reading the file directly for that case).
3. Scan the tab folder for direct-child `.md` files (missing folder ⇒ empty list per
   `[detailed-spec §6.5]`, not an error).
4. Reconcile per `[detailed-spec §3.5]`:
   - metadata entries with no matching file ⇒ dropped;
   - real files with no matching metadata entry ⇒ appended to end of root level, defaults
     (`expanded: true`, no label, no editor override, `locked: false`);
   - matching entries keep hierarchy/label/expand/editor-override.
5. If reconciliation changed anything vs. what was last persisted, mark the in-memory tree dirty so
   the next mutating operation or periodic autosave (see `editor-save/PLAN.md` §3) persists the
   correction — do not force an immediate synchronous write here (`[detailed-spec §3.5]`).
6. Return the reconciled `TreeNode[]` as `TreeSnapshot.nodes`.

This same function, called with an "empty (a) side," is also the corruption-recovery flat-rebuild
mechanism (`[spec §5.9]`, `[detailed-spec §6.4]`): if `.tabtreenote.json` and its `.bak` are both
unreadable, `tree_open` proceeds as if step 2 yielded `[]`, so every real file lands at the end of root
level. Emit `evt:meta:recovered` with `{ scope: 'tab', tab_id }` in that case.

## 3. Commands

### `memo_create(tab_id, parent: Option<NodePath>) -> AppResult<TreeSnapshot>` / `memo_create_child(...)`
- Right-click "Create Memo" (root/end) or "Create as Child" (under selected node) **immediately shows
  a name-input dialog** on menu selection (`[spec §2.2]`) — no auto-naming, no auto-open. The dialog
  itself is frontend-only (shared custom dialog component); this command is invoked once the user
  confirms a name.
- Validates name (Strict) + uniqueness within the tab; creates an empty `.md` file (atomic_write with
  empty content — or a plain `fs::write` since there's no prior version to `.bak`-swap, implementation
  discretion per `persistence-fs/PLAN.md` §3's exception note), inserts a new node into the in-memory
  tree at the requested position (append-to-end of root, or append-to-end of the parent's children),
  then per `[spec §5.3]`/`[detailed-spec §6.3]` sync order: file op first (with self-write
  registration), then atomic-save the metadata JSON.
- Returns the updated `TreeSnapshot` (frontend does **not** auto-open the new memo per
  `[spec §2.2]`'s "AutoReName... discontinued" / "no... auto-open behavior" note).

### `memo_rename(tab_id, path: NodePath, new_name: String) -> AppResult<TreeSnapshot>`
- Also triggered by F2 (single-selection only — multi-select F2 is a no-op or disabled,
  `[detailed-spec §5.2]`).
- Sync order per §6.3 of `detailed-spec.md`: validate (Strict + uniqueness) → rename the `.md` file on
  disk (self-write-registered) → update the tree node's `memoName` in memory → atomic-save metadata.
- **Known consequence, not a bug**: any `memo:` link elsewhere pointing at the old name breaks
  (`[spec §3.1]`, accepted tradeoff). No automatic link-rewrite is performed (out of scope, not
  requested anywhere in `spec.md`).

### `memo_delete(tab_id, paths: Vec<NodePath>) -> AppResult<TreeSnapshot>`
- Supports multi-select (`[detailed-spec §5.2]`).
- Default: OS trash via `persistence-fs`'s `fs::trash`. On `TrashFailed`, frontend shows a confirm
  dialog; only on explicit consent does the frontend re-invoke with a `force_direct: true` flag (or a
  separate `memo_delete_direct` command — implementation discretion) that performs a direct delete
  instead (`[spec §5.8]`, never silent).
- Deleting a node deletes its entire subtree (children go with the parent — `spec.md` does not describe
  a "promote children" option, and the legacy tree model has no such feature either).
- Sync order: file delete(s) first (self-write-registered), then remove the node(s) from the in-memory
  tree, then atomic-save metadata.
- Index update: each deleted memo's entry is purged from the full-text index (`search-index/PLAN.md`).

### `memo_move(tab_id, paths: Vec<NodePath>, target: MoveTarget) -> AppResult<...>`
```rust
enum MoveTarget {
    WithinTab { new_parent: Option<NodePath>, index: usize },   // reorder/reparent within same tab
    OtherTab { target_tab_id: String, new_parent: Option<NodePath>, index: usize, copy: bool },
}
```
- `copy: bool` reflects Ctrl-held-during-drag (`[spec §2.2]`, "Ctrl to copy"). Within-tab moves are
  always a move (no in-tab copy feature specified).
- **Within-tab**: pure metadata mutation (no file rename needed — the file stays in the same folder
  under the same name; only its position in the tree structure changes), atomic-save metadata after
  updating in-memory tree. No self-write registration needed for the file itself (nothing touches it);
  metadata JSON's own atomic-write already registers itself.
- **Cross-tab, both unlocked**: file physically moves (rename, since move = same volume typically; if
  the two tab folders are on different volumes, `developer-lite` should use copy+delete-original as the
  rename fallback, self-write-registering both source and destination paths either way) or copies
  (`copy: true`, leaves source untouched) into the target folder. Name-collision at the destination
  → `NameDuplicate`, surfaced to the user for a rename-on-drop or cancel decision (frontend concern; the
  command itself just returns the error without partially completing). Both tabs' metadata JSON are
  updated and atomic-saved (source tab: node removed, unless `copy`; target tab: node inserted).
- **Cross-tab where the target tab is locked**: delegates to the flow in `tab-lock/PLAN.md` (password
  dialog at drop time, etc.) — this command should detect the locked-no-session case and return control
  to the frontend to trigger that flow (or internally call the lock-domain function once the frontend
  supplies the password — exact call shape is `developer-lite`'s choice, but the *behavior* is fixed by
  `tab-lock/PLAN.md`, not re-specified here).
- **Cross-tab where the source tab has an active unlock session**: the moved-out memo is decrypted at
  the moment of the move and lands as plaintext in the target (if target is a normal unlocked tab) or
  gets re-encrypted immediately (if target is also locked/in-session) — see `tab-lock/PLAN.md`.
- Index update: moved/copied/renamed memos' index entries are patched (`search-index/PLAN.md`), not
  rebuilt from scratch.

### `memo_set_expanded` / `memo_set_label` / `memo_set_editor_override` / `memo_set_edit_lock`
All four are pure metadata mutations — no file operation, just update the in-memory tree and
atomic-save `.tabtreenote.json`. `memo_set_edit_lock` toggles the per-memo `locked?: boolean` field
(**edit lock**, distinct from tab lock — blocks body editing only, no password, `[spec §2.2]`); the
editor (`editor-save/PLAN.md`) is responsible for actually refusing edits when this flag is set, this
command only persists the flag.

`memo_set_label(tab_id, path: NodePath, color: Option<LabelColor>) -> AppResult<TreeSnapshot>` sets
`label.color`. **Confirmed concrete 5-choice palette, adopted verbatim from the legacy app**
(`[detailed-spec §3.3]`, `DECISIONS.md` "Icon Submenu Clarified as the `label.color` Picker..."), a
closed enum — no other values are valid, this is not a free-form color field:

| Choice | `label.color` value |
|---|---|
| None | `null` |
| Red | `"#e05a5a"` |
| Blue | `"#4a90d9"` |
| Green | `"#4caf50"` |
| Yellow | `"#e0c040"` |

This command never touches `label.kind`, which stays reserved and unimplemented for v1 (`null` only)
— there is no corresponding `memo_set_label_kind` command or UI surface in this plan.

### `memo_get_properties(tab_id, path: NodePath) -> AppResult<MemoProperties>`
```rust
struct MemoProperties {
    file_name: String, file_path: String, file_size_bytes: u64, created_ms: u64, modified_ms: u64,
    char_count: usize, char_count_excluding_whitespace: usize, line_count: usize,
}
```
File info from OS metadata + character-count breakdown (reading the body once; encoding-aware count,
i.e. count of decoded characters, not raw bytes — `[spec §2.2]`, "file info + character count").
`char_count`/`char_count_excluding_whitespace`/`line_count` are all computed from the LF-normalized
body (`memo_file_service::normalize_to_lf`). `file_path` is the logical `<folder>/<path>.md` path in
both the plain and tab-locked (session) cases — the locked case never exposes the obfuscated on-disk
token filename. Byte-count-by-encoding / half-width-full-width breakdown is explicitly deferred
(`[spec §7]`, "Unimplemented / deferred") — not part of this command's scope. `created_ms`/
`modified_ms` are raw epoch-ms timestamps (`system_time_to_epoch_ms`, `tree.rs`), not pre-formatted
strings — the frontend (`shared/formatDateTime.ts`) formats them in the user's local timezone with
weekday + relative "n日前", matching the old app's client-side `formatDateTime` verbatim.

### `memo_open_location(tab_id, path: NodePath) -> AppResult<()>`
OS "reveal in file manager" for a single memo (`[spec §2.8]`'s pattern; same `tauri_plugin_opener`
Shell API as `commands/tab::tab_open_location`). Plain (non-locked) tabs reveal the actual memo file
(`<folder>/<path>.md`), selected in Explorer. Locked/session tabs instead reveal the tab's folder
itself (no file selected), since the on-disk file is an obfuscated token name that would confuse the
user if shown — same reasoning as `memo_get_properties`'s `file_path` masking above.

## 4. Frontend (`components/Tree/`)

- Renders `TreeNode[]` with connection lines, expand/collapse (`memo_set_expanded`), color/type label
  badges.
- Selection: click = single-select + open memo (fires `editor_load` via `editor-save/PLAN.md`'s hook);
  Ctrl-click = toggle in multi-select set; Shift-click = range-select over the current visible
  (expanded-aware) flat order. Standard OS-file-explorer-style semantics: flatten the currently-visible
  tree (respecting collapsed nodes) into an ordered list; click sets a single "anchor" and clears any
  other selection; Ctrl-click adds/removes just the clicked node from the selection set without moving
  the anchor; Shift-click selects every node between the anchor and the clicked node in that flattened
  order (inclusive), replacing the previous selection. Implement as pure functions operating on the
  flattened id list + current selection set, independent of rendering, so they're easy to unit-test.
- Click on empty area (below the last row) selects nothing / clears selection (`[spec §2.2]`, issue.md
  carryover). When the tree is empty, show a centered "Create Memo" button only (no rows, no empty-area
  click target needed).
- ↑/↓ arrow keys: only intercepted when the **tree** (not editor) has focus; moves selection to
  previous/next visible row and opens that memo, without moving focus into the editor
  (`[detailed-spec §5.2]`). Click-to-open still focuses the editor as before — these are two different
  interaction paths with different focus outcomes, both required.
- F2: rename (single-selection only).
- When the open memo changes by any means (click, arrow nav, `memo:` link navigation from
  `link/PLAN.md`), tree selection highlight + scroll position auto-follow to the newly-open memo
  (`[spec §2.2]`).
- Dedicated force-delete shortcut (`[spec §9.1 item 3]`): **confirmed key binding: `Ctrl+Delete`**
  (`[detailed-spec §5.2, §9]`, `DECISIONS.md` "Force-Delete Shortcut Key: Confirmed as `Ctrl+Delete`"
  — no longer just an illustrative example, this is the actual default in `shared/shortcuts.ts`,
  still user-rebindable like any other entry in `AppSettings.shortcuts` per `window-operation/PLAN.md`
  §3). Works regardless of current focus target (global `keydown` listener at the app-root level, not
  row-focus-scoped), invokes `memo_delete` on the current tree selection. Plain `Delete` key remains
  focus-dependent as before (does nothing when editor has focus, since the click that opened the memo
  already moved focus there — `[spec §2.2]` "Delete key" bullet, addressed by adding this dedicated
  shortcut rather than changing existing behavior).
- Right-click context menu (shared component, `components/dialogs/` or a dedicated
  `components/ContextMenu/`): "Rename," "Delete," "Create Memo," "Create as Child," "Icon" submenu,
  "Properties," plus move-up/down/hierarchy entries if not covered by DND alone. Flips to render upward
  when it would overflow the bottom of the screen (`[spec §2.2]`), and is also clamped horizontally
  with a scroll fallback so an expanded submenu (e.g. the "Icon" accordion below) can't overflow either
  edge — see `ContextMenu.tsx`'s `useLayoutEffect`-based real-DOM-measurement positioning. **"Icon" submenu is the
  `label.color` picker** — see §3's `memo_set_label` entry above for the confirmed concrete palette;
  selecting an entry calls `memo_set_label` with that `color` value. It never sets `label.kind` (which
  stays reserved/unimplemented for v1 — `null` only, `[detailed-spec §3.3]`).
- DND: HTML5 drag-and-drop or a pointer-based custom implementation (`developer-lite`'s choice) —
  within-tree reorder/reparent, and drag onto another tab in the tab bar for cross-tab move/copy
  (Ctrl-held = copy, cursor/tooltip should reflect this during drag per standard OS DND conventions).

## Open Items

None.
