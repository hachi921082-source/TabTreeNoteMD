# TabTreeNote Integrated Specification (spec.md)

> Requirements baseline for large-scale overhaul (rebuild from scratch). Consolidates `DESIGN.md` / `docs/v3/DESIGNv3.md` / `docs/00-core-principles.md` / `docs/data-model.md` / `docs/post-M7-changes.md` / `docs/post-M10-changes.md` / `issue.md` from the legacy repository (path and reference rules: see [CLAUDE.md](CLAUDE.md)), reflects TabTreeNoteMD-specific decisions, and records **only the currently valid specification** (origin annotated via `[DESIGN §x]` etc.; see legacy repository for details). For decision history, comparative analysis, and rejected proposals, refer to [DECISIONS.md](DECISIONS.md). As of 2026-07-30.

---

## Index

Keep this in sync whenever a section is added, renamed, or moved below.

- **§0 Product Overview** — what the app is, CatMemoNote lineage, intended user/use case.
- **§1 Confirmed Core Principles** — tech stack, storage format, platform target, v1 feature-scope table.
- **§2 Core Feature Specification from v1**
  - §2.1 Tab Management — create/rename/remove/reorder, folder-picker dialog, global always-on-top toggle, open-location.
  - §2.2 Memo Tree Management (Left Pane) — tree scan/hierarchy/DND, edit lock, F2/arrow-key nav, right-click menu, Delete key.
  - §2.3 Text Editing (Right Pane) — undo/redo, wrap/tab/encoding/EOL, Convert/Check menu, checkbox toggle.
  - §2.4 Search and Replace — in-memo search/replace, cross-tab full-text index, locked-tab exclusion.
  - §2.5 Appearance and Theme — light/dark theme, `uiFontScale`.
  - §2.6 Data Storage and Persistence — save triggers, atomic write + `.bak`, trash, portable layout, startup auto-open.
  - §2.7 Window / Operation — 3-pane layout, shortcuts, tray autostart, multi-launch prevention.
  - §2.8 Link Functionality — auto-links, `memo:` notation, `#slug` anchors, external-open behavior.
  - §2.9 Password Protection — per-memo encryption discontinued, unified into Tab Lock (§4.1).
- **§3 Data Model**
  - §3.1 Basic Approach — memo identification = filename, filename sanitization.
  - §3.2 Metadata — `.tabtreenote.json`, metadata-loss recovery principle.
  - §3.3 Following External Changes — file-monitoring/conflict pointers into §5.
  - §3.4 Primary Type Definitions — `Result<T>`, `ErrorCode`, `MetaTreeNode`/`TreeNode`, `TabMeta`, `AppSettings` (TS source).
- **§4 TabTreeNoteMD-Specific Feature Specification**
  - §4.1 Lock Mechanism: Tab Lock (New) — full tab-lock spec: scope, encryption, internal mechanism, cross-tab DND.
  - §4.2 Markdown Editing (CommonMark Full Support) — live/source view, toolbar, image paste/resize, decoration notation, sanitization, TOC, editor engine.
- **§5 Cross-Cutting Principles** — `Result`/`ErrorCode` convention, atomic write, sync order, self-write registry, conflict detection, hash scope, retry policy, error table, corruption recovery, security invariants, name sanitization.
- **§6 Non-Functional Requirements** — performance/reliability/portability/security targets.
- **§7 Implementation Rules to Inherit (Post-M7 Fixes)** — folder-absent handling, validation-strictness split, no native blocking dialogs, Esc-closable dialogs; deferred items.
- **§8 Implementation Rules to Inherit (Post-M10 Fixes)** — auto-save-confirm before lock, CodeMirror decoration risk note.
- **§9 Implementation Rules to Inherit (issue.md)** — legacy issue status table; §9.1 frameless window / unified context menu / force-delete shortcut resolution plan.
- **§10 Out of Scope / Future Consideration** — explicitly excluded features.

---

## 0. Product Overview

- **TabTreeNote** is a personal memo editor for local text notes managed hierarchically in a "tabs × tree structure" (Tauri + Rust, Windows 11 priority). `[DESIGN §1.1]`
- Inspired by the free software "CatMemoNote," implementing its philosophy (tabs = folders / memos = text files, 3-pane layout, DND operations, powerful text processing, registry-independent portable design) as an independent implementation. `[DESIGN §1.2]`
- Intended user: the project creator (personal use, no programming knowledge required). Use case: hierarchically storing notes, to-dos, and fragmentary information. `[DESIGN §1.4]`

---

## 1. Confirmed Core Principles

| # | Topic | Confirmed |
|---|------|----------|
| Q1 | Technology Stack | **Tauri (Rust + WebView2)** (see [DECISIONS.md](DECISIONS.md) for rationale) |
| Q2 | Data Storage Format | Maintain plain text tradition, tabs = folders. **Standardize all memos to Markdown (`.md`)** (§4.2) |
| Q3 | Supported Platforms | **Windows 11 priority**, other OS in the future |
| Q4 | v1 Feature Scope | Core experience (tree-based tab management, editing, search) + **link functionality** + **password protection** (tab lock, §4.1) |

`[DESIGN §0]`

---

## 2. Core Feature Specification from v1

### 2.1 Tab Management (= Management Folders)

- Top tab bar, 1 tab = 1 management folder. Create new / rename / **remove (unregister-only: removes the folder from the app's tab list, never deletes or trashes the folder or its files — deliberately different from memo deletion in §2.2, which does go to trash. See [DECISIONS.md](DECISIONS.md) for rationale)**. **Tab names must be unique (no duplicates allowed)**.
- Reorder by drag, drag-register existing folders, switch via click / shortcut. Each tab holds an independent tree.
- **New tab creation and folder registration use OS standard folder selection dialog** (do not use web standard `window.prompt()` etc.).
- Global "always-on-top" toggle button (📌, session-only, single button shared across all tabs — not per-tab) pins the **application window itself** above other windows at the OS level; sits in the top toolbar immediately to the left of the search button. Right-click tab to "Open tab location."

`[DESIGN §2.1]`

### 2.2 Memo Tree Management (Left Pane)

- Scan only `.md` files directly under the folder for tree traversal and listing (subdirectories excluded). Internal representation of locked tabs: see §4.1.
- Hierarchize by parent-child relationships, expand/collapse, connection lines shown. Rename / delete (to trash) / move up-down/hierarchically (**"Duplicate" removed from right-click context menu**, post-M7). Reorder by DND, change hierarchy, move between tabs (Ctrl to copy, follow sync order in §5.3). Multi-select (Ctrl/Shift). Color/type labels on memos.
- Do not follow symbolic links / junctions.
- **AutoReName (auto-naming memo from first non-empty line) is not implemented** (discontinued, confirmed. See [DECISIONS.md](DECISIONS.md) for rationale).
- No explicit add position specified; right-click "Create Memo" (always append to end) / "Create as Child" will **immediately display name input dialog upon selection** (latest spec from issue.md time. Overwrites auto-naming + auto-open behavior from post-M7 "New Memo").
- **Edit Lock** (`locked?: boolean`, text editing only prohibited, rename/delete not covered, no password required). **Distinct from tab lock in §4.1**.
- F2 key to rename (single selection). ↑/↓ keys to move to previous/next memo (arrow key navigation does not steal editor focus. Click-open focuses editor as usual). When the open memo changes, tree selection and scroll position auto-follow.
- Right-click menu: "Icon" submenu, "Properties" (file info + character count). When tree is empty, show "Create Memo" button at center only. Click empty area to select. Right-click menu displays upward when at screen bottom edge.
- **Delete key**: After clicking a row, editor auto-focuses, so Delete key alone does not reach tree deletion. **Addressed in §9.1 (existing UX retained, dedicated shortcut added)**.

`[DESIGN §2.2]`

### 2.3 Text Editing (Right Pane)

- Undo/Redo (history discarded on memo switch, no cross-memo undo), cut/copy/paste, select all, line-wise operations.
- Toggle word wrap (none / character count / window width), tab width, soft tabs, auto-indent, line numbers, whitespace/line break visibility, cursor line highlight. Display character/line count and cursor position in status bar.
- **Character encoding**: Default UTF-8. Load: "UTF-8 (BOM detection) → if invalid byte sequence, retry with Shift_JIS" (2-stage fallback). Manual encoding change command available.
- **Line endings**: Line ending code detected on load is preserved as-is on save (default). User setting allows unification. If editor engine internally normalizes to LF, implementation must retain original line ending type as metadata and restore before save (§4.2).
- Right-click: "Convert" (uppercase / lowercase / hiragana / katakana / half-width / full-width / tab ↔ space), "Check" (☐/☑ cycle insert), "Search with Google," "Properties."
- ☐/☑ in body text can be toggled by **direct click** (separate feature from right-click insertion).
- No "Format" submenu (overlaps with settings screen). Suppress right-click on filename/encoding bar. Remember scroll position per memo.
- **Right-click menu look unified** (between editor/tree): **addressed in §9.1**.

`[DESIGN §2.3]`

### 2.4 Search and Replace

- In-memo search (incremental / next / prev), replace (single / all / selection / one-by-one), regex support, case / word-wise options. Search history.
- **Full-text search (cross-tab)**: Targets memo name and body. Implementation: **lightweight in-memory index** (built at startup, no external DB or persistent index).
  - **Two paths for index differential update**: ① In-app changes (save, rename, delete, etc.) update index directly on save completion / ② External changes via file monitoring only.
  - **Locked tab handling**: Whether locked or in unlock session, that tab's name and body are always excluded from full-text search index (§4.1).

`[DESIGN §2.4]`

### 2.5 Appearance and Theme

- CSS-based light/dark theme provided as standard. Basic customization of colors, fonts, line spacing.
- UI font size setting (`uiFontScale`, 80–150%, applied to non-editor-body text).

`[DESIGN §2.5]`

### 2.6 Data Storage and Persistence

- Markdown body (`.md`) + per-tab metadata (JSON). Save timing: tab switch / memo load / minimize / exit / explicit save (Ctrl+S) / periodic autosave (don't write if no change).
- **Atomic writes + last generation `.bak`** (scope = memo body, per-tab metadata JSON, app settings JSON, tab lock related files; details §5.2).
- Safe deletion using OS trash. If trash fails, delete directly only upon user consent (no silent deletion).
- **Portable layout**: App settings and tab registration info stored **in folder under exe** (e.g., `config` / `data`) rather than `%APPDATA%` etc. Copying exe alone makes settings and tab registration a completely independent profile (multiple copies can coexist).
- On startup **automatically open the last-opened memo** (`AppSettings.lastOpened`). If tab/memo is deleted, do nothing.

`[DESIGN §2.6]`

### 2.7 Window / Operation

- 3 panes (tab bar / tree / editor) + status bar. Splitter adjusts tree width, toggle left/right layout, hide. Keyboard shortcuts (user-configurable).
- Auto-launch to tray on Windows login (setting available).
- **Multi-launch prevention**: Second launch just focuses existing window (prevent simultaneous writes to same config file).
- **Always-on-top toggle**: the global 📌 button in the top toolbar (see §2.1) pins the app window above other windows via the OS window manager; session-only, not persisted.
- Suppress right-click outside registered tabs. Unwanted native items in title bar right-click resolved by `Menu.setApplicationMenu(null)` equivalent.
- **Tab bar empty area drag for window move**: **addressed in §9.1**.

`[DESIGN §2.7]`

### 2.8 Link Functionality

- **Auto-link recognition**: URLs (`http://` / `https://` / `www.` / `ftp://`), email addresses, file paths (relative paths converted to absolute using tab as base before opening).
- **Memo-to-memo links (`memo:` notation)**: `memo:MemoName` (current tab) / `memo:TabName/MemoName` / `memo:TabName/`. Unresolved links do nothing.
  - **Links to locked tabs**: `memo:` links pointing to memos in locked (no-session) tabs always treated as "unresolved" (§4.1). During unlock session, resolve normally.
- **Same-document heading anchor links (`#slug`)**: generated by the Table of Contents feature (§4.2). Distinct from `memo:` links and external links; resolution is scoped to the current memo only (re-scans its headings on click). No match = silent no-op.
- Operation: default `singleClick`. Confirmation dialog before external launch defaults to OFF. Use OS standard API, no shell string concatenation.

`[DESIGN §2.8]`

### 2.9 Password Protection (formerly: per-memo encryption)

> Per-memo password encryption not provided. Password protection **unified to tab-level lock**. Encryption scheme (AES-256-GCM + scrypt) carried forward from tab lock. See **§4.1 "Tab Lock" for detailed specification**.

---

## 3. Data Model

### 3.1 Basic Approach

- Memo body = `.md` (UTF-8). **Memo identification = filename** (no internal UUID introduced; see [DECISIONS.md](DECISIONS.md) for rationale). Display name and filename always identical.
- **Known constraint**: Rename = file rename, so `memo:` links break.
- **Filename sanitization**: Windows forbidden characters, control characters, reserved names detected and blocked at input/rename. Real-time validation display + replacement suggestions (e.g., `/` → `-` / `_`).
- Tab = folder, only direct-child `.md` files targeted, no duplicate names in same namespace.

`[DESIGN §3.1]`

### 3.2 Metadata

- Per-tab metadata file (`.tabtreenote.json`, hidden in folder) stores tree structure, order, expand/collapse, labels, editor settings. When locked, this `tree` is replaced by `lock` blob (§4.1).
- **Core robustness principle**: If metadata lost, memo bodies (`.md`) are never lost (recovery via flat rebuild on metadata loss, §5.9).
- Metadata JSON and app settings JSON also atomic writes + `.bak` targets (§5.2). Record `schemaVersion` + `appVersion`.
- Sync order between real files and metadata JSON: see §5.3.

`[DESIGN §3.2]`

### 3.3 Following External Changes

- File monitoring auto-detects and reflects folder content changes. Self-write exclusion, conflict detection, Windows lock handling: see §5.4 / §5.5 / §5.7.

`[DESIGN §3.3]`

### 3.4 Primary Type Definitions (`src/shared` equivalent)

```ts
type Result<T = void> =
  | { ok: true; value: T }
  | { ok: false; error: { code: ErrorCode; message: string; path?: string } };

type ErrorCode =
  | 'NAME_DUPLICATE' | 'NAME_INVALID' | 'NOT_FOUND' | 'IO_LOCKED'
  | 'TRASH_FAILED' | 'DECRYPT_FAILED' | 'CONFLICT' | 'META_CORRUPT'
  | 'LONG_PATH' | 'TAB_LOCKED' | 'UNKNOWN';   // TAB_LOCKED for tab lock (§4.1), naming confirmed at implementation

type Eol = 'crlf' | 'lf' | 'cr';

// 【Persistence-only】Actual elements serialized to metadata JSON
interface MetaTreeNode {
  memoName: string;
  expanded: boolean;
  label?: Label;
  editor?: MemoEditorSettings;
  locked?: boolean;        // Edit lock (body edit only prohibited. Distinct from tab lock, §2.2/§4.1)
  children: MetaTreeNode[];
}

// 【Runtime/UI-only】Return type of tab:openTree (metadata + real file reconciliation result)
interface TreeNode {
  readonly memoName: string;
  readonly expanded: boolean;
  readonly label?: Label;
  readonly editor?: MemoEditorSettings;
  readonly children: readonly TreeNode[];
}

interface TabMeta { schemaVersion: number; appVersion: string; tree: MetaTreeNode[]; }
// Locked tabs have lock field instead of tree (§4.1, type confirmed at detailed design phase)

interface AppSettings {
  schemaVersion: number; appVersion: string;
  tabs: { id: string; name: string; folderPath: string; locked?: boolean }[];  // locked is display cache for tab lock state (true source is the tab's own metadata file, §4.1)
  window: { x: number; y: number; width: number; height: number; maximized: boolean; treeWidth: number; treeSide: 'left' | 'right' };
  theme: 'light' | 'dark';
  uiFontScale: number;
  lastOpened: { tabId: string; memoName: string } | null;
  editorDefaults: { wrap: 'none' | 'chars' | 'window'; tabWidth: number; softTab: boolean; showLineNumbers: boolean; fontSize: number; viewMode: 'live' | 'source' };  // viewMode: default Markdown view mode a memo opens in (§4.2); per-memo override in MemoEditorSettings falls back here when absent
  autosave: { plainDebounceMs: number };
  links: { openTrigger: 'ctrlClick' | 'singleClick' | 'doubleClick'; confirmBeforeOpen: boolean };
  encoding: { defaultSave: 'utf-8' };
  eol: { policy: 'preserve' | 'crlf' | 'lf' | 'cr' };
  shortcuts: Record<string, string>;
}
```

`[Base: data-model.md. v3 node lock-specific fields and `.txtenc` related fields removed]`

---

## 4. TabTreeNoteMD-Specific Feature Specification (Lock Mechanism Redesign, Markdown Full Migration)

### 4.1 Lock Mechanism: Tab Lock (New)

| Legacy Feature | Status |
|---|---|
| Node lock (subtree-wise, legacy DESIGNv3 §1) | **Discontinued** |
| M6 per-memo encryption (`.txtenc`, legacy DESIGN §2.9) | **Discontinued** |
| Edit lock (`locked?: boolean`, no password required) | **Retained** (§2.2) |
| Tab lock | **New** |

**Specification**:
- Lock target is **tab (management folder) level**. Without unlock via password, **contents (tree, memos) completely hidden**, and **tab name itself hidden on tab bar** (shown as 🔒).
- **Locked tab cannot be deleted** (unlock required first).
- Password input UI supports hint display (optional hint registered at setup, displayed at unlock).
- Encryption scheme: **AES-256-GCM + scrypt**.

**Internal implementation method** (permanent session + filename obfuscation + encrypted blob approach. Since tab has no "unlocked sibling parts," nested locks and permanent unlock operations do not apply. Tab-to-tab memo movement: see §8 separately):

1. On lock, all `.md` files under tab are renamed to 128-bit random hex tokens (`crypto.randomBytes(16).toString('hex')`, regenerate on collision). File contents themselves individually encrypted with AES-256-GCM.
2. Original tree structure, real tab name, and "real memo name ↔ obfuscated token" correspondence are consolidated into one encrypted blob, **saved in that tab's own `.tabtreenote.json`** (in `lock` field instead of `tree` field). Portable with folder copy, preserving lock state (§6).
3. `AppSettings.tabs[i].name` replaces real name with placeholder (real name exists only in blob). `locked: boolean` is lightweight cache for tab bar rendering (true source: `lock` field in tab's `.tabtreenote.json`).
4. **Unlock (session start)**: Password input → blob decryption → retain real tab name, structure, name correspondence table in memory as "lock session". Edit content continuously re-encrypts correspondence table and structure blob (new IV) and atomically saves, re-encrypts each memo body individually and writes back.
5. **Session lifespan**: Immediately discarded when leaving tab (select another tab). Re-open requires password re-entry each time (reconsider if inconvenient).
6. **Full-text search**: Whether locked or in session, always exclude that tab's name and body from index (§2.4).
7. **Delete restriction**: `tab:remove` (unregister-only — never deletes/trashes the folder or its files, §2.1) rejected while locked (no session) with `TAB_LOCKED`. In session, treated as normal tab removal. To stop locking completely, simply don't re-set "tab lock" thereafter.
8. **Tab-to-tab memo movement (DND)**: This app's tab-to-tab DND works as "drag from active tab's tree, drop on another tab in tab bar," completing without leaving drag-source tab (no collision with rule 5's "discard session when leaving tab").
   - **Drop from other tab → locked (no-session) tab**: **Display password input dialog right at drop moment**; on success, immediately start session and take in memo (no need to leave drag-source tab). The "unlock tab first then drag" workflow cannot work because leaving drag-source tab at unlock completion would re-lock per rule 5.
   - **Drop from unlock-session tab → other (normal) tab**: **Allowed**. At drop moment, that memo decrypts, becomes normal plaintext `.md` in target tab (don't adopt legacy node lock's "one-way vault" style. Tab-level lock means no risk of protected/unprotected parts adjacent in same tree).

`[Legacy: docs/v3/DESIGNv3.md §1 / DESIGN.md §2.9 not adopted. Encryption scheme and technical pattern only reused]`

### 4.2 Markdown Editing (CommonMark Full Support)

- **All memos standardized to Markdown (`.md`)** (§1 Q2). Plain text option discontinued.
- **Edit experience = live view by default** (factory default; user-configurable global default and optional per-memo override, §3.4 `viewMode`). Toggle to source (raw Markdown) also available at any time (runtime toggle, independent of the default setting).
- **Markdown support range = CommonMark equivalent full support**: headings, lists, quotes, code blocks, tables, emphasis (bold/italic), horizontal lines, links, etc.
- **Toolbar = full-featured and floatable**: heading / list / quote / code block / table / emphasis / link / image insert etc. operated from toolbar. Free-position float panel within window.
- **Image paste and resize**:
  - Support paste from clipboard. Body **saved directly under tab's folder** (no subfolders).
  - **Filename generation rule**: `img_<crypto.randomBytes(8).toString('hex')>.<extension>` (e.g., `img_a3f9c1e2b7d4e6f1.png`). Regenerate on collision. Extension determined from actual format; clipboard-sourced raw bitmap defaults to `.png`.
  - **Resize result saved as `<img src="relative path" width=".." height="..">`** in HTML form (standard notation `![alt](url)` has no width/height slot. Unresized images remain as `![alt](url)`).
  - **Auto-cleanup of orphaned files not performed** (confirmed). Images persist after memo deletion or reference removal. Manual cleanup only (avoid data loss risk from mis-detecting shared reference).
- **Decoration notation (hybrid, confirmed)**: Bold/italic use standard Markdown syntax (`**bold**` / `*italic*`). Text color and size lack standard Markdown notation, so use `<span style="color:...;font-size:...">` via allowlist.
- **Safety**: Allowed tags/attributes by allowlist (`<img>` `src`/`width`/`height`, relative paths only. `<span>` `style` with `color`/`font-size` only). **Tags/attributes/CSS properties outside allowlist (including `<script>`, `on*` event attributes, `javascript:` scheme) must be removed during preview rendering**. Arbitrary CSS properties like `position` not allowed.
- **Markdown link notation coexistence (confirmed)**: Enable standard Markdown link syntax `[text](url)`. Treat `memo:` as URL scheme type, allowing labeled memo-to-memo links like `[heading](memo:TabName/MemoName)` (coexist with `memo:` direct-write notation in §2.8).
- **Table of Contents (TOC), confirmed new v1 feature**: Triggered manually by the toolbar's "Update" button only (never automatic/background). On click, scans the current memo's headings (`#`–`######`), generates a nested Markdown list of GitHub-style-slugified `[heading text](#slug)` links, and writes it into a marker-delimited block (e.g. HTML comments) — replacing an existing marker block's contents if one is found in the document, or inserting a new one at the cursor if not. The TOC block is ordinary saved Markdown text, no separate storage. Adds the `#slug` same-document heading-anchor link type (§2.8): clicking jumps to the matching heading in the current memo; no match is a silent no-op.
- **Full-text search**: Remove Markdown syntax/HTML tags for decoration/structure (plaintext conversion) before indexing.
- **Editor engine: Confirmed — CodeMirror 6 with a custom-built decoration extension** (see [DECISIONS.md](DECISIONS.md) for rationale and rejected alternatives). The plain-text buffer CodeMirror operates on is the source of truth; live view is a decoration layer over that same buffer, so switching between live view and raw source never converts or regenerates the text.

`[Legacy: docs/v3/DESIGNv3.md §2 not adopted (complete replacement)]`

---

## 5. Cross-Cutting Principles

### 5.1 `Result<T>` Type Convention and Error Codes

IPC unified with `Result<T>` (discriminated union). Exceptions caught at boundary and converted to error code. Type definition in §3.4.

### 5.2 Atomic Write Pattern

All persistence saves follow "① write to temp file → ② fsync → ③ rename old target to `.bak` (generation swap) → ④ rename temp to target" sequence.

- Scope: memo body (`.md`) / per-tab metadata JSON (includes lock blob when locked) / app settings JSON / obfuscated files during tab lock. Last generation only kept as `.bak`.

### 5.3 Sync Order Between Real Files and Metadata JSON

Operations affecting both (rename, move, copy, duplicate etc.): ① Update store (in-memory) → ② Real file operation + self-write reservation table registration → ③ Atomically save metadata JSON immediately after real file operation succeeds. If rare crash occurs in gap, metadata recovery cascade (§5.9) is safety net.

### 5.4 Self-Write Reservation Table

Mechanism protecting app's own FS operations from file monitoring event false positives. Temp files excluded by fixed prefix from monitoring. App-originating operations (atomic write, `.bak` generation, rename, trash/direct delete, DND move/copy/duplicate, tab lock filename obfuscation) registered in reservation table before execution; corresponding monitor events discarded as self-originating. Registration centralized in FS layer. Record lifespan = clear on match + TTL fallback (default 2–3 sec).

### 5.5 Two-Tier Conflict Detection

- Save API: **Hash comparison only** (one-shot judgment of disk real entity hash vs. `expectedHash` held by renderer).
- File monitoring side (self-fire detection, external change detection): **mtime + size + hash composite judgment**.

### 5.6 Hash Field Scope

All: "hash of entire real file on disk (byte sequence)."

### 5.7 Windows Lock Error Retry Policy

Write, rename, delete operations each have short retries (e.g. 3–5 times, 50–200ms backoff). Only if all fail display `IO_LOCKED` to user.

### 5.8 Error Handling Table

| Scenario | Policy |
|------|------|
| Windows lock (EBUSY/EPERM) | Short retry, display `IO_LOCKED` only if all fail |
| Trash failure | `TRASH_FAILED` → confirmation dialog → **direct delete only on user consent** |
| Metadata JSON corruption | Recovery cascade (`.bak` → flat rebuild → notify). Memo body never lost |
| App settings JSON corruption | Same recovery. Re-generate with defaults and notify if total loss |
| Decryption failure (tab lock) | `DECRYPT_FAILED` "Password incorrect". Return no contents |
| External edit conflict | Detect via mtime + size + hash. If editing: `evt:conflict`, auto-save/input lock during resolution dialog |
| Name duplicate/forbidden chars | Detect before file operation, block (`NAME_DUPLICATE` / `NAME_INVALID`). Never touch real files |
| Locked tab operation | Reject with `TAB_LOCKED` (§4.1) |
| Long path | `LONG_PATH` warning. No formal support |
| Unexpected exception | Always catch at boundary, convert to `{ok:false, code:'UNKNOWN'}`. Send renderer `code` / `message` / `path?` only; details logged internally in main process only |

### 5.9 Metadata Corruption Recovery Cascade

1. If last `.bak` exists, attempt recovery.
2. If not / fails, **rebuild metadata flat from folder's real files** (loses hierarchy, labels etc. but retains memo bodies).
3. Notify user "Metadata reconstructed."

### 5.10 Security Invariants and Internal Diagnostic Logging Policy

- Keys/passwords confined inside boundary. Decrypted plaintext never written to disk, entered in index, or internal logs.
- External launch via OS standard API only, no shell string concatenation.
- `Result` errors: send `code` / `message` / `path?` only externally. Details (OS error codes, exceptions, stacks) logged internally only (keys, passwords, decrypted plaintext never appear in internal logs).

### 5.11 Name Sanitization / Validation

Detect Windows forbidden characters, reserved names, leading/trailing spaces, trailing periods. Real-time validation display, replacement suggestions for `/` etc. Tab names must be unique, memo names unique within same tab's `.md` namespace. Long paths (260+ chars) trigger `LONG_PATH` warning.

`[Base: 00-core-principles.md]`

---

## 6. Non-Functional Requirements

- **Operating environment**: Windows 11 primary target.
- **Performance**: Snappy tree rendering and search even at hundreds to thousands of memos scale. Full-text search via lightweight in-memory index.
- **Reliability**: Memo bodies always held as independent files. No data loss even on metadata corruption. Atomic writes + last generation `.bak`.
- **Portability / Backup**: Folder copy provides backup (works even while tab locked, §4.1).
- **Known limitation**: Long paths (260+ chars) not formally supported.
- **Security**: AES-256-GCM + scrypt (tab lock, §4.1). Keys in session memory only. No external connectivity (local-only).

`[DESIGN §4]`

---

## 7. Implementation Rules to Inherit (Post-M7 Fixes)

- When folder absent, return empty array instead of throwing exception (resilience to tab folder deletion from outside).
- App settings JSON corruption recovery also notified to user via same path as metadata JSON corruption.
- Separate "strict validation at new naming time" and "loose validation to resolve existing entities" (allows handling existing files with leading/trailing spaces etc.).
- Do not use native blocking `window.confirm()` dialogs (destroys IME input state). Unify all dialogs to custom non-blocking components.
- Dialogs escapable by Esc key (except external change conflict dialog, intentionally Esc-disabled).

`[post-M7-changes.md "Bug Fixes"]`

**Unimplemented / deferred (reconsider on rebuild)**:
- **IME auto-enable on startup**: Conditional adoption. In early implementation phase, perform small validation (get HWND → call IMM32 → confirm reflection to WebView2 text input); adopt if works, defer if not (accept tradeoff with login auto-launch + tray-resident only).
- Portable exe launch slowness: Tauri adoption (§1.1) eliminates self-extract concept, expect resolution.
- Properties screen byte count by encoding / half-width/full-width breakdown display: defer.

---

## 8. Implementation Rules to Inherit (Post-M10 Fixes)

- If memo target/descendants being edited during tab lock, auto-save-confirm → close before proceeding to lock (§4.1).
- Markdown decoration implementation (overlapping range edits, decoration widget click select) is a high-risk defect zone. If taking CodeMirror extension self-built route, exercise special caution. Existing WYSIWYG libraries like Milkdown / TipTap likely have design preventing this class of defect (§4.2).

`[post-M10-changes.md, entire]`

---

## 9. Implementation Rules to Inherit (issue.md Delegated Response)

| Item | Status |
|---|---|
| Settings screen: checkbox/dropdown vertical margins, general/editor 2-tab layout, Esc support | Implemented |
| Right pane: memo-wise scroll position memory, suppress right-click on filename/encoding bar | Implemented |
| Top tab bar: suppress right-click outside registered tabs | Implemented |
| Left pane: click empty area to select, right-click menu not cut off at screen bottom, "Create as Child" immediate rename | Implemented |
| Title bar right-click: no unwanted native defaults, delete confirm screen long name abbreviation/Esc, image paste Markdown-only | Implemented |
| Unified right-click menu look, tab bar empty drag for window move, Delete key confirm | **Addressed in §9.1 (#7, #10, #13)** |

`[issue.md, entire]`

### 9.1 Legacy Issues #7, #10, #13 Resolution Plan

1. **#7 Frameless window: Adopt**. `decorations: false` + `data-tauri-drag-region` attribute realizes tab bar empty drag move. Minimize/maximize/close buttons self-implemented. Windows 11 maximize button hover snap layout flyout not shown by default. Edge snap still works.
2. **#10 Unified right-click menu: Adopt**. Unify tree, editor, tab bar to single custom context menu.
3. **#13 Delete key collision: Add dedicated shortcut, retain existing UX**. "Click-open auto-focuses editor" behavior unchanged. New dedicated shortcut (e.g. `Ctrl+Delete`, exact key confirmed at implementation) works regardless of focus, deletes selected tree item. Plain Delete remains context-dependent (right-click delete also continues available).

---

## 10. Out of Scope / Future Consideration

Equation calculation, templates, external tool execution, line operations (sort/shuffle etc.), vertical text, elaborate skins/themes, global hotkey / autoclip, cloud sync, CatMemoNote system.ini import, Windows long-path support, recursive subdirectory scan / symlink follow, macOS/Linux formal support.

Checkbox (☑/☐ toggle) already implemented (§2.3).

`[DESIGN §5]`
