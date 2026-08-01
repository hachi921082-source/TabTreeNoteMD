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
  lastOpened: { tabId: string; memoName: string } | null;
  editorDefaults: EditorDefaults;
  autosave: { plainDebounceMs: number };
  links: { openTrigger: "ctrlClick" | "singleClick" | "doubleClick"; confirmBeforeOpen: boolean };
  encoding: { defaultSave: "utf-8" };
  eol: { policy: "preserve" | "crlf" | "lf" | "cr" };
  shortcuts: Record<string, string>;
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
  lastOpened?: { tabId: string; memoName: string } | null;
  editorDefaults?: Partial<EditorDefaults>;
  autosave?: Partial<{ plainDebounceMs: number }>;
  links?: Partial<{ openTrigger: "ctrlClick" | "singleClick" | "doubleClick"; confirmBeforeOpen: boolean }>;
  encoding?: Partial<{ defaultSave: "utf-8" }>;
  eol?: Partial<{ policy: "preserve" | "crlf" | "lf" | "cr" }>;
  shortcuts?: Record<string, string>;
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
