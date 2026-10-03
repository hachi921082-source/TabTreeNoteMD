//! `AppResult<T>` / `ErrorCode` — the Rust side of the `Result<T>` wire contract
//! mirrored verbatim to the frontend in `src/shared/result.ts`
//! (`docs/spec.md` §3.4, §5.1; `docs/plans/project-architecture/PLAN.md` §3).
//!
//! Every Tauri `#[command]` function returns `AppResult<T>` directly (never a
//! bare `Result<T, String>`, and never `panic!`/`.unwrap()` on a recoverable
//! error) so the frontend never sees a rejected IPC promise for an expected
//! failure path.

use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    NameDuplicate,
    NameInvalid,
    NotFound,
    IoLocked,
    TrashFailed,
    DecryptFailed,
    Conflict,
    MetaCorrupt,
    LongPath,
    TabLocked,
    /// Per-memo Edit Lock (`TreeNode.locked`, distinct from `TabLocked`'s whole-tab password
    /// lock) blocks `memo_rename`/`memo_delete` on the locked memo, or on any memo inside a
    /// requested delete's subtree (GitHub issue #156).
    MemoEditLocked,
    /// An entry was found in the "both or neither exist" anomalous state while permanently
    /// unlocking a tab (`docs/plans/tab-lock-permanent-unlock/PLAN.md` §3) — surfaced during the
    /// *ordinary* (non-recovery) permanent-unlock loop, a case the lock direction's own ordinary
    /// `tab_lock` loop cannot reach (its tokens are always freshly generated and collision-checked).
    AnomalousFileState,
    /// GitHub issue #272: a memo's `<memo>/` image folder could not be renamed to (or back from)
    /// its random name while locking a tab / recovering an interrupted lock. On `tab_lock` the
    /// lock is aborted and every already-renamed folder is renamed back.
    ImageFolderRenameFailed,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppErrorPayload {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl AppErrorPayload {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            path: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

/// Wire shape: `{ ok: true, value: T }` | `{ ok: false, error: AppErrorPayload }`.
/// Serde's default enum tagging cannot produce this exact shape, so `Serialize`
/// is implemented by hand below (`project-architecture/PLAN.md` §3 note).
pub enum AppResult<T> {
    Ok(T),
    Err(AppErrorPayload),
}

impl<T> AppResult<T> {
    pub fn ok(value: T) -> Self {
        AppResult::Ok(value)
    }

    pub fn err(error: AppErrorPayload) -> Self {
        AppResult::Err(error)
    }

    pub fn err_code(code: ErrorCode, message: impl Into<String>) -> Self {
        AppResult::Err(AppErrorPayload::new(code, message))
    }
}

impl<T: Serialize> Serialize for AppResult<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        match self {
            AppResult::Ok(value) => {
                let mut s = serializer.serialize_struct("AppResult", 2)?;
                s.serialize_field("ok", &true)?;
                s.serialize_field("value", value)?;
                s.end()
            }
            AppResult::Err(error) => {
                let mut s = serializer.serialize_struct("AppResult", 2)?;
                s.serialize_field("ok", &false)?;
                s.serialize_field("error", error)?;
                s.end()
            }
        }
    }
}
