# PLAN: OS-Folder → Tree-Pane Drag-and-Drop Import (v1.1.0)

> Status: planning complete, not yet implemented. New feature, no existing v1.0.0 plan owns this
> surface. Depends on `docs/plans/project-architecture/PLAN.md` (module layout, command naming,
> `AppResult`/`ErrorCode`), `docs/plans/persistence-fs/PLAN.md` (atomic write, self-write registry,
> Windows-lock retry, name validation), `docs/plans/memo-tree/PLAN.md` (tree data structures, the
> `MetaTreeNode::default_for_new_file` constructor, DND hit-testing this feature reuses),
> `docs/plans/tab-lock/PLAN.md` (`TabAccess`/`resolve_tab_access` session dispatch), and
> `docs/plans/search-index/PLAN.md` (index patch on import). This file introduces the shared
> `components/Toast/Toast.tsx` non-blocking notification primitive, also reused by
> `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md`.

Sources: `docs/spec.v1.1.0.md` §1; `docs/detailed-spec.v1.1.0.md` §2.1, §3.1, §4.2 (import rows).

---

## 1. Scope Recap

Dragging one or more `.md` files from Windows Explorer (or any app producing a standard OS file-drag
payload) onto the tree pane of an **already-open, already-registered** tab, to import them as memos —
copy-only, no source mutation. Distinct from the existing OS-folder-drop-onto-tab-bar gesture
(`docs/plans/tab-management/PLAN.md` §4, `tab_register_existing`), which targets the tab bar and
registers a whole new tab. Folders in the drop payload are not accepted (silently skipped, same as any
other non-`.md` item) — no recursive scan, per `[spec v1.1.0 §1.1]`.

## 2. Design Approach: Client-Orchestrated Collision Loop, Backend Executes One Resolved Item at a Time

`[detailed-spec v1.1.0 §2.1]`'s per-item processing loop (filter → sanitize → collision-check →
possibly-interactive resolution → write) cannot be one blocking Tauri command, since an unresolved
collision needs to show a dialog and wait for the user. This plan splits it as:

- **Collision detection and resolution run entirely client-side** in a new `state/useImport.ts` hook,
  using data the frontend already holds in memory (the active tab's rendered `TreeNode[]`, which already
  carries `memoName`/`locked` for every existing node) plus a locally-accumulated "claimed this batch"
  name set — no read-only round-trip command is introduced for this. This follows the same
  "client-side mirror for instant feedback, server-side authoritative" split `project-architecture/PLAN.md`
  §2 already establishes for `shared/nameSanitize.ts` (mirroring `fs/path_safety.rs`); this feature adds
  one more client-side mirror, `shared/uniqueName.ts`, porting `src-tauri/src/util/unique_name.rs`'s
  existing `resolve_unique_name(base, exists)` algorithm (`base`, `base(1)`, `base(2)`, ... until free) —
  used server-side today by `memo_create_impl`/`tab_create_new`/`register_tab_at_path` (GitHub issue #97),
  reused client-side here purely to *preview* what a "Rename" choice would produce, so the dialog/toast
  can show the resulting name before any write happens.
- **The backend executes one already-resolved item per call**, via a new command (§3 below), and
  defensively re-validates (a stale/raced collision surfaces as an ordinary `NAME_DUPLICATE`/etc. error,
  which the frontend folds into the skip tally rather than aborting the batch — consistent with
  `[detailed-spec v1.1.0 §2.1]` step 6's "skip this item rather than aborting the whole batch").
- This keeps every write-side mechanism (atomic write, self-write registration, `TabAccess` session
  dispatch, index patch) exactly where every other mutating command already puts it, and requires no new
  interactive/resumable backend state machine.

## 3. Backend

### `import_file(tab_id: String, parent: Option<NodePath>, source_path: String, target_name: String, overwrite: bool) -> AppResult<TreeSnapshot>`

New command, new module `src-tauri/src/commands/import.rs` (domain: `import`, per
`project-architecture/PLAN.md` §4's naming convention). Mirrors `commands/tree.rs`'s
`memo_create_impl` closely, with two differences: content comes from `source_path` instead of being
empty, and an `overwrite` branch exists.

1. `resolve_tab_access(&settings, sessions, tab_id)` (`commands/tab_lock.rs`) — on
   `TabAccess::Session`, delegate to a new `tab_lock::session_import_file(...)` (§5 below); the rest of
   this list is the `TabAccess::Plain` path.
2. Load reconciled tree (`load_and_reconcile`, same helper `memo_create_impl` uses). If `parent` is
   `Some` and not found in the tree → `NotFound` (defensive — frontend only ever passes a `parent` it
   just hit-tested against the live tree).
3. **If `overwrite: false`**: defensively re-check `target_name` isn't already used (case-insensitive,
   matching `search/index.rs`'s `eq_ignore_ascii_case` convention for name comparison) → `NAME_DUPLICATE`
   if it is (race window since the frontend's collision check).
   **If `overwrite: true`**: find the existing node by `target_name` (case-insensitive); `NotFound` if
   missing (raced deletion). If found and `node.locked == Some(true)` (Edit Lock) → reject — this
   backend check exists purely as defense-in-depth, since the frontend already disables the Overwrite
   control for an edit-locked collision target (`[detailed-spec v1.1.0 §2.1]` step 3d, §4.2's table: "no
   new `ErrorCode`... UI-level restriction"); use the same defensive fallback `editor-save/PLAN.md` §2
   step 1 already uses for its own analogous edit-lock-blocked-save case (`Unknown`) rather than inventing
   a new code — if this path is ever found reachable in a way that matters, escalate to `designer-lite`
   at that time, same policy `editor-save/PLAN.md` states for its own case.
4. Read source bytes: `fs::retry::with_retry(|| std::fs::read(&source_path))`. On failure after the
   existing retry schedule (`persistence-fs/PLAN.md` §7) → `IoLocked` (frontend treats as a skip, tallied
   in the toast, batch continues — `[detailed-spec v1.1.0 §2.1]` step 6).
5. Decode via `store/memo_file_service::decode_memo_body` (same BOM/UTF-8/Shift_JIS detection
   `editor_load` uses, `editor-save/PLAN.md` §2), then `detect_eol`/`normalize_to_lf` the same way.
6. Compute final bytes to write: `restore_eol` to the *source file's own detected* EOL, then
   `encode_for_save(_, &settings.encoding.default_save)` — same restore-then-encode pipeline
   `editor_save` steps 3–4 use, just with the EOL source being this newly-read file instead of a
   previously-loaded one (`[detailed-spec v1.1.0 §2.1]` step 4: "EOL style is detected from the source
   file and preserved on write").
7. `target_path = folder.join(format!("{target_name}.md"))`. Self-write-register, then
   `fs::atomic_write::atomic_write(&target_path, &bytes, registry)` — used uniformly (including for a
   brand-new file), matching what `memo_create_impl` itself actually does today, not the older
   discretion note in `memo-tree/PLAN.md` §3 about skipping the `.bak` swap for brand-new files.
8. **If `overwrite: false`**: insert `MetaTreeNode::default_for_new_file(target_name)` (existing
   constructor, `model/tree.rs`) into the tree at `parent`'s children end, or root end if `parent` is
   `None` — `favorite` stays absent (default), matching every other newly-created/reconciled node
   (`docs/plans/v1.1.0/favorites/PLAN.md` §1). **If `overwrite: true`**: tree is not touched (name,
   position, all metadata fields including `favorite`/`locked`/`label` are preserved — only the file
   body changed).
9. **If `overwrite: false`**: atomic-save `.tabtreenote.json` (same `TabMetaFile` save path every other
   tree-mutating command uses). **If `overwrite: true`**: no metadata save needed (nothing in the tree
   changed).
10. `search_index.upsert(tab_id, &target_name, &normalized_text)` (existing `search/index.rs` fn) —
    called in both branches. **Note for whoever implements `docs/plans/v1.1.0/favorites/PLAN.md`'s
    `IndexEntry.favorite` extension first**: `upsert` must preserve an existing entry's `favorite` value
    when patching name/plaintext for an already-present entry (the Overwrite case re-upserts a
    potentially-already-favorited memo) — it must not silently reset `favorite` to `false`.
11. Return the updated `TreeSnapshot` (`nodes`/`tab_name`/`locked`/`recovery_pending`, extended per
    `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §2 with `last_viewed_memo` — this command never changes
    that field, see §6 below).

### Locked-tab-with-no-session / crash-recovery-pending targets

Both are **unreachable drop targets** — `[spec v1.1.0 §1.5]`, `[detailed-spec v1.1.0 §2.1]` step 7: a
locked tab with no session renders no tree (`memo-tree/PLAN.md` §2 step 1), and a tab with an unresolved
`.tabtreenote.lock-inprogress`/permanent-unlock marker shows its recovery dialog instead of the tree
(`TreeSnapshot.recovery_pending`, `tab-lock/CRASH-RECOVERY.md`) — in both cases there is no tree-pane DOM
surface for an OS file-drop event to land on, so no frontend code path needs to special-case rejecting
the drop; it simply cannot be produced.

## 4. New Collision-Dialog State — `state/useImport.ts`

Given `source_paths: string[]` (from the Tauri file-drop event, filtered per §6 below) and the drop
target (`parent: NodePath | null`, resolved by hit-testing, §6 below):

1. For each path, extension-stripped base name → sanitize via `shared/nameSanitize.ts`'s existing
   replacement-suggestion function (auto-applied, never shown as a prompt — `[spec v1.1.0 §1.2]`).
2. Maintain a running `claimedNames: Set<string>` (lowercased), seeded from the active tab's current
   tree's `memoName`s. For each sanitized candidate name, in payload order:
   - **Not in `claimedNames`** → no collision; queue `{ sourcePath, targetName: sanitizedName, overwrite:
     false }`, add `sanitizedName` to `claimedNames`.
   - **In `claimedNames`**:
     - If a batch-wide policy is already set (`rename` / `overwrite` / `skip`, from an earlier "apply to
       all" choice this drop) → apply it silently (no dialog), same edit-lock-forced-to-skip override as
       below if the policy is `overwrite` and the collision target is edit-locked.
     - Else, look up whether the colliding tree node has `locked === true` (Edit Lock). If so, show
       `components/dialogs/ImportCollisionDialog.tsx` with **Overwrite disabled** (grey, non-selectable)
       — only Rename/Skip choosable (`[detailed-spec v1.1.0 §2.1]` step 3d, Q1-1). If the *batch* policy
       being set right now via "apply to all" is `overwrite` while this specific item is edit-locked, this
       item is **silently force-converted to Skip** instead (not shown a dialog at all if a batch policy
       was already active when this item was reached) and its skip is tallied.
     - Otherwise show the dialog with all three choices enabled.
   - Dialog result → `Rename`: compute `shared/uniqueName.ts`'s `resolveUniqueName(sanitizedName, n =>
     claimedNames.has(n.toLowerCase()))`, queue `{ sourcePath, targetName: <resolved>, overwrite: false
     }`, add resolved name to `claimedNames`. → `Overwrite`: queue `{ sourcePath, targetName:
     sanitizedName, overwrite: true }` (name NOT added to `claimedNames` again, it was already there).
     → `Skip`: tally a local skip, queue nothing.
   - **Esc** while the dialog is open: stop the loop entirely — this item and every remaining
     not-yet-processed item are tallied as local skips; items already queued/executed before the Esc are
     not rolled back (`[spec v1.1.0 §1.3]`).
3. Once the loop finishes (or is Esc-stopped), call `import_file` once per queued item, sequentially
   (awaiting each before starting the next, so a later item's defensive backend re-check sees the
   previous item's committed state). Track per-call success/`IoLocked`-class failure; any backend failure
   adds to the skip tally too (§3 step 4/step 3).
4. After the whole batch settles: if `imported === 0 && skipped === 0` (the payload had at least one
   `.md` file and zero were skipped for any reason — everything imported cleanly), show nothing. If
   `skipped > 0`, show the summary toast (§7 below), e.g. "2 imported, 1 skipped (not .md)" — exact copy
   is implementation discretion (`[detailed-spec v1.1.0 §5]`).
5. **No selection/scroll-position side effects**: this hook never touches which memo is open in the
   editor or which tree row is selected/scrolled-to, and never calls
   `docs/plans/v1.1.0/tab-memo-memory/PLAN.md`'s "write last-viewed memo" step (Q1-2) — whatever was open
   before the drop stays open/selected exactly as it was, even for the very last item processed.

While `ImportCollisionDialog` is open, no other tree operation on the target tab is accepted (`[spec
v1.1.0 §1.3]`) — reuses the existing input-lock-during-resolution convention already applied to the
external-edit-conflict dialog (`editor-save/PLAN.md` §3); if a second OS file-drop lands on the same tab
while one collision dialog is already pending, `useImport.ts` ignores it until the current one resolves.

## 5. Locked-Tab-With-Active-Session Target

`import_file`'s `TabAccess::Session` branch (§3 step 1) delegates to a new
`tab_lock::session_import_file(sessions, registry, folder, tab_id, target_name, content_bytes, overwrite,
parent) -> AppResult<TreeSnapshot>`, added to `src-tauri/src/commands/tab_lock.rs` alongside its existing
`session_create_memo`/`session_editor_save` (`tab-lock/PLAN.md` §3 "While a Session Is Active" — every
tree/editor mutation must branch through `TabAccess` rather than duplicating logic). Behaviorally:
encrypts the read source content into the session's in-memory tree/file-map state and re-saves the lock
blob (fresh IV), same as the plain path's steps 8–10 but through the session store instead of a plain
`.md` file — `[spec v1.1.0 §1.5]`: "proceeds exactly like dropping onto a normal unlocked tab." The
Overwrite sub-case's edit-lock check (§3 step 3) is unaffected — `MetaTreeNode.locked` (Edit Lock) is
orthogonal to Tab Lock session state.

## 6. Frontend Drop Surface (`components/Tree/TreeView.tsx`)

- Listens for the same Tauri file-drop event `tab-management/PLAN.md` §4 already uses for the
  drop-onto-tab-bar gesture, but scoped to the tree pane's DOM area instead of the tab bar. Requires no
  new Tauri config — `dragDropEnabled` is already `true` (`tab-management/PLAN.md` §4's implementation
  note) for the existing tab-bar gesture to work at all.
- **Filter**: partition the payload's paths by `.md` extension (case-insensitive); every non-`.md` path
  (folders included, `[spec v1.1.0 §1.1]`) is immediately counted as a local skip, no further processing.
  If every path in the payload is non-`.md`, the summary toast still fires ("0 imported, N skipped (not
  .md)") — this is not the fully-silent zero-`.md`-items path (`[spec v1.1.0 §1.2]`).
- **Placement**: reuses whatever hover-zone/hit-testing `memo-tree/PLAN.md` §4's internal DND already
  implements for distinguishing "drop on empty background" vs. "drop while hovering node X" — dropped on
  empty background → `parent: null` (root-level end); dropped hovering an existing row → `parent:` that
  row's `NodePath` (children end) — `[spec v1.1.0 §1.4]`. Exact hover-zone pixel geometry is not
  redefined here, same reuse the internal-DND-authored plan itself calls for.
- **Drag-over visual feedback**: reuses whatever hover-highlight affordance internal DND already shows
  for a drag hovering a row/background (`[detailed-spec v1.1.0 §3.1]`) — implementation discretion, not a
  new visual language.
- Multiple simultaneously-dropped files are processed in whatever order the OS reports the payload
  (`[spec v1.1.0 §1.4]`, explicit discretion).

## 7. New Shared Primitive: `components/Toast/Toast.tsx`

No lightweight non-blocking notification component exists yet in `src/` (checked: nothing in `src/`
currently listens for `evt:meta:recovered` or renders a toast/notice, despite
`persistence-fs/PLAN.md` §6.3 describing one for settings-corruption recovery — that wiring was never
built). This feature introduces the first one:

- `state/useToast.ts`: a small in-memory queue (`{ id, message, kind }[]`), `pushToast(message, kind?)`,
  auto-dismiss after a fixed duration (implementation discretion) or manual close.
- `components/Toast/Toast.tsx`: renders the queue as stacked, dismissible, non-blocking banners — **not**
  built from the shared `Dialog.tsx` (`components/dialogs/`), since (a) this feature's summary toast has
  no user decision to make, and (b) `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md` §4 reuses this same
  primitive for its pre-timeout warning, which must specifically **not** be a modal `Dialog` instance
  (would pause that feature's own inactivity timer).
- This feature's own use: the skip-summary toast (§4 step 4 above). `evt:meta:recovered` wiring remains
  out of scope for this plan (pre-existing gap, not introduced or worsened by this feature) — flagged
  here only because this is where the primitive that could eventually fix it first gets built.

## 8. Error Handling

No new `ErrorCode` values (`[detailed-spec v1.1.0 §4.2]`). Sanitization auto-applies client-side
(never surfaced). Collisions are UI flow, not errors. Edit-lock-blocked Overwrite is a UI-level
restriction. Source-read failure reuses `IoLocked`. See §3 above for each reused code's exact call site.

## Open Items

None. All discretion points above (toast copy/duration, drag-over visual, per-item vs. coalesced
metadata save — moot here since this plan's design already does one `import_file` call per item, so
"coalesced" doesn't apply — hover-zone geometry reuse, payload processing order) are the same class of
implementation-discretion item `docs/detailed-spec.v1.1.0.md` §5 already lists as not requiring a
decision, and are called out inline above rather than escalated.
