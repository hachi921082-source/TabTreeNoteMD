//! `AppSettings` — mirrors `docs/spec.md` §3.4 and the full example schema in
//! `docs/detailed-spec.md` §3.2 field-for-field. Serialized as `data/settings.json`
//! (`docs/plans/persistence-fs/PLAN.md` §2, §6).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const APP_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub schema_version: u32,
    pub app_version: String,
    pub tabs: Vec<TabSettingsEntry>,
    pub window: WindowSettings,
    pub theme: Theme,
    pub ui_font_scale: f64,
    /// Additive field, `docs/plans/appearance-theme/PLAN.md` §3: "basic customization of
    /// colors/fonts/line spacing" layered on top of the active theme's CSS variables. Not
    /// enumerated in `[spec §3.4]`'s schema example. `#[serde(default)]` so a settings.json
    /// written before this field existed still parses cleanly instead of falling into the
    /// corruption-recovery cascade (`settings::app_settings_store`'s `try_parse`).
    #[serde(default)]
    pub appearance: AppearanceSettings,
    pub last_opened: Option<LastOpened>,
    pub editor_defaults: EditorDefaults,
    pub autosave: AutosaveSettings,
    /// Additive field, `docs/plans/markdown/markdown-preview/PLAN.md` §3 — preview pane re-render
    /// debounce, independent of `autosave.plain_debounce_ms` above (no disk I/O involved).
    /// `#[serde(default)]` so a pre-existing `settings.json` (written before this feature) still
    /// parses, same treatment as `appearance`/`autostart` above.
    #[serde(default)]
    pub preview: PreviewSettings,
    pub links: LinkSettings,
    pub encoding: EncodingSettings,
    pub eol: EolSettings,
    pub shortcuts: HashMap<String, String>,
    /// Additive field, not enumerated in `[spec §3.4]`'s schema example
    /// (`docs/plans/window-operation/PLAN.md` §4: "a boolean in `AppSettings`... implementation
    /// discretion for exact shape since `spec.md` only says 'setting available'").
    pub autostart: AutostartSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabSettingsEntry {
    pub id: String,
    pub name: String,
    pub folder_path: String,
    /// Display cache only; the authoritative lock state is the tab's own
    /// `.tabtreenote.json` (`tree` vs `lock` field) — see
    /// `docs/detailed-spec.md` §3.2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSettings {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
    pub tree_width: u32,
    pub tree_side: TreeSide,
    /// `None` on first launch means "use the implementation-chosen default
    /// position" (`docs/detailed-spec.md` §3.2).
    #[serde(default)]
    pub toolbar_position: Option<ToolbarPosition>,
    pub toolbar_order: Vec<String>,
    /// Additive field (`docs/plans/window-operation/PLAN.md` §2): whether the tree pane's hide
    /// toggle is currently collapsed. Persisted alongside `tree_width`/`tree_side` since it's the
    /// same class of window-layout preference.
    #[serde(default)]
    pub tree_collapsed: bool,
    /// Additive field, GitHub issue #26: whether the floating Markdown toolbar is collapsed to
    /// header-only (grid hidden, panel snapped to the editor pane's bottom-right corner). Same
    /// persistence treatment as `tree_collapsed` above.
    #[serde(default)]
    pub toolbar_minimized: bool,
}

/// `docs/plans/window-operation/PLAN.md` §4: launch-to-tray-on-Windows-login setting. The actual
/// OS registration (Startup-folder shortcut) is performed by `commands::autostart` whenever this
/// is toggled, not derived implicitly from this flag alone — this field is the persisted
/// "what the user asked for" record, mirroring the OS-level state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AutostartSettings {
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TreeSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToolbarPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
}

/// "Basic customization of colors/fonts/line spacing" layered on top of the active theme's CSS
/// variables (`docs/plans/appearance-theme/PLAN.md` §2/§3) — not a full theme-authoring system,
/// just a handful of concrete overridable values. `None` for the two optional fields means "use
/// the active theme's default" (no override applied).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSettings {
    /// CSS color string (e.g. `"#4a90d9"`), overrides the theme's default accent/highlight color.
    /// `None` = use the active theme's default accent.
    #[serde(default)]
    pub accent_color: Option<String>,
    /// CSS `font-family` value for UI chrome text (tab bar, tree, menus, status bar, dialogs) —
    /// same scope as `ui_font_scale`, not the editor body font. `None` = use the built-in default
    /// UI font stack.
    #[serde(default)]
    pub ui_font_family: Option<String>,
    /// Multiplier applied to the tree row's base line height (`docs/plans/appearance-theme/PLAN.md`
    /// §2's "line spacing" customization point). `1.0` is a reasonably compact default; the app's
    /// pre-existing fixed 26px row height corresponds to roughly `1.3`.
    pub tree_row_line_height: f64,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        AppearanceSettings {
            accent_color: None,
            ui_font_family: None,
            tree_row_line_height: 1.3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastOpened {
    pub tab_id: String,
    pub memo_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorDefaults {
    pub wrap: WrapMode,
    pub tab_width: u32,
    /// GitHub issue #116 — restores the setting issue #61 removed.
    pub soft_tab: bool,
    pub show_line_numbers: bool,
    pub font_size: u32,
    /// Additive field, `docs/plans/markdown/markdown-preview/PLAN.md` §3 — default pane layout for
    /// a newly-opened memo with no per-memo override. `#[serde(default)]` on the field itself
    /// (rather than the whole struct, since `EditorDefaults` is non-additive/always fully present)
    /// so an old-format `settings.json` still parses.
    #[serde(default)]
    pub pane_mode: PaneMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WrapMode {
    None,
    Chars,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    Live,
    Source,
}

/// Pane-layout tri-state (`docs/plans/markdown/markdown-preview/PLAN.md` §2/§4) — fully
/// independent of `ViewMode` (live/source); the two toggles never interact except for the one
/// auto-reveal case documented on `model::tree::MemoEditorSettings::pane_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PaneMode {
    Edit,
    Split,
    Preview,
}

impl Default for PaneMode {
    fn default() -> Self {
        PaneMode::Edit
    }
}

/// `docs/plans/markdown/markdown-preview/PLAN.md` §3 — additive top-level settings section,
/// sibling of `autosave`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSettings {
    pub debounce_ms: u32,
}

impl Default for PreviewSettings {
    fn default() -> Self {
        PreviewSettings { debounce_ms: 150 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutosaveSettings {
    pub plain_debounce_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSettings {
    pub open_trigger: OpenTrigger,
    pub confirm_before_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenTrigger {
    CtrlClick,
    SingleClick,
    DoubleClick,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodingSettings {
    /// Only `"utf-8"` is valid at present (`docs/spec.md` §3.4); kept as a
    /// `String` rather than a single-variant enum since a future encoding
    /// addition is a data change, not a type change.
    pub default_save: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EolSettings {
    pub policy: EolPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EolPolicy {
    Preserve,
    Crlf,
    Lf,
    Cr,
}

/// Factory defaults used on first launch and as the "total loss" recovery
/// target (`docs/plans/persistence-fs/PLAN.md` §2, §6.3). Concrete values
/// follow the example given in `docs/detailed-spec.md` §3.2, except `theme`
/// and `window` position/size, which the spec leaves as an implementation
/// pick; `theme: Light` and a centered-ish 1200x800 window are chosen here.
impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            schema_version: APP_SETTINGS_SCHEMA_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            tabs: Vec::new(),
            window: WindowSettings {
                x: 100,
                y: 100,
                width: 1200,
                height: 800,
                maximized: false,
                tree_width: 260,
                tree_side: TreeSide::Left,
                toolbar_position: None,
                toolbar_order: default_toolbar_order(),
                tree_collapsed: false,
                toolbar_minimized: false,
            },
            theme: Theme::Light,
            ui_font_scale: 1.0,
            appearance: AppearanceSettings::default(),
            last_opened: None,
            editor_defaults: EditorDefaults {
                wrap: WrapMode::Window,
                tab_width: 4,
                soft_tab: true,
                show_line_numbers: true,
                font_size: 14,
                pane_mode: PaneMode::Edit,
            },
            autosave: AutosaveSettings {
                plain_debounce_ms: 1500,
            },
            preview: PreviewSettings::default(),
            links: LinkSettings {
                open_trigger: OpenTrigger::SingleClick,
                confirm_before_open: false,
            },
            encoding: EncodingSettings {
                default_save: "utf-8".to_string(),
            },
            eol: EolSettings {
                policy: EolPolicy::Preserve,
            },
            shortcuts: HashMap::new(),
            autostart: AutostartSettings { enabled: false },
        }
    }
}

/// The 16-slot fixed toolbar layout's default order
/// (`docs/detailed-spec.md` §3.2 example; exact ID spellings are
/// implementation-discretion per `docs/detailed-spec.md` §9, reused verbatim
/// from that example rather than invented anew).
fn default_toolbar_order() -> Vec<String> {
    [
        "undo",
        "color",
        "heading",
        "list",
        "bold",
        "strikethrough",
        "link",
        "tocUpdate",
        "redo",
        "fontSize",
        "codeBlock",
        "quote",
        "italic",
        "table",
        "image",
        "rearrangeToggle",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
