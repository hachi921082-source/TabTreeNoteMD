//! In-memory active unlock-session state for locked tabs (`docs/plans/tab-lock/PLAN.md` §3
//! `tab_unlock` step 4: "hold the decrypted lock payload ... in `store/lock_session_store.rs` as
//! this tab's active session"). Individual memo bodies are never pre-decrypted here — only the
//! tree/fileMap/realTabName/key, decrypted on demand by `commands/tab_lock.rs`'s session-path
//! helpers when a specific memo is opened.
//!
//! Lifetime: `tab_unlock` starts a session, `tab_end_session` (triggered the moment the user
//! switches away from this tab, or on app exit) ends it — no idle timeout
//! (`tab-lock/PLAN.md` §3).

use std::collections::HashMap;
use std::sync::Mutex;

use crate::crypto::kdf::KdfParams;
use crate::crypto::lock_blob::FileMapEntry;
use crate::model::tree::MetaTreeNode;

/// One tab's active unlock session.
pub struct LockSession {
    pub real_tab_name: String,
    pub tree: Vec<MetaTreeNode>,
    pub file_map: Vec<FileMapEntry>,
    /// Same salt/params as the persisted `lock.kdf` field — reused verbatim for every re-save
    /// this session performs (`tab-lock/PLAN.md` §2: "Salt: stable for the life of a password,
    /// regenerated only on password change").
    pub kdf: KdfParams,
    pub hint: Option<String>,
    /// Derived once at `tab_unlock`, reused for every encrypt/decrypt this session performs
    /// (`tab-lock/PLAN.md` §2: "key never re-derived per save").
    pub key: [u8; 32],
}

pub struct LockSessionStore {
    sessions: Mutex<HashMap<String, LockSession>>,
}

impl LockSessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn start(&self, tab_id: String, session: LockSession) {
        self.sessions.lock().expect("lock session store poisoned").insert(tab_id, session);
    }

    /// `tab_end_session` (`tab-lock/PLAN.md` §3) — discards the in-memory plaintext session
    /// state. A no-op if no session was active for `tab_id`.
    pub fn end(&self, tab_id: &str) {
        self.sessions.lock().expect("lock session store poisoned").remove(tab_id);
    }

    pub fn has_session(&self, tab_id: &str) -> bool {
        self.sessions.lock().expect("lock session store poisoned").contains_key(tab_id)
    }

    /// Read-only access to `tab_id`'s active session, if any.
    pub fn with_session<T>(&self, tab_id: &str, f: impl FnOnce(&LockSession) -> T) -> Option<T> {
        let sessions = self.sessions.lock().expect("lock session store poisoned");
        sessions.get(tab_id).map(f)
    }

    /// Mutable access to `tab_id`'s active session, if any — used by every structure-changing
    /// tree command (create/rename/delete/move/set-field) while a session is active
    /// (`tab-lock/PLAN.md` §3 "While a Session Is Active").
    pub fn with_session_mut<T>(&self, tab_id: &str, f: impl FnOnce(&mut LockSession) -> T) -> Option<T> {
        let mut sessions = self.sessions.lock().expect("lock session store poisoned");
        sessions.get_mut(tab_id).map(f)
    }
}

impl Default for LockSessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_session() -> LockSession {
        LockSession {
            real_tab_name: "Private".to_string(),
            tree: vec![],
            file_map: vec![],
            kdf: KdfParams::generate_new(),
            hint: None,
            key: [0u8; 32],
        }
    }

    #[test]
    fn start_has_end_round_trip() {
        let store = LockSessionStore::new();
        assert!(!store.has_session("t1"));

        store.start("t1".to_string(), sample_session());
        assert!(store.has_session("t1"));
        assert!(!store.has_session("t2"));

        store.end("t1");
        assert!(!store.has_session("t1"));
    }

    #[test]
    fn with_session_mut_persists_changes() {
        let store = LockSessionStore::new();
        store.start("t1".to_string(), sample_session());

        store.with_session_mut("t1", |session| {
            session.real_tab_name = "Renamed".to_string();
        });

        let name = store.with_session("t1", |session| session.real_tab_name.clone());
        assert_eq!(name, Some("Renamed".to_string()));
    }

    #[test]
    fn missing_session_access_returns_none() {
        let store = LockSessionStore::new();
        assert_eq!(store.with_session("missing", |s| s.real_tab_name.clone()), None);
        assert_eq!(store.with_session_mut("missing", |s| s.real_tab_name.clone()), None);
    }
}
