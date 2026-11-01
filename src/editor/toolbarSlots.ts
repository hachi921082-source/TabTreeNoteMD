// The 18-slot action registry (`docs/plans/markdown-editing/PLAN.md` §4, `DECISIONS.md` "Markdown
// Toolbar: Expanded to 18-Slot Grid" — GitHub issue #176, supersedes the prior 16-slot layout). Icon
// + handler mapping only — the grid layout, drag-to-reposition, and rearrange-mode drag-to-reorder
// state machine live in the UI component (`components/Editor/Toolbar/MarkdownToolbar.tsx`), which
// imports this registry rather than hardcoding action metadata itself.
import {
  ALargeSmall,
  Bold,
  Code,
  CornerDownLeft,
  Eraser,
  GripVertical,
  Heading,
  Image,
  Italic,
  Link,
  List,
  Palette,
  Quote,
  Redo2,
  RefreshCw,
  Strikethrough,
  Table,
  Undo2,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { TranslationKey } from "../i18n";

/** Exact ID spellings are this plan's own call (`docs/plans/markdown-editing/PLAN.md` §4) — used
 * verbatim as `AppSettings.window.toolbarOrder`'s array element values. */
export type ToolbarActionId =
  | "undo"
  | "redo"
  | "color"
  | "fontSize"
  | "heading"
  | "list"
  | "bold"
  | "italic"
  | "strikethrough"
  | "quote"
  | "link"
  | "codeBlock"
  | "tocUpdate"
  | "table"
  | "image"
  | "clearFormatting"
  | "hardBreak"
  | "rearrangeToggle";

export interface ToolbarActionDef {
  id: ToolbarActionId;
  icon: LucideIcon;
  /** Tooltip / accessible label — a `TranslationKey`, not a literal string, so this module-level
   * registry stays translatable via `useT()` at the render site
   * (`docs/plans/v1.2.0/i18n/PLAN.md` §6). */
  labelKey: TranslationKey;
}

/** The fixed 9-columns×2-rows grid's factory-default slot order (row-major: row 1 slots 1-9, then
 * row 2 slots 1-9), matching the table in `docs/plans/markdown-editing/PLAN.md` §4 exactly. Used
 * whenever `AppSettings.window.toolbarOrder` is absent/`null` (first launch). Reordered per GitHub
 * issue #187 — only the default arrangement changed, not the action set. */
export const DEFAULT_TOOLBAR_ORDER: ToolbarActionId[] = [
  "undo",
  "color",
  "heading",
  "fontSize",
  "list",
  "bold",
  "italic",
  "strikethrough",
  "tocUpdate",
  "redo",
  "quote",
  "codeBlock",
  "table",
  "link",
  "image",
  "clearFormatting",
  "hardBreak",
  "rearrangeToggle",
];

export const TOOLBAR_ACTIONS: Record<ToolbarActionId, ToolbarActionDef> = {
  undo: { id: "undo", icon: Undo2, labelKey: "toolbar.action.undo" },
  redo: { id: "redo", icon: Redo2, labelKey: "toolbar.action.redo" },
  color: { id: "color", icon: Palette, labelKey: "toolbar.action.color" },
  fontSize: { id: "fontSize", icon: ALargeSmall, labelKey: "toolbar.action.fontSize" },
  heading: { id: "heading", icon: Heading, labelKey: "toolbar.action.heading" },
  list: { id: "list", icon: List, labelKey: "toolbar.action.list" },
  bold: { id: "bold", icon: Bold, labelKey: "toolbar.action.bold" },
  italic: { id: "italic", icon: Italic, labelKey: "toolbar.action.italic" },
  strikethrough: { id: "strikethrough", icon: Strikethrough, labelKey: "toolbar.action.strikethrough" },
  quote: { id: "quote", icon: Quote, labelKey: "toolbar.action.quote" },
  link: { id: "link", icon: Link, labelKey: "toolbar.action.link" },
  codeBlock: { id: "codeBlock", icon: Code, labelKey: "toolbar.action.codeBlock" },
  tocUpdate: { id: "tocUpdate", icon: RefreshCw, labelKey: "toolbar.action.tocUpdate" },
  table: { id: "table", icon: Table, labelKey: "toolbar.action.table" },
  image: { id: "image", icon: Image, labelKey: "toolbar.action.image" },
  clearFormatting: { id: "clearFormatting", icon: Eraser, labelKey: "toolbar.action.clearFormatting" },
  hardBreak: { id: "hardBreak", icon: CornerDownLeft, labelKey: "toolbar.action.hardBreak" },
  rearrangeToggle: { id: "rearrangeToggle", icon: GripVertical, labelKey: "toolbar.action.rearrangeToggle" },
};

/** Merges a possibly-partial/stale persisted order (e.g. from a prior app version with fewer
 * slots — GitHub issue #176 grew the set from 16 to 18) with the fixed default: any missing action
 * id is appended, any unknown id is dropped — the action *set* is always exactly these 18, only
 * positions are user-configurable (`docs/plans/markdown-editing/PLAN.md` §4: "no add/remove/hide
 * slot"). `rearrangeToggle` is then unconditionally re-pinned to the last position: it's always
 * appended by construction in `DEFAULT_TOOLBAR_ORDER` above, but a 16-element persisted order (which
 * already ends with it) would otherwise get the newly-appended `missing` IDs tacked on *after* it,
 * silently knocking it out of the grid's fixed last slot for every returning user. Re-pinning here
 * (rather than special-casing the `missing` insertion point) keeps this correct for any future grid
 * growth too, not just this one. */
export function normalizeToolbarOrder(persisted: string[] | undefined | null): ToolbarActionId[] {
  const known = new Set(DEFAULT_TOOLBAR_ORDER);
  const fromPersisted = (persisted ?? []).filter((id): id is ToolbarActionId => known.has(id as ToolbarActionId));
  const missing = DEFAULT_TOOLBAR_ORDER.filter((id) => !fromPersisted.includes(id));
  const merged = [...fromPersisted, ...missing];
  const withoutToggle = merged.filter((id) => id !== "rearrangeToggle");
  return [...withoutToggle, "rearrangeToggle"];
}
