# PLAN: Project Architecture (Foundation)

> Status: planning complete, not yet implemented (no `src/` or `src-tauri/` exists yet at time of
> writing). Read this file first — every other `docs/plans/<feature>/PLAN.md` assumes the module
> layout, command-naming convention, event-naming convention, and `Result<T>` mapping defined here,
> and does not repeat them. This file introduces no new product behavior; it only fixes the
> implementation-discretion items `detailed-spec.md` §9 explicitly leaves open (Rust/Tauri naming
> conventions, event names) plus the frontend framework choice, which `detailed-spec.md` does not
> mention at all but which `DECISIONS.md`'s Tauri-adoption rationale ("renderer assets... can be
> carried over largely as is") already implies continuity with the legacy Electron renderer.

Sources: `docs/spec.md` §1 Q1, §2.7; `docs/DECISIONS.md` "Technology Stack" and "Editor Engine"
entries; legacy repo `c:\Users\m9112\vscode2\TabTreeNote\src` (reference only, per `DECISIONS.md`
"Reference to Old Repository").

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
| HTML sanitization (preview) | DOMPurify (or equivalent) with a custom allowlist config | Continuity with legacy (`dompurify` dependency); see `markdown-editing/PLAN.md` §5 |
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

This mirrors the legacy `src/main/{fs,crypto,watch,search,settings,ipc}` and `src/renderer/{components,editor,store,shared}` breakdown closely enough that `developer-lite` can consult the legacy files module-by-module during implementation (per `DECISIONS.md`'s explicit sanction to do so), translating TS → Rust for the backend half.

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
build should be launched in a mode that exposes WebView2 remote debugging (e.g. setting the
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`/`--remote-debugging-port` environment variable Tauri passes
through to WebView2) or via `tauri-driver` (WebDriver protocol). This is not a product feature and has
no `PLAN.md` of its own; `developer-lite` should make one of these paths work early (e.g. alongside
the first implemented feature) rather than leaving it until the end, since every subsequent feature's
functional verification depends on it.

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
