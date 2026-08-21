# Session Handoff

> Written to carry context into a fresh conversation when the previous one has grown too large.
> Read this once at the start of the new session, then treat it as disposable — once its content is
> either acted on or absorbed back into `spec.v1.2.0.md` / `detailed-spec.v1.2.0.md` / `DECISIONS.md` /
> `QA.md` / commits, this file can be deleted. It is not a permanent part of the documentation set
> (unlike `spec.md`, `spec.v1.2.0.md`, `DECISIONS.md`, `detailed-spec.md`, `detailed-spec.v1.2.0.md`,
> `QA.md`, which `docs/README.md` and `CLAUDE.md` already index).
>
> **Before acting on anything below, verify it against actual current state** (`git status`, `git log`,
> `git branch`, whether files described here still exist/match) — this file is a snapshot and can go
> stale before it's even read.

## Index

1. Where the project stands
2. `preview-zoom` (item 7) — implemented, reviewed, merged, pushed (this session)
3. Separate track: GitHub issues #190 and #191 — both filed only, neither started
4. Untracked file — `CLAUDE - コピー.md` still unaddressed
5. Next step — resume the nine-item implementation order, item 8 next
6. Branch policy for v1.2.0 (unchanged)
7. Process note — Auto Mode classifier blocked `git merge` even after chat confirmation (pointer to memory, not repeated in full here)

## 1. Where the project stands

- v1.1.0 is fully complete and merged into `V1.1.0` (unchanged).
- Current branch: **`V1.2.0`**, pushed to `origin/V1.2.0` at `3f42c15` (this session's merge + push).
- **This session's work**: item 7 (`preview-zoom`) implemented, reviewed, merged into `V1.2.0`, and
  pushed. Feature branch deleted locally after merge (per user confirmation). 7 of 9 items now done.

## 2. `preview-zoom` (item 7) — implemented, reviewed, merged, pushed (this session)

- Branch `feature/v1.2.0-preview-zoom` cut from `V1.2.0`, implemented per
  `docs/plans/v1.2.0/preview-zoom/PLAN.md` §1–§7:
  - `src/shared/shortcuts.ts`: `DEFAULT_SHORTCUTS` gains `"preview.zoomIn": "Ctrl+="` /
    `"preview.zoomOut": "Ctrl+-"`.
  - `PreviewPane.tsx`: new `previewZoomPercent` prop, applied as CSS `zoom` (not `transform: scale()`)
    on the rendered-content wrapper.
  - `MarkdownToolbar.css` / `MarkdownToolbar.tsx`: `PREVIEW_ZOOM_MIN/MAX/STEP` (70/180/10) exported from
    the toolbar file (developer-lite's discretionary placement call, PLAN.md left this open); ± /
    percentage-readout button group, conditionally rendered only while `paneMode === "preview"`, disabled
    at bounds, tooltips driven by `resolveShortcuts`.
  - `EditorPane.tsx`: `previewZoomPercent` state (default 100) with three independent reset triggers
    (tab switch, memo switch — joined to the existing `loadGeneration`-gated reset point alongside
    `tab-double-click-jump`'s toggle reset — and `paneMode` leaving `"preview"`, its own effect); a
    focus-unconstrained keydown handler (works even with focus inside the preview pane's DOM, mirroring
    the `markdown.paneModeCycle`/`markdown.pdfExport` pattern) handling `Ctrl+=`/`Ctrl+-` plus the
    confirmed §5.3 numpad-alias asymmetry (`Ctrl+NumpadSubtract` needs no special code since its `e.key`
    already equals `Ctrl+-`'s; `Ctrl+NumpadAdd` needs an explicit non-rebindable `e.code === "NumpadAdd"`
    check since its `e.key` differs from `Ctrl+=`'s).
- `reviewer-lite` ran the full PLAN §7 scenario list live via CDP-driven `tauri dev` — zoom stepping/
  bounds-disabling, percentage-readout reset, keyboard shortcuts including numpad aliases with focus
  inside the preview pane, shortcut-rebinding + numpad-alias survival, reset on tab/memo switch and on
  leaving preview mode, split-mode preview fixed at 100% regardless of preview-mode zoom, ± group absent
  outside `paneMode === "preview"`. `tsc --noEmit` and `cargo check` both clean. **Approved.**
  - One item PLAN.md §3 explicitly reserved for the user (not reviewer-lite's call): whether a wide
    table/image looks correct at 180% zoom. reviewer-lite's test memo had no wide content to check this
    against; the user then launched the app directly (`npm run tauri dev`) and confirmed visually — no
    layout breakage. This closes the one open caveat from PLAN.md.
  - Non-blocking observation: `SettingsDialog.tsx`'s `SHORTCUT_LABELS` has no Japanese label for
    `preview.zoomIn`/`preview.zoomOut` (shows the raw action id) — matches existing precedent
    (`markdown.pdfExport` has the same gap) and is explicitly deferred to item 9 (`i18n`) per PLAN.md's
    own header. Not a regression, not blocking.
- Commits (5, one file each): `0d6b5f7` (MarkdownToolbar.css), `77300a5` (MarkdownToolbar.tsx),
  `0eec54d` (EditorPane.tsx), plus the `shortcuts.ts` and `PreviewPane.tsx` commits → merge commit
  ("Merge feature/v1.2.0-preview-zoom: ...") into `V1.2.0` at `3f42c15`, pushed, feature branch deleted
  locally after user confirmation.

## 3. Separate track: GitHub issues #190 and #191 — both filed only, neither started

Not part of the v1.2.0 feature set — do not fold either into any v1.2.0 `PLAN.md`. Unchanged this
session.

- **#190**: a heading containing the app's own color/font-size `<span style="...">`/`<mark>` tag
  produces a garbled slug in `src/editor/tocGenerator.ts`'s `scanHeadings()`.
  https://github.com/hachi921082-source/TabTreeNoteMD/issues/190
- **#191**: a self-referential `memo:TabName/MemoName#slug` anchor link (target memo already
  open/focused) doesn't jump — pre-existing gap in the already-merged `cross-tab-anchor-links` feature.
  https://github.com/hachi921082-source/TabTreeNoteMD/issues/191
- **User's explicit instruction for both**: file the issue, proceed later. Do not start a `fix/` branch
  on your own initiative — wait for the user to say to proceed.

## 4. Untracked file — `CLAUDE - コピー.md` still unaddressed

`"CLAUDE - コピー.md"` (a confirmed-stale backup, not a duplicate) remains untracked and untouched,
unchanged across this session. Ask the user first if cleanup is ever wanted.

## 5. Next step — resume the nine-item implementation order, item 8 next

Recommended order (unchanged; items 1–7 done):

1. ~~`preview-link-encoding-fix`~~ — done, merged, pushed.
2. ~~`cross-tab-anchor-links`~~ — done, merged, pushed.
3. ~~`preview-scroll-fix`~~ — done, merged, pushed.
4. ~~`quick-open-discoverability`~~ — done, merged, pushed.
5. ~~`pane-mode-toolbar-semantics`~~ — done, merged, pushed.
6. ~~`tab-double-click-jump`~~ — done, merged, pushed.
7. ~~`preview-zoom`~~ — done, merged, pushed (§2, this session).
8. **`math-rendering`** (155 lines, medium-high) — next up. New KaTeX dependency.
9. `i18n` (248 lines, largest, deliberately last) — sweeps up every other feature's new UI strings,
   including the `preview.zoomIn`/`preview.zoomOut` `SHORTCUT_LABELS` gap noted in §2.

Item 8 has no plan-listed dependency on item 9. Per `docs/plans/README.md`'s "Independent
Implementation, Mandatory Final Integration Pass" section, `reviewer-lite` must still do one whole-app
integration pass after all nine land — item 9 still unimplemented, so this pass is not due yet.

Ask the user before starting item 8, same as every stage transition in this project's workflow.

## 6. Branch policy for v1.2.0 (unchanged)

**`V1.2.0` does NOT merge back into `V1.1.0`.** Per the user: "v1.1.0とは別にv1.2.0として開発を継続するの
でv1.1.0へマージはしない".

`feature/v1.2.0-<name>` branches, merged directly into `V1.2.0` (no PR) — item 7 (§2) is the seventh
confirmed instance of this pattern.

## 7. Process note — Auto Mode classifier blocked `git merge` even after chat confirmation

Mid-session, `git checkout V1.2.0 && git merge --no-ff feature/v1.2.0-preview-zoom` was denied outright
by Claude Code's Auto Mode permission classifier ("Blocked by classifier"), despite the user having
already said "ok" to merging in chat. Chat confirmation and the harness's classifier permission are two
independent gates — clearing one does not clear the other. The user resolved it by switching Claude
Code's permission mode from Auto Mode to manual, after which the identical command succeeded
immediately. Full detail (including the three concrete unblock paths and the "don't suggest
`AskUserQuestion` as a workaround" note) recorded in `feedback_confirm_before_merge_push_delete.md`'s
2026-08-21 update — not repeated here in full, read that before the next merge if this recurs.
