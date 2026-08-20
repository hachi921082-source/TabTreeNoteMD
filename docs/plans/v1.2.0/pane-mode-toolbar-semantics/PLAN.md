# PLAN: Pane-Mode / View-Mode Toolbar Button Semantics — Next-State Labels (v1.2.0)

> Status: planning complete, not yet implemented. Modifies existing v1.0.0/shipped behavior — no new
> feature surface, no new plan-owned data model. Depends on `docs/plans/markdown/markdown-preview/
> PLAN.md` §4.1/§6.3 (`PANE_MODE_CYCLE`, `PANE_MODE_LABEL`, the pane-mode toggle and live/source toggle
> buttons this plan edits in place — this plan does not restate that file's own contents, only the two
> specific edits below). Extracted UI copy for the two changed labels folds into `docs/plans/v1.2.0/
> i18n/PLAN.md`'s translation maps per that plan's §6 sequencing note.

Sources: `docs/spec.v1.2.0.md` §3; `docs/detailed-spec.v1.2.0.md` §3, §11.

---

## 1. Scope Recap

Two already-shipped toolbar buttons in `src/components/Editor/Toolbar/MarkdownToolbar.tsx` currently show
their **current** state as the visible label. Both change to show the *next* state (what clicking would
move **to**) instead — current-state display is abolished entirely, no separate indicator added anywhere
else (status bar, etc.). This includes the pane-mode toggle's `aria-label`, which also drops its
current-state suffix.

## 2. Live/Source View-Mode Toggle (`ttn-md-toolbar-view-toggle`)

2-state boolean toggle. `MarkdownToolbar.tsx`, line ~600 (confirmed by inspection):

```diff
- {liveViewEnabled ? "ライブ表示" : "ソース表示"}
+ {liveViewEnabled ? "ソース表示" : "ライブ表示"}
```

- When currently in live view, clicking switches TO source view → label reads "ソース表示" (next state).
  When currently in source view, clicking switches TO live view → label reads "ライブ表示" (next state).
- No `aria-label`/`title` exists on this button today (confirmed by inspection — only the pane-mode
  button and the preview-lock/PDF/minimize buttons have one) — `[spec v1.2.0 §3]` does not require adding
  one, this change is scoped to the visible label text only.
- No prop changes: `liveViewEnabled` is already passed into this component; this is a pure render-
  expression edit at the one cited line.

## 3. Pane-Mode Toggle (`ttn-md-toolbar-pane-mode`)

3-state cycling toggle (`edit → split → preview → edit → ...`). `MarkdownToolbar.tsx`, lines ~599–610
(confirmed by inspection):

```diff
- {PANE_MODE_LABEL[paneMode]}
+ {PANE_MODE_LABEL[PANE_MODE_CYCLE[paneMode]]}
```

```diff
- aria-label={`ペイン表示モードを切り替え (現在: ${PANE_MODE_LABEL[paneMode]})`}
+ aria-label={`${PANE_MODE_LABEL[PANE_MODE_CYCLE[paneMode]]}に切替`}
```

- `PANE_MODE_LABEL` (the `Record<PaneMode, string>` lookup table, "編集のみ"/"分割表示"/"プレビューのみ")
  stays unchanged — only which key looks it up changes, from `paneMode` (current) to
  `PANE_MODE_CYCLE[paneMode]` (next). `PANE_MODE_CYCLE` already exists (`markdown/markdown-preview/
  PLAN.md` §4.1) for the click handler's own `onCyclePaneMode` advancement logic — this is a new *read* of
  an already-existing map, not new logic.
- The `aria-label`'s "(現在: {current label})" suffix is **removed, not reworded to additionally describe
  the next state** — it becomes the same next-state-only text as the visible label, with no separate
  current-state channel for screen readers. Exact wording of the next-state-only phrasing (`"...に切替"`
  above) is `developer-lite` discretion — only "next-state-only, no current-state suffix" is spec-level
  (`[detailed-spec v1.2.0 §11]`).
- `previewLocked` (`disabled={previewLocked}` on this button, GitHub issue #149) is untouched — this plan
  changes only label text, not the button's enabled/disabled logic.

## 4. No Other Touch Points

- No new state, no new props: both buttons already receive `paneMode`/`liveViewEnabled` as props; this is
  a pure render-expression change at the two cited lines in `MarkdownToolbar.tsx`, no new prop threading
  through `EditorPane.tsx`.
- No data-model/schema change, no backend change, no new `ErrorCode`.

## 5. Testing / Verification

- Live view active: view-toggle button reads "ソース表示"; click it → now in source view, button reads
  "ライブ表示".
- Pane mode `edit`: button reads "分割表示" (next state after `edit` in the cycle); `aria-label` matches
  the visible label's next-state phrasing, no "(現在: ...)" substring anywhere. Click through all three
  states (`edit → split → preview → edit`), confirming the label always names the state one click away,
  never the current one.
- `previewLocked === true`: pane-mode button still visually disabled/non-interactive, label text still
  computed correctly (not skipped) even though the button can't be clicked.
- Screen-reader spot check (or DOM inspection of `aria-label` values) confirms no separate current-state
  text is exposed anywhere for either button.
