//! OS trash wrapper (`docs/spec.md` §2.6, §5.8; `docs/plans/persistence-fs/PLAN.md` §9).
//!
//! On failure, returns `TrashFailed` — the caller (`memo_delete`, elaborated
//! in `memo-tree/PLAN.md`) is responsible for the confirm-dialog-then-direct-
//! delete flow; this module never prompts for consent itself (no UI access
//! from the `fs/` layer).

use std::path::Path;

use crate::model::result::ErrorCode;

use super::retry::with_retry;
use super::self_write_registry::SelfWriteRegistry;

/// Moves `path` to the OS trash/recycle bin.
pub fn trash_file(path: &Path, registry: &SelfWriteRegistry) -> Result<(), ErrorCode> {
    registry.register(path);
    trash::delete(path).map_err(|_| ErrorCode::TrashFailed)
}

/// Direct (permanent) delete, used only after explicit user consent when
/// `trash` above fails. Goes through the same lock-retry policy as any other
/// delete (`docs/plans/persistence-fs/PLAN.md` §7).
pub fn delete_directly(path: &Path, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    registry.register(path);
    with_retry(|| std::fs::remove_file(path))
}
