//! `.tabtreenote.json` — per-tab metadata file (`docs/spec.md` §3.4;
//! `docs/detailed-spec.md` §3.3, §3.4; `docs/plans/tab-management/PLAN.md` §2 step 3;
//! `docs/plans/memo-tree/PLAN.md` §1-§2).
//!
//! Unlocked tabs have a `tree` field (`Vec<MetaTreeNode>`, see `model/tree.rs`). Locked tabs
//! (`tab-lock/PLAN.md`, not yet implemented) have a `lock` field instead, and no `tree` field.
//! Both are modeled as optional here so this module can at least recognize *which* shape a given
//! file is (`tree_open`'s locked-tab branch, `memo-tree/PLAN.md` §2 step 1, needs only that much)
//! without needing to understand the `lock` blob's internal structure — that remains
//! `tab-lock/PLAN.md`'s own responsibility, modeled here as an opaque passthrough
//! `serde_json::Value` so a locked tab's metadata round-trips untouched through any code path
//! that doesn't specifically handle locking.

use serde::{Deserialize, Serialize};

use super::tree::MetaTreeNode;

pub const TAB_META_SCHEMA_VERSION: u32 = 1;

/// Filename of the per-tab metadata file, hidden in the tab folder
/// (`docs/spec.md` §3.2). Shared so `commands/tree.rs`
/// (`docs/plans/memo-tree/PLAN.md`) doesn't repeat this string literal.
pub const TAB_META_FILE_NAME: &str = ".tabtreenote.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabMetaFile {
    pub schema_version: u32,
    pub app_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<Vec<MetaTreeNode>>,
    /// Opaque `tab-lock/PLAN.md` blob; `None` for an unlocked tab. Presence (not content) is
    /// all `memo-tree/PLAN.md`'s `tree_open` needs to know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<serde_json::Value>,
}

impl TabMetaFile {
    /// The initial content written for a brand-new tab, or a folder that was
    /// picked but has no `.tabtreenote.json` yet (`tab-management/PLAN.md`
    /// §2 step 3 — "initialize it with an empty tree: []").
    pub fn fresh() -> Self {
        Self {
            schema_version: TAB_META_SCHEMA_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            tree: Some(Vec::new()),
            lock: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_serializes_with_empty_tree() {
        let meta = TabMetaFile::fresh();
        let json = serde_json::to_value(&meta).unwrap();
        assert_eq!(json["tree"], serde_json::json!([]));
        assert_eq!(json["schemaVersion"], serde_json::json!(1));
        assert!(json.get("lock").is_none());
    }

    #[test]
    fn locked_shape_parses_without_a_tree_field() {
        let json = serde_json::json!({
            "schemaVersion": 1,
            "appVersion": "0.1.0",
            "lock": { "formatVersion": 1 }
        });
        let meta: TabMetaFile = serde_json::from_value(json).unwrap();
        assert!(meta.tree.is_none());
        assert!(meta.lock.is_some());
    }
}
