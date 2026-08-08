//! Tree domain data structures — `MetaTreeNode` (persisted, in `.tabtreenote.json`'s `tree`
//! field) vs `TreeNode` (runtime/reconciled, returned to the frontend) — `docs/spec.md` §3.4;
//! `docs/detailed-spec.md` §3.3, §3.5; `docs/plans/memo-tree/PLAN.md` §1.
//!
//! Per `memo-tree/PLAN.md` §1, `TreeNode` carries nothing `MetaTreeNode` doesn't (it's a
//! read-only reconciled view, not a superset) — the split exists on the TS side so the frontend
//! type can be `readonly`-flagged and structurally distinct from what gets serialized back on
//! save, not because the Rust representation needs two structs. Implemented here as a type
//! alias, per the plan's explicit discretion ("`developer-lite` may implement `TreeNode` as a
//! Rust type alias ... if that avoids duplication").

use serde::{Deserialize, Serialize};

use super::app_settings::{ViewMode, WrapMode};

/// A memo's unique identifier within a tab. Since a tab's `.md` files are always direct children
/// of the tab folder (no nested subfolders, `[spec §2.2]`) and the filesystem itself forbids two
/// files sharing a name in the same folder (`persistence-fs/PLAN.md` §8), `memoName` alone
/// already uniquely identifies a node anywhere in the tree — no separate index-path or id scheme
/// is needed.
pub type NodePath = String;

/// Persisted tree element — `.tabtreenote.json`'s `tree` array entries (`[spec §3.4]`,
/// `[detailed-spec §3.3]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaTreeNode {
    pub memo_name: String,
    pub expanded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Label>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<MemoEditorSettings>,
    /// Edit lock (`docs/plans/memo-tree/PLAN.md` §3 `memo_set_edit_lock`) — body-edit-only
    /// block, no password. Distinct from tab lock (`tab-lock/PLAN.md`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(default)]
    pub children: Vec<MetaTreeNode>,
}

impl MetaTreeNode {
    /// A freshly-reconciled node for a real `.md` file with no matching metadata entry
    /// (`[detailed-spec §3.5]`): `expanded: true`, no label, no editor override, `locked: false`.
    pub fn default_for_new_file(memo_name: String) -> Self {
        Self {
            memo_name,
            expanded: true,
            label: None,
            editor: None,
            locked: Some(false),
            children: Vec::new(),
        }
    }
}

/// Runtime/UI-only reconciled tree element, returned by `tree_open` (`[spec §3.4]`'s "Return
/// type of `tab:openTree`"). Intentionally covariant with `MetaTreeNode` (see module doc) —
/// the frontend's TS mirror is what actually enforces the `readonly` / non-persistable
/// distinction, since both wire shapes are identical here.
pub type TreeNode = MetaTreeNode;

/// `label.color` — confirmed concrete 5-choice palette (`[detailed-spec §3.3]`,
/// `DECISIONS.md` "Icon Submenu Clarified as the `label.color` Picker..."). Modeled as a 4-variant
/// enum (Red/Blue/Green/Yellow) so the 5th choice ("None") is expressed as Rust's own
/// `Option<LabelColor> == None`, rather than adding a redundant `None` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelColor {
    Red,
    Blue,
    Green,
    Yellow,
}

impl LabelColor {
    pub fn hex(self) -> &'static str {
        match self {
            LabelColor::Red => "#e05a5a",
            LabelColor::Blue => "#4a90d9",
            LabelColor::Green => "#4caf50",
            LabelColor::Yellow => "#e0c040",
        }
    }
}

/// `label.kind` stays reserved and unimplemented for v1 (`null` only, `[detailed-spec §3.3]`) —
/// modeled as a passthrough `serde_json::Value` so any (currently nonexistent) value round-trips
/// untouched rather than being silently dropped; no command in this codebase ever writes to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<serde_json::Value>,
}

/// Per-memo editor setting overrides (`[detailed-spec §3.3]`). Every field is
/// optional/nullable: absent means "follow `AppSettings.editorDefaults`" (`[spec §3.4]`).
/// Reuses `WrapMode`/`ViewMode` from `model/app_settings.rs` rather than duplicating them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MemoEditorSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<WrapMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_line_numbers: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_mode: Option<ViewMode>,
    /// 1-indexed document line number visible at the top of the viewport at save time
    /// (`[detailed-spec §5.3]`, "Document-Position-Based Anchor, Not Raw Pixel Offset").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_position: Option<u32>,
    /// CodeMirror `EditorSelection.main.anchor` — character offset into the document, at save
    /// time (GitHub issue #28, "Reopening a memo restores scroll position but not cursor
    /// position"). Paired with `cursor_head` below so a real (non-collapsed) selection round-trips
    /// too, not just a collapsed cursor (`anchor === head`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_anchor: Option<u32>,
    /// CodeMirror `EditorSelection.main.head` — character offset into the document, at save time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_head: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_color_hex_matches_confirmed_palette() {
        assert_eq!(LabelColor::Red.hex(), "#e05a5a");
        assert_eq!(LabelColor::Blue.hex(), "#4a90d9");
        assert_eq!(LabelColor::Green.hex(), "#4caf50");
        assert_eq!(LabelColor::Yellow.hex(), "#e0c040");
    }

    #[test]
    fn default_for_new_file_matches_reconciliation_defaults() {
        let node = MetaTreeNode::default_for_new_file("Shopping list".to_string());
        assert!(node.expanded);
        assert!(node.label.is_none());
        assert!(node.editor.is_none());
        assert_eq!(node.locked, Some(false));
        assert!(node.children.is_empty());
    }

    #[test]
    fn meta_tree_node_round_trips_through_json() {
        let node = MetaTreeNode {
            memo_name: "Diary".to_string(),
            expanded: true,
            label: Some(Label { color: Some(LabelColor::Blue.hex().to_string()), kind: None }),
            editor: Some(MemoEditorSettings { wrap: Some(WrapMode::Window), ..Default::default() }),
            locked: Some(false),
            children: vec![],
        };
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(json["memoName"], "Diary");
        assert_eq!(json["label"]["color"], "#4a90d9");
        let back: MetaTreeNode = serde_json::from_value(json).unwrap();
        assert_eq!(back, node);
    }
}
