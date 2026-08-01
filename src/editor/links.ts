// Click-to-open handling for auto-links and `memo:`/`[text](memo:...)` links rendered by the
// Markdown decoration layer (`docs/plans/markdown-editing/PLAN.md` §2/§7). Detection of *which*
// text spans are clickable is split between here and `markdownDecorations.ts`: bare
// `http(s)://`/`www.`/email tokens are already recognized by `@lezer/markdown`'s GFM autolink
// extension as `URL` syntax-tree nodes (so `markdownDecorations.ts` reads those directly from the
// tree rather than re-detecting them with regex here); this module supplies the two token shapes
// the tree parser does NOT know about (bare `memo:...` and bare file paths), plus the
// classification/open-trigger/resolution logic shared by every link kind regardless of how its
// range was detected.
//
// Real vs. stub, per this pass's scope:
// - `url` (bare `http(s)://`/`www.`/`ftp://`, or a Markdown-link whose URL isn't `memo:`/`#slug`):
//   REAL — opens via `ipc/link.ts`'s already-implemented `linkOpenExternal`.
// - `email`, `filePath`: rendering as clickable is this plan's scope, but `commands/link.rs`'s own
//   doc comment explicitly excludes "email/file-path special-casing" from what's implemented so
//   far (mailto:-prefixing, relative-to-absolute path resolution against the tab folder) — calling
//   `linkOpenExternal` with a bare email/relative path would silently fail rather than open
//   anything, so these are explicit stubs instead of a broken real call.
// - `memo`: STUB — `link/PLAN.md`'s `memo:` resolution command does not exist yet.
// - `headingAnchor`: STUB — `table-of-contents/PLAN.md`'s heading scan/resolution does not exist
//   yet.
// Every stub's behavior (silently do nothing) is externally indistinguishable from this app's own
// confirmed "unresolved link" behavior (`[spec §2.8]`), so none of them are user-visible breakage —
// they become real once their owning plan lands, by replacing the function body only.
import type { EditorView } from "@codemirror/view";
import { linkOpenExternal } from "../ipc/link";
import type { AppSettings } from "../shared/models";

export type LinkKind = "url" | "email" | "filePath" | "memo" | "headingAnchor";

export interface DetectedLink {
  /** Clickable region — the whole bare token for an auto-link, or the link's visible label span
   * for `[text](...)` syntax (`[detailed-spec §5.8]`). */
  from: number;
  to: number;
  kind: LinkKind;
  /** The URL/email/path/`memo:...` target, or (for `headingAnchor`) the slug without its leading
   * `#`. */
  target: string;
}

/** `true` if `text` (an already-detected bare token) looks like an email address rather than a
 * URL — both shapes come out of the same `@lezer/markdown` GFM `URL` node type, which does not
 * itself distinguish them (`markdownDecorations.ts` calls this to classify). */
export function looksLikeEmail(text: string): boolean {
  return /^[\w.+-]+@[\w-]+\.[\w.-]+$/.test(text);
}

// Bare-token shapes `@lezer/markdown`'s GFM autolink extension does NOT recognize on its own —
// `[detailed-spec §5.8]`: "exact regexes are an implementation detail, not a spec-level ambiguity".
const BARE_MEMO_RE = /\bmemo:[^\s<>()[\]]*/g;
const FILE_PATH_RE = /\b(?:[a-zA-Z]:[\\/]|\.{1,2}[\\/])[^\s<>()[\]]+/g;

/** Scans one line of plain text (with `lineFrom` its document offset) for bare `memo:...` tokens.
 * Callers (`markdownDecorations.ts`) are responsible for excluding fenced-code-block lines first
 * and for de-duplicating against any overlapping tree-detected `URL`/`Link` range. */
export function scanBareMemoTokens(lineText: string, lineFrom: number): DetectedLink[] {
  const results: DetectedLink[] = [];
  BARE_MEMO_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = BARE_MEMO_RE.exec(lineText))) {
    results.push({ from: lineFrom + m.index, to: lineFrom + m.index + m[0].length, kind: "memo", target: m[0] });
  }
  return results;
}

/** Scans one line of plain text for bare file-path-shaped tokens (`./foo`, `../foo`, `C:\foo`).
 * Same caller responsibilities as `scanBareMemoTokens`. */
export function scanBareFilePathTokens(lineText: string, lineFrom: number): DetectedLink[] {
  const results: DetectedLink[] = [];
  FILE_PATH_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = FILE_PATH_RE.exec(lineText))) {
    results.push({ from: lineFrom + m.index, to: lineFrom + m.index + m[0].length, kind: "filePath", target: m[0] });
  }
  return results;
}

export function isMemoScheme(url: string): boolean {
  return url.startsWith("memo:");
}

export function isHeadingAnchor(url: string): boolean {
  return url.startsWith("#");
}

/** Stub — `link/PLAN.md`'s `memo:` resolution command (`memo:MemoName` / `memo:TabName/MemoName` /
 * `memo:TabName/`) does not exist yet. See this module's header comment for why a silent no-op
 * here is a safe placeholder rather than broken behavior. */
export async function resolveAndOpenMemoLink(target: string): Promise<void> {
  console.info(`[markdown-editing] memo: link resolution not implemented yet (see link/PLAN.md); target=${target}`);
}

/** Stub — `table-of-contents/PLAN.md`'s heading-anchor scan/resolution does not exist yet. Real
 * behavior (`[spec §2.8, §4.2]`): re-scan the current memo's headings at click time and scroll to
 * a match; no match is a silent no-op. */
export function resolveAndScrollToHeadingAnchor(view: EditorView, slug: string): void {
  console.info(`[markdown-editing] #slug heading-anchor resolution not implemented yet (see table-of-contents/PLAN.md); slug=${slug}`);
  void view;
}

/** Stub — `link/PLAN.md`'s email special-casing (`mailto:` prefixing) is not implemented yet. */
export function resolveAndOpenEmailLink(target: string): void {
  console.info(`[markdown-editing] email link opening not implemented yet (see link/PLAN.md); target=${target}`);
}

/** Stub — `link/PLAN.md`'s file-path special-casing (relative-to-absolute resolution against the
 * tab folder) is not implemented yet. */
export function resolveAndOpenFilePathLink(target: string): void {
  console.info(`[markdown-editing] file-path link opening not implemented yet (see link/PLAN.md); target=${target}`);
}

export async function openDetectedLink(view: EditorView, link: DetectedLink): Promise<void> {
  switch (link.kind) {
    case "url":
      void linkOpenExternal(link.target);
      return;
    case "email":
      resolveAndOpenEmailLink(link.target);
      return;
    case "filePath":
      resolveAndOpenFilePathLink(link.target);
      return;
    case "memo":
      await resolveAndOpenMemoLink(link.target);
      return;
    case "headingAnchor":
      resolveAndScrollToHeadingAnchor(view, link.target);
      return;
  }
}

/** `AppSettings.links.openTrigger` gating (`[spec §2.8]`, default `singleClick`). `eventType`
 * distinguishes a `dblclick` DOM event from a plain `click`/`mousedown`, since "doubleClick" mode
 * only ever fires from an actual double-click event, never a single one. */
export function linkClickMatchesTrigger(e: MouseEvent, openTrigger: AppSettings["links"]["openTrigger"], eventType: "click" | "dblclick"): boolean {
  if (openTrigger === "doubleClick") return eventType === "dblclick";
  if (eventType === "dblclick") return false;
  if (openTrigger === "ctrlClick") return e.ctrlKey;
  return true; // "singleClick"
}
