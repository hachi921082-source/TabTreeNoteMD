# PLAN: Link Functionality

> Status: implemented (`src-tauri/src/commands/link.rs`'s `link_resolve`/`link_open_external`;
> frontend `src/editor/links.ts`, wired into `markdown-editing/PLAN.md`'s decoration layer
> (`markdownDecorations.ts`'s `MarkdownDecorationConfig`) and `components/Editor/EditorPane.tsx`).
> Rendering of clickable link regions inside the editor remains owned by `markdown-editing/PLAN.md`'s
> decoration layer, which calls into the commands defined here for resolution/opening — this plan
> owns the resolution logic and the external-open mechanism, not the in-editor click-detection
> rendering. See "Implementation Notes" below for discretion-point resolutions.

Sources: `docs/spec.md` §2.8, §4.1 pt.8, §4.2; `docs/detailed-spec.md` §4.6, §5.8.

## Implementation Notes (resolutions of this plan's discretion points)

- **`link_open_external`'s actual signature** is `(kind, value, currentTabId: Option<String>)` plus
  the usual Tauri `State`/`AppHandle` injections — this plan's §2 pseudocode only listed
  `(kind, value)`. `currentTabId` was added because §2's own next sentence ("relative paths are
  resolved to absolute using the current tab's folder as base") is otherwise impossible for the
  backend to do; this is the same category of omission as every command's pseudocode already
  dropping `AppHandle`/`State` params, not a spec-level ambiguity. `currentTabId` is `Option<String>`
  (not required) since it's only consulted for a relative `FilePath` — `Url`/`Email` callers (e.g.
  the pre-existing "Search with Google" action) can omit it.
- **URL/email normalization**: a bare `www.`-prefixed URL (no scheme) gets `https://` prepended
  before calling the OS opener (which needs a real scheme); `http://`/`https://`/`ftp://`-prefixed
  values pass through unchanged. Email targets are opened as `mailto:<value>` (prefixed only if not
  already present). Exact heuristics, same "implementation discretion" latitude as §3's regexes.
- **`link_open_external` opener API split**: `Url`/`Email` use `tauri_plugin_opener`'s `open_url`;
  `FilePath` uses `open_path` — both call the same underlying `open::open` on desktop, but the split
  keeps each kind's intent explicit or a future mobile-target divergence.
- **`MarkdownDecorationConfig` (`markdown-editing/PLAN.md`'s decoration-layer config facet) was
  extended**, not a new parallel config type: `currentTabId`, `confirmBeforeOpen`, `onOpenMemo`
  (reuses the shared "open memo" path), and `requestExternalConfirm` (an injected async prompt,
  since the click handler runs inside a CodeMirror `ViewPlugin`, not a React component) were added
  alongside the pre-existing `openTrigger`/`readOnly`. `links.ts` imports `MarkdownDecorationConfig`
  as a type-only import from `markdownDecorations.ts` (erased at compile time, so no runtime import
  cycle despite `markdownDecorations.ts` importing values back from `links.ts`).
- **Confirm-before-open dialog** reuses the existing shared `ConfirmDialog`/`Dialog` components
  (already used by tab/memo delete confirmations) rather than a new dialog component — rendered by
  `EditorPane.tsx`, which owns the Promise-resolver bridge (`requestExternalConfirm`) between the
  CodeMirror click handler and the React dialog.
- **`memo:` link "open memo" path**: `App.tsx` gained `openLinkTarget`/`pendingLinkFocus`, a
  deliberately separate (not merged) copy of the existing `openSearchHit`/`pendingSearchFocus`
  cross-tab-focus state machine (`search-index/PLAN.md` §5's own integration), since a resolved
  link's `memoName` can be `null` for the `memo:TabName/` (tab-only) form — a tab switch with
  nothing to focus, which `openSearchHit`'s non-nullable `memoName` doesn't need to handle.
- **`link_resolve`'s locked-tab tab-name matching**: a locked tab with no active session is skipped
  entirely when matching `TabName` by display name (its real name lives only in the encrypted lock
  blob) — not just when looking up memos inside it — so it is unreachable by any `TabName/...` form,
  matching `[spec §2.8]`'s "links to locked tabs are always unresolved without an active session."
  A locked tab *with* a session matches by `LockSession.real_tab_name` instead of
  `AppSettings.tabs[i].name` (which still holds the 🔒 placeholder while a session is active).
- **Not committed this pass**: `src-tauri/src/main.rs`, `src-tauri/src/commands/mod.rs`,
  `src/App.tsx`, and `src/shared/models.ts` each contain this feature's wiring interleaved with an
  unrelated, already-in-progress `search-index/PLAN.md` session's own uncommitted edits to the same
  files (present before this session started) — left uncommitted rather than folding the two
  features' changes together into one commit; see the implementation report for the exact diffs.

---

## 1. Link Types

1. **Auto-links** (no explicit Markdown syntax, recognized by pattern in raw text): URLs (`http://`,
   `https://`, `www.`, `ftp://`), email addresses, file paths.
2. **`memo:` notation** (bare, auto-linked the same as above): `memo:MemoName` (current tab),
   `memo:TabName/MemoName`, `memo:TabName/` (tab only, no memo selected).
3. **Markdown link syntax with `memo:` as the URL scheme**: `[label](memo:TabName/MemoName)`
   (`[spec §4.2]`) — coexists with #2, same resolution logic, different clickable-region computation
   (whole bare token vs. the link's visible label span, `[detailed-spec §5.8]`).
4. **Same-document heading anchors (`#slug`)**: `[heading text](#slug)`, generated by the Table of
   Contents feature (`table-of-contents/PLAN.md`). Resolution is scoped to the *current* memo only
   and is frontend-only (§5 below) — see that section for the full mechanism.

## 2. Commands

### `link_resolve(current_tab_id: String, target: String) -> AppResult<ResolvedLink>`
```rust
enum ResolvedLink {
    Memo { tab_id: String, memo_name: Option<String> },  // memo_name: None for the "TabName/" (tab-only) form
    Unresolved,
}
```
Parses `target` (the string after `memo:`) into one of the three forms (§1.2 above):
- `MemoName` (no `/`) → resolve within `current_tab_id`'s tree only.
- `TabName/MemoName` → resolve `TabName` against `AppSettings.tabs` by **display name** (not folder
  path); ambiguity is impossible since tab names are enforced unique (`[spec §2.1]`,
  `[detailed-spec §5.8]`).
- `TabName/` (trailing slash, empty memo segment) → resolves to `Memo { tab_id, memo_name: None }`,
  meaning "open this tab, don't select a specific memo."
- Target tab locked with **no** active session → `Unresolved` (no-op), even if the memo would
  otherwise exist (`[spec §2.8, §4.1]`). Target tab locked **with** an active session → resolves
  normally against that session's in-memory tree.
- Target tab or memo simply doesn't exist → `Unresolved` (no-op, no error dialog — `[spec §2.8]`,
  "Unresolved links do nothing").
- This command does **not** open anything — pure resolution. The frontend, on receiving `Memo {...}`,
  performs the actual tab-switch + memo-open (reusing `memo-tree/PLAN.md`'s "open memo" path, same one
  used by tree click and search-result click).

### `link_open_external(kind: ExternalLinkKind, value: String) -> AppResult<()>`
```rust
enum ExternalLinkKind { Url, Email, FilePath }
```
- `Url`/`Email`: OS-standard "open with default handler" API (Tauri's `opener` plugin or equivalent),
  never shell string concatenation (`[spec §2.8]`).
- `FilePath`: relative paths are resolved to absolute using the **current tab's folder as base**
  before opening (`[spec §2.8]`), then opened via the same OS-standard API.
- If `AppSettings.links.confirmBeforeOpen` is `true` (default `false`), the **frontend** shows the
  shared non-blocking confirm dialog before calling this command at all — this command itself performs
  no confirmation, it's a pure "open now" action; the setting gate lives entirely on the caller side so
  this command stays simple and testable.

## 3. Auto-Link Recognition Patterns

Elaborates `[detailed-spec §5.8]`: reasonable, widely-used heuristics satisfy `[spec §2.8]` — exact
regexes are implementation discretion, not a spec ambiguity. Starting patterns for
`markdown-editing/PLAN.md`'s decoration layer to implement in `src/editor/links.ts`:
- URL: `\b(https?:\/\/|ftp:\/\/|www\.)[^\s<>"]+` (trim trailing punctuation like `.`/`,`/`)` from the
  match, a standard practical adjustment).
- Email: `[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}` (standard `local@domain.tld` shape).
- File path: a path-separator-containing token with no embedded whitespace (e.g. `[A-Za-z]:\\[^\s]+` or
  `\\\\[^\s]+` for Windows absolute paths, plus relative-path tokens containing `\` or `/` — exact
  boundary rules are implementation discretion; false negatives here are lower-risk than false
  positives, since a missed auto-link just means the user manually navigates, whereas an over-eager
  match could make ordinary text incorrectly clickable).
- `memo:` token: `memo:[^\s)]+` — stops at whitespace or a closing paren (to not swallow trailing
  Markdown/prose punctuation).

## 4. Operation Trigger and Confirm Dialog

- Default click behavior: `singleClick` (`AppSettings.links.openTrigger`, `[spec §2.8]`; other allowed
  values `ctrlClick`/`doubleClick` per `[spec §3.4]`'s type are available as a user setting).
- Confirmation dialog before **external** launch (URL/email/file path — not `memo:` links, which never
  leave the app) defaults to OFF (`[spec §2.8]`), gated by `AppSettings.links.confirmBeforeOpen` as
  described in §2 above. Uses the shared custom dialog component (`[detailed-spec §6.5]`), Esc-closable
  (not one of the named Esc-disabled exceptions).

## 5. Frontend Click Handling

`src/editor/links.ts` (called from `markdown-editing/PLAN.md`'s decoration layer's click handlers):
1. On click matching `AppSettings.links.openTrigger`, determine link type (auto-link pattern match vs.
   `memo:` scheme vs. Markdown-link-with-memo-scheme vs. `#slug`).
2. For `memo:` targets: call `link_resolve`; on `Memo {...}`, open via the shared "open memo" path
   (frontend-only after resolution); on `Unresolved`, do nothing (no toast, no error — matches
   `[spec §2.8]`'s explicit "unresolved links do nothing").
3. For external targets: optionally show confirm dialog (§4), then call `link_open_external`.
4. For `#slug` targets (`[detailed-spec §4.6, §5.8]`): **frontend-only, no backend command** — same
   treatment as in-memo search (`editor-save/PLAN.md` §5), since it operates purely on the
   already-loaded CodeMirror buffer of the current memo.
   - Re-scan the current buffer's headings and regenerate slugs (same scan + slugify logic as
     `table-of-contents/PLAN.md`'s TOC generator — re-used, not duplicated — so a link resolves
     correctly even if headings changed since the last "Update" click).
   - Match against `slug`. No match → silent no-op (no toast, no error).
   - Match → scroll the CodeMirror view so the matching heading's line is visible (CodeMirror's
     `EditorView.scrollIntoView`, target = the heading line's document position), and move the cursor
     there.
   - Never resolves cross-memo or cross-tab.

## Open Items

None.
