//! `AppSettingsPatch` — deep-partial patch shape accepted by `settings_set`
//! (`docs/plans/persistence-fs/PLAN.md` §6.2). Mirrors the TS
//! `AppSettingsPatch` type in `src/shared/models.ts` field-for-field: every
//! field, including nested object fields, is optional; nested objects merge
//! per-field rather than wholesale-replacing the stored object.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

use super::app_settings::{
    AppSettings, AutosaveSettings, AutostartSettings, EditorDefaults, EncodingSettings, EolPolicy, EolSettings,
    LastOpened, LinkSettings, OpenTrigger, TabSettingsEntry, Theme, ToolbarPosition, TreeSide, ViewMode,
    WindowSettings, WrapMode,
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
    #[serde(default, deserialize_with = "double_option")]
    pub last_opened: Option<Option<LastOpened>>,
    #[serde(default)]
    pub editor_defaults: Option<EditorDefaultsPatch>,
    #[serde(default)]
    pub autosave: Option<AutosaveSettingsPatch>,
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
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutostartSettingsPatch {
    #[serde(default)]
    pub enabled: Option<bool>,
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
    #[serde(default)]
    pub view_mode: Option<ViewMode>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutosaveSettingsPatch {
    #[serde(default)]
    pub plain_debounce_ms: Option<u32>,
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
        if let Some(v) = self.last_opened {
            current.last_opened = v;
        }
        if let Some(editor_patch) = self.editor_defaults {
            editor_patch.apply_to(&mut current.editor_defaults);
        }
        if let Some(autosave_patch) = self.autosave {
            autosave_patch.apply_to(&mut current.autosave);
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
    }
}

impl AutostartSettingsPatch {
    fn apply_to(self, current: &mut AutostartSettings) {
        if let Some(v) = self.enabled {
            current.enabled = v;
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
        if let Some(v) = self.view_mode {
            current.view_mode = v;
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
        current.last_opened = Some(LastOpened {
            tab_id: "t1".into(),
            memo_name: "Shopping list".into(),
        });

        // Key absent -> leave unchanged.
        let patch: AppSettingsPatch = serde_json::from_str("{}").unwrap();
        patch.apply_to(&mut current);
        assert!(current.last_opened.is_some());

        // Key present with explicit null -> clear it.
        let patch: AppSettingsPatch = serde_json::from_str(r#"{"lastOpened":null}"#).unwrap();
        patch.apply_to(&mut current);
        assert!(current.last_opened.is_none());
    }
}
