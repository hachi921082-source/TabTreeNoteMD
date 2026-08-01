//! Filesystem primitives (domain-agnostic) — `docs/plans/persistence-fs/PLAN.md`.

pub mod app_data_root;
pub mod atomic_write;
pub mod path_safety;
pub mod retry;
pub mod self_write_registry;
pub mod temp_naming;
pub mod trash;
