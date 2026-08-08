// `markdown-it-task-lists` ships no bundled types and has no `@types/markdown-it-task-lists`
// package (verified at implementation time — `docs/plans/markdown/markdown-preview/RENDERING.md`
// §1's flagged caveat). Minimal ambient declaration covering only the shape this app actually uses
// (`md.use(taskLists, { enabled })`), not a full re-typing of the plugin's internals.
declare module "markdown-it-task-lists" {
  import type MarkdownIt from "markdown-it";

  export interface TaskListsOptions {
    /** Keep rendered `<input>` checkboxes non-`disabled` (this app gates click-to-toggle in JS via
     * `previewInteractions.ts`'s own `readOnly` check instead — RENDERING.md §1/§8). */
    enabled?: boolean;
    /** Wrap the item text in a `<label>` alongside the checkbox. Left at the plugin's default
     * (`false`) — not enabled by this app (verified empirically: default output has no `<label>`). */
    label?: boolean;
    labelAfter?: boolean;
  }

  export default function taskLists(md: MarkdownIt, options?: TaskListsOptions): void;
}
