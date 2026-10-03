//! Link domain commands — `docs/plans/link/PLAN.md` §2.
//!
//! `link_resolve` parses a `memo:` link target (the text after the `memo:` scheme) into a
//! `Memo { tabId, memoName }` / `Unresolved` result. It is pure resolution — it never opens or
//! navigates anything itself; on `Memo`, the frontend performs the actual tab-switch + memo-open,
//! reusing `memo-tree/PLAN.md`'s "open memo" path (`links.ts`'s `resolveAndOpenMemoLink`).
//!
//! `link_open_external` opens a URL, email address, or file path via the OS-standard "open with
//! default handler" API (`tauri_plugin_opener`), never shell string concatenation
//! (`[spec §2.8]`). A relative `filePath` value is resolved to absolute using `current_tab_id`'s
//! folder as base before opening. The `AppSettings.links.confirmBeforeOpen` gate is entirely the
//! frontend's responsibility (`link/PLAN.md` §2) — this command performs no confirmation, it is a
//! pure "open now" action.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::commands::tree::load_and_reconcile;
use crate::fs::self_write_registry::SelfWriteRegistry;
use crate::model::app_settings::AppSettings;
use crate::model::result::{AppErrorPayload, AppResult, ErrorCode};
use crate::model::tree::MetaTreeNode;
use crate::settings::app_settings_store::AppSettingsStore;
use crate::store::lock_session_store::LockSessionStore;

// ---------------------------------------------------------------------------
// link_resolve
// ---------------------------------------------------------------------------

/// `docs/plans/link/PLAN.md` §2 `link_resolve`'s return shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ResolvedLink {
    /// `memo_name: None` for the `TabName/` (tab-only, no memo selected) form.
    #[serde(rename_all = "camelCase")]
    Memo { tab_id: String, memo_name: Option<String> },
    Unresolved,
}

/// Parses `target` (the text after the `memo:` scheme — e.g. `MemoName`, `TabName/MemoName`, or
/// `TabName/`) into a `ResolvedLink` (`docs/plans/link/PLAN.md` §2, `[spec §2.8, §4.1]`):
/// - `MemoName` (no `/`) resolves within `current_tab_id`'s own tree only.
/// - `TabName/MemoName` resolves `TabName` by display name against `AppSettings.tabs` (tab names
///   are enforced unique, `[spec §2.1]`, so no ambiguity is possible).
/// - `TabName/` (trailing slash, empty memo segment) resolves to `Memo { tab_id, memo_name: None
///   }` — "open this tab, don't select a specific memo."
/// - A target tab that is locked with **no** active session never matches by name (so it can
///   never be reached by either form) and, for the bare-`MemoName` case where `current_tab_id`
///   itself is such a tab, always yields `Unresolved` — matching `[spec §2.8]`'s "links to locked
///   tabs are always unresolved without an active session."
/// - A target tab locked **with** an active session resolves normally against that session's
///   in-memory tree (both for name-matching and memo lookup).
/// - Anything else that doesn't exist (tab or memo) is `Unresolved` — no-op, no error dialog
///   (`[spec §2.8]`: "unresolved links do nothing").
#[tauri::command]
pub fn link_resolve(current_tab_id: String, target: String, store: State<AppSettingsStore>, registry: State<Arc<SelfWriteRegistry>>, sessions: State<LockSessionStore>) -> AppResult<ResolvedLink> {
    let (settings, _outcome) = store.get(&registry);

    let (tab_name, memo_name) = match target.split_once('/') {
        Some((tab, memo)) => (Some(tab), if memo.is_empty() { None } else { Some(memo) }),
        None => (None, Some(target.as_str())),
    };

    let tab_id = match tab_name {
        None => current_tab_id,
        Some(name) => match find_tab_id_by_display_name(&settings, &sessions, name) {
            Some(id) => id,
            None => return AppResult::ok(ResolvedLink::Unresolved),
        },
    };

    let Some(memo_name) = memo_name else {
        // `TabName/` form: no memo selected — the target tab was already confirmed resolvable
        // (locked-without-session tabs never match a name, see `find_tab_id_by_display_name`).
        return AppResult::ok(ResolvedLink::Memo { tab_id, memo_name: None });
    };

    match resolve_tab_tree(&settings, &sessions, &tab_id) {
        Some(tree) if tree_contains_memo(&tree, memo_name) => AppResult::ok(ResolvedLink::Memo { tab_id, memo_name: Some(memo_name.to_string()) }),
        _ => AppResult::ok(ResolvedLink::Unresolved),
    }
}

/// Finds the tab whose *display* name (real name, not the 🔒 placeholder) equals `name`
/// (`[detailed-spec §5.8]`: "resolves by tab display name, not folder path"). A locked tab with
/// no active session is skipped entirely — its real name lives only inside the encrypted lock
/// blob, unreachable without a session (`[spec §2.8, §4.1]`), so it can never be matched here. A
/// locked tab *with* an active session is matched against that session's `real_tab_name`, since
/// `AppSettings.tabs[i].name` itself still holds the placeholder while a session is active.
fn find_tab_id_by_display_name(settings: &AppSettings, sessions: &LockSessionStore, name: &str) -> Option<String> {
    settings.tabs.iter().find_map(|entry| {
        if entry.locked.unwrap_or(false) {
            let real_name = sessions.with_session(&entry.id, |s| s.real_tab_name.clone())?;
            (real_name == name).then(|| entry.id.clone())
        } else {
            (entry.name == name).then(|| entry.id.clone())
        }
    })
}

/// The tree to search for a memo-name match: the active session's in-memory tree for a locked
/// tab with a session, a fresh disk-reconciled tree for an unlocked tab, or `None` (tab not
/// found, or locked with no session — `[spec §2.8, §4.1]`).
fn resolve_tab_tree(settings: &AppSettings, sessions: &LockSessionStore, tab_id: &str) -> Option<Vec<MetaTreeNode>> {
    let entry = settings.tabs.iter().find(|t| t.id == tab_id)?;
    if entry.locked.unwrap_or(false) {
        sessions.with_session(tab_id, |s| s.tree.clone())
    } else {
        Some(load_and_reconcile(Path::new(&entry.folder_path)).tree)
    }
}

/// `memoName` alone uniquely identifies a node anywhere in a tab's tree (`model/tree.rs`'s
/// `NodePath` doc comment), so a recursive search — not just top-level — is correct here.
fn tree_contains_memo(tree: &[MetaTreeNode], name: &str) -> bool {
    tree.iter().any(|n| n.memo_name == name || tree_contains_memo(&n.children, name))
}

// ---------------------------------------------------------------------------
// link_open_external
// ---------------------------------------------------------------------------

/// `docs/plans/link/PLAN.md` §2 `link_open_external`'s `kind` discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExternalLinkKind {
    Url,
    Email,
    FilePath,
}

/// Opens `value` with the OS's default handler for its kind (`tauri_plugin_opener`, matching
/// `commands/tab.rs`'s existing `tab_open_location` use of the same plugin), never shell string
/// concatenation (`[spec §2.8]`):
/// - `Url`: a bare `http(s)://`/`ftp://`-prefixed value opens as-is; a bare `www.`-prefixed value
///   (no scheme) gets an `https://` prefix added first, since the OS opener needs a real scheme.
/// - `Email`: opened as a `mailto:` URL (prefixed if not already present).
/// - `FilePath`: an absolute path opens as-is; a relative path is resolved against
///   `current_tab_id`'s folder as base first (`[spec §2.8]`). If `current_tab_id` is absent or
///   doesn't resolve to a known tab, the relative value is passed through unresolved rather than
///   failing the whole command — the OS opener will simply fail to find it, same end result as
///   any other "file not found" case. A relative value containing a `..` path segment, or one that
///   is "rooted" without being absolute (e.g. a leading `\` or `/` with no drive prefix, such as
///   `\Windows\System32\cmd.exe`), is rejected outright (`ErrorCode::NameInvalid`) rather than
///   resolved, since joining either against the tab's folder could otherwise escape that folder —
///   the same class of path-traversal problem `markdownSanitize.ts`'s `isSafeRelativeImageSrc`
///   already guards against for `<img src>` (`docs/DECISIONS.md`: "Image `src` Allowlist: Reject
///   `..` Path-Traversal Segments").
#[tauri::command]
pub fn link_open_external(app: AppHandle, kind: ExternalLinkKind, value: String, current_tab_id: Option<String>, extended_search_text: Option<String>, store: State<AppSettingsStore>, registry: State<Arc<SelfWriteRegistry>>) -> AppResult<()> {
    let opener = app.opener();
    let result = match kind {
        ExternalLinkKind::Url => opener.open_url(normalize_url(&value), None::<String>),
        ExternalLinkKind::Email => opener.open_url(normalize_mailto(&value), None::<String>),
        ExternalLinkKind::FilePath => {
            let (settings, _outcome) = store.get(&registry);
            let resolved = match resolve_file_path_widened(&value, extended_search_text.as_deref(), current_tab_id.as_deref(), &settings) {
                Ok(resolved) => resolved,
                Err(err) => {
                    tracing::warn!(value, "link_open_external: rejected a file path as a path-traversal attempt ('..' segment or a rooted-but-not-absolute path)");
                    return AppResult::err(err);
                }
            };
            opener.open_path(resolved, None::<String>)
        }
    };
    match result {
        Ok(()) => AppResult::ok(()),
        Err(err) => {
            tracing::error!(error = %err, kind = ?kind, value, "link_open_external: failed to open");
            AppResult::err_code(ErrorCode::Unknown, "failed to open the external link")
        }
    }
}

fn normalize_url(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("ftp://") {
        value.to_string()
    } else {
        format!("https://{value}")
    }
}

fn normalize_mailto(value: &str) -> String {
    if value.to_ascii_lowercase().starts_with("mailto:") {
        value.to_string()
    } else {
        format!("mailto:{value}")
    }
}

/// Resolves a `FilePath` link target to the string handed to the OS opener. An absolute `value`
/// is returned as-is. A relative `value` is rejected (`Err`) if it contains any `..` path
/// segment — the same check as `markdownSanitize.ts`'s `isSafeRelativeImageSrc`, applied here so
/// `folder.join(path)` below can never escape `current_tab_id`'s own folder. A relative `value`
/// that is "rooted" (`Path::has_root()` true) but not `is_absolute()` — e.g. a leading `\` or `/`
/// with no drive-letter prefix, such as `\Windows\System32\cmd.exe` or `/etc/passwd` — is rejected
/// too: `has_root()` without `is_absolute()` means the value has no `..` segment yet is *not*
/// actually relative to the base folder, since `PathBuf::join`'s Windows semantics replace the
/// base's root entirely (keeping only its drive prefix) when joining a rooted-but-not-absolute
/// path, letting it escape `current_tab_id`'s folder just like a `..` segment would. Again, this
/// mirrors `isSafeRelativeImageSrc`, which rejects a value starting with `\` or `//` as
/// absolute-like for the same reason. Otherwise `value` is joined onto that tab's folder (or
/// passed through unresolved if there is no known base — see this function's caller's doc
/// comment).
///
/// Body delegates to `fs::path_safety::resolve_relative_within_base`
/// (`docs/plans/markdown/image-handling/PLAN.md` §3.1) — this function's own signature, error
/// codes, and doc comment above are unchanged; this is a behavior-preserving internal refactor
/// only, verified by the `resolve_file_path_*` tests below continuing to pass unmodified.
fn resolve_file_path(value: &str, current_tab_id: Option<&str>, settings: &AppSettings) -> Result<String, AppErrorPayload> {
    let base = current_tab_id.and_then(|id| settings.tabs.iter().find(|t| t.id == id)).map(|t| PathBuf::from(&t.folder_path));
    crate::fs::path_safety::resolve_relative_within_base(value, base.as_deref(), crate::fs::path_safety::AbsolutePathPolicy::AllowPassthrough)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|code| AppErrorPayload::new(code, "file path is invalid"))
}

/// Whitespace-delimited word-boundary end offsets of `s`, longest-prefix-first (issue #235). E.g.
/// `"C:\Program Files (x86)\Steam"` yields offsets for `"C:\Program Files (x86)\Steam"`, then
/// `"C:\Program Files (x86)"`, then `"C:\Program Files"`, then `"C:\Program"` — the widened
/// candidate trimmed one trailing word at a time, never splitting mid-word. Trailing whitespace is
/// never included in a returned prefix.
fn candidate_prefixes(s: &str) -> Vec<&str> {
    let mut word_ends = Vec::new();
    let mut in_word = false;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            if in_word {
                word_ends.push(i);
                in_word = false;
            }
        } else {
            in_word = true;
        }
    }
    if in_word {
        word_ends.push(s.len());
    }
    word_ends.iter().rev().map(|&end| &s[..end]).collect()
}

/// Issue #235: `FILE_PATH_RE` (`links.ts`) deliberately stops at the first space/bracket so its
/// decoration doesn't swallow trailing prose — but that truncates a real path that itself contains
/// a space or `(`/`)` (e.g. `C:\Program Files (x86)\Steam\userdata`), which then fails to open
/// since the truncated prefix doesn't exist on disk. This widens the search at CLICK time only
/// (never at decoration/render time — a `metadata()` filesystem probe on every keystroke would be
/// a real perf concern): given `extended_search_text` (the rest of the source line from the
/// regex match's start to end-of-line, supplied by the frontend), tries progressively shorter
/// whitespace-trimmed prefixes of it (longest first, via `candidate_prefixes`), resolving each
/// through the same `resolve_file_path` traversal-safety check a plain `value` would get, and
/// returns the resolved string of the longest one that both resolves AND exists on disk
/// (`std::fs::metadata`). Falls back to resolving `value` (the original short regex match) as-is,
/// existence unchecked, if no wider prefix panned out — preserving today's behavior (and its
/// "file not found" opener failure) for a path that plain doesn't exist, or for the "prose after a
/// short real path" case where the short match itself is what's real.
fn resolve_file_path_widened(value: &str, extended_search_text: Option<&str>, current_tab_id: Option<&str>, settings: &AppSettings) -> Result<String, AppErrorPayload> {
    if let Some(extended) = extended_search_text {
        for prefix in candidate_prefixes(extended) {
            if let Ok(resolved) = resolve_file_path(prefix, current_tab_id, settings) {
                if Path::new(&resolved).metadata().is_ok() {
                    return Ok(resolved);
                }
            }
        }
    }
    resolve_file_path(value, current_tab_id, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::kdf::KdfParams;
    use crate::model::app_settings::TabSettingsEntry;
    use crate::store::lock_session_store::LockSession;

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-link-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn tab_entry(id: &str, name: &str, folder: &Path, locked: bool) -> TabSettingsEntry {
        TabSettingsEntry { id: id.to_string(), name: name.to_string(), folder_path: folder.to_string_lossy().into_owned(), locked: Some(locked), lock_label: None }
    }

    fn settings_with(tabs: Vec<TabSettingsEntry>) -> AppSettings {
        let mut settings = AppSettings::default();
        settings.tabs = tabs;
        settings
    }

    fn empty_session(real_tab_name: &str, tree: Vec<MetaTreeNode>) -> LockSession {
        LockSession {
            real_tab_name: real_tab_name.to_string(),
            tree,
            file_map: Vec::new(),
            kdf: KdfParams::generate_new(),
            hint: None,
            key: [0u8; 32],
            last_viewed_memo: None,
            image_folders: Vec::new(),
        }
    }

    #[test]
    fn resolves_bare_memo_name_within_current_tab() {
        let dir = tempdir();
        std::fs::write(dir.join("Shopping list.md"), b"").unwrap();
        let settings = settings_with(vec![tab_entry("t1", "Personal", &dir, false)]);
        let sessions = LockSessionStore::new();

        let tab_id = "t1".to_string();
        let tree = resolve_tab_tree(&settings, &sessions, &tab_id).unwrap();
        assert!(tree_contains_memo(&tree, "Shopping list"));
        assert!(!tree_contains_memo(&tree, "Missing"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolves_tab_name_memo_name_form_by_display_name() {
        let dir_a = tempdir();
        let dir_b = tempdir();
        std::fs::write(dir_b.join("Diary.md"), b"").unwrap();
        let settings = settings_with(vec![tab_entry("a", "Work", &dir_a, false), tab_entry("b", "Personal", &dir_b, false)]);
        let sessions = LockSessionStore::new();

        let found = find_tab_id_by_display_name(&settings, &sessions, "Personal");
        assert_eq!(found, Some("b".to_string()));
        let tree = resolve_tab_tree(&settings, &sessions, "b").unwrap();
        assert!(tree_contains_memo(&tree, "Diary"));

        std::fs::remove_dir_all(&dir_a).ok();
        std::fs::remove_dir_all(&dir_b).ok();
    }

    #[test]
    fn locked_tab_with_no_session_is_unreachable_by_name_and_by_tree() {
        let dir = tempdir();
        let settings = settings_with(vec![tab_entry("locked", "Private", &dir, true)]);
        let sessions = LockSessionStore::new();

        assert_eq!(find_tab_id_by_display_name(&settings, &sessions, "Private"), None);
        assert_eq!(resolve_tab_tree(&settings, &sessions, "locked"), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn locked_tab_with_active_session_resolves_by_real_name_and_session_tree() {
        let dir = tempdir();
        let settings = settings_with(vec![tab_entry("locked", "🔒", &dir, true)]);
        let sessions = LockSessionStore::new();
        sessions.start("locked".to_string(), empty_session("Private", vec![MetaTreeNode::default_for_new_file("Secret note".to_string())]));

        assert_eq!(find_tab_id_by_display_name(&settings, &sessions, "Private"), Some("locked".to_string()));
        let tree = resolve_tab_tree(&settings, &sessions, "locked").unwrap();
        assert!(tree_contains_memo(&tree, "Secret note"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tree_contains_memo_searches_nested_children() {
        let child = MetaTreeNode::default_for_new_file("Child".to_string());
        let mut parent = MetaTreeNode::default_for_new_file("Parent".to_string());
        parent.children = vec![child];
        assert!(tree_contains_memo(&[parent.clone()], "Child"));
        assert!(!tree_contains_memo(&[parent], "Nonexistent"));
    }

    #[test]
    fn normalize_url_adds_https_scheme_only_when_missing() {
        assert_eq!(normalize_url("https://example.com"), "https://example.com");
        assert_eq!(normalize_url("http://example.com"), "http://example.com");
        assert_eq!(normalize_url("ftp://example.com"), "ftp://example.com");
        assert_eq!(normalize_url("www.example.com"), "https://www.example.com");
    }

    #[test]
    fn normalize_mailto_adds_prefix_only_when_missing() {
        assert_eq!(normalize_mailto("someone@example.com"), "mailto:someone@example.com");
        assert_eq!(normalize_mailto("mailto:someone@example.com"), "mailto:someone@example.com");
    }

    #[test]
    fn resolve_file_path_leaves_absolute_paths_untouched() {
        let settings = settings_with(vec![tab_entry("t1", "Personal", Path::new("C:\\ignored"), false)]);
        assert_eq!(resolve_file_path("C:\\already\\absolute.txt", Some("t1"), &settings).unwrap(), "C:\\already\\absolute.txt");
    }

    #[test]
    fn resolve_file_path_joins_relative_paths_against_the_current_tab_folder() {
        let dir = tempdir();
        let settings = settings_with(vec![tab_entry("t1", "Personal", &dir, false)]);
        let resolved = resolve_file_path("notes.txt", Some("t1"), &settings).unwrap();
        assert_eq!(resolved, dir.join("notes.txt").to_string_lossy().into_owned());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_file_path_passes_through_relative_paths_with_no_known_base() {
        let settings = AppSettings::default();
        assert_eq!(resolve_file_path("notes.txt", None, &settings).unwrap(), "notes.txt");
    }

    #[test]
    fn resolve_file_path_rejects_relative_paths_containing_a_parent_dir_segment() {
        let dir = tempdir();
        let settings = settings_with(vec![tab_entry("t1", "Personal", &dir, false)]);

        let err = resolve_file_path("../secrets.txt", Some("t1"), &settings).unwrap_err();
        assert_eq!(err.code, ErrorCode::NameInvalid);

        let err = resolve_file_path("sub\\..\\..\\secrets.txt", Some("t1"), &settings).unwrap_err();
        assert_eq!(err.code, ErrorCode::NameInvalid);

        // No known base — must still be rejected rather than passed through, since a resolved
        // relative `..` value would otherwise be handed to the opener unresolved-but-unsafe.
        let err = resolve_file_path("../secrets.txt", None, &settings).unwrap_err();
        assert_eq!(err.code, ErrorCode::NameInvalid);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_file_path_rejects_rooted_but_not_absolute_paths() {
        let dir = tempdir();
        let settings = settings_with(vec![tab_entry("t1", "Personal", &dir, false)]);

        // Leading `\` with no drive-letter prefix: `has_root()` true, `is_absolute()` false on
        // Windows — `folder.join(path)` would otherwise replace the base's root entirely and
        // escape the tab folder (`PathBuf::join`'s Windows semantics).
        let err = resolve_file_path("\\Windows\\System32\\cmd.exe", Some("t1"), &settings).unwrap_err();
        assert_eq!(err.code, ErrorCode::NameInvalid);

        // POSIX-style rooted path — same "rooted but not absolute" shape on Windows.
        let err = resolve_file_path("/etc/passwd", Some("t1"), &settings).unwrap_err();
        assert_eq!(err.code, ErrorCode::NameInvalid);

        // No known base — must still be rejected rather than passed through unresolved-but-unsafe.
        let err = resolve_file_path("\\Windows\\System32\\cmd.exe", None, &settings).unwrap_err();
        assert_eq!(err.code, ErrorCode::NameInvalid);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_file_path_accepts_relative_paths_with_subdirectories_that_are_not_parent_dir_segments() {
        let dir = tempdir();
        let settings = settings_with(vec![tab_entry("t1", "Personal", &dir, false)]);
        let resolved = resolve_file_path("sub\\notes.txt", Some("t1"), &settings).unwrap();
        assert_eq!(resolved, dir.join("sub").join("notes.txt").to_string_lossy().into_owned());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn candidate_prefixes_trims_one_trailing_word_at_a_time_longest_first() {
        // No space between `(x86)` and `\Steam` — they're one whitespace-delimited word, so no
        // prefix boundary falls between them (matches the real `C:\Program Files (x86)\Steam`
        // path, which has no space there either).
        let prefixes = candidate_prefixes("C:\\Program Files (x86)\\Steam and more prose");
        assert_eq!(prefixes, vec![
            "C:\\Program Files (x86)\\Steam and more prose",
            "C:\\Program Files (x86)\\Steam and more",
            "C:\\Program Files (x86)\\Steam and",
            "C:\\Program Files (x86)\\Steam",
            "C:\\Program Files",
            "C:\\Program",
        ]);
    }

    #[test]
    fn candidate_prefixes_of_a_single_word_is_just_that_word() {
        assert_eq!(candidate_prefixes("C:\\Program"), vec!["C:\\Program"]);
    }

    #[test]
    fn resolve_file_path_widened_finds_the_longest_prefix_that_exists_on_disk() {
        let dir = tempdir();
        // A real directory whose name itself contains a space and parens — GitHub issue #235's
        // exact repro shape (`C:\Program Files (x86)\...`).
        let real_dir = dir.join("Program Files (x86)");
        std::fs::create_dir_all(&real_dir).unwrap();
        let settings = settings_with(vec![tab_entry("t1", "Personal", &dir, false)]);

        // `value` is the short, regex-truncated match (stops at the first space); `extended` is the
        // rest-of-line the frontend would send — extends past the real directory into trailing prose
        // that does not itself exist on disk.
        let value = "Program";
        let extended = "Program Files (x86) plus some trailing prose";
        let resolved = resolve_file_path_widened(value, Some(extended), Some("t1"), &settings).unwrap();
        assert_eq!(resolved, real_dir.to_string_lossy().into_owned());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_file_path_widened_falls_back_to_the_short_match_when_nothing_wider_exists() {
        let dir = tempdir();
        let settings = settings_with(vec![tab_entry("t1", "Personal", &dir, false)]);

        let value = "Missing";
        let extended = "Missing Path That Does Not Exist Anywhere";
        // No prefix of `extended` exists on disk — falls back to resolving `value` as-is (today's
        // pre-issue-#235 behavior), existence unchecked.
        let resolved = resolve_file_path_widened(value, Some(extended), Some("t1"), &settings).unwrap();
        assert_eq!(resolved, dir.join("Missing").to_string_lossy().into_owned());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_file_path_widened_with_no_extended_text_behaves_like_resolve_file_path() {
        let dir = tempdir();
        std::fs::write(dir.join("notes.txt"), b"").unwrap();
        let settings = settings_with(vec![tab_entry("t1", "Personal", &dir, false)]);
        let resolved = resolve_file_path_widened("notes.txt", None, Some("t1"), &settings).unwrap();
        assert_eq!(resolved, dir.join("notes.txt").to_string_lossy().into_owned());
        std::fs::remove_dir_all(&dir).ok();
    }
}
