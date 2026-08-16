# PLAN: Cross-Tab Favorites (v1.1.0)

> Status: planning complete, not yet implemented. New feature, no existing v1.0.0 plan owns this surface.
> Extends `docs/plans/memo-tree/PLAN.md` (`MetaTreeNode`, tree right-click context menu) and
> `docs/plans/search-index/PLAN.md` (`IndexEntry`, `SearchIndex`). Depends on
> `docs/plans/tab-management/PLAN.md` §4 (toolbar group placement) and reuses
> `docs/plans/v1.1.0/quick-open/PLAN.md` §2's `QuickOpenHit` shape and ordering rule verbatim, and its §3
> overlay component in a second "favorites mode." `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §1 owns the
> coordination note for this plan's combined `TabMeta.schemaVersion` bump.

Sources: `docs/spec.v1.1.0.md` §4; `docs/detailed-spec.v1.1.0.md` §1.1, §1.3, §2.2, §2.3, §3.4.

---

## 1. Data Model

Extends `MetaTreeNode` (`src-tauri/src/model/tree.rs`, `memo-tree/PLAN.md` §1) with one optional field,
sibling to the existing `locked`:

```rust
pub struct MetaTreeNode {
    pub memo_name: String,
    pub expanded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Label>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<MemoEditorSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite: Option<bool>,   // NEW — [spec v1.1.0 §4.1]
    #[serde(default)]
    pub children: Vec<MetaTreeNode>,
}
```

- Absent and `Some(false)` are equivalent — mirrors how `locked`/`label` are already treated. Every
  freshly-created memo (`memo_create`/`memo_create_child`), every reconciliation-appended entry
  (`[detailed-spec §3.5]`), and every memo imported via `docs/plans/v1.1.0/folder-import/PLAN.md`
  (`MetaTreeNode::default_for_new_file`) starts with `favorite` absent.
- Mirrored onto `shared/models.ts`'s `TreeNode`/`MetaTreeNode` TS types as `favorite?: boolean`.
- **No new Tab-Lock placement decision needed** — `favorite` rides inside the same `tree: Vec<MetaTreeNode>`
  field that is already either fully plaintext (unlocked tab, `TabMetaFile.tree`) or fully inside the
  encrypted blob (`LockPayload.tree`, `tab-lock/PLAN.md` §2) — unlike
  `docs/plans/v1.1.0/tab-memo-memory/PLAN.md`'s `lastViewedMemo`, which needed a fresh placement decision
  because it is a new *top-level* field sibling to `tree` itself (`[spec v1.1.0 §4.1]`).
- **`schemaVersion` bump**: combined with `tab-memo-memory/PLAN.md`'s `TabMetaFile.last_viewed_memo`
  addition into a single `TAB_META_SCHEMA_VERSION` bump (`1` → `2`) — see that plan's §1 for the exact
  coordination note (whichever of the two plans is implemented first performs the bump; the second
  verifies rather than re-bumps).

## 2. Toggle Command — `memo_set_favorite(tab_id: String, path: NodePath, favorite: bool) -> AppResult<TreeSnapshot>`

New command, `commands/tree.rs`, structurally identical to the existing `memo_set_edit_lock`
(`memo-tree/PLAN.md` §3) — same `mutate_node_field` helper, same `TabAccess` dispatch (plain tabs mutate
`TabMetaFile.tree` in place; session tabs mutate the active session's in-memory `LockPayload.tree` and
re-encrypt+re-save the blob, per `tab-lock/PLAN.md` §3 "While a Session Is Active"):

```rust
pub fn memo_set_favorite(/* app, tab_id, path, favorite, store, registry, sessions, search_index */) -> AppResult<TreeSnapshot> {
    let result = mutate_node_field(..., |node| {
        node.favorite = Some(favorite);
        Ok(())
    });
    // On success, also patch the index (unlocked-tab case only — see §3):
    search_index.set_favorite(&tab_id, &path, favorite);
    result
}
```

- `search_index.set_favorite` is a no-op if the tab has no entries in the index at all (locked tab,
  whether no-session or in-session — `[spec v1.1.0 §4.4]`, consistent with the index's existing
  unconditional-locked-tab-exclusion rule, `search-index/PLAN.md` §1). This command still succeeds for a
  locked-session tab's `MetaTreeNode.favorite` mutation itself — only the index side-effect is skipped.
- **Two call sites** (`[detailed-spec v1.1.0 §2.2]`):
  1. Tree right-click context menu (§4 below) — single-target = the current tree selection; **disabled**
     when the selection is not exactly one node, same precondition already used for the Edit-Lock menu
     item (`[detailed-spec §5.2]`, enforced client-side in `components/ContextMenu/`, same as today).
  2. The star glyph on a Quick Open result row (`docs/plans/v1.1.0/quick-open/PLAN.md` §3, Q4-3) —
     target is the row's own `{tabId, memoName}`, no selection precondition.
  Both surfaces call this same command; only how `{tab_id, path}` is resolved differs.
- **Not called by**: importing a memo (`docs/plans/v1.1.0/folder-import/PLAN.md`) — imported memos always
  start unfavorited, no separate mechanism needed since that's already `MetaTreeNode.favorite`'s default
  (§1 above).

## 3. Full-Text Index Extension

`search/index.rs`'s `IndexEntry` gains one field:

```rust
pub struct IndexEntry {
    pub tab_id: String,
    pub memo_name: String,
    pub plaintext: String,
    pub plaintext_name: String,
    pub favorite: bool,   // NEW — [spec v1.1.0 §4.3]
}
```

- New method: `pub fn set_favorite(&self, tab_id: &str, memo_name: &str, favorite: bool)` — finds the
  matching entry (`tab_id` + `memo_name.eq_ignore_ascii_case`, same comparison `upsert`/`remove` already
  use) and sets `.favorite`; no-op if no matching entry exists (§2 above's locked-tab case).
- **`upsert()` must preserve `favorite` across an in-place update.** Its current implementation
  (`search/index.rs`) overwrites `memo_name`/`plaintext`/`plaintext_name` on an existing entry but has no
  `favorite` field to preserve today; once this field exists, the match-and-update branch must **not**
  reset `favorite` to `false` — every other in-app mutation that calls `upsert` for an already-favorited
  memo (rename, ordinary save, and `docs/plans/v1.1.0/folder-import/PLAN.md`'s Overwrite-collision
  re-import) must leave that memo's favorite status untouched. Only the "brand-new entry" branch of
  `upsert` (nothing matched) sets `favorite: false` — matching `MetaTreeNode.favorite`'s own
  absent-by-default rule (§1 above).
- Index-build-at-startup (`search-index/PLAN.md` §1) reads each reconciled `MetaTreeNode.favorite` (or
  its absence, coerced to `false`) into the corresponding fresh `IndexEntry.favorite` for every unlocked
  tab.

## 4. Frontend — Toggle UI (Tree Context Menu)

Extends `memo-tree/PLAN.md` §4's tree right-click context menu (`components/ContextMenu/`) with one more
entry, alongside "Rename," "Delete," "Create Memo," "Create as Child," "Icon," "Properties": a
label-flipping toggle — "お気に入りに追加" when `!node.favorite`, "お気に入りから削除" when
`node.favorite` — invoking `memo_set_favorite` with the flipped value. Same single-selection-only
disabling as Edit Lock (`[detailed-spec v1.1.0 §3.4]`, `[detailed-spec §5.2]`).

## 5. Frontend — Cross-Tab List View

### `search_get_favorites() -> AppResult<Vec<QuickOpenHit>>`

New command, `commands/search.rs`. Reuses `docs/plans/v1.1.0/quick-open/PLAN.md` §2's `QuickOpenHit`
struct verbatim (`{ tab_id, tab_name, memo_name, is_last_viewed, is_favorite }` — `is_favorite` is always
`true` for every row here, kept in the shape for row-rendering consistency with Quick Open). No query
string input; filters `SearchIndex.entries` by `favorite == true` instead of a name-substring match, same
enrichment (`tab_name`, `is_last_viewed` via `TabViewStore`) and the exact same ordering rule as
`quick-open/PLAN.md` §2 (own-tab `is_last_viewed` first, then alphabetical) — reused verbatim, not
re-derived (`[spec v1.1.0 §4.3]`). **No result cap** (Q4-2) — unlike Quick Open, this command does not
truncate; every favorited, index-eligible memo is returned.

### UI Component

Reuses `docs/plans/v1.1.0/quick-open/PLAN.md` §3's `QuickOpenDialog`/`state/useQuickOpen.ts` in a second
mode (a `mode: 'quickOpen' | 'favorites'` prop/flag) rather than a separate component — the query text
input is hidden/disabled and the data source is `search_get_favorites()` (called once on open, no live
re-query per keystroke) instead of `search_quick_open(query)`. This is the "starting default" choice
among the two `[detailed-spec v1.1.0 §5]` explicitly leaves as implementation discretion (reuse vs.
separate component) — chosen for reuse to avoid duplicating the two-column-plus-star row rendering and
keyboard-nav logic; `developer-lite` may split it into a dedicated component later if the shared one grows
awkward carrying two modes.

- **Star is read-only in this mode** — not a click target (reverting an earlier draft's proposed
  extension of Quick Open's click-to-toggle star; the user reviewed and declined it,
  `[detailed-spec v1.1.0 §0]`). Removing a favorite is **only** available via the tree's right-click menu
  (§4 above) — there is no in-list removal affordance.
- **Selecting a row** (click anywhere on it, since the star isn't a separate target here): identical
  jump-to-tab-and-open behavior as `quick-open/PLAN.md` §3's "Selecting a result" — reused verbatim,
  including the `restoreTabView`-bypass + direct `tree_set_last_viewed` call
  (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §6, Q2-2) — `[spec v1.1.0 §4.5]`.
- **Dismissal**: Esc closes, consistent with the shared `Dialog.tsx` convention this component is built
  on.

## 6. Trigger

New toolbar ★ button in the tab bar's toolbar group (`tab-management/PLAN.md` §4 — "pin, search, theme
toggle," left-to-right), added immediately to the left of the pin button (mirroring how the pin button
itself is described as sitting "immediately to the left of the search button," `[spec §2.1]`) — always
visible, single click target, no default keyboard shortcut. An action id (`favorites.open`) may still
exist in the shortcut registry with **no entry** in `DEFAULT_SHORTCUTS`, so a user can bind one via the
Settings screen's Shortcuts tab if they choose to, without this feature shipping a default combo
(`[spec v1.1.0 §4.6]`).

## 7. Tab Lock Exclusion

Automatic via the shared index (§3 above) — a locked tab's memos, whether locked-with-no-session or
locked-with-an-active-session, never have index entries, so `search_get_favorites` never returns them,
for the same reason `search_quick_open` never does (`[spec v1.1.0 §4.4]`). No separate exclusion logic.

## Open Items

None.
