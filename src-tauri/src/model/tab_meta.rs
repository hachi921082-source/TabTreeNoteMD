//! `.tabtreenote.json` — unlocked-tab metadata file
//! (`docs/spec.md` §3.4; `docs/detailed-spec.md` §3.3;
//! `docs/plans/tab-management/PLAN.md` §2 step 3).
//!
//! Only the shape needed to freshly initialize this file when a tab is
//! created/registered is defined here (`tree: []`, plus the schema/app
//! version header shared with `settings.json`'s convention). The full
//! `MetaTreeNode` element type (label, editor overrides, expand state, etc.)
//! is `memo-tree/PLAN.md`'s responsibility; until that feature lands, `tree`
//! is only ever read/written here as an empty array or passed through
//! untouched, so a permissive `serde_json::Value` element type avoids this
//! file needing to duplicate or anticipate that not-yet-implemented shape.

use serde::{Deserialize, Serialize};

pub const TAB_META_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabMetaFile {
    pub schema_version: u32,
    pub app_version: String,
    pub tree: Vec<serde_json::Value>,
}

impl TabMetaFile {
    /// The initial content written for a brand-new tab, or a folder that was
    /// picked but has no `.tabtreenote.json` yet (`tab-management/PLAN.md`
    /// §2 step 3 — "initialize it with an empty tree: []").
    pub fn fresh() -> Self {
        Self {
            schema_version: TAB_META_SCHEMA_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            tree: Vec::new(),
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
    }
}
