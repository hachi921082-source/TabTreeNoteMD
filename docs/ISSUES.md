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

### `sanitizeMarkdownHtml()` defined but unused; enforcement path diverges from PLAN.md §5

- **Found by**: `reviewer-lite`, reviewing `docs/plans/markdown-editing/PLAN.md`.
- **Symptom**: `src/editor/markdownSanitize.ts` defines `sanitizeMarkdownHtml()`, a DOMPurify-configured
  wrapper, per PLAN §5's explicit instruction to "Use DOMPurify... configured with a strict custom
  allowlist... rather than DOMPurify's permissive defaults." It is never called anywhere in `src/`. The
  actual enforcement path is `markdownSpanParser.ts` calling the same module's lower-level
  `filterStyleDeclarations()`/`isSafeRelativeImageSrc()` helpers directly, bypassing DOMPurify entirely
  — this works because the decoration engine never injects raw HTML into the DOM (it builds CodeMirror
  mark/widget decorations from parsed attributes), so CodeMirror's own text rendering keeps everything
  else inert by construction.
- No functional or security defect: hands-on testing confirmed the effective sanitization properties
  (disallowed CSS properties stripped, `..` path-traversal rejected, `<script>`/`on*` handlers inert)
  fully hold via the actual path used.
- Fix direction is a `developer-lite` implementation call, not a user decision: either wire
  `sanitizeMarkdownHtml()` in for real, or update the plan's Implementation Notes to state why the
  DOMPurify wrapper was left unused in favor of the narrower parser-level filters (and remove the dead
  code if so).

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
