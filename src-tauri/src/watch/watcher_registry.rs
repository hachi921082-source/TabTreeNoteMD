//! Maps tab id -> currently-active `FolderWatcher`, owning the start/stop lifecycle for
//! ordinary (non-locked) tabs (`docs/plans/persistence-fs/PLAN.md` §5; GitHub issue #209).
//!
//! Wired at three points: app startup (one watcher per registered, non-locked tab), new-tab
//! registration, and tab removal. Locked-tab session-scoped start/stop is a separate concern
//! (issue #210), not handled here — locked tabs simply never get an entry in this map.
//!
//! `route_change` below is the composite conflict-check + emission pipeline the
//! `on_external_change` callback passed to `FolderWatcher::watch` drives (issue #211):
//! `Created`/`Deleted` bypass `watch/conflict.rs`'s classification entirely — there is no
//! "previous content" to hash-compare for a file that just appeared or vanished, the event itself
//! already unambiguously means "genuine change." Only `Modified` needs the composite mtime + size
//! + hash check, against a per-memo `SnapshotTracker` baseline seeded once when the watcher starts
//! and refreshed after every `Created`/`Modified` event.
//!
//! A self-write is filtered out of `on_external_change` entirely (`watcher.rs`'s
//! `SelfWriteRegistry::consume_if_present` match), so it never goes through `route_change` and never
//! triggers an event or an index patch — that part is intentional, the write already got its own
//! index-patch/UI handling at its own call site (e.g. `editor_save`). But the `SnapshotTracker`
//! baseline still needs to reflect that write, or a later genuine external touch (mtime changes,
//! content identical) gets hash-compared against a stale pre-write baseline and is wrongly classified
//! as a `GenuineChange` — spuriously firing `evt:fs:external-change` and `evt:editor:conflict` for
//! content the user just saved themselves (this was a real bug, GitHub issue #211 follow-up: the
//! module doc comment used to claim this was harmless, which was wrong). So `start()` below passes
//! `FolderWatcher::watch` a second `on_self_write` callback that refreshes the baseline for that path
//! — nothing else — keeping the conflict-check baseline in sync without duplicating any
//! event/index-patch logic.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::search::index::SearchIndex;
use crate::watch::active_memo::ActiveMemoTracker;
use crate::watch::conflict::{classify_change, compute_snapshot, ChangeClassification};
use crate::watch::snapshot_tracker::{is_md_file, memo_name_of, SnapshotTracker};
use crate::watch::watcher::{ExternalChangeEvent, ExternalChangeKind, FolderWatcher};

pub struct WatcherRegistry {
    active: Mutex<HashMap<String, FolderWatcher>>,
}

impl WatcherRegistry {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(HashMap::new()),
        }
    }

    /// Starts watching `folder` for `tab_id`, replacing any watcher already tracked for it
    /// (best-effort: a start failure is logged and otherwise ignored). Generic over `R:
    /// tauri::Runtime` only so this module's own tests can pass a `tauri::test::mock_app()`
    /// handle — every real call site just passes its ordinary `AppHandle`, `R` inferred as `Wry`.
    pub fn start<R: tauri::Runtime>(&self, app: &AppHandle<R>, tab_id: &str, folder: &Path, self_write_registry: Arc<SelfWriteRegistry>) {
        let owned_tab_id = tab_id.to_string();
        let owned_app = app.clone();

        let snapshots = Arc::new(SnapshotTracker::new());
        snapshots.seed(folder);
        let self_write_snapshots = snapshots.clone();

        let watcher = match FolderWatcher::watch(
            folder,
            self_write_registry,
            move |event| {
                let Some(routed) = route_change(&event, &snapshots) else { return };

                let search_index = owned_app.state::<SearchIndex>();
                match &routed.index_patch {
                    IndexPatch::Upsert(body) => search_index.upsert(&owned_tab_id, &routed.memo_name, body),
                    IndexPatch::Remove => search_index.remove(&owned_tab_id, &routed.memo_name),
                }

                emit_external_change(&owned_app, &owned_tab_id, &routed.memo_name, routed.kind);

                let active_memo = owned_app.state::<ActiveMemoTracker>();
                if active_memo.is_active(&owned_tab_id, &routed.memo_name) {
                    emit_editor_conflict(&owned_app, &owned_tab_id, &routed.memo_name);
                }
            },
            move |path| refresh_baseline_for_self_write(path, &self_write_snapshots),
        ) {
            Ok(watcher) => watcher,
            Err(err) => {
                tracing::error!(error = %err, path = %folder.display(), tab_id, "failed to start folder watcher for tab");
                return;
            }
        };
        self.active.lock().expect("watcher registry poisoned").insert(tab_id.to_string(), watcher);
    }

    /// Stops (drops) the watcher tracked for `tab_id`, if any. A no-op if none is tracked.
    pub fn stop(&self, tab_id: &str) {
        self.active.lock().expect("watcher registry poisoned").remove(tab_id);
    }

    /// `true` if a watcher is currently tracked for `tab_id`.
    pub fn is_watching(&self, tab_id: &str) -> bool {
        self.active.lock().expect("watcher registry poisoned").contains_key(tab_id)
    }
}

impl Default for WatcherRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Conflict-check + routing (pure, unit-testable without a real AppHandle)
// ---------------------------------------------------------------------------

/// What a genuine external change should drive: an index patch plus (via the caller) the two
/// event emissions. Kept as data rather than performing the `app.emit`/`SearchIndex` calls inline
/// so `route_change` stays a plain, easily unit-testable function.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RoutedChange {
    memo_name: String,
    kind: ExternalChangeKind,
    index_patch: IndexPatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IndexPatch {
    Upsert(String),
    Remove,
}

/// Runs one raw `ExternalChangeEvent` through the composite conflict check (bypassed for
/// `Created`/`Deleted`, run for `Modified` against `snapshots`' baseline) and updates that
/// baseline. Returns `None` for a non-`.md` path, a `TouchedOnly`/`Unchanged` classification (not
/// a genuine change), or an I/O error reading the file (logged and dropped — matches the rest of
/// this codebase's best-effort watcher error handling).
fn route_change(event: &ExternalChangeEvent, snapshots: &SnapshotTracker) -> Option<RoutedChange> {
    if !is_md_file(&event.path) {
        return None;
    }
    let memo_name = memo_name_of(&event.path)?;

    match event.kind {
        ExternalChangeKind::Deleted => {
            snapshots.remove(&memo_name);
            Some(RoutedChange { memo_name, kind: ExternalChangeKind::Deleted, index_patch: IndexPatch::Remove })
        }
        ExternalChangeKind::Created => {
            let content = read_body(&event.path)?;
            if let Ok(snapshot) = compute_snapshot(&event.path) {
                snapshots.set(&memo_name, snapshot);
            }
            Some(RoutedChange { memo_name, kind: ExternalChangeKind::Created, index_patch: IndexPatch::Upsert(content) })
        }
        ExternalChangeKind::Modified => {
            let previous = snapshots.get(&memo_name);
            let classification = match &previous {
                Some(prev) => match classify_change(prev, &event.path) {
                    Ok(classification) => classification,
                    Err(err) => {
                        tracing::error!(error = %err, path = %event.path.display(), "watch: failed to classify a Modified event");
                        return None;
                    }
                },
                // No baseline yet (e.g. the watcher started after the file already existed and
                // the seed scan missed it) — nothing to compare against, so bypass the check
                // entirely, same as `Created`.
                None => match compute_snapshot(&event.path) {
                    Ok(snapshot) => ChangeClassification::GenuineChange { snapshot },
                    Err(err) => {
                        tracing::error!(error = %err, path = %event.path.display(), "watch: failed to snapshot a Modified event with no prior baseline");
                        return None;
                    }
                },
            };

            let snapshot = match classification {
                ChangeClassification::Unchanged => return None,
                ChangeClassification::TouchedOnly { .. } => {
                    // mtime-only touch: refresh the baseline so a later genuine edit still
                    // compares against current on-disk state, but this event itself isn't
                    // reported (not a content change).
                    if let Ok(fresh) = compute_snapshot(&event.path) {
                        snapshots.set(&memo_name, fresh);
                    }
                    return None;
                }
                ChangeClassification::GenuineChange { snapshot } => snapshot,
            };
            snapshots.set(&memo_name, snapshot);

            let content = read_body(&event.path)?;
            Some(RoutedChange { memo_name, kind: ExternalChangeKind::Modified, index_patch: IndexPatch::Upsert(content) })
        }
    }
}

/// `on_self_write` callback passed to `FolderWatcher::watch` (GitHub issue #211 follow-up): a
/// self-write never goes through `route_change` (it's filtered upstream in `watcher.rs`, never even
/// reaching `on_external_change`), but the `SnapshotTracker` baseline still needs to reflect it, or a
/// later genuine external touch gets hash-compared against stale pre-write content and is wrongly
/// classified as a `GenuineChange`. Best-effort: a non-`.md` path or a snapshot failure (e.g. the
/// file was already replaced again in the moment between the write and this callback running) is
/// logged and otherwise ignored — deliberately does not emit any event or touch the search index,
/// both already handled at the self-write's own call site (e.g. `editor_save`).
fn refresh_baseline_for_self_write(path: &Path, snapshots: &SnapshotTracker) {
    if !is_md_file(path) {
        return;
    }
    let Some(memo_name) = memo_name_of(path) else { return };
    match compute_snapshot(path) {
        Ok(snapshot) => snapshots.set(&memo_name, snapshot),
        Err(err) => {
            tracing::error!(error = %err, path = %path.display(), "watch: failed to refresh conflict-check baseline after a self-write");
        }
    }
}

/// Reads and decodes `path`'s content for a `SearchIndex::upsert` call. `None` (logged) on a read
/// failure — e.g. the file was deleted again in the moment between the `Created`/`Modified` event
/// firing and this callback running.
fn read_body(path: &Path) -> Option<String> {
    match std::fs::read(path) {
        Ok(bytes) => Some(crate::store::memo_file_service::decode_memo_body(&bytes).0),
        Err(err) => {
            tracing::error!(error = %err, path = %path.display(), "watch: failed to read file to patch search index");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Event emission
// ---------------------------------------------------------------------------

/// `evt:fs:external-change` payload — no established frontend contract yet (issue #212 builds the
/// listener against this shape). `kind` serializes lowercase via `ExternalChangeKind`'s own derive.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExternalChangePayload<'a> {
    tab_id: &'a str,
    memo_name: &'a str,
    kind: ExternalChangeKind,
}

/// `evt:editor:conflict` payload — established frontend contract, do not change:
/// `src/state/useEditor.ts`'s `listen<{ tabId: string; memoName: string }>("evt:editor:conflict", ...)`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EditorConflictPayload<'a> {
    tab_id: &'a str,
    memo_name: &'a str,
}

fn emit_external_change<R: tauri::Runtime>(app: &AppHandle<R>, tab_id: &str, memo_name: &str, kind: ExternalChangeKind) {
    if let Err(err) = app.emit("evt:fs:external-change", ExternalChangePayload { tab_id, memo_name, kind }) {
        tracing::error!(error = %err, "failed to emit evt:fs:external-change");
    }
}

fn emit_editor_conflict<R: tauri::Runtime>(app: &AppHandle<R>, tab_id: &str, memo_name: &str) {
    if let Err(err) = app.emit("evt:editor:conflict", EditorConflictPayload { tab_id, memo_name }) {
        tracing::error!(error = %err, "failed to emit evt:editor:conflict");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-watcher-registry-test-{}", rand::random::<u64>()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A real (but headless/test-only) `AppHandle`, needed now that `start()` wires the
    /// callback's `app.emit`/`app.state` calls — none of these lifecycle tests trigger an actual
    /// filesystem event, so the callback never runs and no state needs to be `.manage()`d here.
    fn test_app_handle() -> tauri::AppHandle<tauri::test::MockRuntime> {
        tauri::test::mock_app().handle().clone()
    }

    #[test]
    fn start_then_stop_tracks_membership() {
        let dir = tempdir();
        let app = test_app_handle();
        let registry = WatcherRegistry::new();
        assert!(!registry.is_watching("tab-1"));

        registry.start(&app, "tab-1", &dir, Arc::new(SelfWriteRegistry::new()));
        assert!(registry.is_watching("tab-1"));

        registry.stop("tab-1");
        assert!(!registry.is_watching("tab-1"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn starting_again_for_the_same_tab_replaces_the_previous_watcher() {
        let dir = tempdir();
        let app = test_app_handle();
        let registry = WatcherRegistry::new();
        registry.start(&app, "tab-1", &dir, Arc::new(SelfWriteRegistry::new()));
        assert!(registry.is_watching("tab-1"));

        // Re-registration (e.g. re-running the startup loop) must not leave two watchers
        // stacked or panic — the previous entry is simply replaced.
        registry.start(&app, "tab-1", &dir, Arc::new(SelfWriteRegistry::new()));
        assert!(registry.is_watching("tab-1"));

        registry.stop("tab-1");
        assert!(!registry.is_watching("tab-1"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stop_on_an_untracked_tab_is_a_no_op() {
        let registry = WatcherRegistry::new();
        registry.stop("missing");
        assert!(!registry.is_watching("missing"));
    }

    #[test]
    fn lookup_is_independent_per_tab_id() {
        let dir_a = tempdir();
        let dir_b = tempdir();
        let app = test_app_handle();
        let registry = WatcherRegistry::new();

        registry.start(&app, "tab-a", &dir_a, Arc::new(SelfWriteRegistry::new()));
        assert!(registry.is_watching("tab-a"));
        assert!(!registry.is_watching("tab-b"));

        registry.start(&app, "tab-b", &dir_b, Arc::new(SelfWriteRegistry::new()));
        assert!(registry.is_watching("tab-a"));
        assert!(registry.is_watching("tab-b"));

        registry.stop("tab-a");
        assert!(!registry.is_watching("tab-a"));
        assert!(registry.is_watching("tab-b"));

        fs::remove_dir_all(&dir_a).ok();
        fs::remove_dir_all(&dir_b).ok();
    }

    // -----------------------------------------------------------------
    // route_change: pure conflict-check/routing tests (no AppHandle needed)
    // -----------------------------------------------------------------

    fn event(path: std::path::PathBuf, kind: ExternalChangeKind) -> ExternalChangeEvent {
        ExternalChangeEvent { path, kind }
    }

    #[test]
    fn non_md_paths_are_never_routed() {
        let dir = tempdir();
        let path = dir.join(".tabtreenote.json");
        fs::write(&path, b"{}").unwrap();
        let snapshots = SnapshotTracker::new();

        assert!(route_change(&event(path, ExternalChangeKind::Modified), &snapshots).is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn created_bypasses_conflict_check_and_upserts() {
        let dir = tempdir();
        let path = dir.join("a.md");
        fs::write(&path, b"hello").unwrap();
        let snapshots = SnapshotTracker::new();

        let routed = route_change(&event(path.clone(), ExternalChangeKind::Created), &snapshots).expect("expected a routed change");
        assert_eq!(routed.memo_name, "a");
        assert_eq!(routed.kind, ExternalChangeKind::Created);
        assert_eq!(routed.index_patch, IndexPatch::Upsert("hello".to_string()));
        // baseline was seeded by processing the Created event.
        assert!(snapshots.get("a").is_some());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleted_bypasses_conflict_check_and_removes_baseline() {
        let dir = tempdir();
        let path = dir.join("a.md");
        let snapshots = SnapshotTracker::new();
        snapshots.set("a", compute_snapshot_of(b"hello"));

        let routed = route_change(&event(path, ExternalChangeKind::Deleted), &snapshots).expect("expected a routed change");
        assert_eq!(routed.memo_name, "a");
        assert_eq!(routed.kind, ExternalChangeKind::Deleted);
        assert_eq!(routed.index_patch, IndexPatch::Remove);
        assert!(snapshots.get("a").is_none());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn modified_with_no_prior_baseline_bypasses_the_check_like_created() {
        let dir = tempdir();
        let path = dir.join("a.md");
        fs::write(&path, b"hello").unwrap();
        let snapshots = SnapshotTracker::new();

        let routed = route_change(&event(path, ExternalChangeKind::Modified), &snapshots).expect("expected a routed change");
        assert_eq!(routed.index_patch, IndexPatch::Upsert("hello".to_string()));
        assert!(snapshots.get("a").is_some());
    }

    #[test]
    fn modified_with_unchanged_content_is_not_routed() {
        let dir = tempdir();
        let path = dir.join("a.md");
        fs::write(&path, b"hello").unwrap();
        let snapshots = SnapshotTracker::new();
        snapshots.seed(&dir);

        assert!(route_change(&event(path, ExternalChangeKind::Modified), &snapshots).is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn modified_with_mtime_only_touch_is_not_routed_but_baseline_refreshes() {
        let dir = tempdir();
        let path = dir.join("a.md");
        fs::write(&path, b"hello").unwrap();
        let snapshots = SnapshotTracker::new();
        snapshots.seed(&dir);
        let original = snapshots.get("a").unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&path, b"hello").unwrap(); // same content, mtime advances

        assert!(route_change(&event(path, ExternalChangeKind::Modified), &snapshots).is_none());
        let refreshed = snapshots.get("a").unwrap();
        assert_eq!(refreshed.hash, original.hash);
        assert_ne!(refreshed.mtime, original.mtime);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn modified_with_genuine_content_change_is_routed_and_baseline_updates() {
        let dir = tempdir();
        let path = dir.join("a.md");
        fs::write(&path, b"hello").unwrap();
        let snapshots = SnapshotTracker::new();
        snapshots.seed(&dir);

        fs::write(&path, b"hello world").unwrap();

        let routed = route_change(&event(path, ExternalChangeKind::Modified), &snapshots).expect("expected a routed change");
        assert_eq!(routed.kind, ExternalChangeKind::Modified);
        assert_eq!(routed.index_patch, IndexPatch::Upsert("hello world".to_string()));
        assert_eq!(snapshots.get("a").unwrap().hash, compute_snapshot_of(b"hello world").hash);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn self_write_refresh_prevents_spurious_conflict_on_subsequent_identical_touch() {
        // Reproduces reviewer-lite's live-found bug (GitHub issue #211 follow-up): seed a baseline
        // pointing at pre-save content, simulate a self-write via `refresh_baseline_for_self_write`
        // (the same helper `WatcherRegistry::start`'s `on_self_write` callback calls) that changes
        // the on-disk content, then a later mtime-only external touch with byte-identical (i.e.
        // post-save) content must classify as unchanged (not routed) — not a spurious
        // `GenuineChange`. Before this fix, nothing refreshed the baseline for a self-write (it
        // never reached `route_change` at all, filtered upstream in `watcher.rs`), so the touch's
        // current-disk hash would disagree with the still-stale pre-save baseline and this
        // `route_change` call would wrongly return `Some(..)` instead of `None`.
        let dir = tempdir();
        let path = dir.join("a.md");
        fs::write(&path, b"before save").unwrap();
        let snapshots = SnapshotTracker::new();
        snapshots.seed(&dir); // baseline: "before save"

        // Self-write: the app itself saves new content, then the watcher pipeline's
        // on_self_write callback refreshes the baseline (this is what would be skipped without
        // the fix).
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&path, b"after save").unwrap();
        refresh_baseline_for_self_write(&path, &snapshots);
        assert_eq!(snapshots.get("a").unwrap().hash, compute_snapshot_of(b"after save").hash);

        // External touch: byte-identical content, mtime-only (e.g. a sync/backup tool touching
        // the file without changing its bytes).
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&path, b"after save").unwrap();

        assert!(
            route_change(&event(path, ExternalChangeKind::Modified), &snapshots).is_none(),
            "an mtime-only touch with content identical to the self-written baseline must not be routed as a genuine change"
        );

        fs::remove_dir_all(&dir).ok();
    }

    /// Test-only helper: a `FileSnapshot` for in-memory `content`, computed via a throwaway temp
    /// file (`compute_snapshot` reads from disk, no pure-bytes variant exists or is needed
    /// elsewhere).
    fn compute_snapshot_of(content: &[u8]) -> crate::watch::conflict::FileSnapshot {
        let path = std::env::temp_dir().join(format!("ttn-watcher-registry-snapshot-of-{}.md", rand::random::<u64>()));
        fs::write(&path, content).unwrap();
        let snapshot = compute_snapshot(&path).unwrap();
        fs::remove_file(&path).ok();
        snapshot
    }
}
