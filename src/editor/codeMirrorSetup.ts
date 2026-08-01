// Base Markdown extension composition (`docs/plans/markdown-editing/PLAN.md` §2/§3): the
// CommonMark+GFM language/parser (always on, needed for the syntax tree regardless of view mode)
// plus the custom decoration extension (`markdownDecorations.ts`), gated by a `Compartment` so the
// live/source toggle is purely an extension swap — never a document change, never a cursor/scroll
// reset (`[detailed-spec §5.10]`'s "hard implementation constraint"). Composed alongside
// `editor/textCommands.ts`'s plain-text extensions (history, keymap, search, settings
// compartments, etc.) by `components/Editor/EditorPane.tsx`, not a replacement for them.
import { Compartment } from "@codemirror/state";
import type { Extension } from "@codemirror/state";
import { markdown } from "@codemirror/lang-markdown";
import { GFM } from "@lezer/markdown";
import { markdownDecorationExtension } from "./markdownDecorations";
import type { MarkdownDecorationConfig } from "./markdownDecorations";

/** Always-on language support: CommonMark plus GitHub-Flavored-Markdown's table/task-list/
 * strikethrough/autolink extensions (`[spec §4.2]`: "CommonMark equivalent full support"; tables
 * and task-list checkboxes and bare-URL autolinks all need GFM specifically, not bare CommonMark).
 * This is independent of the live/source toggle — it only provides the syntax tree
 * `markdownDecorations.ts` reads from and `table-of-contents/PLAN.md`'s eventual heading scanner
 * will read from too; it renders no visual styling of its own (no `syntaxHighlighting()` extension
 * is added here), so with the decoration compartment below empty, "source" view is genuinely plain,
 * undecorated text. */
export function markdownLanguageExtension(): Extension {
  return markdown({ extensions: [GFM] });
}

/** Holds either `markdownDecorationExtension(config)` (live view) or `[]` (source view) —
 * reconfigured via `Compartment.reconfigure`, never by tearing down/recreating the `EditorView`,
 * so cursor/scroll survive every toggle (`docs/plans/markdown-editing/PLAN.md` §3). */
export const liveViewCompartment = new Compartment();

export function liveViewExtension(enabled: boolean, config: MarkdownDecorationConfig): Extension {
  return enabled ? markdownDecorationExtension(config) : [];
}

/** Convenience composer for `EditorPane.tsx`: the language support (always on) plus the initial
 * live-view compartment content. */
export function buildMarkdownExtensions(liveViewEnabled: boolean, config: MarkdownDecorationConfig): Extension[] {
  return [markdownLanguageExtension(), liveViewCompartment.of(liveViewExtension(liveViewEnabled, config))];
}
