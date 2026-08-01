// TS mirror of Rust `model/*` structs (single source of truth per
// `docs/plans/project-architecture/PLAN.md` §1/§2). Only the types owned by
// features implemented so far are declared here; each feature's own PLAN.md
// adds its own types to this file when implemented (e.g. `TreeNode`,
// `MetaTreeNode`, `TabMeta`, `Label` belong to `memo-tree/PLAN.md`).

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
