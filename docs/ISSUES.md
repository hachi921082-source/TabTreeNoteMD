# Known Issues (ISSUES.md)

> Lightweight tracker for minor bugs, gaps, or rough edges found during implementation/review that do
> **not** need the user's judgment call to even understand (unlike [docs/QA.md](QA.md), which is for
> open questions that need the user to decide something before work can proceed). Entries here are
> meant to accumulate and be cleaned up together in a batch, rather than being triaged one at a time as
> they're found.
>
> **Convention for resolved items**: once an entry is fixed, remove it from this file (history remains
> in git). If an entry turns out to actually need a user decision rather than just a fix, move it to
> `docs/QA.md` instead of leaving it here.

---

## Open

### `markdown-editing/PLAN.md` still describes the pre-TOC stub wiring

- **Found by**: `reviewer-lite`, reviewing the `table-of-contents` implementation
  (commits `8ea4039`..`4a7b75e`).
- **Symptom**: `docs/plans/markdown-editing/PLAN.md` §2 (around lines 124-128) still says the toolbar's
  "Update" slot calls `decorationCommands.ts`'s `tocUpdateStub`, pending `table-of-contents/PLAN.md`.
  That stub was removed in commit `bc02228` and the slot now calls the real `tocGenerator.ts` — the
  `table-of-contents` feature's own commits didn't touch `markdown-editing/PLAN.md` (out of scope for
  that work), so the two plans' descriptions of the same wiring now disagree.
- **Fix**: update `markdown-editing/PLAN.md` §2 to reference `tocGenerator.ts` instead of the removed
  stub. Pure doc correction, no code change.

### Edit Lock: live enforcement for an already-open memo

- **Found by**: `reviewer-lite`, reviewing `docs/plans/editor-save/PLAN.md`.
- **Symptom**: `editor_load`'s `edit_locked` flag is a load-time snapshot; if Edit Lock is toggled on
  while the memo is already open in the editor, the editor doesn't go read-only immediately — typing
  still works, and a save attempt then surfaces a generic error instead of being blocked client-side.
  No data-loss risk (backend correctly refuses the write).
- **Not reachable via any shipped UI today** (no context-menu control toggles Edit Lock at all yet).
- Direction (live-enforce now vs. defer until an Edit-Lock-toggle UI ships) is tracked as an open
  question in [docs/QA.md](QA.md).

### In-memo replace "selection" mode missing

- **Found by**: `reviewer-lite`, reviewing `docs/plans/editor-save/PLAN.md`.
- **Symptom**: `spec.md` §2.4 lists replace modes as "single / all / selection / one-by-one"; the
  implemented `components/Search/SearchPanel.tsx` / `state/useSearch.ts` only has replace-one and
  replace-all. No "constrain to current selection" mode.
- Whether this was an intentional drop or needs to be built is tracked as an open question in
  [docs/QA.md](QA.md).

### `Cargo.toml` autostart comment describes the wrong mechanism

- **Found by**: `reviewer-lite`, reviewing `docs/plans/window-operation/PLAN.md`.
- **Symptom**: `src-tauri/Cargo.toml`'s `tauri-plugin-autostart` dependency comment still describes the
  originally-considered "Windows-Startup-folder auto-launch-to-tray... keeps user data/settings entirely
  out of the registry" approach. The actual shipped implementation uses the Registry `Run`-key fallback
  (as pre-approved by `detailed-spec.md` §5.7, since no Startup-folder-shortcut Tauri plugin was found)
  — `main.rs` and `commands/autostart.rs`'s own comments correctly describe this. Only the `Cargo.toml`
  comment is stale.
- No functional defect (verified: the Registry Run-key is written/removed correctly at runtime); risk is
  a future reader being misled about where autostart data is stored.
- **Fix**: update the `Cargo.toml` comment to match the Registry Run-key approach. Pure doc correction,
  no code change.

### Toolbar image-insert file input leaks on dialog cancel

- **Found by**: `reviewer-lite`, reviewing `docs/plans/image-handling/PLAN.md`.
- **Symptom**: `src/editor/imagePaste.ts`'s `insertImageViaFilePicker` creates a hidden
  `<input type="file">` and only removes it (`document.body.removeChild(input)`) inside the `change`
  event listener. If the user cancels the native file-picker dialog without choosing a file, `change`
  never fires, so the element is never removed — permanently orphaned in the DOM.
- No functional/visible impact; each cancelled toolbar "Image insert" click leaks one hidden node
  (negligible memory footprint).
- **Fix**: also remove the input on cancel (e.g. listen for the input's `cancel` event, or a
  `window focus`/timeout fallback) so cancelling the dialog cleans it up like a successful pick does.

### Tab Lock session mutations aren't rolled back if the final lock-blob save fails

- **Found by**: Claude, reviewing `docs/plans/tab-lock/PLAN.md`/`CRASH-RECOVERY.md` at the user's
  request for a tab-lock-focused bug hunt.
- **Symptom**: `src-tauri/src/commands/tab_lock.rs`'s session-path mutators (`session_create_memo`,
  `session_rename_memo`, `session_delete_memo`, `session_move_within`, `session_mutate_node_field`,
  `apply_session_move_target`, `cleanup_session_move_source`) all follow the same shape inside
  `LockSessionStore::with_session_mut`: mutate `session.tree`/`session.file_map` (and sometimes write
  or delete a physical obfuscated file) first, then call `save_lock_blob` as the last step.
  `with_session_mut` hands the closure a plain `&mut LockSession` with no snapshot/rollback, so if
  `save_lock_blob`'s `atomic_write` to `.tabtreenote.json` fails (plausible on Windows — it does two
  renames, including a `.bak` rotation, either of which can hit a transient AV/indexer lock right
  after the sibling obfuscated-file write just succeeded), the in-memory tree/fileMap mutation is
  **not undone** even though the command reports failure to the user.
  Concretely: `session_create_memo` can report "failed to create memo" while the memo still exists in
  the in-memory session tree and its encrypted body is already on disk — a *later*, unrelated
  successful mutation's `save_lock_blob` call then silently persists and resurrects it. Conversely, if
  the session ends first, the encrypted body file is orphaned forever (never referenced by any
  persisted fileMap). The equivalent delete/move paths can leave a fileMap entry pointing at a
  now-deleted token file (a tree node that can no longer be opened) if `save_lock_blob` fails after the
  physical delete succeeded.
- No security impact (bodies stay encrypted either way), and `tab_lock`/`tab_unlock` themselves are
  unaffected — only structural mutations made *during* an already-active unlock session.
- **Fix direction**: needs a `developer-lite` implementation call — e.g. build the new `tree`/
  `file_map` in a local copy, only write it into `session` after `save_lock_blob` succeeds (mirrors
  `session_delete_memo`'s own file-delete-before-mutate ordering, just extended to the final save
  step too).

### `cleanup_session_move_source` swallows file-delete failures instead of aborting

- **Found by**: Claude, same tab-lock review as above.
- **Symptom**: `src-tauri/src/commands/tab_lock.rs`'s `cleanup_session_move_source` (the "not copy"
  cleanup for a cross-tab move out of a locked session tab) removes each moved name from
  `session.tree`/`session.file_map` unconditionally, and only `tracing::error!`-logs (rather than
  aborting) if deleting the source's obfuscated token file fails. This is the opposite ordering from
  `session_delete_memo`, which deletes the physical file *first* and returns `Err` (leaving tree/
  fileMap untouched) if that fails. The looser ordering here means a failed delete during a non-copy
  cross-tab move can leave an orphaned encrypted file in the source tab's folder that's no longer
  referenced anywhere, while the move itself still reports success.
- **Fix direction**: match `session_delete_memo`'s ordering — attempt the physical delete first and
  bail out (propagating an error) before mutating `tree`/`file_map` on failure.

### Bare `memo:` false-positive auto-link match on ordinary prose

- **Found by**: `reviewer-lite`, reviewing `docs/plans/link/PLAN.md`.
- **Symptom**: `src/editor/links.ts`'s `BARE_MEMO_RE` (`/\bmemo:[^\s<>()[\]]*/g`) allows a zero-length
  target after the colon, so any standalone literal occurrence of the word "memo:" in prose (not
  intended as a link) gets rendered as a clickable `cm-md-link` span.
- No functional break: clicking it safely resolves to `Unresolved` and is a silent no-op, per
  `[spec §2.8]`'s "unresolved links do nothing." Purely an over-eager-match UX quirk — exactly the kind
  of risk PLAN.md §3 itself flagged as lower priority than false negatives.
- **Fix**: require at least one non-whitespace char after the colon (e.g. `/\bmemo:\S+/`) so a bare
  `memo:` with nothing following it doesn't match. Pure regex tightening in `src/editor/links.ts`, no
  behavior change to any valid `memo:` link form.

### Bare file-path auto-link click-to-open has no `..`-traversal guard (security-relevant)

- **Found by**: `reviewer-lite`, hands-on testing of `docs/plans/markdown-editing/PLAN.md` §5's
  sanitization allowlist at the user's request.
- **Symptom**: `src-tauri/src/commands/link.rs`'s `resolve_file_path` (~line 184) does a raw
  `folder.join(path)` with no `..`-segment validation at all — unlike
  `src/editor/markdownSanitize.ts`'s `isSafeRelativeImageSrc`, which explicitly rejects any `..`
  segment (`DECISIONS.md`'s "Image `src` Allowlist: Reject `..` Path-Traversal Segments"). Confirmed
  live: text containing a `../`-prefixed relative path (reproduced via the leftover text of a
  sanitizer-rejected `<img src="../outside.png">` tag) gets picked up by `src/editor/links.ts`'s
  bare-file-path auto-link scanner and rendered as an ordinary clickable link, completely independent
  of and unaware of the image sanitizer's own rejection of that same string. Clicking it would resolve
  to a path *outside the tab's own folder* via `link_open_external` and open it with the OS's default
  handler (not clicked live, to avoid an actual external-open side effect — the missing guard itself is
  confirmed by direct code reading, the clickable-link rendering is confirmed by live reproduction).
- This is the same class of risk the image-src allowlist's `..`-rejection was explicitly written to
  prevent, just reachable through a different, unguarded code path (`link/PLAN.md`'s feature) that
  neither `link/PLAN.md` nor `markdown-editing/PLAN.md` appear to have connected to the other.
- **Fix**: apply the same `..`-segment rejection as `isSafeRelativeImageSrc` (or an equivalent check)
  inside `resolve_file_path`, before `folder.join`.

