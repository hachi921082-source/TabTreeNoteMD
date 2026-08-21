// `markdown-it-texmath` ships no bundled types and has no `@types/markdown-it-texmath` package
// (verified at implementation time — same situation as `markdown-it-task-lists.d.ts` right next to
// this file, `docs/plans/v1.2.0/math-rendering/PLAN.md` §2.1). Minimal ambient declaration covering
// only the shape this app actually uses (`md.use(texmath, { engine, delimiters, katexOptions })`
// to register the `$...$`/`$$...$$` tokenizer rules; `markdownIt.ts` fully overrides every
// `math_inline`/`math_inline_double`/`math_block`/`math_block_eqno` renderer rule the plugin
// installs, so the plugin's own rendering internals — `texmath.render`, its `<eq>`/`<section>`
// wrapper templates — are never called and are not typed here).
declare module "markdown-it-texmath" {
  import type MarkdownIt from "markdown-it";
  import type katex from "katex";

  export interface TexmathOptions {
    /** Required in this app's usage: `markdown-it-texmath`'s own fallback (`require("katex")` at
     * `md.use()` time) assumes a CommonJS module system, which does not exist in a Vite/ESM
     * browser bundle — omitting this would silently fall back to its "No math renderer found."
     * stub. Unused by this app's own renderer-rule overrides (see file header above) but still
     * required by the plugin's internal setup path. */
    engine: typeof katex;
    /** `"dollars"` is this app's only configured preset (`$...$` inline, `$$...$$` block/inline-
     * display) — `PLAN.md` §2.1's delimiter choice. */
    delimiters?: string | string[];
    katexOptions?: Record<string, unknown>;
    outerSpace?: boolean;
    macros?: Record<string, string>;
  }

  export default function texmath(md: MarkdownIt, options: TexmathOptions): void;
}
