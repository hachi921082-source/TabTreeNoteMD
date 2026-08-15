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
  /** Tooltip / accessible label (Japanese, per this app's UI-text convention). */
  label: string;
}

/** The fixed 9-columns×2-rows grid's factory-default slot order (row-major: row 1 slots 1-9, then
 * row 2 slots 1-9), matching the table in `docs/plans/markdown-editing/PLAN.md` §4 exactly. Used
 * whenever `AppSettings.window.toolbarOrder` is absent/`null` (first launch). The 15 IDs carried
 * over from the prior 16-slot default keep their previous relative order; `clearFormatting` and
 * `hardBreak` (GitHub issue #176) are inserted at their new slots (row 1 col 9, row 2 col 8), and
 * `rearrangeToggle` shifts to the new last slot (row 2 col 9). */
export const DEFAULT_TOOLBAR_ORDER: ToolbarActionId[] = [
  "undo",
  "color",
  "heading",
  "list",
  "bold",
  "strikethrough",
  "link",
  "tocUpdate",
  "clearFormatting",
  "redo",
  "fontSize",
  "codeBlock",
  "quote",
  "italic",
  "table",
  "image",
  "hardBreak",
  "rearrangeToggle",
];

export const TOOLBAR_ACTIONS: Record<ToolbarActionId, ToolbarActionDef> = {
  undo: { id: "undo", icon: Undo2, label: "元に戻す" },
  redo: { id: "redo", icon: Redo2, label: "やり直す" },
  color: { id: "color", icon: Palette, label: "文字色" },
  fontSize: { id: "fontSize", icon: ALargeSmall, label: "文字サイズ" },
  heading: { id: "heading", icon: Heading, label: "見出し" },
  list: { id: "list", icon: List, label: "リスト" },
  bold: { id: "bold", icon: Bold, label: "太字" },
  italic: { id: "italic", icon: Italic, label: "斜体" },
  strikethrough: { id: "strikethrough", icon: Strikethrough, label: "取り消し線" },
  quote: { id: "quote", icon: Quote, label: "引用" },
  link: { id: "link", icon: Link, label: "リンク挿入" },
  codeBlock: { id: "codeBlock", icon: Code, label: "コードブロック" },
  tocUpdate: { id: "tocUpdate", icon: RefreshCw, label: "目次を更新" },
  table: { id: "table", icon: Table, label: "表挿入" },
  image: { id: "image", icon: Image, label: "画像挿入" },
  clearFormatting: { id: "clearFormatting", icon: Eraser, label: "書式クリア" },
  hardBreak: { id: "hardBreak", icon: CornerDownLeft, label: "強制改行" },
  rearrangeToggle: { id: "rearrangeToggle", icon: GripVertical, label: "並べ替えモード" },
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
