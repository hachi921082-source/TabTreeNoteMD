//! Folder watching + external-change/conflict classification
//! (`docs/plans/persistence-fs/PLAN.md` §5).

pub mod active_memo;
pub mod conflict;
pub mod snapshot_tracker;
pub mod watcher;
pub mod watcher_registry;
