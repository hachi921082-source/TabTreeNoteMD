# PLAN: Per-Tab Last-Viewed-Memo and Scroll-Position Memory (v1.1.0)

> Status: planning complete, not yet implemented. Extends `docs/plans/tab-management/PLAN.md` (`TabMeta`
> shape, owned by `persistence-fs/PLAN.md`'s settings/metadata store), `docs/plans/memo-tree/PLAN.md`
> (`TabMetaFile`, `TreeSnapshot`, `tree_open`), `docs/plans/tab-lock/PLAN.md` (`LockPayload`, session
> write path), and `docs/plans/editor-save/PLAN.md` (startup auto-open, `MemoEditorSettings.scrollPosition`
> reuse — unchanged by this feature). Every other v1.1.0 plan that opens a specific memo directly
> (`docs/plans/v1.1.0/quick-open/PLAN.md`, `docs/plans/v1.1.0/favorites/PLAN.md`) cites this file's
> `restoreTabView`/"write last-viewed memo" split rather than repeating it.

Sources: `docs/spec.v1.1.0.md` §2; `docs/detailed-spec.v1.1.0.md` §1.1, §2.2, §3.2.

---

## 1. Data Model

Extends `TabMetaFile` (`src-tauri/src/model/tab_meta.rs`, `memo-tree/PLAN.md` §1) with a new top-level
field, sibling to `tree`:

```rust
pub struct TabMetaFile {
    pub schema_version: u32,
    pub app_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<Vec<MetaTreeNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_viewed_memo: Option<String>,   // NEW — [spec v1.1.0 §2.2]
}
```

- Absent and `None` are equivalent (never-viewed tab, or a pre-upgrade file predating this field).
- **Locked-tab placement**: for a locked tab, this value must live inside the decrypted `LockPayload`
  (`src-tauri/src/crypto/lock_blob.rs`), not as a plaintext sibling of `TabMetaFile.lock` — even *which*
  memo was last viewed must not be readable without the password (`[spec v1.1.0 §2.2]`):

```rust
pub struct LockPayload {
    pub real_tab_name: String,
    pub tree: Vec<MetaTreeNode>,
    pub file_map: Vec<FileMapEntry>,
    pub last_viewed_memo: Option<String>,   // NEW — plaintext only inside the decrypted payload
}
```

- **`schemaVersion` bump**: `TAB_META_SCHEMA_VERSION` (`src-tauri/src/model/tab_meta.rs`, currently `1`)
  bumps to `2`, **combined** with `docs/plans/v1.1.0/favorites/PLAN.md`'s `MetaTreeNode.favorite`
  addition — a single bump covers both, not two increments (`[detailed-spec v1.1.0 §4.1]`). **Coordination
  note**: whichever of this plan and `favorites/PLAN.md` is implemented first performs the actual constant
  bump (`1` → `2`); the second implementation pass must verify the constant wasn't already bumped by the
  other (i.e. check for a pre-existing `2` before changing it) rather than assuming it starts this session
  at `1`.
- No destructive migration: a pre-upgrade file simply parses with `last_viewed_memo: None` via
  `#[serde(default)]`.

### `AppSettings.lastOpened` Narrowing

```rust
// src-tauri/src/model/app_settings.rs — CHANGED
pub struct LastOpened {
    pub tab_id: String,
    // "memo_name: String" field REMOVED
}
```

- **`schemaVersion` bump**: `APP_SETTINGS_SCHEMA_VERSION` (currently `1`) bumps to `2`, **combined** with
  `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md`'s `tabLockAutoRelockMinutes` addition — same
  single-bump, same first-implementer-bumps / second-implementer-verifies coordination note as above.
- No destructive migration: a pre-upgrade `settings.json` with a `lastOpened.memoName` field simply has
  that field ignored by `serde` (unknown-field tolerance, already the default for this struct) —
  `[spec v1.1.0 §2.2]`: "a pre-upgrade `lastOpened.memoName` value is simply not read going forward."

## 2. `TreeSnapshot` Extension

`commands/tree.rs`'s `TreeSnapshot` (`memo-tree/PLAN.md` §2) gains one field so the frontend receives
`lastViewedMemo` in the same round trip as the reconciled tree, rather than a second call:

```rust
pub struct TreeSnapshot {
    pub nodes: Vec<MetaTreeNode>,
    pub tab_name: String,
    pub locked: bool,
    pub recovery_pending: bool,
    pub last_viewed_memo: Option<String>,   // NEW
}
```

`tree_open` populates it from `TabMetaFile.last_viewed_memo` (plain tabs) or the active session's
`LockPayload.last_viewed_memo` (locked-with-session tabs, via the same session-store read path
`memo-tree/PLAN.md` §2 step 2 already uses for the rest of the tree). **Reconciliation**: if
`last_viewed_memo` names a memo `[detailed-spec §3.5]` reconciliation dropped, `tree_open` returns it
unchanged (the raw persisted/session value) — reconciling *that* value down to `None` is the frontend's
`restoreTabView`'s job (§4 below), matching the existing pattern where `tree_open` returns the reconciled
`nodes` list but leaves "does this pointer still resolve" checks to the caller.

## 3. New Runtime Cache — `store/tab_store.rs`

`project-architecture/PLAN.md` §2 already reserves this module path in its directory layout (not yet
created in `src/`). This feature is what actually needs it, to let
`docs/plans/v1.1.0/quick-open/PLAN.md` and `docs/plans/v1.1.0/favorites/PLAN.md` sort results by
"currently this tab's last-viewed memo" without re-reading every registered tab's `.tabtreenote.json`
from disk on every keystroke.

```rust
pub struct TabViewStore {
    last_viewed: Mutex<HashMap<String, Option<String>>>,   // tab_id -> current lastViewedMemo
}

impl TabViewStore {
    pub fn set(&self, tab_id: &str, memo_name: Option<String>);
    pub fn get(&self, tab_id: &str) -> Option<String>;   // None if never populated OR value is None
}
```

Registered as Tauri managed state (`app.manage(TabViewStore::default())`), same pattern as
`SearchIndex`/`LockSessionStore`. Kept in sync at exactly two points, both already-existing call sites:

- `tree_open` (§2 above): after resolving `last_viewed_memo` for a tab, also `TabViewStore::set(tab_id,
  value)` — this both populates the cache lazily (first time a tab is opened this run) and refreshes it
  if the on-disk/session value changed underneath (e.g. crash-recovery flat-rebuild).
- `tree_set_last_viewed` (§4 below): updates the cache in the same call that persists the new value, so
  the two never drift within a running session.

This is the same "runtime cache mirroring persisted state, updated at both the read and write call sites"
pattern already established for `AppSettings.tabs[i].locked` (`tab-management/PLAN.md` §3).

## 4. Write Path — `tree_set_last_viewed(tab_id: String, memo_name: Option<String>) -> AppResult<()>`

New command, `commands/tree.rs` (tree domain — same file as `memo_set_expanded`/`memo_set_edit_lock`,
even though this field is a `TabMetaFile`-level sibling of `tree` rather than a per-node field, since it
persists through the same store/file). Pure metadata mutation, no file-body I/O:

1. `resolve_tab_access` — `TabAccess::Plain`: read-modify-write `TabMetaFile.last_viewed_memo`, atomic-save.
   `TabAccess::Session`: same for the active session's in-memory `LockPayload.last_viewed_memo`, then
   re-encrypt+atomic-save the lock blob (fresh IV) — same "structure change → re-encrypt and re-save the
   blob" rule `tab-lock/PLAN.md` §3 "While a Session Is Active" already states for any session-tree
   mutation.
2. `TabViewStore::set(tab_id, memo_name)` (§3 above).
3. Returns `()` — the frontend already holds the authoritative in-memory tree/selection state; no
   `TreeSnapshot` round-trip is needed for this narrow mutation (unlike `memo_set_expanded` etc., which
   return one because they also touch node-level fields the frontend might not have echoed locally yet).

**Not a new save-trigger category** (`[spec v1.1.0 §2.3]`): this command is called from the same
trigger points that already exist — memo click, arrow-key nav, `memo:` link navigation, tab switch — at
the same moment the *outgoing* memo's `MemoEditorSettings.scrollPosition` is captured and persisted via
the existing `memo_set_editor_override` (`editor-save/PLAN.md` "Scroll Position Write Path"). It rides
along, it does not introduce a new debounce/flush timer.

**Not called by**:
- Import (`docs/plans/v1.1.0/folder-import/PLAN.md` §4 step 5, Q1-2) — imported memos never become a
  tab's `lastViewedMemo`.
- A Quick Open star-click favorite toggle (`docs/plans/v1.1.0/quick-open/PLAN.md`) — toggling favorite
  status is orthogonal to "last viewed."
- Selecting a Quick Open / Favorites result for a *different* memo than what `restoreTabView` would have
  restored — see §6 below; those call this command directly with the *selected* memo, bypassing
  `restoreTabView` for that tab switch specifically (Q2-2).

## 5. `restoreTabView(tabId)` — Single Restore Function, Two Call Sites

New frontend function, `state/useTabView.ts` (composes `state/useTree.ts`'s tree-loading and
`state/useEditor.ts`'s memo-opening, per `[spec v1.1.0 §2.2]`'s "startup restore and mid-session
tab-switch restore become the same code path"):

1. Call `tree_open(tabId)` (already happens on any tab switch regardless of this feature) → read
   `TreeSnapshot.last_viewed_memo`.
2. If `null`/absent, **or** the named memo is not present among the reconciled `nodes` (deleted, renamed
   away, or dropped by reconciliation): editor pane shows no memo open, tree shows no row selected — same
   end state as `editor-save/PLAN.md` §6's existing "if the referenced memo no longer exists... do
   nothing" rule, reused verbatim, not re-implemented.
3. Otherwise: open the named memo via the normal `editor_load` path (`editor-save/PLAN.md` §2) — this
   already returns/restores `MemoEditorSettings.scrollPosition` exactly as an ordinary memo-switch does;
   no change needed there. Opening the memo through the normal path also triggers
   `memo-tree/PLAN.md` §4's existing "tree selection highlight + scroll position auto-follow to the
   newly-open memo" behavior automatically (`[spec v1.1.0 §2.3]`'s "tree-pane visual restoration" bullet
   is therefore **not** a new mechanism — it falls out of the existing auto-follow behavior once step 3
   opens the memo through the same code path any other memo-open uses).

### Call sites

- **App startup**: after `settings_get()` resolves `AppSettings.lastOpened` (now `{ tabId } | null`), if
  `tabId` still exists in `AppSettings.tabs`, call `restoreTabView(tabId)`. If the tab no longer exists,
  do nothing at the tab level (matches `editor-save/PLAN.md` §6's existing rule, now reached via this
  function instead of directly reading a since-removed `lastOpened.memoName`).
- **Ordinary in-session tab switch** (click, `tab.switchNext`/`tab.switchPrev` shortcuts) to a tab that is
  **not** in the middle of a Tab-Lock session start (see §6 below).

## 6. Explicit Exclusions From `restoreTabView` (Not Literal Restatements of `spec.v1.1.0.md` — User Decisions From the Q&A Round)

- **Tab-Lock session start** (`tab_unlock` succeeds): `restoreTabView` is **not** called for this
  transition (Q2-1) — the session begins with the editor showing no memo open and the tree showing no row
  selected, regardless of what `last_viewed_memo` holds. The user must click a row even on the very first
  moment content becomes decryptable. **Practical consequence, not an independent decision**: because a
  session always ends the instant its tab is left (`[spec §4.1 pt.5]`), the *only* moment a locked tab's
  tree is ever shown is "just after a session starts" — which this rule excludes — so a locked tab's
  `last_viewed_memo` is written every time the open memo within a session changes (§4 above, session
  branch) but is **never read back automatically anywhere in v1.1.0**. It exists purely for data-model
  symmetry with the unlocked case.
- **Selecting a Quick Open or Favorites result that targets a different tab**
  (`docs/plans/v1.1.0/quick-open/PLAN.md` §5, `docs/plans/v1.1.0/favorites/PLAN.md` §3): switches to that
  tab but does **not** call `restoreTabView` for it — instead opens the *specific* selected memo directly
  and calls `tree_set_last_viewed` with that memo (Q2-2). Those two plans implement this by calling
  `editor_load`/tree-select directly, then `tree_set_last_viewed`, never `restoreTabView`.
- **Tab never viewed yet** (`last_viewed_memo` still `null`/absent): switching to it shows no memo
  pre-selected — this is just step 2 of `restoreTabView` above, not a separate rule.

## Open Items

None.
