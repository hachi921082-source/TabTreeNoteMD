# PLAN: Project Architecture (Foundation)

> Status: implemented. This status line was stale for some time after the actual `src/`/`src-tauri/`
> module layout, command/event naming, and `Result<T>`/`AppResult<T>` mapping it specifies were
> already built and in active use by every other feature plan (`tab-management`, `memo-tree`,
> `editor-save`, `markdown-editing`, `table-of-contents`, `tab-lock`, etc.) — corrected here
> (`tab-lock/PLAN.md`'s implementation pass, per its own task's instruction to verify actual `src/`
> state rather than trust this line) rather than left to keep drifting; see `docs/ISSUES.md`
> git history for the "Several `PLAN.md` `Status` lines say 'not yet implemented'" entry this
> resolves. Read this file first — every other `docs/plans/<feature>/PLAN.md` assumes the module
> layout, command-naming convention, event-naming convention, and `Result<T>` mapping defined here,
> and does not repeat them. This file introduces no new product behavior; it only fixes the
> implementation-discretion items `detailed-spec.md` §9 explicitly leaves open (Rust/Tauri naming
> conventions, event names) plus the frontend framework choice, which `detailed-spec.md` does not
> mention at all but which `DECISIONS.md`'s Tauri-adoption rationale ("renderer assets... can be
> carried over largely as is") already implies continuity with the legacy Electron renderer.

Sources: `docs/spec.md` §1 Q1, §2.7; `docs/DECISIONS.md` "Technology Stack" and "Editor Engine" entries.
Per `DECISIONS.md` "Implementation Policy: No Legacy Code Reference — Build from Scratch," this plan is
implemented without reading or copying the old repository's source code.

---

## 1. Technology Stack

| Layer | Choice | Rationale |
|---|---|---|
| App shell | Tauri 2.x | `[spec §1 Q1]`, `DECISIONS.md` "Technology Stack" |
| Backend | Rust (edition 2021+) | Tauri requirement |
| WebView | WebView2 (Windows 11 primary target) | `[spec §1 Q3]` |
| Frontend framework | **React 18 + TypeScript** | Continuity with legacy renderer (`react`/`react-dom` in legacy `package.json`); `DECISIONS.md` explicitly gives "renderer assets... can be carried over largely as is" as part of the Tauri-adoption rationale. This is treated as an implementation-discretion decision (no product-observable consequence, unlike the editor-engine choice), not escalated to `designer-lite`. |
| Build tool (frontend) | Vite | Same as legacy (`electron-vite` → plain `vite` since Tauri's dev server model replaces `electron-vite`'s main/preload/renderer split) |
| Editor engine | CodeMirror 6 | `[spec §4.2]`, confirmed, see `markdown-editing/PLAN.md` |
| HTML sanitization (preview) | Parser-level attribute filtering (`filterStyleDeclarations()`/`isSafeRelativeImageSrc()` in `markdownSanitize.ts`, called from `markdownSpanParser.ts`) | No HTML-string round-trip; see `markdown-editing/PLAN.md` §5 |
| File watching (Rust) | `notify` crate | Rust ecosystem equivalent of legacy `chokidar` |
| Crypto (Rust) | `aes-gcm` + `scrypt` crates (RustCrypto) | `[spec §4.1]` AES-256-GCM + scrypt |
| Encoding (Rust) | `encoding_rs` (Shift_JIS fallback) | Rust ecosystem equivalent of legacy `iconv-lite` |
| Serialization | `serde` / `serde_json` | Standard Tauri IPC serialization |
| Type-safety bridge (Rust ⇄ TS) | Hand-maintained TS mirror types in `src/shared/models.ts`, **or** `specta` + `tauri-specta` codegen if `developer-lite` judges it worth the setup cost | Legacy used hand-maintained `ipc-contract.ts`/`models.ts`; either approach satisfies `spec.md` (no product-level requirement), left as implementation discretion. If hand-maintained, keep `src/shared/models.ts` as the single source of truth mirrored 1:1 from the Rust `model/` structs described in §3 below, to minimize drift risk. |

## 2. Directory Layout

```
TabTreeNoteMD/
├── src-tauri/                  Rust backend crate
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs             app entry, window setup, tray, multi-launch guard
│   │   ├── commands/           one module per domain, Tauri #[command] fns (see §4)
│   │   │   ├── tab.rs
│   │   │   ├── tree.rs
│   │   │   ├── editor.rs
│   │   │   ├── search.rs
│   │   │   ├── settings.rs
│   │   │   ├── link.rs
│   │   │   ├── image.rs
│   │   │   └── lock.rs
│   │   ├── fs/                 filesystem primitives (domain-agnostic)
│   │   │   ├── atomic_write.rs
│   │   │   ├── self_write_registry.rs
│   │   │   ├── retry.rs        Windows lock retry policy (§3.7 constants)
│   │   │   ├── trash.rs
│   │   │   ├── path_safety.rs  name sanitization/validation (§5.11)
│   │   │   ├── temp_naming.rs
│   │   │   └── app_data_root.rs  portable-layout path resolution (config/data next to exe)
│   │   ├── watch/
│   │   │   ├── watcher.rs      `notify`-based folder watcher, one per registered tab folder
│   │   │   ├── conflict.rs     mtime+size+hash composite classification
│   │   │   └── active_memo.rs  tracks which memo is currently open, for conflict-event targeting
│   │   ├── crypto/
│   │   │   ├── cipher.rs       AES-256-GCM wrapper
│   │   │   ├── kdf.rs          async scrypt wrapper
│   │   │   ├── lock_blob.rs    lock-blob encode/decode (§3.4 of detailed-spec)
│   │   │   └── obfuscated_naming.rs  token generation/collision retry
│   │   ├── model/               plain data structs mirroring `spec.md` §3.4 types
│   │   │   ├── app_settings.rs
│   │   │   ├── tab_meta.rs
│   │   │   ├── tree.rs
│   │   │   └── result.rs        `AppResult<T>` + `AppError`/`ErrorCode` (§5 below)
│   │   ├── store/                in-memory runtime state (tab registry, lock sessions, index)
│   │   │   ├── tab_store.rs
│   │   │   ├── lock_session_store.rs
│   │   │   └── memo_file_service.rs   read/write a single memo body (encoding+EOL aware)
│   │   ├── search/
│   │   │   └── index.rs
│   │   ├── settings/
│   │   │   └── app_settings_store.rs
│   │   └── util/
│   │       └── name_sanitize.rs
│   └── icons/
├── src/                         React + TS frontend
│   ├── main.tsx
│   ├── App.tsx
│   ├── App.css
│   ├── ipc/                     thin typed wrappers around `@tauri-apps/api` invoke/listen, one file per domain, mirrors src-tauri/src/commands/*
│   │   ├── tab.ts
│   │   ├── tree.ts
│   │   ├── editor.ts
│   │   ├── search.ts
│   │   ├── settings.ts
│   │   ├── link.ts
│   │   ├── image.ts
│   │   └── lock.ts
│   ├── shared/                  types + pure logic shared across components, mirrors legacy `src/shared`
│   │   ├── models.ts            TS mirror of Rust `model/*` (or specta-generated)
│   │   ├── result.ts            `Result<T>` discriminated union + `ErrorCode` (§5 below)
│   │   ├── nameSanitize.ts       client-side mirror of validation for instant inline feedback (server-side is authoritative)
│   │   ├── treeOps.ts            pure tree helper functions (find/insert/move node, etc.)
│   │   └── shortcuts.ts          default shortcut keymap
│   ├── state/                    React state, one hook/context per domain (no external state library needed at this app's scale — matches legacy's plain-hooks approach)
│   │   ├── useTabs.ts
│   │   ├── useTree.ts
│   │   ├── useEditor.ts
│   │   ├── useSearch.ts
│   │   └── useSettings.ts
│   ├── components/
│   │   ├── TabBar/
│   │   ├── Tree/
│   │   ├── Editor/
│   │   ├── Search/
│   │   ├── Settings/
│   │   ├── StatusBar/
│   │   └── dialogs/              shared custom dialog component (§6.5 of detailed-spec — no native blocking dialogs)
│   └── editor/                   CodeMirror 6 extension code (see markdown-editing/PLAN.md)
│       ├── codeMirrorSetup.ts
│       ├── markdownDecorations.ts
│       ├── markdownSpanParser.ts
│       ├── markdownSanitize.ts
│       ├── checkboxClick.ts
│       ├── decorationCommands.ts
│       ├── textCommands.ts
│       ├── eolKeeper.ts
│       ├── imagePaste.ts
│       └── links.ts
├── docs/
├── package.json
├── tsconfig.json
└── vite.config.ts
```

This directory breakdown is structurally similar to a typical Electron main/renderer split (`fs`,
`crypto`, `watch`, `search`, `settings` on the backend; `components`, `editor`, `store`, `shared` on the
frontend) for anyone comparing module responsibilities conceptually, but is implemented fresh from this
plan and `detailed-spec.md` — per `DECISIONS.md`'s no-legacy-reference policy, `developer-lite` does not
consult the old repository's source files for this.

## 3. `Result<T>` / `ErrorCode` Mapping (Rust ⇄ TS)

Elaborates `[spec §3.4, §5.1]`. Rust side (`model/result.rs`):

```rust
#[derive(serde::Serialize)]
#[serde(tag = "ok")]
pub enum AppResult<T> {
    #[serde(rename = "true")]
    Ok { value: T },
    #[serde(rename = "false")]
    Err { error: AppErrorPayload },
}
// NOTE: serde's default enum tagging does not produce exactly `{ok: true, value}` /
// `{ok: false, error}` out of the box; `developer-lite` implements a custom Serialize impl
// (or a plain struct with an `ok: bool` field plus `Option<T>`/`Option<AppErrorPayload>`)
// to match the exact wire shape in `[spec §3.4]`. Either implementation technique is
// implementation-discretion; the wire shape itself is not.

#[derive(serde::Serialize)]
pub struct AppErrorPayload {
    pub code: ErrorCode,
    pub message: String,
    pub path: Option<String>,
}

#[derive(serde::Serialize, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    NameDuplicate, NameInvalid, NotFound, IoLocked,
    TrashFailed, DecryptFailed, Conflict, MetaCorrupt,
    LongPath, TabLocked, Unknown,
}
```

All Tauri `#[command]` functions return `AppResult<T>` (never a bare `Result<T, String>` and never
`panic!`/unwrap on recoverable errors) — every fallible internal `Result`/`Option` is converted to
`AppResult::Err` at the command boundary (`[spec §5.1]`: "Exceptions caught at boundary and converted
to error code"). Internal error detail (OS error codes, stack context) is `tracing::error!`-logged
inside the command before being collapsed to the generic `message`/`code` sent to the frontend
(`[spec §5.10]`).

Frontend side (`shared/result.ts`) mirrors `[spec §3.4]`'s TS definition verbatim; every `ipc/*.ts`
wrapper function returns `Promise<Result<T>>` and never throws for expected/recoverable failure
paths (a thrown/rejected promise is reserved for truly unexpected transport-level failures, e.g. the
Tauri IPC channel itself being unavailable).

## 4. Tauri Command Naming Convention and Master Inventory

Convention: `snake_case`, `<domain>_<verb>[_<qualifier>]`. This table is the **single source of
truth for command names**; each per-feature `PLAN.md` restates only the subset relevant to it and
must not invent divergent names. New commands discovered necessary during implementation should be
added here first (or if that's impractical mid-session, at minimum use this naming pattern so a
later pass can reconcile).

| Command | Domain | Purpose | Elaborated in |
|---|---|---|---|
| `tab_list` | tab | list registered tabs for the tab bar | `tab-management/PLAN.md` |
| `tab_create` | tab | folder-picker → create+register new tab | `tab-management/PLAN.md` |
| `tab_register_existing` | tab | folder-picker → register pre-existing folder | `tab-management/PLAN.md` |
| `tab_rename` | tab | rename a tab | `tab-management/PLAN.md` |
| `tab_reorder` | tab | persist new tab order | `tab-management/PLAN.md` |
| `tab_remove` | tab | unregister (never deletes folder) | `tab-management/PLAN.md` |
| `tab_open_location` | tab | reveal folder in Explorer | `tab-management/PLAN.md` |
| `tab_lock` | tab lock | encrypt+lock an unlocked tab | `tab-lock/PLAN.md` |
| `tab_unlock` | tab lock | start a session (password entry) | `tab-lock/PLAN.md` |
| `tab_end_session` | tab lock | explicit session discard (also implicit on tab switch, frontend-driven) | `tab-lock/PLAN.md` |
| `tree_open` | tree | open/reconcile a tab's tree (§3.5) | `memo-tree/PLAN.md` |
| `memo_create` | tree | create memo (root, append-to-end) | `memo-tree/PLAN.md` |
| `memo_create_child` | tree | create memo as child of selection | `memo-tree/PLAN.md` |
| `memo_rename` | tree | rename memo (also F2) | `memo-tree/PLAN.md` |
| `memo_delete` | tree | delete (trash, or direct after consent) | `memo-tree/PLAN.md` |
| `memo_move` | tree | move/reorder/re-parent within a tab, or across tabs (DND) | `memo-tree/PLAN.md` |
| `memo_copy_to_tab` | tree | Ctrl-drag cross-tab copy | `memo-tree/PLAN.md` |
| `memo_set_expanded` | tree | toggle expand/collapse (metadata-only) | `memo-tree/PLAN.md` |
| `memo_set_label` | tree | set color/kind label (metadata-only) | `memo-tree/PLAN.md` |
| `memo_set_editor_override` | tree | per-memo editor settings override | `memo-tree/PLAN.md` |
| `memo_set_edit_lock` | tree | toggle per-memo edit lock (metadata-only) | `memo-tree/PLAN.md` |
| `memo_get_properties` | tree | file info + character count | `memo-tree/PLAN.md` |
| `editor_load` | editor | load memo body + encoding/EOL/hash | `editor-save/PLAN.md` |
| `editor_save` | editor | save (hash-conflict-checked, atomic) | `editor-save/PLAN.md` |
| `search_query` | search | full-text query against in-memory index | `search-index/PLAN.md` |
| `settings_get` | settings | fetch full `AppSettings` | `persistence-fs/PLAN.md` |
| `settings_set` | settings | deep-partial patch + atomic save | `persistence-fs/PLAN.md` |
| `link_resolve` | link | resolve a `memo:` target | `link/PLAN.md` |
| `link_open_external` | link | open URL/email/file path via OS API | `link/PLAN.md` |
| `image_save_pasted` | image | write pasted image, return relative path+dims | `image-handling/PLAN.md` |

Not a Tauri command (frontend-only / OS-window-manager-only, listed here so it's not mistaken for a
gap): "pin tab to foreground" (`[spec §2.1]`, session-only UI state), in-editor text conversion and
checkbox-cycle operations (`[spec §2.3]`, pure client-side text transforms — see
`editor-save/PLAN.md` §4), in-memo search/replace (`[spec §2.4]`, operates on already-loaded content),
window minimize/maximize/close button clicks (native Tauri window API calls from the frontend, no
custom command needed).

## 5. Backend → Frontend Event Naming Convention and Inventory

Convention: `evt:<domain>:<name>`, emitted via Tauri's `app_handle.emit(...)` and subscribed to via
`@tauri-apps/api/event`'s `listen(...)` in the corresponding `src/state/use*.ts` hook.

| Event | Payload (informal) | Purpose | Elaborated in |
|---|---|---|---|
| `evt:fs:external-change` | `{ tabId, memoName, kind: 'created'\|'modified'\|'deleted' }` | genuine (non-self) external file change detected, reconcile tree/index | `persistence-fs/PLAN.md` |
| `evt:editor:conflict` | `{ tabId, memoName }` | save-time or watch-time conflict detected while memo is open | `editor-save/PLAN.md` |
| `evt:meta:recovered` | `{ scope: 'tab'\|'app-settings', tabId? }` | metadata corruption recovery cascade completed, notify user | `persistence-fs/PLAN.md` |
| `evt:app:focus-request` | none | second-instance launch detected, bring window to foreground | `window-operation/PLAN.md` |
| `evt:tray:show-request` | none | tray icon activated, restore window from tray | `window-operation/PLAN.md` |

New events, if found necessary during implementation, follow the same `evt:<domain>:<name>` pattern
and should be added to this table.

## 6. IPC Call Shape

Every frontend `ipc/*.ts` function signature: `async function <verb>(input: InputType):
Promise<Result<OutputType>>`, thin wrapper over `invoke<Result<OutputType>>('<command_name>',
{ ...input })`. No business logic in `ipc/*.ts` — validation/branching on the `Result` happens in the
calling `state/use*.ts` hook or component.

## 7. Testing/Verification Hook (context for `reviewer-lite`, not a build requirement of this plan)

`DECISIONS.md`'s Tauri-adoption rationale explicitly relies on "automatic testing via CDP/WebDriver"
as the safety net (the user cannot read code). For `reviewer-lite` to actually drive the app, the dev
build should be launched in a mode that exposes WebView2 remote debugging, or via `tauri-driver`
(WebDriver protocol). This is not a product feature and has no `PLAN.md` of its own.

The CDP port is opened via `WebviewWindowBuilder`'s own `.additional_browser_args()` builder method,
called in `main.rs`'s `.setup()` under `#[cfg(debug_assertions)]` — **not** by setting the
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` env var before `tauri::Builder`, which does not work: `wry`
always passes an explicit, non-null `additional_browser_arguments` string to
`CoreWebView2EnvironmentOptions`, and WebView2 ignores the env var whenever that string is non-null
(confirmed empirically and by reading `wry`'s source; see issue #21). Because `WebviewWindowBuilder`
can only attach that arg at window-creation time, and Tauri otherwise creates the declaratively
configured `"main"` window automatically before `.setup()` runs, `tauri.conf.json`'s `"main"` window
entry sets `"create": false` and `main.rs`'s `.setup()` creates it itself via
`WebviewWindowBuilder::from_config(app, &window_config)`, reading every other property (title, size,
decorations, starting hidden, ...) from that same config so it remains the single source of truth;
only the CDP arg differs between debug and release builds.

Live-verified (not just reasoned through — issue #21 exists precisely because the prior approach's
"should work" claim went unverified for a long time): `curl http://127.0.0.1:9222/json` against a
freshly-launched `npx tauri dev` build returns a real target list with a `webSocketDebuggerUrl`;
connecting to it and issuing CDP commands (`Runtime.evaluate`, `Page.captureScreenshot`) confirmed the
page had actually rendered the app's real UI, not just an empty/blank WebView2 shell.

## 8. Cargo Dependencies (starting list, to be refined at `Cargo.toml` creation time)

`tauri`, `serde`, `serde_json`, `notify`, `aes-gcm`, `scrypt`, `rand` (token/IV/salt generation),
`encoding_rs`, `sha2` (hash for conflict detection, §5.5/§5.6 of `spec.md`), `trash` (OS trash crate),
`tracing` + `tracing-subscriber` (internal-only diagnostic logging, `[spec §5.10]`), `tokio` (async
runtime, needed for async scrypt per `[detailed-spec §3.4]`'s "must use the asynchronous scrypt API").
Exact versions are a `Cargo.toml`-time decision, not a spec concern.

---

## Open Items

None. This plan resolves purely implementation-discretion items (`[detailed-spec §9]` category) and
introduces no product-observable behavior requiring `designer-lite` consultation.
