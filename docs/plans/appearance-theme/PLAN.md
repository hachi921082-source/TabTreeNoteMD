# PLAN: Appearance and Theme

> Status: implemented. Two CSS variable sets (`src/theme.css`'s `:root[data-theme="light"]` /
> `:root[data-theme="dark"]`), theme switch via `state/useAppearance.ts` toggling `data-theme` on
> `document.documentElement` (no re-render/remount). `uiFontScale` and `appearance.*`
> (`accentColor`/`uiFontFamily`/`treeRowLineHeight`) applied as inline CSS custom properties by the
> same hook; the editor body font stays structurally separate via `editor/textCommands.ts`'s
> `EditorView.theme` fixed-px extension, unaffected by `--ui-font-scale`. Settings UI: Appearance
> section in `components/Settings/SettingsDialog.tsx` (theme select, UI font scale slider, accent
> color picker, UI font family input, tree row line-height slider), all writing immediately via
> `settings_set`. Schema: `AppearanceSettings` in `src-tauri/src/model/app_settings.rs` (`#[serde(default)]`
> for backward compatibility) mirrored in `src/shared/models.ts`. Depends on `project-architecture/PLAN.md`
> and `persistence-fs/PLAN.md` (`settings_get`/`settings_set` for persisting theme/font choices).

Sources: `docs/spec.md` §2.5; `docs/detailed-spec.md` §5.5.

---

## 1. Scope

- CSS-based light/dark theme, standard-provided (`[spec §2.5]`).
- Basic customization of colors, fonts, line spacing layered on top of the chosen theme's defaults —
  **not** a full theme-authoring system (`[detailed-spec §5.5]`).
- `uiFontScale` (80–150%, `AppSettings.uiFontScale`, stored as a `1.0`-based multiplier per
  `[detailed-spec §3.2]`'s example) applies to all UI chrome text — tab bar, tree, menus, status bar,
  dialogs — but explicitly **not** the editor body font, which has its own independent
  `editorDefaults.fontSize` (global) / per-memo override (`MemoEditorSettings.fontSize`).

## 2. Implementation

- Two CSS variable sets (`:root[data-theme="light"]` / `:root[data-theme="dark"]`) covering the app's
  color palette (background/foreground/accent/border/selection colors for each pane). Theme switch =
  toggling the `data-theme` attribute on the root element, no re-render/remount needed.
- `uiFontScale` applied via a CSS custom property (e.g. `--ui-font-scale`) multiplying a base `rem`
  size on chrome-text-bearing elements; editor-body text uses a **separate**, unscaled font-size chain
  driven by `editorDefaults.fontSize`/per-memo override (CodeMirror's own font-size config, not the
  `--ui-font-scale` variable) — this separation must be structural (different CSS custom property /
  different CodeMirror theme extension), not just "happens to not be affected today," so a future
  chrome-CSS change can't accidentally leak into the editor.
- "Basic customization of colors, fonts, line spacing": exposed via the Settings screen (Appearance
  tab, see `window-operation/PLAN.md` §5 for the settings-screen shell this lives inside) as a handful
  of concrete overridable values layered on the active theme's CSS variables (e.g. accent color, UI
  font family, line-spacing multiplier) — exact enumerated list of customizable properties is an
  implementation-discretion UI-detail (`spec.md` says "basic," not an exhaustive spec), reasonable
  starting set: accent/highlight color, UI font family, tree row line-height.
- All Appearance settings persist via `settings_set` (`persistence-fs/PLAN.md` §6.2), applied
  immediately on change (no separate "Apply" step needed given these are cheap CSS-variable updates).

## 3. Settings Schema Touchpoints

No new fields beyond what `[spec §3.4]` already defines: `theme: 'light' | 'dark'`, `uiFontScale:
number`. If "basic customization of colors/fonts/line spacing" needs additional persisted fields
beyond these two, they nest under a new `appearance` object in `AppSettings` — this is an additive,
backward-compatible schema change (bump handling not required since no prior schema version ships yet)
left to `developer-lite` to finalize the exact shape at implementation time, since `spec.md` doesn't
enumerate the customizable properties beyond the "basic" qualifier.

## Open Items

None.
