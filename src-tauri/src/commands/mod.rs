//! One module per domain, Tauri `#[command]` fns
//! (`docs/plans/project-architecture/PLAN.md` §2). Domains other than
//! `settings`/`tab`/`window`/`tree`/`editor`/`autostart`/`link`/`image`
//! (search, lock, import — `docs/plans/v1.1.0/folder-import/PLAN.md`) are added by their own
//! feature `PLAN.md` when implemented.

pub mod about;
pub mod autostart;
pub mod editor;
pub mod image;
pub mod import;
pub mod link;
pub mod pdf_export;
pub mod search;
pub mod settings;
pub mod tab;
pub mod tab_lock;
pub mod tree;
pub mod window;

use std::path::Path;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::fs::path_safety::is_long_path;

use crate::settings::app_settings_store::RecoveryOutcome;

#[derive(Serialize, Clone)]
struct MetaRecoveredPayload {
    scope: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tab_id: Option<String>,
}

/// GitHub issue #318: 管理用ファイルの復旧が起きたことを「保留中」として保持する。起動時の復旧は
/// フロントの listener 登録前に起きてイベントが失われるため、フロントが準備後に
/// `meta_take_pending_recovery` で取りに行く。同一起動内の複数回の復旧は 1 つにまとまる。
#[derive(Default)]
pub struct PendingMetaRecovery {
    pending: AtomicBool,
    /// 同一起動内で通知済みの (scope, tab_id)。`.bak` 復元はプライマリを書き直さないので、以降の
    /// 読み込みでも毎回復旧扱いになる。同じ復旧を何度も通知しないために保持する。
    notified: Mutex<HashSet<(String, Option<String>)>>,
}

impl PendingMetaRecovery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark(&self) {
        self.pending.store(true, Ordering::SeqCst);
    }

    /// 未通知の (scope, tab_id) なら記録して `true`(通知する)。通知済みなら `false`。
    pub fn should_notify(&self, scope: &str, tab_id: Option<&str>) -> bool {
        self.notified
            .lock()
            .expect("pending meta recovery lock poisoned")
            .insert((scope.to_string(), tab_id.map(str::to_string)))
    }

    /// 保留中なら `true` を返して解除する。
    pub fn take(&self) -> bool {
        self.pending.swap(false, Ordering::SeqCst)
    }
}

/// フロントが listener 登録後に呼ぶ。保留中の復旧通知があれば `true`(1 回で解除)。
#[tauri::command]
pub fn meta_take_pending_recovery(pending: tauri::State<PendingMetaRecovery>) -> crate::model::result::AppResult<bool> {
    crate::model::result::AppResult::ok(pending.take())
}

/// Emits `evt:meta:recovered` (`docs/plans/persistence-fs/PLAN.md` §6.3,
/// `docs/plans/project-architecture/PLAN.md` §5) with the given `scope`
/// (`'app-settings'` or `'tab'`) and, for the `'tab'` scope, the affected
/// `tab_id`. Shared by every command module whose corruption-recovery
/// cascade can trigger on any read/write, not just the domain's own
/// "get"/"set" command.
pub(crate) fn emit_meta_recovered(app: &AppHandle, scope: &'static str, tab_id: Option<String>) {
    // 通知の失敗は非致命。イベントが失われても保留フラグで拾える。
    if let Some(pending) = app.try_state::<PendingMetaRecovery>() {
        if !pending.should_notify(scope, tab_id.as_deref()) {
            return;
        }
        pending.mark();
    }
    if let Err(err) = app.emit("evt:meta:recovered", MetaRecoveredPayload { scope, tab_id }) {
        tracing::error!(error = %err, "failed to emit evt:meta:recovered");
    }
}

/// `emit_meta_recovered(app, "app-settings", None)`, gated on the
/// `AppSettingsStore` recovery outcome — kept as its own function since this
/// is by far the most common call site (every command that touches the
/// settings store).
pub(crate) fn emit_settings_recovery_event(app: &AppHandle, outcome: RecoveryOutcome) {
    if outcome == RecoveryOutcome::Clean {
        return;
    }
    emit_meta_recovered(app, "app-settings", None);
}

/// GitHub issue #319: 渡したパスのうち 1 つでも長いパス(`is_long_path`)なら `true`。
pub(crate) fn any_long_path<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>) -> bool {
    paths.into_iter().any(|p| is_long_path(&p.as_ref().to_string_lossy()))
}

/// GitHub issue #319: 操作の結果できたパスのいずれかが長ければ、非致命の警告イベント
/// `evt:path:long-warning` を 1 回だけ送る(仕様 §5.11: 警告のみ、操作は止めない)。
/// 呼び出しは操作の成功後に 1 操作につき 1 回。フロントがトーストにする。
pub(crate) fn warn_if_long_path<P: AsRef<Path>>(app: &AppHandle, paths: impl IntoIterator<Item = P>) {
    if !any_long_path(paths) {
        return;
    }
    if let Err(err) = app.emit("evt:path:long-warning", ()) {
        tracing::error!(error = %err, "failed to emit evt:path:long-warning");
    }
}

#[cfg(test)]
mod pending_recovery_tests {
    use super::*;

    #[test]
    fn nothing_pending_on_clean_start() {
        assert!(!PendingMetaRecovery::new().take());
    }

    #[test]
    fn same_scope_and_tab_notifies_only_once() {
        let p = PendingMetaRecovery::new();
        assert!(p.should_notify("app-settings", None));
        assert!(!p.should_notify("app-settings", None));
        assert!(p.should_notify("tab", Some("a")));
        assert!(!p.should_notify("tab", Some("a")));
    }

    #[test]
    fn different_tab_ids_are_separate() {
        let p = PendingMetaRecovery::new();
        assert!(p.should_notify("tab", Some("a")));
        assert!(p.should_notify("tab", Some("b")));
        assert!(p.should_notify("app-settings", None));
    }

    #[test]
    fn pending_after_mark_and_cleared_by_take() {
        let p = PendingMetaRecovery::new();
        p.mark();
        p.mark();
        assert!(p.take());
        assert!(!p.take());
    }
}

#[cfg(test)]
mod long_path_tests {
    use super::*;

    fn path_of_len(n: usize, ch: char) -> String {
        ch.to_string().repeat(n)
    }

    #[test]
    fn exactly_260_chars_is_not_long() {
        assert!(!any_long_path([path_of_len(260, 'a')]));
    }

    #[test]
    fn over_260_chars_is_long() {
        assert!(any_long_path([path_of_len(261, 'a')]));
    }

    #[test]
    fn japanese_names_are_counted_by_chars_not_bytes() {
        // 260 文字(バイト数では 780)は長くない、261 文字は長い
        assert!(!any_long_path([path_of_len(260, 'あ')]));
        assert!(any_long_path([path_of_len(261, 'あ')]));
    }

    #[test]
    fn any_one_long_path_is_enough() {
        assert!(any_long_path([path_of_len(10, 'a'), path_of_len(261, 'a')]));
        assert!(!any_long_path(Vec::<String>::new()));
    }
}
