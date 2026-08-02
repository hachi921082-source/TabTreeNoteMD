// TS mirror of Rust `model/*` structs (single source of truth per
// `docs/plans/project-architecture/PLAN.md` §1/§2). Only the types owned by
// features implemented so far are declared here; each feature's own PLAN.md
// adds its own types to this file when implemented.

// `docs/detailed-spec.md` §3.2, `docs/spec.md` §3.4.
export interface AppSettings {
  schemaVersion: number;
  appVersion: string;
  tabs: TabSettingsEntry[];
  window: WindowSettings;
  theme: "light" | "dark";
  uiFontScale: number;
  /** `docs/plans/appearance-theme/PLAN.md` §3 — "basic customization of colors/fonts/line
   * spacing" layered on top of the active theme's CSS variables. */
  appearance: AppearanceSettings;
  lastOpened: { tabId: string; memoName: string } | null;
  editorDefaults: EditorDefaults;
  autosave: { plainDebounceMs: number };
  links: { openTrigger: "ctrlClick" | "singleClick" | "doubleClick"; confirmBeforeOpen: boolean };
  encoding: { defaultSave: "utf-8" };
  eol: { policy: "preserve" | "crlf" | "lf" | "cr" };
  shortcuts: Record<string, string>;
  /** Additive field, `docs/plans/window-operation/PLAN.md` §4 — "auto-launch to tray on Windows
   * login" setting. Actual OS registration happens via `ipc/autostart.ts`'s commands whenever
   * this is toggled from the Settings screen. */
  autostart: { enabled: boolean };
}

/** "Basic customization" values layered on top of the active theme's CSS variables
 * (`docs/plans/appearance-theme/PLAN.md` §2/§3) — not a full theme-authoring system. `null` for
 * `accentColor`/`uiFontFamily` means "use the active theme's default". */
export interface AppearanceSettings {
  accentColor: string | null;
  uiFontFamily: string | null;
  /** Multiplier applied to the tree row's base line height; `1.3` matches the app's pre-existing
   * fixed row height. */
  treeRowLineHeight: number;
}

// `docs/plans/tab-management/PLAN.md` §2 `tab_list` — the tab-bar list
// entry returned by the tab domain's read commands. For a locked tab, `name`
// is already the 🔒-placeholder (`docs/DECISIONS.md` "Tab-Lock Placeholder
// Display Name: 🔒 Glyph Alone").
export interface TabInfo {
  id: string;
  name: string;
  locked: boolean;
}

export interface TabSettingsEntry {
  id: string;
  name: string;
  folderPath: string;
  /** Display cache only; the authoritative lock state lives in the tab's own
   * `.tabtreenote.json` (`docs/detailed-spec.md` §3.2). */
  locked?: boolean;
}

export interface WindowSettings {
  x: number;
  y: number;
  width: number;
  height: number;
  maximized: boolean;
  treeWidth: number;
  treeSide: "left" | "right";
  toolbarPosition: { x: number; y: number } | null;
  toolbarOrder: string[];
  /** `docs/plans/window-operation/PLAN.md` §2's tree-hide toggle, persisted like `treeWidth`/
   * `treeSide`. */
  treeCollapsed: boolean;
}

export interface EditorDefaults {
  wrap: "none" | "chars" | "window";
  tabWidth: number;
  softTab: boolean;
  showLineNumbers: boolean;
  fontSize: number;
  viewMode: "live" | "source";
}

// Deep-partial patch shape accepted by `settings_set`
// (`docs/plans/persistence-fs/PLAN.md` §6.2). Every field, including nested
// object fields, is optional; nested objects merge per-field, not
// wholesale-replace.
export type AppSettingsPatch = {
  schemaVersion?: number;
  appVersion?: string;
  tabs?: TabSettingsEntry[];
  window?: Partial<WindowSettings>;
  theme?: "light" | "dark";
  uiFontScale?: number;
  appearance?: Partial<AppearanceSettings>;
  lastOpened?: { tabId: string; memoName: string } | null;
  editorDefaults?: Partial<EditorDefaults>;
  autosave?: Partial<{ plainDebounceMs: number }>;
  links?: Partial<{ openTrigger: "ctrlClick" | "singleClick" | "doubleClick"; confirmBeforeOpen: boolean }>;
  encoding?: Partial<{ defaultSave: "utf-8" }>;
  eol?: Partial<{ policy: "preserve" | "crlf" | "lf" | "cr" }>;
  shortcuts?: Record<string, string>;
  autostart?: Partial<{ enabled: boolean }>;
};

// ---------------------------------------------------------------------------
// Tree / memo domain (`docs/plans/memo-tree/PLAN.md` §1-§3; mirrors Rust
// `model/tree.rs` and `commands/tree.rs`'s command I/O types).
// ---------------------------------------------------------------------------

/** `memo_set_label`'s input palette (`docs/plans/memo-tree/PLAN.md` §3's confirmed 5-choice
 * palette). "None" has no member of its own here — it's expressed as `color: null` in the
 * `memoSetLabel` call, mirroring how the Rust side models it as `Option<LabelColor>`. */
export type LabelColor = "red" | "blue" | "green" | "yellow";

export interface Label {
  /** One of the 5 confirmed hex values, or `null`/absent for "None". */
  color?: string | null;
  /** Reserved, unimplemented for v1 — always `null`, never set by any command in this app. */
  kind?: unknown | null;
}

/** Per-memo editor setting overrides; every field absent/`null` means "follow
 * `AppSettings.editorDefaults`" (`docs/detailed-spec.md` §3.3). */
export interface MemoEditorSettings {
  wrap?: "none" | "chars" | "window" | null;
  tabWidth?: number | null;
  softTab?: boolean | null;
  showLineNumbers?: boolean | null;
  fontSize?: number | null;
  viewMode?: "live" | "source" | null;
  /** 1-indexed document line number visible at the top of the viewport at save time. */
  scrollPosition?: number | null;
}

/** Runtime/UI-only reconciled tree element, returned by `tree_open`
 * (`docs/spec.md` §3.4's "Return type of `tab:openTree`"). `readonly`-flagged since the
 * frontend never assembles or persists this shape itself — every mutation goes through a
 * `memo_*` command, which returns a fresh `TreeSnapshot`. */
export interface TreeNode {
  readonly memoName: string;
  readonly expanded: boolean;
  readonly label?: Label;
  readonly editor?: MemoEditorSettings;
  readonly locked?: boolean;
  readonly children: readonly TreeNode[];
}

export interface TreeSnapshot {
  nodes: TreeNode[];
  tabName: string;
  locked: boolean;
  /** `true` when a `tab-lock/PLAN.md` crash-mid-lock marker was found — the frontend should
   * show that feature's recovery dialog instead of the normal tree view (not yet implemented;
   * this flag is a forward-looking seam). */
  recoveryPending: boolean;
}

export interface MemoProperties {
  fileSizeBytes: number;
  created: string;
  modified: string;
  charCount: number;
}

/** `memo_move`'s target parameter (`docs/plans/memo-tree/PLAN.md` §3). */
export type MoveTarget =
  | { kind: "withinTab"; newParent: string | null; index: number }
  | { kind: "otherTab"; targetTabId: string; newParent: string | null; index: number; copy: boolean };

/** `memo_move`'s return shape. `target` is only present for the cross-tab case; since this
 * app only ever holds one tab's tree in memory at a time (the active tab), the frontend does
 * not need to apply it anywhere else — switching to that other tab later re-fetches its tree
 * via `tree_open` regardless. */
export interface MemoMoveResult {
  source: TreeSnapshot;
  target?: TreeSnapshot;
}

// ---------------------------------------------------------------------------
// Editor / save domain (`docs/plans/editor-save/PLAN.md` §2; mirrors Rust
// `store/memo_file_service.rs` and `commands/editor.rs`'s command I/O types).
// ---------------------------------------------------------------------------

/** Informational only (status bar + re-save policy) — save always re-encodes as UTF-8 today
 * regardless of this value (`AppSettings.encoding.defaultSave` is currently single-valued). */
export type Encoding = "utf-8" | "shift-jis";

/** Detected (on load) / target (on save) line-ending style. */
export type Eol = "crlf" | "lf" | "cr";

/** Return type of `editor_load`. */
export interface MemoContent {
  /** LF-normalized for the editor buffer. */
  text: string;
  encoding: Encoding;
  eol: Eol;
  /** Hash of the on-disk byte sequence at load time — becomes the next `editor_save`'s
   * `expectedHash` (`[spec §5.5]`). */
  hash: string;
  /** This memo's per-memo Edit Lock flag, read from tree metadata. */
  editLocked: boolean;
  /** `MemoEditorSettings.scrollPosition` — 1-indexed top-visible document line number, `null` if
   * never recorded (open at the top). */
  scrollPosition: number | null;
}

/** Return type of `editor_save`. */
export interface SaveResult {
  /** Hash of the just-written bytes — update `expectedHash` with this for the next save. */
  hash: string;
}

// ---------------------------------------------------------------------------
// Tab Lock domain (`docs/plans/tab-lock/PLAN.md`, `docs/plans/tab-lock/CRASH-RECOVERY.md`;
// mirrors Rust `commands/tab_lock.rs`'s command I/O types).
// ---------------------------------------------------------------------------

/** UI default-button tie-break for the crash-recovery dialog (`CRASH-RECOVERY.md`). */
export type RecoveryDefaultAction = "finish" | "restore";

/** Return type of `tab_lock_recovery_status`. */
export interface RecoveryStatus {
  renamedCount: number;
  notRenamedCount: number;
  anomalousCount: number;
  total: number;
  defaultAction: RecoveryDefaultAction;
  /** `true` if any entry is in the anomalous (both-or-neither) state — show a distinct error
   * message instead of the ordinary Finish/Restore choice. */
  anomalous: boolean;
}

// ---------------------------------------------------------------------------
// Cross-tab full-text search (`docs/plans/search-index/PLAN.md` §4-§5; mirrors Rust
// `commands/search.rs`'s command I/O types). Distinct from the in-memo search/replace feature
// (`editor-save/PLAN.md` §5, `state/useSearch.ts`/`components/Search/SearchPanel.tsx`), which has
// no backend command of its own.
// ---------------------------------------------------------------------------

/** `search_query`'s options — mirrors the in-memo search's own option set where applicable to a
 * global query; the exact set is implementation discretion beyond `spec.md`'s "name+body
 * substring match at minimum". */
export interface CrossTabSearchOptions {
  caseSensitive: boolean;
  regex: boolean;
}

export type MatchLocation = "name" | "body" | "both";

export interface SearchHit {
  tabId: string;
  tabName: string;
  memoName: string;
  matchedIn: MatchLocation;
}

// ---------------------------------------------------------------------------
// Link domain (`docs/plans/link/PLAN.md` §2; mirrors Rust `commands/link.rs`'s command I/O types).
// ---------------------------------------------------------------------------

/** `link_resolve`'s return shape — pure resolution, never opens/navigates anything itself.
 * `memoName` is `null` for the `memo:TabName/` (tab-only, no memo selected) resolved form. */
export type ResolvedLink = { kind: "memo"; tabId: string; memoName: string | null } | { kind: "unresolved" };

/** `link_open_external`'s `kind` discriminant (`docs/plans/link/PLAN.md` §2). */
export type ExternalLinkKind = "url" | "email" | "filePath";

// ---------------------------------------------------------------------------
// Image domain (`docs/plans/image-handling/PLAN.md` §3; mirrors Rust `commands/image.rs`'s
// command I/O types).
// ---------------------------------------------------------------------------

/** `image_save_pasted`'s return shape. */
export interface SavedImage {
  /** Filename only, relative to the tab folder — the Markdown body always references images
   * relatively (`[spec §4.2]`). */
  relativePath: string;
  width: number;
  height: number;
}
