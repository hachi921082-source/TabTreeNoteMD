//! In-memory runtime state / shared read-write helpers for a single memo body
//! (`docs/plans/project-architecture/PLAN.md` §2, `store/`). Currently just
//! `memo_file_service` (encoding + EOL logic shared by `commands/editor.rs` and
//! `commands/tree.rs`, `docs/plans/editor-save/PLAN.md`); `tab_store.rs`/
//! `lock_session_store.rs` are added by the features that need them
//! (`tab-lock/PLAN.md`).

pub mod memo_file_service;
