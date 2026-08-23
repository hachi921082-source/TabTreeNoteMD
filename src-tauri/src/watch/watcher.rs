//! `notify`-based folder watcher, one per currently-registered,
//! currently-unlocked tab folder (`docs/spec.md` §2.2, §3.3;
//! `docs/plans/persistence-fs/PLAN.md` §5).
//!
//! Non-recursive (the tree scanner only looks at direct children, so
//! watching does not need to be recursive either). Locked tabs (no session)
//! are not watched — see the rationale in `persistence-fs/PLAN.md` §5.
//!
//! Uses `notify-debouncer-full` rather than talking to `notify` directly:
//! observed in practice on Windows/NTFS, a single logical write (even one
//! `fs::write` call) already produces more than one raw ReadDirectoryChangesW
//! notification for the same path. The self-write registry's "clear on
//! match" rule (`docs/spec.md` §5.4) would otherwise treat the second raw
//! notification for our own write as if it were a genuine external change,
//! since the registry entry was already consumed by the first one. The
//! debouncer coalesces same-path bursts within `DEBOUNCE_TIMEOUT` into a
//! single logical event, matching the debounced behavior chokidar (the
//! legacy app's file watcher) provided out of the box.
//!
//! Each coalesced event first passes through
//! `SelfWriteRegistry::consume_if_present` (a match means "ignore, this was
//! us") and temp-file names are filtered out before even that lookup. What
//! remains is handed to the caller as an `ExternalChangeEvent`; turning that
//! into the composite conflict check, `evt:fs:external-change`, and
//! `evt:editor:conflict` is `watch/watcher_registry.rs`'s job (GitHub issue
//! #211) — this module only owns the raw, self-write-filtered event stream.
//!
//! A self-write match is not simply dropped, though: it is reported to a
//! separate `on_self_write` callback (GitHub issue #211 follow-up) so the
//! caller can still refresh whatever conflict-check baseline it keeps for
//! that path, without emitting any event or touching the search index for
//! it — see `watch/watcher_registry.rs`'s module doc comment for why that
//! baseline refresh matters.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use serde::Serialize;

use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::fs::temp_naming::is_temp_file_name;

/// Quiet period before a coalesced event is emitted. Comfortably shorter
/// than the self-write registry's 2.5s TTL fallback (`docs/detailed-spec.md`
/// §3.7), so a registered self-write's coalesced event still arrives well
/// within the entry's lifetime. Exact value is implementation-discretion,
/// same as the other constants in `docs/detailed-spec.md` §3.7.
const DEBOUNCE_TIMEOUT: Duration = Duration::from_millis(300);

/// `Serialize`s lowercase (`"created"`/`"modified"`/`"deleted"`) for `evt:fs:external-change`'s
/// payload (`watch/watcher_registry.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExternalChangeKind {
    Created,
    Modified,
    Deleted,
}

#[derive(Debug, Clone)]
pub struct ExternalChangeEvent {
    pub path: PathBuf,
    pub kind: ExternalChangeKind,
}

/// Owns a single debounced `notify` watch on one tab folder. Dropping this
/// value stops the watch.
pub struct FolderWatcher {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl FolderWatcher {
    /// `on_external_change` fires for a genuine (non-self, non-temp-file) `Created`/`Modified`/
    /// `Deleted` event. `on_self_write` fires instead — never both — when `registry.consume_if_present`
    /// matches, i.e. this coalesced event is our own write; callers use it only to refresh their own
    /// bookkeeping (e.g. a conflict-check baseline) for that path, never to emit an event or patch the
    /// search index, since a self-write already gets that handling at its own call site.
    pub fn watch(
        folder: &Path,
        registry: Arc<SelfWriteRegistry>,
        on_external_change: impl Fn(ExternalChangeEvent) + Send + Sync + 'static,
        on_self_write: impl Fn(&Path) + Send + Sync + 'static,
    ) -> notify::Result<Self> {
        let mut debouncer = new_debouncer(DEBOUNCE_TIMEOUT, None, move |result: DebounceEventResult| match result {
            Ok(events) => {
                for debounced in events {
                    handle_coalesced_event(debounced.event, &registry, &on_external_change, &on_self_write);
                }
            }
            Err(errors) => {
                for err in errors {
                    tracing::error!(error = %err, "folder watcher error");
                }
            }
        })?;
        debouncer.watch(folder, RecursiveMode::NonRecursive)?;
        Ok(Self { _debouncer: debouncer })
    }
}

fn handle_coalesced_event(
    event: Event,
    registry: &SelfWriteRegistry,
    on_external_change: &(impl Fn(ExternalChangeEvent) + Sync),
    on_self_write: &(impl Fn(&Path) + Sync),
) {
    let kind = match classify_event_kind(&event.kind) {
        Some(kind) => kind,
        None => return, // access/metadata-only events etc. are not content changes
    };

    for path in event.paths {
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Temp-file churn is never even worth registering/looking up.
        if is_temp_file_name(file_name) {
            continue;
        }
        if registry.consume_if_present(&path) {
            on_self_write(&path);
            continue;
        }
        on_external_change(ExternalChangeEvent { path: path.clone(), kind });
    }
}

fn classify_event_kind(kind: &EventKind) -> Option<ExternalChangeKind> {
    match kind {
        EventKind::Create(_) => Some(ExternalChangeKind::Created),
        EventKind::Modify(_) => Some(ExternalChangeKind::Modified),
        EventKind::Remove(_) => Some(ExternalChangeKind::Deleted),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn external_create_is_reported_and_self_write_invokes_on_self_write_instead() {
        let dir = std::env::temp_dir().join(format!("ttn-watcher-test-{}", rand::random::<u64>()));
        fs::create_dir_all(&dir).unwrap();

        let registry = Arc::new(SelfWriteRegistry::new());
        let (tx, rx) = channel::<ExternalChangeEvent>();
        let (self_write_tx, self_write_rx) = channel::<PathBuf>();

        let _watcher = FolderWatcher::watch(
            &dir,
            registry.clone(),
            move |event| {
                let _ = tx.send(event);
            },
            move |path| {
                let _ = self_write_tx.send(path.to_path_buf());
            },
        )
        .unwrap();

        // Give the watcher a moment to start (notify sets up an OS handle
        // asynchronously on some backends).
        std::thread::sleep(Duration::from_millis(100));

        // Self-write: registered first, so it invokes on_self_write instead of on_external_change.
        let self_path = dir.join("self-written.md");
        registry.register(&self_path);
        fs::write(&self_path, b"from the app").unwrap();

        // External write: not registered, so the resulting event is reported via on_external_change.
        let external_path = dir.join("external.md");
        fs::write(&external_path, b"from outside").unwrap();

        let received = rx.recv_timeout(Duration::from_secs(5)).expect("expected an external-change event");
        assert_eq!(received.path.file_name().unwrap(), "external.md");

        let self_write_received = self_write_rx.recv_timeout(Duration::from_secs(5)).expect("expected an on_self_write call");
        assert_eq!(self_write_received.file_name().unwrap(), "self-written.md");

        // No further on_external_change event should arrive for the self-write within a short window.
        let extra = rx.recv_timeout(Duration::from_millis(500));
        assert!(
            extra.is_err() || extra.as_ref().unwrap().path.file_name().unwrap() != "self-written.md",
            "self-write should not have been reported via on_external_change"
        );

        fs::remove_dir_all(&dir).ok();
    }
}
