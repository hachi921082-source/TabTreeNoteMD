//! Tab Lock cryptography (`docs/plans/tab-lock/PLAN.md` §2; `docs/plans/tab-lock/CRASH-RECOVERY.md`).
//!
//! Self-contained: depends only on `serde`/`serde_json` and the crypto crates, never on
//! `commands/`/`store/`, so it is independently testable and reusable from both the ordinary
//! `tab_lock`/`tab_unlock` happy path and the crash-mid-lock recovery flow (which needs to
//! re-derive the exact same key from the marker's own `kdf`/`saltB64`).

pub mod kdf;
pub mod lock_blob;
pub mod obfuscated_naming;
