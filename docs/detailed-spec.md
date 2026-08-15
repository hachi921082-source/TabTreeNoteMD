# TabTreeNoteMD Detailed Specification (detailed-spec.md)

> Elaboration of [spec.md](spec.md) to a level usable by `developer-lite` when writing per-feature
> implementation plans (`docs/plans/<feature-name>/PLAN.md`). This document does not introduce new
> product decisions; it translates and refines what `spec.md` already confirms. Where `spec.md`
> explicitly leaves something open and the resolution requires the user's own judgment, that item is
> **not** decided here — it is logged in [QA.md](QA.md) instead (see §10).
>
> Every section below cites the `spec.md` section it elaborates as `[spec §x]`. Read `spec.md` first;
> this document assumes familiarity with it and does not repeat rationale already covered in
> [DECISIONS.md](DECISIONS.md).

---

## Index

Keep this in sync whenever a section is added, renamed, or moved below.

- **§0 How to Use This Document** — relationship to `spec.md`; what's a "starting default" vs. binding.
- **§1 Terminology / Glossary** — Tab/Memo/Tree/Edit Lock/Tab Lock/Lock Session/Self-Write Registry/Metadata JSON/App Settings JSON definitions.
- **§2 Application Structure Overview** — Tauri backend/frontend split, `Result<T>` boundary, backend→frontend events overview.
- **§3 Data Model — Detailed**
  - §3.1 On-Disk Layout — folder-tree diagram (tab folder contents, app-adjacent config folder).
  - §3.2 App Settings JSON — Full Schema — concrete JSON example; `toolbarPosition`/`toolbarOrder`; `tabs[i].locked` cache semantics.
  - §3.3 Tab Metadata JSON — Unlocked Tab — concrete JSON example; `label.color` 5-choice palette; `scrollPosition`; `viewMode` fallback.
  - §3.4 Tab Lock Blob — Locked Tab — lock-blob JSON shape, decrypted payload shape, token/IV/salt rules, async-scrypt requirement.
  - §3.5 Runtime Tree vs Persisted Tree — reconciliation rules on tab open (dropped/appended entries, flat-rebuild reuse).
  - §3.6 Image Paste Storage — filename rule, no-orphan-cleanup policy.
  - §3.7 Constants Inventory — table of concrete numeric defaults (retry counts, TTL, scrypt params, etc.).
- **§4 Backend Operation Inventory** — logical command list by domain: §4.1 Tab, §4.2 Tree/memo, §4.3 Editor/save, §4.4 Search, §4.5 Settings, §4.6 Link, §4.7 Image.
- **§5 Feature Behavior Details** — elaboration per feature: §5.1 Tab Management, §5.2 Memo Tree (incl.
  tree guide lines), §5.3 Text Editing, §5.4 Search/Index, §5.5 Appearance/Theme, §5.6 Persistence/Save Triggers, §5.7 Window/Operation, §5.8 Link Functionality, §5.9 Tab Lock End-to-End Flows (locking/crash-recovery/unlocking/cross-tab DND; permanent unlock and its own crash recovery are now in `docs/tab-lock-detailed-spec.md`), §5.10 Markdown Editing (18-slot toolbar, icon set, sanitization allowlist, TOC; the split-pane preview, PDF export, and live-view real-image-rendering details are now in `docs/MDEditor-spec.md`).
- **§6 Cross-Cutting Mechanism Details** — §6.1 Atomic Write concrete sequence, §6.2 Self-Write Registry, §6.3 Sync Order, §6.4 Conflict Detection & Corruption Recovery, §6.5 other Post-M7 rules (folder-not-found, validation strictness, no native dialogs, Esc rule).
- **§7 Error Handling — Expanded** — table of every `ErrorCode`: trigger scenario + expected UI response.
- **§8 Non-Functional Elaboration** — performance qualitative bar, backup/portability acceptance check.
- **§9 Deferred-to-Implementation Details** — explicit list of `developer-lite`-discretion items (not ambiguities).
- **§10 Open Questions Requiring User Decision** — currently empty; resolved-item history.

---

## 0. How to Use This Document

- `spec.md` = authoritative "what is decided." `detailed-spec.md` (this file) = "how the decided
  behavior plays out precisely enough to plan and build it." Neither file dictates Rust module layout,
  crate choice, or Tauri command naming convention — that belongs to `developer-lite`'s
  `PLAN.md` for each feature.
- Anywhere this document proposes a concrete numeric value, file format, or default inside a range
  that `spec.md` left as "e.g." or otherwise flexible, treat it as a **starting default**:
  `developer-lite` may adjust it during implementation planning if a good reason emerges, as long as
  the observable behavior and guarantees described in `spec.md` are preserved.
- §9 explicitly enumerates items intentionally left to `developer-lite`'s discretion (exact shortcut
  keys, exact regex patterns, etc.) so that these are not mistaken for gaps requiring escalation.
- §10 lists items that genuinely need the user's decision; these are also recorded in `QA.md`.

---

## 1. Terminology / Glossary

| Term | Meaning |
|---|---|
| Tab | One top-level entry in the tab bar; backed 1:1 by one OS folder ("management folder"). |
| Memo | One `.md` file directly under a tab's folder. Identified by filename (no UUID, `[spec §3.1]`). |
| Tree | The parent/child hierarchy of memos within one tab, persisted in that tab's metadata JSON. |
| Edit Lock | Per-memo flag (`locked?: boolean`) that blocks body editing only. No password. `[spec §2.2]` |
| Tab Lock | Whole-tab password protection (encryption + name/content hiding). Distinct from Edit Lock. `[spec §4.1]` |
| Lock Session | The in-memory decrypted state of a tab after a successful Tab Lock password entry. Discarded on leaving the tab. `[spec §4.1 pt.5]` |
| Permanent Unlock | An explicit, irreversible action that fully decrypts a locked tab back into an ordinary unencrypted tab (distinct from a Lock Session, which is temporary and automatically discarded). `[spec §4.1 pt.9]`, full flow in `docs/tab-lock-detailed-spec.md` |
| Self-Write Registry | Table used to suppress false "external change" events caused by the app's own file writes. `[spec §5.4]` |
| Metadata JSON | `.tabtreenote.json`, one per tab folder, hidden. Holds tree/order/labels/editor settings, or the lock blob when locked. `[spec §3.2]` |
| App Settings JSON | Single file holding cross-tab settings (window state, theme, tab registry, etc.), stored portably next to the executable. `[spec §2.6]` |

---

## 2. Application Structure Overview (Restated, No New Decisions)

- Tauri app: Rust backend (filesystem, crypto, file watching, encoding/EOL handling, search index) +
  WebView2-hosted frontend (3-pane UI: tab bar / tree / editor + status bar). `[spec §1 Q1, §2.7]`
- All persistence-affecting operations happen in the backend; the frontend calls backend operations
  and receives `Result<T>`-shaped responses (`[spec §3.4, §5.1]`). §4 below inventories the operations
  needed to cover every feature in `spec.md`, expressed as logical operation names — final Rust/Tauri
  naming convention (e.g. `snake_case` commands) is a `PLAN.md`-level decision.
- Backend → frontend push notifications (equivalent to old repo's `webContents.send` events) are needed
  at minimum for: external file-change reconciliation, save conflicts, metadata-recovery notices, and
  tray/second-instance focus requests. Exact event names are a `PLAN.md`-level decision.

---

## 3. Data Model — Detailed

### 3.1 On-Disk Layout

```
<tab folder>/                     one folder per tab, path stored in App Settings JSON
├── .tabtreenote.json             hidden metadata file (tree, or lock blob when locked)
├── .tabtreenote.json.bak         last-generation backup (§6.1)
├── Shopping list.md              a memo (unlocked tab: real filename = display name)
├── img_a3f9c1e2b7d4e6f1.png      pasted image (§3.6)
└── ... (other .md files / images, direct children only, no recursion)

<app-adjacent folder>/            e.g. "config" or "data" next to the .exe (portable, [spec §2.6])
├── settings.json                 App Settings JSON
└── settings.json.bak
```

- Only `.md` files directly under a tab folder are scanned as memos; subfolders, symlinks, and
  junctions are ignored (`[spec §2.2]`). Non-`.md` files (e.g. pasted images) are ignored by the tree
  scanner but remain on disk.
- A locked tab's `.md` files do not exist as `.md` on disk while locked — see §3.4 and §5.9.

### 3.2 App Settings JSON — Full Schema

Elaborates the `AppSettings` interface already given in `[spec §3.4]`. Concrete example:

```jsonc
{
  "schemaVersion": 1,
  "appVersion": "0.1.0",
  "tabs": [
    { "id": "t1", "name": "Work", "folderPath": "C:\\notes\\work" },
    { "id": "t2", "name": "🔒", "folderPath": "C:\\notes\\private", "locked": true }
  ],
  // Tab order = array order (single source of truth, no separate "order" field).
  "window": { "x": 100, "y": 100, "width": 1200, "height": 800, "maximized": false, "treeWidth": 260, "treeSide": "left", "toolbarPosition": { "x": 480, "y": 120 }, "toolbarOrder": ["undo","color","heading","list","bold","strikethrough","link","tocUpdate","clearFormatting","redo","fontSize","codeBlock","quote","italic","table","image","hardBreak","rearrangeToggle"] },
  "theme": "dark",
  "uiFontScale": 1.0,
  "lastOpened": { "tabId": "t1", "memoName": "Shopping list" },
  "editorDefaults": { "wrap": "window", "tabWidth": 4, "showLineNumbers": true, "fontSize": 14 },
  "autosave": { "plainDebounceMs": 1500 },
  "links": { "openTrigger": "singleClick", "confirmBeforeOpen": false },
  "encoding": { "defaultSave": "utf-8" },
  "eol": { "policy": "preserve" },
  "shortcuts": { }
}
```

- `tabs[i].locked` is a **display cache only**; the authoritative lock state is "does this tab's
  `.tabtreenote.json` have a `lock` field instead of a `tree` field" (§3.4). If the cache and the real
  file ever disagree (e.g. after an external folder copy), the real file wins; the cache is corrected
  on next read.
- `tabs[i].name` for a locked tab holds a placeholder string (not the real name). **Confirmed**: the
  placeholder is the 🔒 glyph alone, with no accompanying text (e.g. not "🔒 (locked)") — decided by
  the user; see `DECISIONS.md` ("Tab-Lock Placeholder Display Name: 🔒 Glyph Alone").
- Partial updates (settings dialog saving one field) use a deep-partial patch shape
  (`AppSettingsPatch`, mirroring old repo's `data-model.md` §2 pattern) merged into the stored object,
  then atomically saved (§6.1).
- `window.toolbarPosition` (`{ x: number; y: number }`, window-relative coordinates) persists the
  Markdown-editing toolbar's free-floating drag position across app restarts (`[spec §4.2]`'s
  "Free-position float panel"; persistence and `window`-vs-`MemoEditorSettings` placement confirmed
  per `DECISIONS.md` "UI-State Persistence Decisions"). Absent/`null` on first launch means the toolbar
  uses its implementation-chosen default position.
- `window.toolbarOrder` (18-element array of action-ID strings) persists the result of rearrange-mode
  reordering (§5.10) across app restarts (`DECISIONS.md` "Markdown Toolbar: Expanded to 18-Slot Grid").
  The **set** of 18 IDs is fixed (one entry per action in the §5.10 grid, no add/remove); array order is
  slot order, read left-to-right then top-to-bottom (row 1 slots 1–9, then row 2 slots 1–9). Default
  value on first launch is the finalized default layout shown above. Exact ID spellings: §9.
- `editorDefaults` has no `softTab` or `viewMode` field (removed, `DECISIONS.md` "Remove Unused Editor
  Default Settings: Display Mode Selector and Soft-Tab Checkbox", GitHub issue #61). The Settings →
  Editor dialog no longer shows a "display mode" selector or an "insert tab as spaces" checkbox.
  - Markdown view mode a memo opens in: fixed to `'live'`, not user-configurable. The per-memo
    `MemoEditorSettings.viewMode` override (§3.3) is unaffected and still falls back to this fixed
    `'live'` default when absent — see §5.10.
  - Tab key: always inserts a literal tab character (`\t`); the soft-tab (spaces) behavior and its
    per-memo `MemoEditorSettings.softTab` override field are both removed — **behavior change**: prior
    factory default was `softTab: true` (spaces).

### 3.3 Tab Metadata JSON — Unlocked Tab

```jsonc
{
  "schemaVersion": 1,
  "appVersion": "0.1.0",
  "tree": [
    {
      "memoName": "Shopping list",
      "expanded": true,
      "label": { "color": "#4a90d9", "kind": null },
      "editor": { "wrap": "window", "tabWidth": null, "showLineNumbers": null, "fontSize": null, "viewMode": null, "scrollPosition": 340 },
      "locked": false,
      "children": []
    }
  ]
}
```

- `memoName` excludes the `.md` extension; the real on-disk file is `<memoName>.md`.
- `MemoEditorSettings` fields are all optional/nullable: `null` or absent means "follow
  `AppSettings.editorDefaults`" (`[spec §3.4]`). This includes `viewMode` (`'live' | 'source' | null`):
  a memo with `viewMode: null`/absent opens in the fixed `'live'` default (§3.2); a memo with an
  explicit `viewMode` always opens in that mode regardless of the default. Confirmed engine choice:
  CodeMirror 6 (`[spec §4.2]`, `DECISIONS.md`) — see §5.10.
- `MemoEditorSettings` has no `softTab` field (removed along with `AppSettings.editorDefaults.softTab`,
  §3.2, GitHub issue #61); Tab-key insertion is fixed at the editor-engine level, not per-memo.
- `editor.scrollPosition` (integer, **1-indexed document line number** of the line visible at the top
  of the viewport at the time of saving — **not** a raw pixel/scrollTop value, confirmed per
  `DECISIONS.md` "Scroll Position: Document-Position-Based Anchor, Not Raw Pixel Offset") **persists
  this memo's scroll position across app restarts**, decided by the user (see `DECISIONS.md`,
  "UI-State Persistence Decisions"; supersedes the earlier session-only-by-default framing in §5.3
  below). `null`/absent means "no remembered position yet" (e.g. a never-opened or freshly-created
  memo), and the editor opens at the top as usual.
- **`label.color` — confirmed concrete 5-choice palette, adopted verbatim from the legacy app**, set
  via the right-click "Icon" submenu confirmed in `[spec §2.2]` (this submenu **is** the `label.color`
  picker — see the Icon-submenu bullet in §5.2 below, and `DECISIONS.md` "Icon Submenu Clarified as the
  `label.color` Picker..." for the investigation that established this). Values:

  | Choice | `label.color` value |
  |---|---|
  | None | `null` |
  | Red | `"#e05a5a"` |
  | Blue | `"#4a90d9"` |
  | Green | `"#4caf50"` |
  | Yellow | `"#e0c040"` |

  No other values are valid; this is a closed enum, not a free-form color field.
- `label.kind` **remains reserved and unimplemented for v1** (`null` only, `DECISIONS.md` "Icon Submenu
  Clarified as the `label.color` Picker..."). The Icon submenu above sets `label.color` only, never
  `label.kind`.

### 3.4 Tab Lock Blob — Locked Tab (elaborates `[spec §3.4 line "type confirmed at detailed design phase"]` and `[spec §4.1]`)

When a tab is locked, its `.tabtreenote.json` has this shape instead of the unlocked shape in §3.3:

```jsonc
{
  "schemaVersion": 1,
  "appVersion": "0.1.0",
  "lock": {
    "formatVersion": 1,
    "kdf": { "algo": "scrypt", "N": 32768, "r": 8, "p": 1, "keyLen": 32, "saltB64": "..." },
    "cipher": { "algo": "aes-256-gcm", "ivB64": "...(12 bytes, fresh every save)", "tagB64": "..." },
    "hint": "optional, plaintext, shown before password entry",
    "ciphertextB64": "..."
  }
}
```

- `hint` and the `kdf`/`cipher` parameters are **plaintext fields outside the ciphertext** — a hint must
  be readable before the password is entered. Mirrors the legacy `.txtenc` container pattern
  (`DECISIONS.md`).
- `ciphertextB64` decrypts (AES-256-GCM, key = scrypt(password, salt, N/r/p/keyLen)) to a JSON payload
  with this logical shape (the "lock payload"):

```jsonc
{
  "realTabName": "Private",
  "tree": [ /* MetaTreeNode[], same shape as §3.3's "tree", using REAL memoName values */ ],
  "fileMap": [
    { "memoName": "Diary 2026-07", "token": "9f2c7a1e4b3d0f8c6a5e2d1b0c9a8f7e" }
  ]
}
```

- `token` is the on-disk filename of the corresponding obfuscated file, with **no extension** — an
  extension would leak that the original file was a `.md` text memo to anyone browsing the folder
  outside the app. Tokens are 128-bit random hex (`[spec §4.1 pt.1]`), regenerated on collision.
- Each obfuscated memo file's **content** is independently encrypted (AES-256-GCM, same
  password-derived key, a fresh random 12-byte IV per save). The exact on-disk byte layout of an
  encrypted memo body (e.g. `[IV][ciphertext][GCM tag]`, with or without a small format-version
  header) is left for `developer-lite` to finalize during `PLAN.md` creation — the only spec-level
  requirements are: IV never reused, key never derived twice for the same save, and plaintext never
  touches disk (`[spec §5.10]`).
- Salt is stable for the life of a given password; regenerated only on password change. IV is always
  fresh per write, for both the lock blob and each memo body (`[spec §4.1]`, mirrors legacy `.txtenc`
  rule in `DECISIONS.md`).
- KDF cost parameters (`N`/`r`/`p`/`keyLen`) are read from the file at decrypt time (not hardcoded),
  so future parameter increases don't break old locked tabs. `N=32768, r=8, p=1, keyLen=32` is the
  default for newly-created locks (~32MB memory cost), matching the legacy scheme reused here.
- Key derivation must use the asynchronous scrypt API (not a blocking/sync call) so the UI thread /
  Tauri command dispatch is never frozen during unlock.

### 3.5 Runtime Tree vs Persisted Tree (Reconciliation on Tab Open)

Elaborates the `MetaTreeNode` (persistence) vs `TreeNode` (runtime) split already declared in
`[spec §3.4]`.

When a tab is opened (or reopened after external changes are detected), the backend reconciles:

1. **(a)** the persisted tree in `.tabtreenote.json` (`MetaTreeNode[]`), against
2. **(b)** the real `.md` files found by a fresh scan of the tab folder's direct children.

Reconciliation rules (consistent with the "memo body is never lost" principle, `[spec §3.2, §5.9]`):

- A metadata entry whose file no longer exists on disk is **dropped** from the runtime tree (it is not
  re-created, and it is not kept as a "missing" placeholder).
- A real `.md` file with no corresponding metadata entry (new/externally-added file) is **appended to
  the end of the root level** of the runtime tree, with default settings (`expanded: true`, no label,
  no per-memo editor override, `locked: false`).
- Existing metadata entries that do match a real file keep their hierarchy position, label, expand
  state, and per-memo editor settings.
- The reconciled result is the runtime `TreeNode[]` handed to the frontend. If the reconciliation
  changed anything relative to the last-saved metadata (dropped/appended entries), the corrected
  `MetaTreeNode[]` is scheduled for the next metadata save (does not require an immediate forced write,
  but must not be silently lost — next mutating operation or periodic autosave should pick it up).
- This same reconciliation logic is what implements the corruption-recovery "flat rebuild" step in
  `[spec §5.9]`: when metadata is unreadable, treat step (a) as empty, so every real file lands at the
  end of the root level.

### 3.6 Image Paste Storage

Elaborates `[spec §4.2]`.

- Target folder: the tab's own folder (no subfolder), same level as `.md` files.
- Filename: `img_<16 lowercase hex chars>.<ext>`, generated via a cryptographically random 8-byte
  value formatted as hex. Regenerate on filename collision.
- Extension: derived from the actual pasted image format (e.g. `png`, `jpg`, `gif`, `webp`). Clipboard
  data that is a raw bitmap with no format info defaults to `.png`.
- These files are never scanned as memos (only `.md` is scanned per §3.1), so they do not appear in
  the tree, tab-lock obfuscation (§5.9), or full-text search indexing.
- No automatic cleanup of orphaned images occurs, ever (`[spec §4.2]`, confirmed — do not add this
  later without a spec change).

### 3.7 Constants Inventory

Concrete defaults for values `spec.md` expressed as ranges (`developer-lite` may tune within the
spirit of the range if empirically justified):

| Constant | Default | Source range in spec.md |
|---|---|---|
| Windows I/O retry count | 4 attempts | "3–5 times" `[spec §5.7]` |
| Windows I/O retry backoff | 50ms, 100ms, 200ms, 200ms (per attempt) | "50–200ms backoff" `[spec §5.7]` |
| Self-write registry TTL fallback | 2.5s | "2–3 sec" `[spec §5.4]` |
| Scrypt cost params (new locks) | N=32768, r=8, p=1, keyLen=32 | carried from legacy scheme, `DECISIONS.md` |
| Obfuscated token length | 16 bytes → 32 hex chars | `[spec §4.1]` ("128-bit") |
| Image random suffix length | 8 bytes → 16 hex chars | `[spec §4.2]` |
| `.bak` generations kept | 1 (last only) | `[spec §5.2]` |
| Long-path warning threshold | 260 characters (full path) | `[spec §5.11]` |

Other required constants with no numeric spec, to be fixed by `developer-lite` and recorded in
`PLAN.md` (not blocking; §9 confirms these are implementation-discretion items): temp-file prefix
pattern, `schemaVersion` starting value (`1`), UI shortcut keymap defaults.

---

## 4. Backend Operation Inventory

Logical operations needed to cover every feature below. Grouped by domain; each entry lists purpose,
key inputs, and the `Result<T>`/`ErrorCode`s from `[spec §3.4]` it can surface. This is not a Rust API
signature list — naming convention and exact parameter types are `PLAN.md`-level.

### 4.1 Tab domain
- **List tabs** — returns ordered `TabInfo`-like list (id, display name or lock placeholder, locked
  flag) for the tab bar.
- **Create tab (folder picker)** — opens OS folder-selection dialog (`[spec §2.1]`, never
  `window.prompt`), validates name uniqueness, appends to the end of the tab list, initializes
  `.tabtreenote.json` if absent. Errors: `NAME_DUPLICATE`.
- **Register existing folder as tab** — same as create, but folder already contains memos; existing
  `.tabtreenote.json` (if present) is reconciled per §3.5, otherwise created fresh from a folder scan.
- **Rename tab** — updates `AppSettings.tabs[i].name`. Errors: `NAME_DUPLICATE`, `NAME_INVALID`.
- **Reorder tabs** — replaces `AppSettings.tabs` array order.
- **Remove tab** — moves folder registration out of `AppSettings.tabs`. Does **not** delete or trash the
  folder or its files — unregister-only (`DECISIONS.md` "Tab Deletion Semantics: Unregister-Only, No
  Folder-Trash Option"). Rejected with `TAB_LOCKED` if the tab is currently locked and has no active
  session (`[spec §4.1 pt.7]`).
- **Toggle window always-on-top** — global, session-only UI state (📌), not persisted. Pins the entire
  app window above other windows via the OS window manager; not a per-tab or tab-position effect. One
  shared button (not one per tab), placed to the left of the search button (`[spec §2.1]`).
- **Open tab folder location** — OS "reveal in Explorer," standard API only.
- **Lock tab** / **Unlock tab (start session)** / **End session (leave tab)** — see §5.9 for the full
  flow.

### 4.2 Tree / memo domain
- **Open tree** — performs §3.5 reconciliation, returns `TreeSnapshot`-like read-only structure.
- **Create memo** / **create as child** — name-input dialog shown immediately on menu selection
  (`[spec §2.2]`), append-to-end or append-as-child positioning, no auto-naming/auto-open.
- **Rename memo** (also F2 shortcut) — sync order per §6.3. Errors: `NAME_DUPLICATE`, `NAME_INVALID`.
- **Delete memo** — trash by default; direct delete only after `TRASH_FAILED` + user consent
  (`[spec §5.8]`).
- **Move / reorder / re-parent memo** (DND within tab) — sync order per §6.3.
- **Move / copy memo between tabs** (DND across tabs, Ctrl = copy) — see §5.9 for the locked-tab
  interaction; unlocked-to-unlocked case is a plain file move/copy + tree update in both tabs' metadata.
- **Toggle expand/collapse**, **set label**, **set per-memo editor override**, **toggle edit lock**
  (`locked?: boolean`) — metadata-only mutations, no file operation.
- **Get memo properties** — file info + character count (`[spec §2.2]`).

### 4.3 Editor / save domain
- **Load memo body** — returns content (normalized to LF for the editor, per `[spec §2.3]`), detected
  encoding, detected EOL, and a hash of the on-disk byte sequence (for later conflict check).
- **Save memo body** — hash-only conflict check against the `expectedHash` the frontend last received
  (`[spec §5.5]`); on mismatch returns `CONFLICT` without writing. On success, restores the original
  EOL style (unless the user's EOL policy setting overrides), writes atomically (§6.1), updates the
  full-text index (§5.4), and returns the new hash.
- **Autosave** — same as Save, triggered by a debounce timer (`AppSettings.autosave.plainDebounceMs`)
  and by tab switch / memo switch / minimize / exit (`[spec §2.6]`); skipped if content is unchanged.
- **Text conversion operations** (uppercase/lowercase/hiragana/katakana/half-width/full-width,
  tab↔space) and **checkbox cycle** — pure in-editor text transforms, no separate backend call
  strictly required (may be frontend-only), but must feed into the same undo history as manual edits.
- **Generate/update Table of Contents** (§5.10) — pure in-editor text transform triggered by the
  toolbar's "Update" button: scan headings, slugify, insert-or-replace the marker-delimited TOC block
  as a single transaction. Same "no separate backend call strictly required, must feed into undo
  history" treatment as the text-conversion operations above.

### 4.4 Search domain
- **Full-text search query** — searches the in-memory index built at startup (§5.4), excluding any
  tab that is locked or in an unlock session (`[spec §2.4, §4.1 pt.6]`).
- **In-memo search/replace** — frontend-editor-local operation (regex, case, word-boundary options),
  no backend call needed beyond the already-loaded memo content.
- **Index update (in-app path)** — invoked after any save/rename/delete/move that changes indexable
  content, directly from the operation that performed the change (no separate scan).
- **Index update (external-change path)** — invoked from the file-watcher pipeline (§6.2) when it
  detects a genuine external change to a `.md` file in an unlocked, non-session tab.

### 4.5 Settings domain
- **Get settings** / **Set settings (patch)** — deep-partial merge into `AppSettings`, atomic save.
- **Get/restore window state**, **get/set theme**, **get/set shortcuts**.

### 4.6 Link domain
- **Resolve `memo:` link** — parses `memo:MemoName` / `memo:TabName/MemoName` / `memo:TabName/`
  against currently registered tabs; returns "not found" (no-op) if the target tab is locked and has
  no active session (`[spec §2.8, §4.1]`).
- **Open external link** (URL / email / file path) — OS-standard API only, optional confirm dialog
  gated by `AppSettings.links.confirmBeforeOpen`.
- **Resolve `#slug` heading-anchor link** (TOC feature, §5.10) — re-scans the *current* memo's headings
  for a slug match at click time (not cached from the last "Update" run); no match is a silent no-op,
  nothing surfaced to the user.

### 4.7 Image domain
- **Save pasted image** — writes file per §3.6, returns relative path + dimensions for the editor to
  insert either `![alt](url)` or `<img src=".." width=".." height="..">` (resized case).

---

## 5. Feature Behavior Details

### 5.1 Tab Management — Elaboration

- Tab name uniqueness check is applied after trimming, per `[spec §5.11]`'s general name-sanitization
  rule ("Tab names must be unique"). Case-sensitivity of the comparison is **not** specified by
  `spec.md`. **Starting default** (§0): case-insensitive; `developer-lite` may implement it as
  case-sensitive instead if simpler.
- "Create new" and "register existing folder" both go through the OS folder-picker; the two differ
  only in whether an existing `.tabtreenote.json` is expected. If a picked folder already has one, it
  is reconciled (§3.5) rather than overwritten.
- Drag-reorder of tabs only changes `AppSettings.tabs` array order; no per-tab file is touched.
- **Mouse-wheel tab switching removed from v1 scope** (user decision, `[spec §2.1]` updated
  accordingly; see `DECISIONS.md` "Mouse-Wheel Tab Switching: Removed from v1 Scope"). Click and
  keyboard-shortcut switching are unaffected.

### 5.2 Memo Tree (Left Pane) — Elaboration

- Multi-select (Ctrl/Shift) applies to: delete, move (DND), and cross-tab move/copy. Bulk rename is
  out of scope (only single-selection F2 rename is specified, `[spec §2.2]`).
- Arrow-key navigation (↑/↓) changes the tree selection and updates the open memo, but must not steal
  focus from the editor if the editor currently has it (`[spec §2.2]`) — i.e. arrow keys are only
  intercepted by the tree when the tree (not the editor) has keyboard focus.
- When the open memo changes by any means (click, arrow-key nav, `memo:` link navigation), the tree
  selection highlight and scroll position must follow to keep the newly-open memo visible.
- Right-click context menu positioning: flips upward when it would otherwise overflow the bottom of
  the screen (`[spec §2.2]`); this same custom menu component is shared with the editor and tab bar
  per `[spec §9.1 item 2]`.
- **"Icon" submenu** (`[spec §2.2]`, right-click menu) **is the `label.color` picker**: presents the
  5-choice palette confirmed in §3.3 above (None / Red / Blue / Green / Yellow), applying the selected
  value to the memo's `label.color`. It does not set `label.kind` (which stays reserved/unimplemented
  for v1, §3.3) — see `DECISIONS.md` for how this connection was established.
- Dedicated force-delete shortcut (`[spec §9.1 item 3]`): must work regardless of current focus target
  and must not depend on the tree row having focus. **Confirmed key binding: `Ctrl+Delete`** (no longer
  just an example — see `DECISIONS.md` "Force-Delete Shortcut Key: Confirmed as `Ctrl+Delete`").
- **Edit-lock menu item** (`[spec §2.2]`, right-click menu; GitHub issue #64): a single two-state item
  toggling the target memo's `locked` flag (§3.3/§4.2 "toggle edit lock"), same single-target-only
  pattern as "名前を変更" / "子として作成" / "プロパティ" above — **disabled when the current selection
  is not a single item**, since a mixed multi-selection (some locked, some not) has no unambiguous
  single label/action.
  - Target memo currently unlocked → label "編集ロック" → sets `locked: true`.
  - Target memo currently locked → label "編集ロック解除" → sets `locked: false`.
- **Tree guide lines** (indent connector lines; GitHub issue #60): each row renders one guide-line
  segment per ancestor depth level, positioned before the existing expand/collapse toggle button
  (replacing the current plain `paddingLeft: depth * 16 + 6` indent with per-depth segments).
  - For an ancestor depth where that ancestor still has more siblings below it, the segment shows a
    continuing vertical line; otherwise the segment is blank (that ancestor was itself a last child, so
    its line already terminated above).
  - The row's own connector (at its own depth) renders as a branch shape: `├─`-style if the row has
    more siblings following it, `└─`-style (corner only, no continuation below) if it is the last child.
  - Presentational only — no new interaction, state, or data-model field; purely a rendering change to
    the existing `TreeRow` depth-based layout.
  - Line color: existing theme token `--ttn-border`.

### 5.3 Text Editing (Right Pane) — Elaboration

- **Tab key**: always inserts a literal tab character (`\t`); fixed, not user-configurable (no
  Settings → Editor "insert tab as spaces" checkbox, `AppSettings.editorDefaults` has no `softTab`
  field, GitHub issue #61). Independent of the right-click "Convert" tab↔space transform below, which
  is an explicit on-demand text conversion, not a Tab-key input mode.
- **Encoding detection order on load**: (1) check for a UTF-8 BOM; if present, decode as UTF-8 and
  strip the BOM. (2) If no BOM, attempt strict UTF-8 decode; if the byte sequence is invalid UTF-8,
  retry decoding as Shift_JIS (`[spec §2.3]`). If both fail, treat as a hard read error (surface via a
  generic error rather than silently mangling text — exact error code is `developer-lite`'s call,
  reasonable to reuse `UNKNOWN` or `IO_LOCKED`-adjacent handling since this scenario is not explicitly
  enumerated in `[spec §3.4]`'s `ErrorCode` list).
- **EOL handling**: detect the dominant line-ending style present in the loaded file (`crlf` / `lf` /
  `cr`); if the file is empty or has no line breaks, default to the platform default (`crlf` on
  Windows). The editor engine operates on LF-normalized text internally; on save, the original style is
  reapplied unless `AppSettings.eol.policy` requests unification to a specific style.
- **Right-click "Convert" operations**: uppercase, lowercase, hiragana, katakana, half-width,
  full-width, tab↔space — apply to the current selection; if no selection, apply to the current line
  (consistent with the general "line-wise operations" principle in `[spec §2.3]`).
- **Checkbox click toggle**: clicking directly on a rendered `☐`/`☑` glyph in the live-view mode
  toggles it in place, independent of the right-click "Check" menu action that inserts a new one
  (`[spec §2.3]`).
- Per-memo scroll position is remembered across memo switches within the same app session, **and is
  persisted across app restarts** (confirmed by user decision, not merely permitted — see
  `DECISIONS.md` "UI-State Persistence Decisions"), stored in `MemoEditorSettings.scrollPosition`
  (§3.3) inside the tab's `.tabtreenote.json`.
- **Document-position-based, not pixel-based** (`DECISIONS.md` "Scroll Position: Document-Position-Based
  Anchor, Not Raw Pixel Offset"): a raw pixel/scrollTop value is meaningless across a live/source view
  toggle or a font-size change, since decorations (headings, resized images, etc.) change visual line
  heights independently of document position. Storing the top-visible line number instead means the
  same document line is restored to the top of the viewport regardless of which view mode or font size
  is active at restore time.

### 5.4 Search and Full-Text Index — Elaboration

- **Index build (startup)**: scan every registered, currently-unlocked tab's folder for direct-child
  `.md` files, read + convert each to plaintext (see below), and hold `{tabId, memoName, plaintext}`
  entries in memory. Locked tabs never contribute to the index, whether locked or in an active unlock
  session — per `[spec §2.4, §4.1 pt.6]`, a tab's name and body are excluded from the index
  unconditionally, with no exception for the session case.
- **Plaintext conversion for indexing** (elaborates `[spec §4.2]`'s "remove Markdown syntax/HTML tags
  before indexing"): strip Markdown structural/decoration markers (`#`, `*`, `_`, `` ` ``, `>`, table
  pipes, horizontal-rule sequences) and HTML tags (including the allowlisted `<img>`/`<span>`),
  retaining the human-readable text content (heading text, list item text, code block contents, link
  label text). Whether raw URLs inside link targets are retained is an implementation nicety, not a
  spec-mandated behavior — either choice is acceptable.
- **Fenced code blocks must be recognized as such, not scanned by the same marker-stripping pass**
  (`DECISIONS.md` "Code-Fence Awareness in Markdown Scanning"): the characters being stripped as
  "structural markers" (`#`, `*`, `` ` ``, `>`) are also common, legitimate characters inside code
  (comments, multiplication, shell redirects). Code block content must be identified via a real
  fence-boundary scan and retained verbatim (not run through the marker-stripping regex), consistent
  with this section's own "retaining... code block contents" requirement above.
- **Index data structure/algorithm** (inverted index vs. linear scan over cached plaintext, etc.) is
  left to `developer-lite`; `spec.md` only mandates "lightweight in-memory index, no external DB or
  persistent index" (`[spec §2.4]`). At the "hundreds to thousands of memos" scale (`[spec §6]`), a
  simple approach is expected to be sufficient.
- **Update triggers**: any in-app save/rename/delete/move immediately patches the affected entry/entries
  in the index (no full rebuild). External changes detected by the file watcher (§6.2) trigger the same
  patch path once confirmed as a genuine external change (not a self-write).
- **Search history** (`[spec §2.4]`, "Search history" — distinct from the full-text index above; this
  refers to the in-memo search/replace feature's history of recently-used search/replace strings, e.g.
  surfaced as a dropdown/autocomplete in the in-memo search UI). **Confirmed session-only** (in-memory,
  cleared on app restart) — decided by the user; see `DECISIONS.md` ("UI-State Persistence Decisions").
  Not persisted anywhere, including `AppSettings`.

### 5.5 Appearance and Theme — Elaboration

- Theme is a simple `light`/`dark` toggle backed by two CSS variable sets; "basic customization of
  colors, fonts, line spacing" (`[spec §2.5]`) means user-adjustable values layered on top of the
  chosen theme's defaults, not a full theme-authoring system.
- `uiFontScale` (80–150%) applies to all UI chrome text (tab bar, tree, menus, status bar, dialogs) but
  explicitly not to the editor body font, which has its own independent `fontSize` setting
  (`editorDefaults.fontSize` / per-memo override).

### 5.6 Persistence and Save Triggers — Elaboration

Save triggers enumerated in `[spec §2.6]`: tab switch, memo load (switching away from the
previously-open memo), app minimize, app exit, explicit `Ctrl+S`, periodic autosave. Elaboration:

- "Don't write if no change" applies to every trigger, not just periodic autosave — compare against
  the last-saved content (or hash) before invoking the atomic-write path at all.
- On app exit, all triggers that would fire on "switch away from current memo" must complete (or be
  explicitly awaited) before the process is allowed to terminate, so in-flight edits are not lost.
- Startup auto-open (`AppSettings.lastOpened`): if the referenced tab no longer exists in
  `AppSettings.tabs`, or the referenced memo no longer exists after §3.5 reconciliation, do nothing
  (no error dialog, no fallback memo auto-selected) — matches `[spec §2.6]` exactly.

### 5.7 Window / Operation — Elaboration

- Frameless window (`[spec §9.1 item 1]`): `decorations: false` equivalent in Tauri, with a
  drag-region on the empty tab-bar area (`data-tauri-drag-region`-equivalent) and self-drawn
  minimize/maximize/close buttons. Windows 11's snap-layout flyout on maximize-button hover is
  suppressed by default per that same section; edge-drag snapping continues to work.
- Multi-launch prevention: second launch must (a) not open a second window, (b) not attempt to write
  App Settings JSON from two processes, and (c) bring the existing window to the foreground. The
  specific IPC/lock-file mechanism to achieve this is `developer-lite`'s call (§9).
- Auto-launch to tray on Windows login: implement via a mechanism that does **not** compromise the
  portable/registry-independent design goal for app *data* (`[spec §0]`) — e.g. a Startup-folder
  shortcut rather than writing app configuration into the Windows registry. (This does not conflict
  with the portability principle, which concerns where *settings/data* live, not whether the OS
  autostart mechanism itself touches the registry at all — Run-key registration is also acceptable if
  `developer-lite` judges the Startup-folder approach impractical, since either way no user data or
  settings are stored in the registry.)
- Right-click suppression outside registered tabs / unwanted native title-bar menu items: both already
  descoped as behavior to preserve from the legacy app (`[spec §2.7, §9]`), no new elaboration needed.

### 5.8 Link Functionality — Elaboration

- **Auto-link recognition patterns** (URLs, email, file paths) are standard best-effort heuristics; a
  reasonable, widely-used pattern set (e.g. scheme-prefixed or `www.`-prefixed token boundaries for
  URLs, standard `local@domain.tld` shape for email, path-separator-containing tokens without embedded
  whitespace for file paths) satisfies `[spec §2.8]`. Exact regexes are an implementation detail (§9),
  not a spec-level ambiguity, since no specific pattern was ever specified or implied by the legacy
  behavior beyond the prefixes/schemes already listed in `spec.md`.
- **`memo:` link resolution**: `memo:MemoName` resolves within the *currently open* tab only;
  `memo:TabName/MemoName` resolves by tab display name (not folder path) against
  `AppSettings.tabs`; `memo:TabName/` (trailing slash, no memo name) opens the target tab without
  selecting a specific memo. Ambiguous/duplicate tab display names cannot occur because tab names are
  enforced unique (`[spec §2.1]`).
- Locked-tab exclusion for `memo:` resolution (`[spec §2.8, §4.1]`) applies to both the no-session
  and, importantly, is bypassed (resolves normally) once a session is active for that target tab.
- Markdown link coexistence (`[spec §4.2]`): `[text](memo:...)` and bare `memo:...` auto-link
  recognition use the same resolution logic; the only difference is how the clickable region is
  determined (whole bare token vs. the link's visible text span).
- **Same-document heading-anchor links (`#slug`)** (new, TOC feature, §5.10): resolution is scoped to
  the currently open memo only, re-computed at click time from a fresh heading scan (not from whatever
  the TOC block last generated), so a link still resolves correctly even if headings were edited since
  the last "Update" click. A non-matching slug does nothing — no error UI, per `[spec §4.2, §2.8]`.

### 5.9 Tab Lock — End-to-End Flows

Elaborates `[spec §4.1]` into concrete sequences.

**Locking a tab** (from an unlocked state):
1. If any memo in this tab (or its descendants) is currently open and has unsaved changes, force a
   save-confirm (auto-save-and-close), per `[spec §8]`, before proceeding.
2. Read the current unlocked tree (`MetaTreeNode[]`, real names) from `.tabtreenote.json`.
3. **Write the in-progress marker** (crash-safety measure, `DECISIONS.md` "Tab Lock: Crash-Mid-Lock
   Recovery via Explicit Marker + Resume/Rollback"): before any rename happens, compute the **full**
   planned `fileMap` — a fresh 128-bit hex token for *every* `.md` file directly under the tab folder
   (retry on filesystem-collision), none generated lazily during the loop below — and atomically write
   it (§6.1) to a sibling marker (e.g. `.tabtreenote.lock-inprogress`; exact filename/format is
   `developer-lite`'s call, see `plans/tab-lock/PLAN.md` for why the complete mapping must be written
   up front rather than incrementally). The marker write itself is registered in the self-write
   registry (§6.2) like any other self-caused FS event.
4. For each entry in the marker's `fileMap`, in any order: register both the source and destination
   paths in the self-write registry (§6.2), encrypt the file's content (fresh IV), then rename+replace
   with the encrypted content under the token filename (no extension, §3.4).
5. Build the lock payload (`realTabName`, the tree with real names intact, the same `fileMap` written
   to the marker in step 3), encrypt it, and atomically write the resulting `lock` blob into
   `.tabtreenote.json`, replacing the `tree` field.
6. **Delete the in-progress marker** (self-write-registered, like any other self-caused delete) only
   after step 5 has succeeded. This is the sole success signal recovery relies on: a marker still
   present on next access means the lock operation did not finish.
7. Update `AppSettings.tabs[i]`: set `locked: true`, replace `name` with a placeholder.
8. Any full-text index entries for this tab are purged.

**Recovery on tab-folder access (crash mid-lock)**: elaborates `DECISIONS.md`'s Option B resolution of
former `QA.md` Q1. Whenever a tab's folder is accessed and its in-progress marker is found still
present, the app must resolve it before treating the tab as normally lockable/unlockable again:

- **Per-entry state check**: for each `{ memoName, token }` pair recorded in the marker, exactly one of
  the following is expected to be true on disk: `<memoName>.md` still exists and the `<token>` file does
  not (this entry's rename never happened, or was rolled back already), or the `<token>` file exists and
  `<memoName>.md` does not (this entry's rename completed). Neither-exists or both-exist is an anomalous
  state (e.g. external interference during the crash window) that `developer-lite` must decide how to
  surface — not expected in the normal crash-only case, so no specific handling is mandated here.
- **Finish vs. roll back**: once every entry's state is known, the app deterministically either (a)
  completes the lock by renaming/encrypting the remaining not-yet-renamed entries and then proceeding
  from steps 5–6 above (build and write the lock blob, then delete the marker), or (b) rolls back by
  renaming/decrypting every already-renamed entry back to its original `.md` name and then deleting the
  marker without ever writing a `lock` blob (the tab remains unlocked, as if the lock operation was
  never started). Both paths are idempotent and safe to re-attempt if interrupted again. **Which of the
  two the app picks, and any tie-breaking rule for a case where some entries already succeeded and
  others didn't, is left to `developer-lite`'s `PLAN.md`-level judgment** — the only spec-level
  requirement is that the outcome is always one of these two fully-consistent end states, never a
  half-renamed tab left for the user to discover manually.
- **Trigger point**: whether this check runs for every registered tab at app startup, or lazily the
  first time that specific tab's folder is opened/selected, is also left to `PLAN.md`-level judgment.
- Every rename/decrypt/encrypt performed during recovery is self-write-registered (§6.2) and uses the
  same atomic-write mechanism (§6.1) as the rest of this flow — no special-cased I/O path for recovery.

**Unlocking (starting a session)**:
1. User selects the locked (🔒) tab and enters a password (hint, if set, shown alongside the prompt).
2. Decrypt the lock blob using scrypt(password, stored salt, stored KDF params). Auth-tag failure →
   `DECRYPT_FAILED`, "password incorrect," no content of any kind returned.
3. On success, hold the decrypted lock payload (real tree, real tab name, file map) in memory as the
   active session for this tab. Individual memo bodies are decrypted on demand as they're opened for
   viewing/editing (not all up front), using the same session key.
4. While the session is active: edits re-encrypt and atomically re-save (a) the modified memo body
   (fresh IV) and (b) the lock blob (fresh IV) whenever the tree/name/file-map changes (e.g. rename,
   reorder, create, delete within the tab) — same save-trigger cadence as an unlocked tab (§5.6).
5. **Session end**: triggered solely by switching away from this tab (selecting a different tab). No
   idle timeout, no manual "lock now" button distinct from switching tabs. On session end, nothing
   further needs to happen on disk (everything was already re-encrypted incrementally in step 4) —
   session end just discards the in-memory plaintext state.

**Permanently unlocking a tab** (elaborates `[spec §4.1 pt.9]`; irreversibly ends the tab's locked state
entirely — distinct from, and not required for, ending a session as described above): the full flow,
crash-safety design, and crash-mid-permanent-unlock recovery are now specified in
`docs/tab-lock-detailed-spec.md` (split out of this section because that content is large and
permanent-unlock-specific — everything else in this §5.9 stayed here).

**Tab bar during a pending recovery (either direction)**: neither marker introduces any new badge or
indicator in the tab bar itself — the tab continues to render exactly as its current `AppSettings` state
already dictates, unchanged. In practice this looks different at rest for the two directions (a
lock-direction marker leaves the tab looking like an ordinary unlocked tab with its real name, since
`AppSettings.tabs[i].locked` isn't set until near the end of `tab_lock`'s own flow, `[detailed-spec §5.9]`
"Locking a tab" step 7; a permanent-unlock-direction marker leaves the tab still showing its 🔒 placeholder
name, since `locked` isn't cleared until step 8 of `docs/tab-lock-detailed-spec.md`'s "Permanently
unlocking a tab" flow) — but that difference is just the
ordinary settings-driven render in each case, not a special "recovery" treatment, so the two states are
consistent with each other in the sense that matters: **neither gets its own tab-bar visual language**.
The only observable effect of a pending marker is what happens when that tab's folder is next opened:
`tree_open` returns `recoveryPending: true` instead of a normal tree, and the frontend shows the
corresponding recovery dialog — "Finish Locking"/"Restore" for a lock-direction marker, "Finish
Unlocking"/"Restore Lock" for a permanent-unlock-direction marker — instead of the tab's ordinary
tree/empty-state view. Only which dialog/choice set opens differs; the trigger mechanism and the tab
bar's own appearance do not.

**Deleting a locked tab**: rejected with `TAB_LOCKED` while no session is active; unlock first, then
delete normally (delete is then equivalent to deleting any other tab's registration, `[spec §4.1 pt.7]`).
A session alone is sufficient to permit deletion — permanent unlock (above) is a separate, not required,
step for deletion purposes.

**Cross-tab DND involving a locked tab** (elaborates `[spec §4.1 pt.8]`):
- *Dragging from a locked (no-session) tab*: not applicable — a locked tab's tree/memos are not
  visible in the UI at all while locked, so there is nothing to drag from it.
- *Dropping onto a locked (no-session) tab*: show the password dialog at the moment of drop. On
  success, start a session for that tab immediately (as in the unlock flow above) and then complete the
  drop as an intra-session memo addition (encrypt the incoming memo body, add a `fileMap` entry, insert
  into the tree, re-save the lock blob). The drag-source tab is unaffected and does not need to be
  left/reselected.
- *Dropping onto a locked tab that already has an active session*: proceeds exactly like dropping onto
  a normal unlocked tab (encrypt on the way in).
- *Dragging from a tab with an active unlock session, dropping onto a normal (unlocked) tab*: the
  memo is decrypted at the moment of drop and lands as a normal plaintext `.md` file in the target tab
  (no "vault" one-way restriction, `[spec §4.1 pt.8]`).

### 5.10 Markdown Editing — Elaboration

**The split-pane preview, PDF export, and live-view real-image-rendering details are now specified in
`docs/MDEditor-spec.md`** (split out because that content is large and self-contained — everything else
in this §5.10 stayed here).

- **Editor engine: CodeMirror 6** (confirmed, `[spec §4.2]`; see `DECISIONS.md` for rationale and
  rejected alternatives). All Markdown editing — live view and source view alike — is built as a
  custom decoration extension over a single CodeMirror 6 editor instance whose document is the memo's
  plain-text buffer. There is no ProseMirror-style document-tree model anywhere in this feature: the
  buffer CodeMirror holds *is* the file's Markdown text, so saving never involves a
  parse-tree-to-text serialization step. This choice is final and no longer blocked — implementation
  work (live-view decoration/click handling, cursor-position mapping across the live/source toggle) can
  proceed in `developer-lite`'s `PLAN.md`.
- **Known high-risk defect zone** (`[spec §8]`, carried forward from `DECISIONS.md`'s CodeMirror 6
  trade-off writeup): overlapping-range decoration edits and decoration-widget click/selection handling
  are flagged as a high-risk area for this specific implementation approach — cursor/selection mapping
  can go wrong when a decoration and its underlying hidden syntax markers occupy different visual space
  than their underlying text length. This does not corrupt the saved file (the buffer beneath the
  decorations is always correct plain text), but it is exactly the kind of bug that is easy to miss in
  manual testing and easy to hit in daily use. `developer-lite` should plan explicit test coverage for:
  editing text at a decoration boundary, editing inside a decorated range (e.g. inside `**bold**`),
  clicking to place the cursor on/near a decoration widget (e.g. a rendered checkbox or resized image),
  rapid live/source toggling while a selection spans a decorated range, and a `memo:`/`[text](memo:...)`
  link rendered inside a `<span style="color:...">`-decorated range (nested/overlapping decorations,
  `DECISIONS.md` "Overlapping Text-Styling Decorations: Implement as Composable Mark Decorations").
- **Overlapping styling decorations should compose without special-casing if implemented as CodeMirror
  "mark" decorations** (a CSS class/style applied to a text range) rather than "widget" decorations (a
  text range replaced by a rendered element) — two mark decorations (e.g. color-span styling and link
  click-affordance styling) can coexist over the same or overlapping ranges without the cursor-mapping
  risk described above, which is specific to widget replacement and to hidden-syntax-marker/visible-text
  length mismatches. This is a lower-risk sub-case of the same defect zone, not a separate open design
  question — still included in the required test coverage above as a concrete regression check.
- **Live view vs source toggle**: default view renders CommonMark-equivalent formatting inline
  (headings, lists, quotes, code blocks, tables, emphasis, horizontal rules, links) as you type, by
  applying/removing the decoration layer over the buffer; a toggle switches the same memo to raw-text
  source editing (decorations off). **Hard implementation constraint**: because both modes operate on
  the exact same CodeMirror document, toggling between them must never re-parse-and-regenerate the
  text or perform any text-content conversion of any kind — it is purely a decoration-layer on/off
  switch, and cursor/scroll position must be preserved across the toggle (mapped through CodeMirror's
  own position system, not recomputed from scratch). This applies however often the user toggles,
  including many times per editing session.
- **Which mode a memo opens in**: fixed to `'live'` (not a user-configurable global default; there is
  no Settings → Editor "display mode" selector, GitHub issue #61), with an optional per-memo override in
  `MemoEditorSettings.viewMode` (`[spec §3.4]`, §3.3 above). The runtime toggle described above writes
  its new value into the currently-open memo's `MemoEditorSettings.viewMode` override (never the fixed
  default), so reopening a memo — or restarting the app — resumes in that memo's last-toggled mode; a
  memo with no override yet still falls back to the fixed `'live'` default (GitHub issue #31).
- **Toolbar — finalized 18-slot layout** (`DECISIONS.md` "Markdown Toolbar: Expanded to 18-Slot Grid",
  GitHub issue #176; supersedes the earlier 16-slot layout in the same entry's predecessor): a floating,
  freely repositionable (drag anywhere within the window) panel, fixed
  **9 columns × 2 rows = 18 slots, no blanks**:

  | | Col 1 | Col 2 | Col 3 | Col 4 | Col 5 | Col 6 | Col 7 | Col 8 | Col 9 |
  |---|---|---|---|---|---|---|---|---|---|
  | Row 1 | Undo | Color | Heading | List | Bold | Strikethrough | Link insert | Update (TOC) | Clear formatting |
  | Row 2 | Redo | Font size | Code block | Quote | Italic | Table insert | Image insert | Hard line break | Rearrange-mode toggle |

  Bold and Italic are two separate actions/icons (the earlier "combined single button" idea is
  dropped). Each action applies to the current selection or inserts at the cursor, consistent with
  standard Markdown-editor toolbar conventions. The 15 actions carried over from the prior 16-slot
  layout keep their previous row/column positions unchanged; only `rearrangeToggle` moves, from
  (row 2, col 8) to (row 2, col 9), so it remains the fixed last slot as the grid grows.
- **Icon set**: Lucide (MIT-licensed open-source SVG icon set), bundled locally into the app — no
  CDN/network dependency, consistent with the app's portable/offline design (`[spec §0, §6]`). Mapping:

  | Action | Lucide icon name |
  |---|---|
  | Undo | `Undo2` |
  | Redo | `Redo2` |
  | Color | `Palette` |
  | Font size | `ALargeSmall` |
  | Heading | `Heading` |
  | List | `List` |
  | Quote | `Quote` |
  | Code block | `Code` |
  | Table insert | `Table` |
  | Bold | `Bold` |
  | Italic | `Italic` |
  | Strikethrough | `Strikethrough` |
  | Link insert | `Link` |
  | Image insert | `Image` |
  | Update (TOC) | `RefreshCw` |
  | Clear formatting | `Eraser` |
  | Hard line break | `CornerDownLeft` |
  | Rearrange-mode toggle | `GripVertical` |

- **Rearrange mode**: the 18th slot (`GripVertical`) toggles rearrange mode on/off; it does not open a
  settings screen.
  - **OFF (default)**: clicking any of the other 17 icons invokes that icon's action immediately, as
    normal.
  - **ON**: clicking an icon does not invoke its action — only dragging works, to reorder icons among
    the 18 slots. The *set* of 18 actions is fixed; only positions change (no add/remove/hide slot).
  - Exit rearrange mode by clicking the toggle icon again, or by pressing Esc.
  - Visual indicator while ON: the toolbar's border/background changes (whole-toolbar tint), so the
    mode is unambiguous at a glance.
- **Persistence**: the toolbar's dragged position persists across app restarts, stored in
  `AppSettings.window.toolbarPosition` (§3.2, `DECISIONS.md` "UI-State Persistence Decisions"). The icon
  order resulting from rearrange mode also persists, stored in `AppSettings.window.toolbarOrder` (§3.2,
  `DECISIONS.md` "Markdown Toolbar: Expanded to 18-Slot Grid").

- **New action — "Insert hard line break"** (`hardBreak`, tooltip label "強制改行", GitHub issue #176):
  inserts a CommonMark hard line break (two trailing spaces + `\n`) at the insertion point, as a single CodeMirror transaction
  (one undo step). Insertion point is the end of the current selection if one exists, or the cursor
  position if the selection is collapsed; existing text is never deleted. Because this is a
  point-insertion action rather than a selection-wrapping one (unlike Bold/Italic/Color), it behaves
  identically whether or not there is a selection — there is no separate empty-selection case to
  special-case.
- **New action — "Clear formatting"** (`clearFormatting`, tooltip label "書式クリア", GitHub issue #176): strips Markdown
  decoration/structure syntax and this app's own `<span style="color:...;font-size:...">` allowlist tag
  from the selected text, reducing it to plain text, as a single CodeMirror transaction (one undo step).
  - **Stripped**: bold (`**`/`__`), italic (`*`/`_`), strikethrough (`~~`), inline code (`` ` ``),
    fenced/indented code-block markers (block text content is kept, only the fence/indent markers are
    removed), blockquote `>` line prefixes, heading `#`–`######` line prefixes, list markers
    (`-`/`*`/`+`/ordered `N.`), horizontal-rule lines (`---`/`***`/`___`) that fall fully inside the
    selection, table pipe/header-separator syntax, and the `<span style="color:...;font-size:...">`
    wrapper (inner text is kept — this is the combined equivalent of the Color and Font-size buttons'
    own "clear color"/"clear size" controls, applied together plus everything else above).
  - **Not stripped** (left exactly as-is): link syntax `[text](url)` (including `memo:` links and
    `#slug` same-document heading-anchor links) and image syntax (`![alt](url)` and
    `<img src width height>`) — both are content/references, not decoration, so Clear Formatting never
    discards a link target or an embedded image. Any HTML tag outside the sanitization allowlist is
    also left untouched, same as everywhere else in the app; this action does not change the allowlist,
    it only strips the one allowlisted decoration tag (`<span style>`).
  - **No selection (collapsed cursor)**: no-op — no transaction is created. There is nothing to strip,
    and unlike Bold/Italic this action has no "apply to future typing" concept in this app's
    plain-text-buffer editing model.

- **Table of Contents (TOC) — new v1 feature, triggered by the toolbar's "Update" button** (row 1,
  slot 8; `[spec §4.2, §2.8]`):
  - **Trigger**: manual only. Clicking "Update" runs the scan/generate/insert-or-replace steps below
    once. Must never run automatically or in the background while the user types or edits headings
    elsewhere — same category as any other discrete toolbar insert action (e.g. Table insert), not a
    live/reactive feature.
  - **Heading scan**: on click, scan the current memo's CodeMirror buffer for Markdown headings (`#`
    through `######`), collecting level + heading text, in document order. **Must use the same
    Markdown-aware parse (respecting fenced-code-block boundaries) as the decoration layer, not an
    independent line-by-line `#`-regex** (`DECISIONS.md` "Code-Fence Awareness in Markdown Scanning") —
    a `#` inside a fenced code block is not a heading under CommonMark, and a naive regex scan would
    incorrectly generate a TOC entry from one (e.g. a shell/Python comment).
  - **Slug generation**: GitHub-style slugification (lowercase; spaces → hyphens; strip punctuation not
    part of a word). On duplicate slugs within the same document, append `-1`, `-2`, etc. to each
    later occurrence (first occurrence keeps the bare slug) — the same convention as re-used by the
    heading-anchor click resolution in §4.6/§5.8, so generation and resolution stay consistent.
  - **TOC block generation**: a nested Markdown list of `[heading text](#slug)` links, nesting relative
    to the document's shallowest heading level (e.g. if the shallowest heading is `##`, it becomes the
    flat top-level TOC entry, and a following `###` nests one level under it) — see `docs/DECISIONS.md`
    "Table of Contents: Relative Heading-Nesting Indentation".
  - **Marker-delimited block**: the generated TOC is wrapped in a pair of HTML-comment markers, e.g.
    `<!-- TOC start -->` / `<!-- TOC end -->` — exact marker text is `developer-lite`'s call (§9), as
    long as it stays inert/invisible in live-view rendering (HTML comments already render as nothing
    under the CommonMark-equivalent support in `[spec §4.2]`, so no new sanitization-allowlist entry is
    needed for the markers themselves).
  - **Insert vs. replace**: on click, scan the document for an existing marker-delimited block.
    - If found: replace the block's contents (between the markers) with the freshly generated TOC, as a
      single CodeMirror transaction (one undo step — consistent with every other toolbar action in this
      section).
    - If not found: insert a new marker-delimited block at the current cursor position.
  - **No live/background auto-update**: regeneration only happens on an explicit "Update" click; the
    feature must not attempt to detect heading edits and refresh the TOC on its own.
  - **Persistence**: the TOC block is ordinary Markdown text — saved and reloaded as part of the memo
    body like the rest of the memo, with no separate storage or metadata field.
  - **New link type — same-document heading anchors (`#slug`)**: see §4.6/§5.8 for click-resolution
    behavior (re-scan current memo, CodeMirror scroll-to-position, silent no-op on no match).
  - **Scope**: self-contained — no cross-memo or cross-tab resolution, no persistence changes beyond
    the memo body text itself.
- **Sanitization allowlist** (elaborates `[spec §4.2]`), applied to preview/live-view rendering:
  - Allowed tags: `<img>` with only `src` (relative path only — reject absolute paths, `file://`, any
    scheme other than a same-folder-relative reference, **and any path containing a `..` segment**, to
    prevent a hand-typed/pasted `src` from referencing a file outside the tab folder,
    `DECISIONS.md` "Image `src` Allowlist: Reject `..` Path-Traversal Segments") and `width`/`height`
    attributes; `<span>` with only a `style` attribute, and within `style`, only the `color` and
    `font-size` properties (any other CSS property in the same `style` string is stripped, not the
    whole tag).
  - Everything else — `<script>`, any `on*` event-handler attribute, `javascript:`-scheme URLs in any
    attribute, `<iframe>`, `<style>`, `position`/`behavior`/etc. CSS — is stripped entirely during
    preview rendering, on every render pass (not just at save time), so pasted or hand-typed unsafe
    HTML never executes even transiently.
- **Decoration notation**: bold `**text**` / italic `*text*` use standard CommonMark syntax and render
  through the chosen editor engine's normal emphasis handling. Color/size use
  `<span style="color:...;font-size:...">...</span>` inserted/edited via toolbar helpers (manual
  hand-typing of the same syntax is also valid input).
- **Toolbar Color button palette — confirmed, distinct from `label.color`** (§3.3's tree-memo color tag
  is a separate feature; this is the toolbar's text-decoration Color button, applying
  `<span style="color:...">` to the current selection). Closed 5-choice enum plus a separate "clear
  color" control, adopted verbatim from the legacy app's `MarkdownToolbar.tsx`:

  | Choice | value |
  |---|---|
  | Red | `#e53935` |
  | Blue | `#1e88e5` |
  | Green | `#43a047` |
  | Orange | `#fb8c00` |
  | Purple | `#8e24aa` |

  A 6th control, separate from the 5 swatches, removes the color styling from the selection ("clear
  color"), carried over from the legacy toolbar. No "None" swatch among the 5 — clearing is its own
  control.
- **Toolbar Font-size button palette — confirmed**: applies `<span style="font-size:...">` to the
  current selection. Closed 3-choice enum plus a separate "clear size" control, adopted verbatim from
  the legacy app:

  | Choice | value |
  |---|---|
  | Small | `12px` |
  | Large | `20px` |
  | Extra-large | `28px` |

  A 4th control removes the font-size styling from the selection ("clear size"), same treatment as
  Color's clear control.

  Both palettes above are closed enums, not free-form pickers — same "adopted verbatim, no new design"
  treatment as the `label.color` palette in §3.3.
- **Default keyboard shortcuts for all 18 toolbar actions — confirmed policy**: every one of the 18
  actions in the grid above gets a default keyboard shortcut, registered as an entry in the existing
  generic shortcut system (`AppSettings.shortcuts`, `[spec §3.4]`; default keymap already tracked in
  `docs/plans/window-operation/PLAN.md` §3 — no new mechanism). Users may rebind any of them via
  Settings → Shortcuts, same as any other shortcut in the app. This is a policy decision only (every
  action gets *some* default binding, none stay toolbar-click-only) — the exact key combination for
  each of the 18 is `developer-lite`'s implementation discretion (§9).
- **Resize behavior**: pasting an image inserts standard `![alt](relative/path.png)`; resizing (via a
  UI resize handle on the rendered image) converts that single image's markup to
  `<img src="relative/path.png" width=".." height="..">`. Reverting a resize back to "natural size" may
  either restore the `![alt](url)` form or keep the `<img>` form with natural-size dimensions — either
  is acceptable, not spec-mandated.

---

## 6. Cross-Cutting Mechanism Details

### 6.1 Atomic Write — Concrete Sequence

Elaborates `[spec §5.2]`. For any persisted file (memo body, tab metadata JSON incl. lock blob, app
settings JSON, obfuscated tab-lock files):

1. Write full new content to a temp file (fixed, self-write-registry-exempt prefix, §6.2) in the same
   directory as the target (same-volume rename guarantee).
2. Flush/fsync the temp file's contents to disk.
3. Register both the existing target path and the `.bak` path in the self-write registry (§6.2).
4. Rename the existing target → `<target>.bak` (overwriting any previous `.bak`; only one generation is
   kept).
5. Rename the temp file → target.
6. No explicit directory-entry fsync is performed (NTFS metadata journaling is relied upon, consistent
   with legacy `00-core-principles.md` §2's documented rationale — Windows/NTFS is this app's sole
   target per `[spec §6]`).

Crash-safety property this preserves: a crash during step 1–2 leaves the live target untouched; a crash
during steps 4–5 always leaves either the old target or the `.bak` recoverable (§6.4).

### 6.2 Self-Write Registry — Elaboration

Elaborates `[spec §5.4]`. Every FS operation the app performs on itself (atomic write's temp→bak→target
renames, plain rename, trash/direct delete, DND move/copy, tab-lock obfuscation renames) must register
the affected path(s) **before** performing the operation, so the concurrently-running file watcher can
recognize and discard the resulting filesystem event as self-caused rather than external. Registration
must happen in one centralized place in the filesystem layer (not scattered per call-site) so that no
operation can accidentally skip registration. Record lifetime: cleared immediately once the matching
watcher event is consumed; falls back to the TTL default in §3.7 if no matching event ever arrives
(e.g. because the OS coalesced or dropped it).

### 6.3 Sync Order Between Real Files and Metadata JSON — Elaboration

Elaborates `[spec §5.3]`, for any operation touching both (rename, move, copy, re-parent):

1. Update the in-memory tree representation first (post-validation — name-collision/sanitization checks
   happen here, before touching any real file; a validation failure exits here with an error and no
   filesystem or metadata write of any kind).
2. Perform the real file operation, with self-write registration (§6.2) immediately before it.
3. Immediately after the file operation succeeds, atomically save the metadata JSON (§6.1).

If a crash occurs between steps 2 and 3, the corruption-recovery cascade (§6.4) is the safety net; the
memo body is never lost, though the specific tree position/hierarchy change might need to be redone by
the user (accepted tradeoff, `[spec §5.3]`).

### 6.4 Conflict Detection and Corruption Recovery — Elaboration

- **Save-time conflict check** (elaborates `[spec §5.5]`): purely a hash comparison of the loaded
  `expectedHash` against a freshly computed hash of the current on-disk bytes, evaluated once at the
  moment `Save memo body` executes. No mtime/size check at this layer.
- **Watcher-side change classification** (self-write vs. genuine external change): mtime + size + hash,
  composite — any registry match (§6.2) short-circuits to "self-write, ignore" before this composite
  check is even needed.
- **Metadata corruption recovery cascade** (elaborates `[spec §5.9]`): on JSON-parse failure or schema
  mismatch of `.tabtreenote.json` or the App Settings JSON, attempt `.bak` first; if that also fails or
  is absent, fall back to the §3.5 reconciliation logic with an empty "(a)" side (i.e. every real file
  becomes a flat root-level entry); notify the user that metadata was reconstructed. App Settings JSON
  total loss additionally regenerates with hardcoded defaults (empty tab list, default window geometry,
  etc.) rather than failing to start — **and the user is notified via the same path/mechanism used for
  metadata JSON corruption notification** (`[spec §7]`, explicit: "App settings JSON corruption recovery
  also notified to user via same path as metadata JSON corruption"), not silently.

### 6.5 Other Post-M7 Implementation Rules — Elaboration

Elaborates the remaining bullets of `[spec §7]` not already covered by §6.1–§6.4 above.

- **Folder-not-found returns empty, not an exception** (`[spec §7]`): any backend operation that lists
  or scans a tab folder's contents (tree open/reconciliation §3.5, index build §5.4, etc.) must treat a
  missing folder (e.g. the registered path was deleted or the drive is unavailable) as "zero entries
  found," returning an empty array/result through the normal `Result<T>` success path — not as a thrown
  exception or an `UNKNOWN`/`NOT_FOUND` error. This keeps the app resilient to a tab's folder having been
  deleted or become unreachable from outside the app (the tab itself still shows in the tab bar; its
  tree just renders empty until the folder reappears or the user removes the tab per §4.1).
- **Validation strictness differs by context** (`[spec §7]`): name validation (`[spec §5.11]`, `NAME_INVALID`)
  is **strict** when the user is typing a brand-new name (tab create/rename, memo create/rename, image
  filename generation collision retry) — forbidden characters, reserved names, leading/trailing spaces,
  and trailing periods are all rejected outright, with sanitized-name suggestions offered. The same
  validation is **lenient** when the operation is reconciling or rescuing an *existing* on-disk entity
  that already has one of these otherwise-invalid names (e.g. a `.md` file with a trailing space in its
  name that was created by some other tool, encountered during the §3.5 reconciliation scan, or during
  the corruption-recovery flat rebuild in §6.4) — such existing entries are accepted as-is into the tree
  rather than being blocked or force-renamed, since refusing to show a pre-existing file the user cannot
  otherwise reach would violate the "memo body is never lost" principle (`[spec §3.2]`). The strict path
  only ever applies at the moment of *creating or changing* a name, never retroactively to what is
  already on disk.
- **No native blocking dialogs anywhere** (`[spec §7]`): `window.confirm()`, `window.alert()`,
  `window.prompt()`, and any other OS-native/browser-native blocking dialog are prohibited throughout
  the app (breaks IME input state). Every confirmation/prompt/notification surface in this document
  (delete confirm, `TRASH_FAILED` consent, tab-lock unlock prompt §5.9, conflict-resolution dialog §6.4,
  corruption notices §6.4) is built from one shared custom non-blocking dialog component, matching
  `[spec §9.1 item 2]`'s "unify all custom UI surfaces" already established for the context menu.
- **Dialogs are Esc-closable, with one named exception** (`[spec §7]`): every dialog built with the
  shared custom dialog component above must close on Esc — **except** the external-change conflict
  dialog (§6.4 "External edit conflict"), which is intentionally Esc-disabled so a user cannot
  accidentally dismiss an unresolved on-disk conflict without making an explicit choice.

---

## 7. Error Handling — Expanded

Builds on the table in `[spec §5.8]`. Each `ErrorCode` from `[spec §3.4]`, with triggering scenarios:

| Code | Triggered by | Expected UI response |
|---|---|---|
| `NAME_DUPLICATE` | Tab or memo name collision at create/rename time | Inline validation message before submission is even attempted; no file touched |
| `NAME_INVALID` | Forbidden character, reserved name, leading/trailing space, trailing period | Same as above, with a one-click sanitized-name suggestion where feasible (`[spec §5.11]`) |
| `NOT_FOUND` | Operation targets a tab/memo/path that no longer exists (e.g. stale UI state after external deletion) | Refresh the affected view (tree/tab list) from disk and inform the user the item is gone |
| `IO_LOCKED` | All Windows-lock retries (§3.7) exhausted on write/rename/delete | Non-blocking error toast/dialog; operation did not complete, no partial state |
| `TRASH_FAILED` | OS trash API failed (e.g. network drive) | Confirmation dialog offering direct delete; only proceeds on explicit user consent (`[spec §5.8]`) |
| `DECRYPT_FAILED` | Tab-lock password wrong, or auth-tag verification failed | "Password incorrect" message, no content revealed, no retry-count limit specified (unlimited attempts unless the user later asks for lockout behavior) |
| `CONFLICT` | Save-time hash mismatch (§6.4) | Conflict-resolution dialog; auto-save and further input suspended until resolved |
| `META_CORRUPT` | Metadata JSON unreadable and unrecoverable via `.bak` | Notify "metadata reconstructed" after flat rebuild (§6.4); never blocks the user from proceeding |
| `LONG_PATH` | Resulting path would exceed 260 characters | Warning shown at the point the long name/path would be created; operation may still proceed at user's own risk (no formal support, `[spec §5.11]`) |
| `TAB_LOCKED` | Any mutating operation (delete, and any others `developer-lite` judges unsafe) targets a locked tab with no active session | Explain that the tab must be unlocked first |
| `UNKNOWN` | Any unexpected/unclassified exception caught at the backend boundary | Generic error message; full exception detail (OS error code, stack) logged only on the backend side, never sent to the frontend (`[spec §5.10]`) |

---

## 8. Non-Functional Elaboration

- Performance target (`[spec §6]`, "hundreds to thousands of memos") is a qualitative bar ("stays
  snappy"), not a specific benchmark number — no specific millisecond budget is mandated by `spec.md`,
  so none is invented here. If `reviewer-lite` finds a concrete scale/latency problem during real-app
  testing, that is the mechanism for surfacing a firmer target, not a pre-set number in this document.
- Backup/portability: verifying that a full folder copy of a *locked* tab, moved to a different
  machine/path, still unlocks correctly with the same password is a reasonable acceptance check for
  the Tab Lock feature's implementation plan (`[spec §6]`).

---

## 9. Deferred-to-Implementation Details (Not Ambiguities — No User Decision Needed)

These are explicitly left open by `spec.md` itself for `developer-lite` to finalize during planning or
implementation, and should not be escalated to `QA.md`:

- Exact error code used for the (rare) "both UTF-8 and Shift_JIS decode failed" case, since
  `[spec §3.4]`'s `ErrorCode` enum doesn't name one specifically for this.
- Exact regex/heuristics for auto-link recognition of URLs/emails/file paths (§5.8).
- Exact on-disk byte layout of an individual encrypted memo body under Tab Lock (§3.4), and of the
  temp-file naming prefix used by atomic writes (§6.1).
- Multi-launch-prevention mechanism (named pipe, lock file, mutex, etc., §5.7).
- Windows autostart mechanism (Startup-folder shortcut vs. registry Run key, §5.7) — either satisfies
  the spec; pick whichever is more reliable to implement in Tauri.
- Full-text search index's internal data structure/algorithm (§5.4).
- IME auto-enable-on-startup validation spike, and its go/no-go outcome (`[spec §7]`, already framed by
  `spec.md` as a build-time experiment, not a spec decision).
- Tauri/Rust naming conventions for the operations inventoried in §4 and for backend→frontend events.
- Exact HTML-comment marker text delimiting the TOC block (§5.10), as long as it stays inert in
  live-view rendering.
- Exact action-ID string spellings used in `AppSettings.window.toolbarOrder` (§3.2, §5.10), as long as
  the set stays fixed at 18 and the array's slot→action mapping role is preserved (`hardBreak` and
  `clearFormatting` are this document's own suggested spellings, not binding — same discretion as the
  other 16).
- Exact key combination assigned as the default shortcut for each of the 18 Markdown-toolbar actions
  (§5.10) — only the policy that all 18 get *a* default binding is spec-level; the specific combos are
  added to `docs/plans/window-operation/PLAN.md` §3's default keymap list by `developer-lite`.

---

## 10. Open Questions Requiring User Decision

No open items remain from the original pass.

- Editor engine choice for Markdown editing (CodeMirror 6 vs. a ProseMirror-based WYSIWYG library such
  as Milkdown/TipTap) was resolved by the user (CodeMirror 6, confirmed) and is now reflected in
  `[spec §4.2]`, `DECISIONS.md`, and §5.10 above; it has been removed from `QA.md` (see that file's
  header for the resolved-item convention).
- Former `QA.md` Q1 ("Tab Lock: crash window mid-locking leaves orphaned encrypted files invisible to
  recovery") was resolved by the user (Option B: explicit in-progress marker with resume/rollback
  recovery) and is now reflected in `DECISIONS.md` ("Tab Lock: Crash-Mid-Lock Recovery via Explicit
  Marker + Resume/Rollback") and §5.9 above (marker-write step + "Recovery on tab-folder access"
  subsection); it has been removed from `QA.md` per the same resolved-item convention.

Any newly identified genuine spec-level ambiguity should be appended here and to `QA.md` following the
same process.
