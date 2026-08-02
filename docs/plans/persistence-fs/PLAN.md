# PLAN: Persistence & Filesystem Infrastructure

> Status: implemented, except §5's folder watcher (`src-tauri/src/watch/watcher.rs`,
> `watch/conflict.rs`) is built and unit-tested but not yet *instantiated* against any live,
> currently-registered tab folder — no command anywhere starts one, so `evt:fs:external-change`/
> `evt:editor:conflict` are never actually emitted yet (each depending feature's own frontend
> listener exists as a harmless forward seam in the meantime). Everything else — atomic write
> (§3), self-write registry (§4), corruption-recovery cascade (§6), OS trash (§9), retry policy
> (§7), portable app-data layout (§2), name validation (§8) — is implemented and in active use by
> every other feature plan. This status line was stale for some time after the above was already
> built; corrected here (`tab-lock/PLAN.md`'s implementation pass, a heavy consumer of nearly all
> of the above, per its own task's instruction to verify actual `src/` state rather than trust
> this line) — see `docs/ISSUES.md` git history for the "Several `PLAN.md` `Status` lines say 'not
> yet implemented'" entry this resolves. Foundational/cross-cutting plan: every other feature plan
> (`tab-management`, `memo-tree`, `editor-save`, `tab-lock`, etc.) reads and writes through the
> mechanisms defined here rather than re-deriving them. Read `project-architecture/PLAN.md` first for
> module layout and naming conventions used below.

Sources: `docs/spec.md` §2.6, §3.2, §3.3, §5.2–§5.9, §7; `docs/detailed-spec.md` §3.1–3.3, §3.7, §6,
§7.

---

## 1. Scope

This plan covers the mechanisms shared by *every* persisted file in the app:

1. Portable app-data layout (where settings/tab-registry live).
2. Atomic write pattern (temp → bak → target).
3. Self-write registry (suppressing false external-change events).
4. File watching + external-change / conflict classification.
5. Metadata corruption recovery cascade.
6. App Settings JSON: schema, get/patch/save commands.
7. Windows lock-error retry policy.
8. Name sanitization/validation (strict-vs-lenient rule).

It does **not** cover: tab metadata JSON's `tree`/`lock` *content* semantics (see `memo-tree/PLAN.md`
and `tab-lock/PLAN.md`), memo body load/save (see `editor-save/PLAN.md`) — those features call the
primitives defined here.

## 2. Portable App-Data Layout

Elaborates `[spec §2.6]`, `[detailed-spec §3.1]`.

- `app_data_root.rs` resolves a folder next to the running executable (e.g. `<exe_dir>/data/`) at
  startup, **not** `%APPDATA%`. Resolution must work correctly whether launched via a shortcut or
  directly, and must be computed once at startup and reused (no repeated `current_exe()` lookups mid
  session, since a portable copy's location doesn't change while running).
- Files under this root:
  - `data/settings.json` — App Settings JSON (§6 below).
  - `data/settings.json.bak`
- If `data/` does not exist on first launch, create it and initialize `settings.json` with hardcoded
  defaults (empty `tabs`, default `window` geometry, `theme: 'light'` or `'dark'` — pick one as the
  factory default, `uiFontScale: 1.0`, etc.) — this is the same code path as the "total loss" recovery
  branch in §5 below, so implement it once and reuse.

## 3. Atomic Write — `fs/atomic_write.rs`

Elaborates `[spec §5.2]`, `[detailed-spec §6.1]`. Single shared function, used by every persistence
call site in the app (memo body writes, tab metadata JSON, app settings JSON, tab-lock obfuscated
files):

```rust
pub fn atomic_write(
    target_path: &Path,
    content: &[u8],
    registry: &SelfWriteRegistry,
) -> std::io::Result<()> {
    // 1. write content to `<target_dir>/<TEMP_PREFIX><random-suffix>` (same directory as target)
    // 2. flush + fsync the temp file
    // 3. registry.register(target_path); registry.register(&bak_path(target_path));
    // 4. rename(target_path, bak_path(target_path)) — ignore NotFound (first-ever save has no prior target)
    // 5. rename(temp_path, target_path)
    // steps 4-5 each go through the retry-wrapped rename in fs/retry.rs (§7 below)
}
```

- `TEMP_PREFIX` (e.g. `.ttn-tmp-`) is defined once in `fs/temp_naming.rs` and is the fixed prefix the
  self-write registry / file watcher use to ignore temp-file churn entirely (never even worth
  registering — filtered before registry lookup).
- `bak_path(p)` = `p` with `.bak` appended to the full filename (e.g. `Shopping list.md` →
  `Shopping list.md.bak`, `.tabtreenote.json` → `.tabtreenote.json.bak`). Only one generation is ever
  kept — each call's step 4 overwrites the previous `.bak`.
- No directory-entry `fsync` (`[detailed-spec §6.1]` point 6, relies on NTFS journaling, Windows-only
  target per `[spec §6]`).
- Every call site that mutates persisted content must go through this function; no ad-hoc `fs::write`
  anywhere in the codebase for these file classes (grep-able rule for `reviewer-lite`/self-review: only
  `atomic_write.rs` calls `std::fs::write`/`rename` directly for target-content writes; image writes
  and trash operations are the two legitimate exceptions — a *new* file with no prior version doesn't
  need the bak-swap step 4, and trash is a delete not a write, see `image-handling/PLAN.md` and §8
  below respectively).

## 4. Self-Write Registry — `fs/self_write_registry.rs`

Elaborates `[spec §5.4]`, `[detailed-spec §6.2]`.

```rust
pub struct SelfWriteRegistry {
    // path -> insertion Instant, guarded by a Mutex (accessed from both command handlers and the
    // watcher's event-processing thread/task)
    entries: Mutex<HashMap<PathBuf, Instant>>,
}

impl SelfWriteRegistry {
    pub fn register(&self, path: &Path);          // call BEFORE performing the FS op
    pub fn consume_if_present(&self, path: &Path) -> bool;  // called by watcher; true = self-write, discard event
    fn sweep_expired(&self);  // TTL fallback, called opportunistically (e.g. on every register/consume)
}
```

- TTL fallback: 2.5s (`[detailed-spec §3.7]`).
- Registration is centralized: every FS-mutating call in `fs/`, `crypto/obfuscated_naming.rs`,
  `fs/trash.rs` registers its target path(s) immediately before performing the operation. No
  command-handler code (`commands/*.rs`) calls the underlying FS primitives directly without going
  through a `fs/`/`crypto/` function that already does this — this is the "centralized in one place"
  requirement from `[detailed-spec §6.2]`.
- Rename/move operations register **both** the source and destination paths (a rename produces a
  delete-like event at the source and a create-like event at the destination on most watcher
  backends).

## 5. File Watching + Conflict Classification — `watch/`

Elaborates `[spec §3.3, §5.5]`, `[detailed-spec §6.4]`.

- `watch/watcher.rs`: one `notify::RecommendedWatcher` per currently-registered, currently-unlocked
  tab folder (non-recursive — `[spec §2.2]` only scans direct children, so watching does not need to
  be recursive either). Locked tabs (no session) are **not** watched (their `.md` files don't exist on
  disk as `.md` while locked, and their obfuscated files' changes are irrelevant to the app unless a
  session is active — even then, the app itself is the only writer, so self-write registry coverage is
  sufficient without needing OS-level watch granularity beyond what's already watched for the metadata
  file).
- Each raw watcher event first passes through `SelfWriteRegistry::consume_if_present` — a match means
  "ignore, this was us." No match proceeds to `watch/conflict.rs`.
- `watch/conflict.rs`: composite mtime + size + hash check to classify a non-self event as a genuine
  external change (vs. e.g. an editor or backup tool touching mtime without content change). Only a
  genuine content change:
  - triggers `tree_open`-style reconciliation (§3.5 of `detailed-spec.md`, implemented in
    `memo-tree/PLAN.md`) for create/delete events,
  - triggers `search/index.rs` patch (see `search-index/PLAN.md`) for any indexable content change,
  - triggers `evt:fs:external-change` to the frontend,
  - and, if the changed memo is the one currently open in the editor (`watch/active_memo.rs` tracks
    this — set by `editor_load`, cleared on switch-away), triggers `evt:editor:conflict` instead of (or
    in addition to) the generic reconciliation event, per `[spec §5.8]` "External edit conflict" row.
- Save-time conflict check (`editor_save` command, elaborated fully in `editor-save/PLAN.md`) is a
  **separate**, simpler mechanism: pure hash comparison at the moment of save, no mtime/size involved
  (`[detailed-spec §6.4]`). This file only owns the watcher-side composite check.

## 6. App Settings JSON

### 6.1 Schema

Full schema already given verbatim in `[detailed-spec §3.2]`; `model/app_settings.rs` mirrors the
`AppSettings` TS interface from `[spec §3.4]` field-for-field. No changes proposed here.

### 6.2 Commands

- **`settings_get() -> AppResult<AppSettings>`** — reads `data/settings.json` (recovery cascade below
  applies), returns the full struct.
- **`settings_set(patch: AppSettingsPatch) -> AppResult<AppSettings>`** — deep-partial merge (only
  fields present in `patch` are overwritten; nested objects like `window` merge per-field, not
  wholesale-replace) into the in-memory settings, then `atomic_write` the result, returns the new full
  struct. `AppSettingsPatch` is the same shape as `AppSettings` but every field (including nested
  object fields) is `Option<...>`.
- Every settings mutation in the app (theme toggle, window geometry on resize/move, `lastOpened`
  update, tab registry changes, etc.) funnels through `settings_set` — no direct file write from
  elsewhere. `tabs[i].locked`/`tabs[i].name` cache updates on lock/unlock (`tab-lock/PLAN.md`) also go
  through this same function.
- Window geometry persistence: debounce writes on resize/move (don't `atomic_write` on every pixel of
  a drag) — reuse the same debounce constant class as autosave, exact value is implementation
  discretion.

### 6.3 Corruption Recovery Cascade — `settings/app_settings_store.rs`

Elaborates `[spec §5.9, §7]`, `[detailed-spec §6.4]`:

1. On `settings_get` (and at startup), attempt to parse `data/settings.json`.
2. On parse/schema failure, attempt `data/settings.json.bak`.
3. If that also fails/is absent, regenerate with hardcoded defaults (§2 above) — **do not** fail to
   start.
4. In cases 2 and 3, emit `evt:meta:recovered` with `{ scope: 'app-settings' }` so the frontend shows a
   non-blocking notice (`[spec §7]`: "same path as metadata JSON corruption").

The identical cascade shape (bak → flat-rebuild-equivalent → notify) is reused for tab metadata JSON
in `memo-tree/PLAN.md` §7 and `tab-lock/PLAN.md` — this file owns the generic pattern description;
each consumer implements it for its own file.

## 7. Windows Lock Error Retry Policy — `fs/retry.rs`

Elaborates `[spec §5.7]`, defaults from `[detailed-spec §3.7]`:

```rust
pub fn with_retry<T>(op: impl Fn() -> std::io::Result<T>) -> std::io::Result<T> {
    // attempt op(); on EBUSY/EPERM-class error, sleep then retry.
    // backoff schedule: 50ms, 100ms, 200ms, 200ms (4 attempts total)
    // on final failure, propagate the error (caller maps to IoLocked at the command boundary)
}
```

Used by `atomic_write`'s rename steps, `memo_delete`'s direct-delete fallback, `tab-lock`'s file
rename/re-encrypt operations, and any other write/rename/delete call site touching a possibly-locked
file.

## 8. Name Sanitization / Validation — `fs/path_safety.rs` + `util/name_sanitize.rs`

Elaborates `[spec §5.11]`, `[detailed-spec §6.5]`.

```rust
pub enum ValidationMode { Strict, Lenient }

pub fn validate_name(name: &str, mode: ValidationMode) -> Result<(), ErrorCode> {
    // checks (all modes): non-empty, Windows-forbidden characters (\/:*?"<>|),
    // control characters, Windows reserved names (CON, PRN, AUX, NUL, COM1-9, LPT1-9,
    // case-insensitively, with or without extension)
    // Strict-only checks: leading/trailing spaces, trailing period -> reject
    // Lenient mode: same forbidden-char/reserved-name checks (these can never be valid
    // NTFS entries regardless), but leading/trailing-space and trailing-period are accepted
    // as-is for pre-existing entities (they ARE valid, if unusual, NTFS names)
}

pub fn suggest_sanitized(name: &str) -> String {
    // replacement suggestions, e.g. '/' -> '-', ':' -> '_', trim leading/trailing space+period
}
```

- **Strict** mode: tab create/rename, memo create/rename, image-filename-generation collision retry
  (though generated image names never hit forbidden chars by construction — this is a defensive check,
  not expected to ever fire in practice).
- **Lenient** mode: §3.5 tree reconciliation scan, corruption-recovery flat rebuild — both accept
  whatever already exists on disk into the runtime tree without blocking or renaming it.
- Long-path check: full resulting path length > 260 chars → `LONG_PATH` (warning only, operation may
  still proceed — `[spec §5.11]`).
- Tab-name uniqueness: case-insensitive comparison (starting default per `[detailed-spec §5.1]`),
  checked in `commands/tab.rs` before calling this validator (uniqueness is a cross-tab-registry
  concern, not a single-name-shape concern, so it lives in the tab command handler, not here).
- Memo-name uniqueness within a tab: enforced implicitly by the filesystem itself (two `.md` files
  can't share a name in the same folder) — `commands/tree.rs` still performs an explicit pre-check
  against the in-memory tree so it can return `NAME_DUPLICATE` *before* attempting the file operation
  (`[spec §5.3]` step 1: validation happens before any real file write), rather than relying on the OS
  rename call failing.

## 9. Trash — `fs/trash.rs`

- Wraps the `trash` crate's OS-trash API. On failure, returns `TrashFailed` — caller (`memo_delete` in
  `memo-tree/PLAN.md`) is responsible for the confirm-dialog-then-direct-delete flow; this module does
  not itself prompt for consent (no UI access from the `fs/` layer).
- Direct delete (used only after user consent) goes through `with_retry` (§7) same as any other delete.

---

## 10. Cross-References

- `tab-management/PLAN.md`, `memo-tree/PLAN.md`, `editor-save/PLAN.md`, `tab-lock/PLAN.md`,
  `image-handling/PLAN.md` all call `atomic_write`, `SelfWriteRegistry`, `with_retry`, and
  `validate_name` as defined here rather than reimplementing them.
- `search-index/PLAN.md`'s "external-change index update path" is fed by §5's watcher pipeline.
- `window-operation/PLAN.md` covers multi-launch prevention, which is related to but distinct from
  this file's scope (that's about preventing two *processes* from both owning `settings.json`, not
  about the write mechanism itself — this file's atomic-write+bak pattern is what makes a single
  process's writes crash-safe; multi-launch prevention is what ensures there's only ever one such
  process).

## Open Items

None. All defaults used here (retry counts/backoff, TTL, temp prefix pattern) are explicitly marked
as implementation-discretion starting points in `[detailed-spec §3.7, §9]`, not open questions.
