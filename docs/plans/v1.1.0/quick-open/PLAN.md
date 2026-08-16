# PLAN: Cross-Tab Quick Open (v1.1.0)

> Status: planning complete. Default shortcut is confirmed as `Ctrl+O` — see §4 below (previously blocked
> on `docs/QA.md`; resolved by the user and reflected into `docs/spec.v1.1.0.md` §3.1 and
> `docs/detailed-spec.v1.1.0.md` §3.3, `docs/QA.md` is empty again). New feature, no existing v1.0.0 plan
> owns this surface. Depends on `docs/plans/search-index/PLAN.md` (`SearchIndex`,
> `IndexEntry` — extended here and by `docs/plans/v1.1.0/favorites/PLAN.md`), `docs/plans/v1.1.0/
> tab-memo-memory/PLAN.md` (`TabViewStore` for the "last viewed" ordering signal, `restoreTabView`-bypass
> rule for a selected result), `docs/plans/v1.1.0/favorites/PLAN.md` (`memo_set_favorite`, this plan's
> star-click target), `docs/plans/window-operation/PLAN.md` §3 (shortcut system), and
> `docs/plans/project-architecture/PLAN.md` §2 (`components/dialogs/`).

Sources: `docs/spec.v1.1.0.md` §3; `docs/detailed-spec.v1.1.0.md` §1.3, §1.4, §2.3, §3.3.

---

## 1. Scope Recap

A keyboard-driven overlay: type to live-filter memo **display names** (not body content) across every
tab, jump-open a result. Distinct from the existing cross-tab **content** search
(`search-index/PLAN.md` §5, `components/Search/CrossTabSearchDialog.tsx`) — different trigger, different
matching target, same underlying index as its data source only.

## 2. Backend

### `search_quick_open(query: String) -> AppResult<Vec<QuickOpenHit>>`

New command, `commands/search.rs` (extends the existing `search` domain file — `search_query` already
lives there per `search-index/PLAN.md` §4).

```rust
pub struct QuickOpenHit {
    pub tab_id: String,
    pub tab_name: String,
    pub memo_name: String,
    pub is_last_viewed: bool,
    pub is_favorite: bool,
}
```

- **Candidate set**: `SearchIndex.entries` (`search/index.rs`) directly — no new index or data structure
  (`[spec v1.1.0 §3.3]`). Locked-tab exclusion is automatic and requires no special-casing here, since a
  locked tab (whether locked-with-no-session or locked-with-an-active-session) never has entries in this
  index to begin with (`search-index/PLAN.md` §1).
- **Matching**:
  - **Empty query**: every entry is a candidate — not an empty result list (Q3-1).
  - **Non-empty query**: case-insensitive substring match against `IndexEntry.plaintext_name`, using the
    exact same technique `commands/search.rs`'s existing `build_matcher`'s case-insensitive branch already
    uses (`text.to_lowercase().contains(&needle)`) — applied to `plaintext_name` instead of `plaintext`
    (Q3-2). `plaintext_name` is not currently lowercased at index-build time (confirmed by reading
    `search/index.rs`'s `upsert`), so the lowercasing happens at query time here, same as `build_matcher`
    already does for its own case-insensitive path.
- **Enrichment**: for each candidate, resolve `tab_name` from `AppSettings.tabs` (the command takes
  `store: State<AppSettingsStore>`, same as `search_query`); `is_last_viewed = TabViewStore::get(tab_id)
  == Some(memo_name)` (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §3 — exact-match comparison, not
  substring); `is_favorite = entry.favorite` (`docs/plans/v1.1.0/favorites/PLAN.md` §2's `IndexEntry`
  extension).
- **Ordering** (applies identically to the empty-query full list and any filtered subset, `[spec v1.1.0
  §3.2]`): `is_last_viewed == true` sorts first; tie-break (including among the remainder, and among
  multiple `is_last_viewed` matches) is alphabetical by `memo_name` (case-insensitive compare). This exact
  rule is reused **verbatim** by `docs/plans/v1.1.0/favorites/PLAN.md` §3 — that plan cites this section
  rather than re-deriving it (`[spec v1.1.0 §4.3]`, "identical rule... reused verbatim").
- **Result cap**: capped and truncated after sorting; exact count is implementation discretion
  (`[detailed-spec v1.1.0 §5]`, spec's own explicit discretion callout) — a constant in `search/index.rs`
  or `commands/search.rs`, e.g. `QUICK_OPEN_RESULT_CAP`.

## 3. Frontend Overlay

New `components/dialogs/QuickOpenDialog.tsx`, built on the shared `Dialog.tsx`
(`project-architecture/PLAN.md` §2) — text input (autofocused on open) + result list below, Esc-closable
(`[spec v1.1.0 §3.5]`). New `state/useQuickOpen.ts` hook: holds open/closed state, current query,
debounced (or direct, implementation discretion) calls to `search_quick_open`, and the
highlighted-result index for keyboard nav.

- **Initial state on open**: shows the full, ordering-rule-sorted candidate list, not an empty result
  (Q3-1) — the first `search_quick_open("")` call fires the moment the overlay opens.
- **Live re-query on every keystroke.**
- **Result row**: display name + owning tab name (two-column shape, `[spec v1.1.0 §3.2]` — needed since
  memo names aren't guaranteed unique across tabs, `[spec §2.1]`/`[spec §3.1]`) + a star glyph reflecting
  `isFavorite`.
- **Keyboard**: Up/Down moves the highlighted row; Enter opens the highlighted result, or the top result
  if none is highlighted yet (meaningful now that the list is never empty on open, Q3-1); Esc dismisses
  without acting.
- **Star click** (Q4-3): a click target **distinct from the rest of the row** — clicking it calls
  `docs/plans/v1.1.0/favorites/PLAN.md` §2's `memo_set_favorite(tabId, memoName, !isFavorite)` directly
  for that specific row (no "current tree selection" precondition — the row itself unambiguously
  identifies the target). It does **not** also select/navigate to the result. After toggling, the overlay
  stays open and that row's star glyph updates in place; the list's order is unaffected (ordering never
  depends on `favorite`, only on `is_last_viewed`/alphabetical, §2 above).
- **Selecting a result** (click the row body, outside the star; or Enter): switches to that result's
  owning tab if it isn't already active (existing tab-switch mechanism, `state/useTabs.ts`), then opens
  the specific selected memo directly via the normal `editor_load`/tree-select path (same end effect as
  clicking the tab then the tree row — `memo-tree/PLAN.md` §4's existing tree-selection-auto-follow
  behavior applies here too, no new mechanism) and calls `tree_set_last_viewed(tabId, memoName)`
  (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §4) **directly** — this is the `restoreTabView`-bypass
  case that plan's §6 documents (Q2-2): selecting a Quick Open result never calls `restoreTabView` for the
  target tab.
- **Dismissal**: Esc closes the overlay without acting — no tab switch, no memo opened. A star toggle
  already committed before Esc stays committed (it was a direct, already-completed backend call, not a
  pending/undoable local edit).

## 4. Trigger / Shortcut

New action id `search.quickOpen` in `shared/shortcuts.ts`'s `DEFAULT_SHORTCUTS`
(`window-operation/PLAN.md` §3's existing rebindable-shortcut mechanism — no new mechanism, just one more
entry) — rebindable via the Settings → ショートカット tab like every other action.

**Default combo, confirmed: `Ctrl+O`** (`[spec v1.1.0 §3.1]`, resolved via `docs/QA.md` after
developer-lite found the originally-specified `Ctrl+Shift+P` already bound to `markdown.paneModeCycle` in
the live `src/shared/shortcuts.ts`). `Ctrl+O` was verified against that same live `DEFAULT_SHORTCUTS` map
(not just `window-operation/PLAN.md`'s original table, which misses later additions like
`markdown.paneModeCycle`'s `Ctrl+Shift+P` and `markdown.pdfExport`'s `Ctrl+P`) and confirmed unused as of
this writing. `markdown.paneModeCycle`'s existing `Ctrl+Shift+P` default is left untouched — this feature
does not reassign it. Add to `shared/shortcuts.ts`:

```ts
"search.quickOpen": "Ctrl+O",
```

## Open Items

None.
