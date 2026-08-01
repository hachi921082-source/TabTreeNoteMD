//! One module per domain, Tauri `#[command]` fns
//! (`docs/plans/project-architecture/PLAN.md` §2). Domains other than
//! `settings` (tab, tree, editor, search, link, image, lock) are added by
//! their own feature `PLAN.md` when implemented.

pub mod settings;
