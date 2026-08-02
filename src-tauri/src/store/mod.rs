//! In-memory runtime state / shared read-write helpers for a single memo body
//! (`docs/plans/project-architecture/PLAN.md` §2, `store/`): `memo_file_service` (encoding + EOL
//! logic shared by `commands/editor.rs` and `commands/tree.rs`, `docs/plans/editor-save/PLAN.md`)
//! and `lock_session_store` (active tab-lock unlock sessions, `docs/plans/tab-lock/PLAN.md` §3).

pub mod lock_session_store;
pub mod memo_file_service;
