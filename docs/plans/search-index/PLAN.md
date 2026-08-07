# PLAN: Search & Full-Text Index

> Status: implemented, except the external-change update path (§1's "path 2"): `persistence-fs
> /PLAN.md` §5's file-watcher pipeline is itself not yet instantiated for any live tab folder (see
> that plan's own status line and `main.rs`'s note), so there is currently no call site that would
> invoke `SearchIndex::upsert`/`remove` from a watcher-detected external change — that plan's own
> text already describes the eventual integration as "calls the same patch function," so no
> separate adapter is needed here once it exists. Everything else — startup build (§1 "path 1"'s
> in-app triggers), the `IndexEntry`/`SearchIndex` data structure and patch operations (§2), the
> fence-aware plaintext conversion (§3), `search_query` (§4), and the cross-tab search UI (§5) — is
> implemented. Also wired beyond the plan's own explicit list: the three locked-session cross-tab
> move combinations in `commands/tab_lock.rs` (`move_cross_tab_plain_to_session`/
> `_session_to_plain`/`_session_to_session`), since those are part of `memo_move`'s full behavior
> surface and directly implicate this plan's "locked tabs are unconditionally excluded" invariant.
> Not wired: `tab_register_existing` (a newly-registered folder's pre-existing `.md` files aren't
> indexed until the next app restart) and `tab_remove` (a removed tab's entries aren't purged from
> memory, though `search_query` defensively filters them out of results either way) — neither
> command is in this plan's own §1 trigger list or the implementing task's explicit scope; flagged
> here for a future pass rather than decided unilaterally.
>
> Depends on `project-architecture/PLAN.md` and `persistence-fs/PLAN.md` (the watcher pipeline
> feeds this index's external-change update path). In-memo search/replace is specified in
> `editor-save/PLAN.md` §5 (it's a frontend/CodeMirror concern with no backend index involvement)
> — this file covers only the cross-tab full-text index.

Sources: `docs/spec.md` §2.4, §4.1 pt.6, §4.2, §6; `docs/detailed-spec.md` §4.4, §5.4.

---

## 1. Index Scope and Lifecycle

- **Built at startup**: for every registered, currently-**unlocked** tab, scan direct-child `.md`
  files, read + convert each to plaintext (§3 below), hold in memory. No external DB, no on-disk
  persisted index (`[spec §2.4]`).
- **Locked tabs are unconditionally excluded** — whether locked-with-no-session or locked-with-an-
  active-unlock-session, that tab's memo names and bodies never enter the index
  (`[spec §2.4, §4.1 pt.6]`, `[detailed-spec §5.4]` — no exception for the session case, this is worth
  restating because it is easy to get wrong by analogy with how session state unlocks *other*
  behaviors like `memo:` link resolution).
- **Update triggers** (`[spec §2.4]`'s "two paths"):
  1. In-app path: any save/rename/delete/move that changes indexable content patches the affected
     entry directly, called from the same command that performed the change (`memo_create`,
     `memo_rename`, `memo_delete`, `memo_move`, `editor_save` — see `memo-tree/PLAN.md` and
     `editor-save/PLAN.md`). No full rebuild on any of these.
  2. External-change path: the file-watcher pipeline (`persistence-fs/PLAN.md` §5), once it classifies
     an event as a genuine (non-self) content change to a `.md` file in an unlocked, non-session tab,
     calls the same patch function.
  - Lock/unlock transitions also patch the index: locking a tab purges all its entries; unlocking does
    **not** add them back (locked tabs are excluded even in-session, per above) — so unlock is actually
    a no-op for the index, and lock is a bulk-purge. Worth stating explicitly since it's a slightly
    unusual (index shrinks on unlock-adjacent events but never grows from them) asymmetry that could
    otherwise be implemented backwards by mistake.

## 2. Data Structure

`search/index.rs`:

```rust
struct IndexEntry {
    tab_id: String,
    memo_name: String,   // no .md extension
    plaintext: String,    // converted body (§3), lowercased at index-build time for case-insensitive matching
    plaintext_name: String, // memo name, similarly normalized, since search targets both name and body
}

struct SearchIndex {
    entries: Vec<IndexEntry>,  // linear scan is acceptable at "hundreds to thousands of memos" scale
                                 // (`[spec §6]`); no inverted-index structure is required by spec.md,
                                 // and a simple approach is expected to be sufficient
                                 // (`[detailed-spec §5.4]`). If profiling during implementation shows
                                 // linear scan is inadequate, an inverted index keyed on lowercased
                                 // whitespace-tokenized words is the natural upgrade path — left to
                                 // developer-lite's judgment, not a spec requirement either way.
}
```

Patch operations: `upsert(tab_id, memo_name, plaintext)`, `remove(tab_id, memo_name)`,
`purge_tab(tab_id)` (used on lock). All are simple `Vec` mutations under a `Mutex`/`RwLock` guarding
the shared index (accessed from multiple command handlers concurrently).

## 3. Plaintext Conversion for Indexing

Elaborates `[spec §4.2]`'s "remove Markdown syntax/HTML tags... before indexing"
(`[detailed-spec §5.4]`): strip Markdown structural/decoration markers (`#`, `*`, `_`, `` ` ``, `>`,
table pipes, horizontal-rule sequences) and HTML tags (including the allowlisted `<img>`/`<span>` from
`markdown-editing/PLAN.md` §5), retaining human-readable text content (heading text, list item text,
code block contents, link label text). This is a **different, simpler** transform than the
sanitization allowlist in `markdown-editing/PLAN.md` — that one produces safe-to-render HTML for
preview; this one produces plain searchable text, and can be implemented as a standalone
regex/line-scanner in `search/index.rs` rather than sharing code with the frontend's parser-level
sanitizer (`markdownSanitize.ts`'s `filterStyleDeclarations()`/`isSafeRelativeImageSrc()`) (they run in
different processes/languages anyway — Rust backend vs. TS frontend — so code sharing isn't applicable
regardless). Whether raw URLs inside link targets are retained in the
indexed plaintext is an implementation nicety either way (`[detailed-spec §5.4]`).

**Fenced code blocks must be recognized via a real fence-boundary scan, not swept up by the same
marker-stripping regex** (`DECISIONS.md` "Code-Fence Awareness in Markdown Scanning"): `#`, `*`,
`` ` ``, `>` are all common, legitimate characters inside code (comments, multiplication, shell
redirects). Code block content must be identified by its fence delimiters and retained verbatim in the
indexed plaintext, not run through the structural-marker-stripping pass meant for prose.

## 4. Command

### `search_query(query: String, options: SearchOptions) -> AppResult<Vec<SearchHit>>`
```rust
struct SearchOptions { case_sensitive: bool, regex: bool }  // mirrors in-memo search options where applicable to a global query; exact option set is implementation discretion beyond what spec.md requires (name+body substring match at minimum)
struct SearchHit { tab_id: String, tab_name: String, memo_name: String, matched_in: MatchLocation }
enum MatchLocation { Name, Body, Both }
```
Searches `SearchIndex.entries` for `query` against both `plaintext_name` and `plaintext`
(`[spec §2.4]`, "Targets memo name and body"). Returns hits grouped/ordered however the frontend wants
to render them (e.g. tab-grouped) — ordering is a UI-detail, not a spec requirement.

## 5. Frontend (`components/Search/`, cross-tab mode)

- Triggered from the search button in the top bar (adjacent to the 📌 pin button per `[spec §2.1]`'s
  layout note — pin sits to its left).
- Results list: clicking a hit opens that memo (switches tab if needed, selects the tree node, opens in
  editor) — reuses the same "open memo" path as tree click / `memo:` link navigation
  (`memo-tree/PLAN.md` §4's "tree selection auto-follows" behavior applies here too).
- No results from locked tabs ever appear, and there is no indication in the UI that locked-tab content
  was "excluded" (excluding entirely, silently, is the specified behavior — no partial-match teaser or
  count-of-excluded-tabs UI is implied by `spec.md`).

## Open Items

None.
