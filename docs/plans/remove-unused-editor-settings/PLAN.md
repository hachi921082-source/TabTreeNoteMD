# PLAN: Remove Unused Editor Default Settings

> Status: planning complete.

Sources: `docs/spec.md` §2.3, §3.4, §4.2; `docs/detailed-spec.md` §3.2, §3.3, §5.3, §5.10;
`docs/DECISIONS.md` "Remove Unused Editor Default Settings: Display Mode Selector and Soft-Tab
Checkbox" (GitHub issue #61).

## Scope

Remove two Settings → Editor rows and their backing schema fields:

1. **Display mode selector** (`AppSettings.editorDefaults.viewMode`) — global default becomes a
   hardcoded `'live'` constant. No behavior change (already the factory default). The per-memo
   `MemoEditorSettings.viewMode` override and its runtime live/source toggle
   (`markdown.toggleLiveView`) are unaffected — they still exist, they just fall back to the fixed
   `'live'` constant instead of a settings-backed value.
2. **"Insert tab as spaces" checkbox** (`AppSettings.editorDefaults.softTab`, plus the parallel
   unused `MemoEditorSettings.softTab` per-memo override field) — Tab-key insertion becomes
   hardcoded to always insert `\t`. **Behavior change**: prior factory default was
   `softTab: true` (spaces).

Out of scope: every other `EditorDefaults`/`MemoEditorSettings` field (`wrap`, `tabWidth`,
`showLineNumbers`, `fontSize`, scroll/cursor position) — unchanged even where similarly
under-used. `src-tauri/Cargo.toml`'s pre-existing uncommitted local changes — untouched.

## Frontend changes

### `src/shared/models.ts`
- `EditorDefaults`: remove `softTab: boolean` and `viewMode: "live" | "source"` fields.
- `MemoEditorSettings`: remove `softTab?: boolean | null` and `viewMode?: "live" | "source" | null`
  fields.
- `AppSettingsPatch.editorDefaults` is `Partial<EditorDefaults>` already — no separate change
  needed beyond `EditorDefaults` itself shrinking.

### `src/editor/textCommands.ts`
- `ResolvedEditorSettings`: remove `softTab`.
- `resolveEditorSettings()`: drop the `softTab` line.
- `makeTabCommand()`: drop the `softTab` parameter; body always does
  `view.state.replaceSelection("\t")`.
- `buildSettingsExtension()`: update the `keymap.of([{ key: "Tab", run: makeTabCommand(settings.tabWidth) }])`
  call site to match the new signature.

### `src/components/Editor/EditorPane.tsx`
- `FALLBACK_EDITOR_DEFAULTS`: drop `softTab`/`viewMode` from the literal (type shrinks to match).
- Add a local constant `const DEFAULT_VIEW_MODE: "live" | "source" = "live";` (or inline the
  literal) next to `FALLBACK_EDITOR_DEFAULTS`, replacing every read of `editorDefaults.viewMode`.
- `resolvedViewMode`: `memoEditorOverride?.viewMode ?? DEFAULT_VIEW_MODE` (was
  `?? editorDefaults.viewMode`). No other change to the live/source toggle logic — it keeps
  reading/writing `memoEditorOverride.viewMode` exactly as today.
- Comment at line ~137 referencing `editorDefaults.viewMode` updated to reference the fixed
  default instead (the "never the global setting" framing stays accurate, just reworded since
  there's no longer a global *setting* to contrast against — a global *constant*).

### `src/components/Settings/SettingsDialog.tsx`
- Remove the "タブをスペースとして挿入する" checkbox row (softTab, ~lines 285-292).
- Remove the "表示モード" selector row (viewMode, ~lines 311-320).
- No `AppSettings["editorDefaults"]["viewMode"]`/`["softTab"]` type references remain after their
  removal from `models.ts`, so no further edit needed there beyond deleting the two JSX blocks.

## Backend changes (`src-tauri`)

### `src-tauri/src/model/app_settings.rs`
- `EditorDefaults` struct: remove `soft_tab: bool` and `view_mode: ViewMode` fields.
- `ViewMode` enum: **keep** — still used by `MemoEditorSettings.view_mode` in `model/tree.rs`.
- `AppSettings::default()`: drop `soft_tab: true` / `view_mode: ViewMode::Live` from the
  `EditorDefaults { ... }` literal.

### `src-tauri/src/model/app_settings_patch.rs`
- `EditorDefaultsPatch`: remove `soft_tab: Option<bool>` and `view_mode: Option<ViewMode>` fields.
- `EditorDefaultsPatch::apply_to()`: drop the two corresponding `if let Some(v) = ...` blocks.
- Drop `ViewMode` from the `use super::app_settings::{...}` import list only if it becomes
  otherwise unused in this file (check after editing — `WrapMode` definitely stays).

### `src-tauri/src/model/tree.rs`
- `MemoEditorSettings` struct: remove `soft_tab: Option<bool>` field (`view_mode: Option<ViewMode>`
  stays — that's the still-live per-memo override).

### Migration / recovery JSON
- No `schemaVersion` bump: this is a field removal from a `#[serde(...)]` struct without
  `#[serde(deny_unknown_fields)]`, so an old `settings.json` containing stray `softTab`/`viewMode`
  keys under `editorDefaults` simply has those keys ignored by `serde_json` on next load/save
  (confirmed pattern already used elsewhere in this codebase, e.g. `appearance`'s `#[serde(default)]`
  additive-field precedent). No explicit migration code needed.

## Verification plan

- `npx tsc --noEmit` (or the project's configured typecheck script) — catches any remaining
  `softTab`/`viewMode` references in `EditorDefaults`/`MemoEditorSettings` call sites.
- `cargo check` inside `src-tauri` — catches any remaining `soft_tab`/`view_mode` field references
  in the removed `EditorDefaults`/`EditorDefaultsPatch` struct literals, plus unused-import
  warnings.
- Grep sweep for `softTab`/`viewMode`/`soft_tab`/`view_mode` after edits to confirm only the
  intentionally-kept `MemoEditorSettings.viewMode`/`view_mode` (TS/Rust) and `ViewMode` enum
  (Rust) remain.
