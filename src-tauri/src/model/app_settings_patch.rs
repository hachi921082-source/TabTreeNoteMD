//! `AppSettingsPatch` — deep-partial patch shape accepted by `settings_set`
//! (`docs/plans/persistence-fs/PLAN.md` §6.2). Mirrors the TS
//! `AppSettingsPatch` type in `src/shared/models.ts` field-for-field: every
//! field, including nested object fields, is optional; nested objects merge
//! per-field rather than wholesale-replacing the stored object.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

use super::app_settings::{
    AppSettings, AppearanceSettings, AutosaveSettings, AutostartSettings, EditorDefaults, EncodingSettings,
    EolPolicy, EolSettings, LastOpened, LinkSettings, OpenTrigger, PaneMode, PreviewSettings, SplitOrientation, TabSettingsEntry,
    Theme, ToolbarPosition, TreeSide, WindowSettings, WrapMode,
};

/// Distinguishes "field absent from patch" (outer `None`, leave unchanged)
/// from "field present with an explicit `null`" (`Some(None)`, clear the
/// value) for nullable fields, which plain `Option<Option<T>>` cannot do
/// with serde_json by default (both collapse to the same result otherwise).
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsPatch {
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub app_version: Option<String>,
    #[serde(default)]
    pub tabs: Option<Vec<TabSettingsEntry>>,
    #[serde(default)]
    pub window: Option<WindowSettingsPatch>,
    #[serde(default)]
    pub theme: Option<Theme>,
    #[serde(default)]
    pub ui_font_scale: Option<f64>,
    #[serde(default)]
    pub appearance: Option<AppearanceSettingsPatch>,
    #[serde(default, deserialize_with = "double_option")]
    pub last_opened: Option<Option<LastOpened>>,
    #[serde(default)]
    pub editor_defaults: Option<EditorDefaultsPatch>,
    #[serde(default)]
    pub autosave: Option<AutosaveSettingsPatch>,
    /// Additive field, `docs/plans/markdown/markdown-preview/PLAN.md` §3.
    #[serde(default)]
    pub preview: Option<PreviewSettingsPatch>,
    #[serde(default)]
    pub links: Option<LinkSettingsPatch>,
    #[serde(default)]
    pub encoding: Option<EncodingSettingsPatch>,
    #[serde(default)]
    pub eol: Option<EolSettingsPatch>,
    #[serde(default)]
    pub shortcuts: Option<HashMap<String, String>>,
    #[serde(default)]
    pub autostart: Option<AutostartSettingsPatch>,
    /// Additive field, `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md` §1/§8. Nullable ("set to
    /// disabled" vs "leave unchanged"), same `double_option` treatment as `last_opened`/
    /// `toolbar_position`/`accent_color` above — a patch that never mentions this key must leave
    /// the stored value untouched, while a patch carrying an explicit `null` (the Settings UI's
    /// "無効" choice) must clear it to disabled.
    #[serde(default, deserialize_with = "double_option")]
    pub tab_lock_auto_relock_minutes: Option<Option<u32>>,
    /// Additive field, `docs/plans/v1.2.0/i18n/PLAN.md` §2. Nullable ("reset to auto-detect" vs
    /// "leave unchanged"), same `double_option` treatment as `tab_lock_auto_relock_minutes` above.
    #[serde(default, deserialize_with = "double_option")]
    pub language: Option<Option<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSettingsPatch {
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub maximized: Option<bool>,
    #[serde(default)]
    pub tree_width: Option<u32>,
    #[serde(default)]
    pub tree_side: Option<TreeSide>,
    #[serde(default, deserialize_with = "double_option")]
    pub toolbar_position: Option<Option<ToolbarPosition>>,
    #[serde(default)]
    pub toolbar_order: Option<Vec<String>>,
    #[serde(default)]
    pub tree_collapsed: Option<bool>,
    #[serde(default)]
    pub toolbar_minimized: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutostartSettingsPatch {
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// `accent_color`/`ui_font_family` are nullable ("use theme default" vs "leave unchanged"), same
/// `double_option` treatment as `last_opened`/`toolbar_position` above.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSettingsPatch {
    #[serde(default, deserialize_with = "double_option")]
    pub accent_color: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub ui_font_family: Option<Option<String>>,
    #[serde(default)]
    pub tree_row_line_height: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorDefaultsPatch {
    #[serde(default)]
    pub wrap: Option<WrapMode>,
    #[serde(default)]
    pub tab_width: Option<u32>,
    #[serde(default)]
    pub soft_tab: Option<bool>,
    #[serde(default)]
    pub show_line_numbers: Option<bool>,
    #[serde(default)]
    pub font_size: Option<u32>,
    /// Additive field, `docs/plans/markdown/markdown-preview/PLAN.md` §3.
    #[serde(default)]
    pub pane_mode: Option<PaneMode>,
    /// GitHub issue #295。
    #[serde(default)]
    pub split_orientation: Option<SplitOrientation>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutosaveSettingsPatch {
    #[serde(default)]
    pub plain_debounce_ms: Option<u32>,
}

/// `docs/plans/markdown/markdown-preview/PLAN.md` §3 — additive top-level settings section patch,
/// sibling of `AutosaveSettingsPatch`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSettingsPatch {
    #[serde(default)]
    pub debounce_ms: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSettingsPatch {
    #[serde(default)]
    pub open_trigger: Option<OpenTrigger>,
    #[serde(default)]
    pub confirm_before_open: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodingSettingsPatch {
    #[serde(default)]
    pub default_save: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EolSettingsPatch {
    #[serde(default)]
    pub policy: Option<EolPolicy>,
}

impl AppSettingsPatch {
    /// Applies this patch onto `current` in place, merging nested objects
    /// per-field rather than wholesale-replacing them
    /// (`docs/plans/persistence-fs/PLAN.md` §6.2).
    pub fn apply_to(self, current: &mut AppSettings) {
        if let Some(v) = self.schema_version {
            current.schema_version = v;
        }
        if let Some(v) = self.app_version {
            current.app_version = v;
        }
        if let Some(v) = self.tabs {
            current.tabs = v;
        }
        if let Some(window_patch) = self.window {
            window_patch.apply_to(&mut current.window);
        }
        if let Some(v) = self.theme {
            current.theme = v;
        }
        if let Some(v) = self.ui_font_scale {
            current.ui_font_scale = v;
        }
        if let Some(appearance_patch) = self.appearance {
            appearance_patch.apply_to(&mut current.appearance);
        }
        if let Some(v) = self.last_opened {
            current.last_opened = v;
        }
        if let Some(editor_patch) = self.editor_defaults {
            editor_patch.apply_to(&mut current.editor_defaults);
        }
        if let Some(autosave_patch) = self.autosave {
            autosave_patch.apply_to(&mut current.autosave);
        }
        if let Some(preview_patch) = self.preview {
            preview_patch.apply_to(&mut current.preview);
        }
        if let Some(links_patch) = self.links {
            links_patch.apply_to(&mut current.links);
        }
        if let Some(encoding_patch) = self.encoding {
            encoding_patch.apply_to(&mut current.encoding);
        }
        if let Some(eol_patch) = self.eol {
            eol_patch.apply_to(&mut current.eol);
        }
        if let Some(v) = self.shortcuts {
            current.shortcuts = v;
        }
        if let Some(autostart_patch) = self.autostart {
            autostart_patch.apply_to(&mut current.autostart);
        }
        if let Some(v) = self.tab_lock_auto_relock_minutes {
            current.tab_lock_auto_relock_minutes = v;
        }
        if let Some(v) = self.language {
            current.language = v;
        }
    }
}

impl WindowSettingsPatch {
    fn apply_to(self, current: &mut WindowSettings) {
        if let Some(v) = self.x {
            current.x = v;
        }
        if let Some(v) = self.y {
            current.y = v;
        }
        if let Some(v) = self.width {
            current.width = v;
        }
        if let Some(v) = self.height {
            current.height = v;
        }
        if let Some(v) = self.maximized {
            current.maximized = v;
        }
        if let Some(v) = self.tree_width {
            current.tree_width = v;
        }
        if let Some(v) = self.tree_side {
            current.tree_side = v;
        }
        if let Some(v) = self.toolbar_position {
            current.toolbar_position = v;
        }
        if let Some(v) = self.toolbar_order {
            current.toolbar_order = v;
        }
        if let Some(v) = self.tree_collapsed {
            current.tree_collapsed = v;
        }
        if let Some(v) = self.toolbar_minimized {
            current.toolbar_minimized = v;
        }
    }
}

impl AutostartSettingsPatch {
    fn apply_to(self, current: &mut AutostartSettings) {
        if let Some(v) = self.enabled {
            current.enabled = v;
        }
    }
}

impl AppearanceSettingsPatch {
    fn apply_to(self, current: &mut AppearanceSettings) {
        if let Some(v) = self.accent_color {
            current.accent_color = v;
        }
        if let Some(v) = self.ui_font_family {
            current.ui_font_family = v;
        }
        if let Some(v) = self.tree_row_line_height {
            current.tree_row_line_height = v;
        }
    }
}

impl EditorDefaultsPatch {
    fn apply_to(self, current: &mut EditorDefaults) {
        if let Some(v) = self.wrap {
            current.wrap = v;
        }
        if let Some(v) = self.tab_width {
            current.tab_width = v;
        }
        if let Some(v) = self.soft_tab {
            current.soft_tab = v;
        }
        if let Some(v) = self.show_line_numbers {
            current.show_line_numbers = v;
        }
        if let Some(v) = self.font_size {
            current.font_size = v;
        }
        if let Some(v) = self.pane_mode {
            current.pane_mode = v;
        }
        if let Some(v) = self.split_orientation {
            current.split_orientation = v;
        }
    }
}

impl AutosaveSettingsPatch {
    fn apply_to(self, current: &mut AutosaveSettings) {
        if let Some(v) = self.plain_debounce_ms {
            current.plain_debounce_ms = v;
        }
    }
}

impl PreviewSettingsPatch {
    fn apply_to(self, current: &mut PreviewSettings) {
        if let Some(v) = self.debounce_ms {
            current.debounce_ms = v;
        }
    }
}

impl LinkSettingsPatch {
    fn apply_to(self, current: &mut LinkSettings) {
        if let Some(v) = self.open_trigger {
            current.open_trigger = v;
        }
        if let Some(v) = self.confirm_before_open {
            current.confirm_before_open = v;
        }
    }
}

impl EncodingSettingsPatch {
    fn apply_to(self, current: &mut EncodingSettings) {
        if let Some(v) = self.default_save {
            current.default_save = v;
        }
    }
}

impl EolSettingsPatch {
    fn apply_to(self, current: &mut EolSettings) {
        if let Some(v) = self.policy {
            current.policy = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_fields_leave_current_value_unchanged() {
        let mut current = AppSettings::default();
        current.ui_font_scale = 1.2;
        let patch: AppSettingsPatch = serde_json::from_str("{}").unwrap();
        patch.apply_to(&mut current);
        assert_eq!(current.ui_font_scale, 1.2);
    }

    #[test]
    fn nested_window_patch_merges_per_field() {
        let mut current = AppSettings::default();
        current.window.width = 1200;
        current.window.height = 800;

        let patch: AppSettingsPatch = serde_json::from_str(r#"{"window":{"width":1000}}"#).unwrap();
        patch.apply_to(&mut current);

        assert_eq!(current.window.width, 1000);
        assert_eq!(current.window.height, 800, "untouched sibling field must survive the merge");
    }

    #[test]
    fn double_option_distinguishes_absent_from_explicit_null() {
        let mut current = AppSettings::default();
        current.last_opened = Some(LastOpened { tab_id: "t1".into() });

        // Key absent -> leave unchanged.
        let patch: AppSettingsPatch = serde_json::from_str("{}").unwrap();
        patch.apply_to(&mut current);
        assert!(current.last_opened.is_some());

        // Key present with explicit null -> clear it.
        let patch: AppSettingsPatch = serde_json::from_str(r#"{"lastOpened":null}"#).unwrap();
        patch.apply_to(&mut current);
        assert!(current.last_opened.is_none());
    }

    /// `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §1: a pre-upgrade `lastOpened` patch carrying
    /// the now-removed `memoName` field must still parse (unknown-field tolerance is this struct's
    /// default), with just `tabId` applied.
    #[test]
    fn last_opened_patch_tolerates_a_pre_upgrade_memo_name_field() {
        let mut current = AppSettings::default();
        let patch: AppSettingsPatch = serde_json::from_str(r#"{"lastOpened":{"tabId":"t1","memoName":"Shopping list"}}"#).unwrap();
        patch.apply_to(&mut current);
        assert_eq!(current.last_opened, Some(LastOpened { tab_id: "t1".into() }));
    }

    #[test]
    fn preview_and_pane_mode_patches_merge_per_field_and_leave_siblings_untouched() {
        // `docs/plans/markdown/markdown-preview/PLAN.md` §3 — `preview.debounceMs` and
        // `editorDefaults.paneMode` are both additive fields; verify their patches merge like every
        // other nested-object field above rather than replacing the whole nested object.
        let mut current = AppSettings::default();
        assert_eq!(current.preview.debounce_ms, 150);
        assert_eq!(current.editor_defaults.pane_mode, PaneMode::Edit);

        let patch: AppSettingsPatch = serde_json::from_str(r#"{"preview":{"debounceMs":500}}"#).unwrap();
        patch.apply_to(&mut current);
        assert_eq!(current.preview.debounce_ms, 500);

        let patch: AppSettingsPatch = serde_json::from_str(r#"{"editorDefaults":{"paneMode":"split"}}"#).unwrap();
        patch.apply_to(&mut current);
        assert_eq!(current.editor_defaults.pane_mode, PaneMode::Split);
        // Untouched sibling `EditorDefaults` fields must survive the merge.
        assert_eq!(current.editor_defaults.wrap, WrapMode::Window);
        assert_eq!(current.editor_defaults.font_size, 14);
    }

    #[test]
    fn split_orientation_defaults_to_horizontal_and_patch_merges() {
        // GitHub issue #295 — 既定は左右、パッチで上下に切り替わり他フィールドは不変。
        let mut current = AppSettings::default();
        assert_eq!(current.editor_defaults.split_orientation, SplitOrientation::Horizontal);
        let patch: AppSettingsPatch = serde_json::from_str(r#"{"editorDefaults":{"splitOrientation":"vertical"}}"#).unwrap();
        patch.apply_to(&mut current);
        assert_eq!(current.editor_defaults.split_orientation, SplitOrientation::Vertical);
        assert_eq!(current.editor_defaults.pane_mode, PaneMode::Edit);
    }

    #[test]
    fn missing_preview_field_in_a_pre_existing_settings_json_falls_back_to_defaults() {
        // Simulates loading a `settings.json` written before `AppSettings.preview` existed
        // (`#[serde(default)]` on that field) — must parse cleanly, same treatment already verified
        // for `appearance` above.
        let json = serde_json::to_value(AppSettings::default()).unwrap();
        let mut without_preview = json.as_object().unwrap().clone();
        without_preview.remove("preview");
        let parsed: AppSettings = serde_json::from_value(serde_json::Value::Object(without_preview)).unwrap();
        assert_eq!(parsed.preview, PreviewSettings::default());
    }

    #[test]
    fn appearance_patch_merges_per_field_and_null_clears_optional_overrides() {
        let mut current = AppSettings::default();
        current.appearance.accent_color = Some("#ff0000".into());
        current.appearance.tree_row_line_height = 1.3;

        // Setting `uiFontFamily` must not disturb the sibling `accentColor`/`treeRowLineHeight`.
        let patch: AppSettingsPatch = serde_json::from_str(r#"{"appearance":{"uiFontFamily":"Meiryo"}}"#).unwrap();
        patch.apply_to(&mut current);
        assert_eq!(current.appearance.accent_color.as_deref(), Some("#ff0000"));
        assert_eq!(current.appearance.ui_font_family.as_deref(), Some("Meiryo"));
        assert_eq!(current.appearance.tree_row_line_height, 1.3);

        // Explicit null on `accentColor` clears the override back to "use theme default".
        let patch: AppSettingsPatch = serde_json::from_str(r#"{"appearance":{"accentColor":null}}"#).unwrap();
        patch.apply_to(&mut current);
        assert!(current.appearance.accent_color.is_none());
        assert_eq!(current.appearance.ui_font_family.as_deref(), Some("Meiryo"), "untouched sibling field must survive the merge");
    }

    /// `docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md` §1: the field-absent-from-JSON case (a
    /// pre-upgrade `settings.json`) must resolve to the documented default `15`, not to disabled —
    /// distinct from an explicit `null`, which DOES mean disabled (next test).
    #[test]
    fn missing_auto_relock_field_in_a_pre_existing_settings_json_falls_back_to_the_documented_default() {
        let json = serde_json::to_value(AppSettings::default()).unwrap();
        let mut without_field = json.as_object().unwrap().clone();
        without_field.remove("tabLockAutoRelockMinutes");
        let parsed: AppSettings = serde_json::from_value(serde_json::Value::Object(without_field)).unwrap();
        assert_eq!(parsed.tab_lock_auto_relock_minutes, Some(15));
    }

    /// An explicit `null` already written to `settings.json` (e.g. by a prior "無効" selection)
    /// must deserialize as disabled, not silently fall back to the default like the absent-key case
    /// above — the two must stay distinguishable.
    #[test]
    fn explicit_null_auto_relock_field_deserializes_as_disabled() {
        let json = serde_json::to_value(AppSettings::default()).unwrap();
        let mut with_null = json.as_object().unwrap().clone();
        with_null.insert("tabLockAutoRelockMinutes".to_string(), serde_json::Value::Null);
        let parsed: AppSettings = serde_json::from_value(serde_json::Value::Object(with_null)).unwrap();
        assert_eq!(parsed.tab_lock_auto_relock_minutes, None);
    }

    #[test]
    fn auto_relock_patch_distinguishes_absent_from_explicit_null_and_a_value() {
        let mut current = AppSettings::default();
        assert_eq!(current.tab_lock_auto_relock_minutes, Some(15));

        // Key absent -> leave unchanged.
        let patch: AppSettingsPatch = serde_json::from_str("{}").unwrap();
        patch.apply_to(&mut current);
        assert_eq!(current.tab_lock_auto_relock_minutes, Some(15));

        // Key present with a value -> set it.
        let patch: AppSettingsPatch = serde_json::from_str(r#"{"tabLockAutoRelockMinutes":30}"#).unwrap();
        patch.apply_to(&mut current);
        assert_eq!(current.tab_lock_auto_relock_minutes, Some(30));

        // Key present with explicit null -> disabled.
        let patch: AppSettingsPatch = serde_json::from_str(r#"{"tabLockAutoRelockMinutes":null}"#).unwrap();
        patch.apply_to(&mut current);
        assert_eq!(current.tab_lock_auto_relock_minutes, None);
    }

    #[test]
    fn missing_appearance_field_in_a_pre_existing_settings_json_falls_back_to_defaults() {
        // Simulates loading a `settings.json` written before `AppSettings.appearance` existed
        // (`#[serde(default)]` on that field, `model/app_settings.rs`) — must parse cleanly rather
        // than falling into `settings::app_settings_store`'s corruption-recovery cascade.
        let json = serde_json::to_value(AppSettings::default()).unwrap();
        let mut without_appearance = json.as_object().unwrap().clone();
        without_appearance.remove("appearance");
        let parsed: AppSettings = serde_json::from_value(serde_json::Value::Object(without_appearance)).unwrap();
        assert_eq!(parsed.appearance, AppearanceSettings::default());
    }
}
