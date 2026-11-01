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

// v1.1.0: bumped 1 -> 2, combined bump covering both this file's `last_viewed_memo` addition
// (`docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §1) and `docs/plans/v1.1.0/favorites/PLAN.md`'s
// `MetaTreeNode.favorite` addition — a single bump for both, not two separate increments
// (`[detailed-spec v1.1.0 §4.1]`).
pub const TAB_META_SCHEMA_VERSION: u32 = 2;

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
    /// `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §1 — the tab's own per-tab "last viewed memo"
    /// pointer, sibling to `tree`. `None`/absent means never-viewed (or a pre-upgrade file
    /// predating this field, via `#[serde(default)]`). For a **locked** tab this field is always
    /// `None` here — the real value lives inside the encrypted `lock` payload instead
    /// (`crypto::lock_blob::LockPayload::last_viewed_memo`), never as a plaintext sibling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_viewed_memo: Option<String>,
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
            last_viewed_memo: None,
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
        assert_eq!(json["schemaVersion"], serde_json::json!(2));
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

    /// `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §1: "no destructive migration — a pre-upgrade
    /// file simply parses with `last_viewed_memo: None` via `#[serde(default)]`".
    #[test]
    fn pre_v1_1_0_file_with_no_last_viewed_memo_field_parses_as_none() {
        let json = serde_json::json!({
            "schemaVersion": 1,
            "appVersion": "0.1.0",
            "tree": []
        });
        let meta: TabMetaFile = serde_json::from_value(json).unwrap();
        assert_eq!(meta.last_viewed_memo, None);
    }

    #[test]
    fn last_viewed_memo_round_trips_and_is_omitted_when_none() {
        let mut meta = TabMetaFile::fresh();
        assert!(serde_json::to_value(&meta).unwrap().get("lastViewedMemo").is_none());

        meta.last_viewed_memo = Some("Diary".to_string());
        let json = serde_json::to_value(&meta).unwrap();
        assert_eq!(json["lastViewedMemo"], serde_json::json!("Diary"));
        let round_tripped: TabMetaFile = serde_json::from_value(json).unwrap();
        assert_eq!(round_tripped.last_viewed_memo.as_deref(), Some("Diary"));
    }
}
