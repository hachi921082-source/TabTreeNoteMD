// Click-to-open handling for auto-links and `memo:`/`[text](memo:...)` links rendered by the
// Markdown decoration layer (`docs/plans/markdown-editing/PLAN.md` §2/§7). Detection (which text
// spans are clickable) lives here and in `markdownDecorations.ts`; actual `memo:` resolution is
// `link/PLAN.md`'s command (not yet implemented — stubbed below, see that function's doc comment)
// and same-document `#slug` heading-anchor resolution is `table-of-contents/PLAN.md`'s scan (also
// not yet implemented — likewise stubbed). External URL/email/file-path opening already works
// today via `ipc/link.ts`'s `linkOpenExternal` (`docs/plans/editor-save/PLAN.md`'s "Search with
// Google" forward seam), so that path is real, not a stub.
import type { EditorView } from "@codemirror/view";
import { linkOpenExternal } from "../ipc/link";
import type { AppSettings } from "../shared/models";

export type LinkKind = "external" | "memo" | "heading-anchor";

export interface DetectedLink {
  /** Clickable region — the whole bare token for an auto-link, or the link's visible label span
   * for `[text](...)` syntax (`[detailed-spec §5.8]`). */
  from: number;
  to: number;
  kind: LinkKind;
  /** For `external`: the URL/email/path to open. For `memo`: the raw `memo:...` target (scheme
   * included). For `heading-anchor`: the slug, without the leading `#`. */
  target: string;
}

// Reasonable best-effort patterns (`[detailed-spec §5.8]`: "exact regexes are an implementation
// detail, not a spec-level ambiguity"). Applied per-line (line text passed in), never across
// fenced code blocks — `markdownDecorations.ts` is responsible for excluding code-block lines
// before calling these.
const URL_RE = /\b(?:https?:\/\/|ftp:\/\/|www\.)[^\s<>()[\]]+/gi;
const EMAIL_RE = /\b[\w.+-]+@[\w-]+\.[\w.-]+\b/g;
const BARE_MEMO_RE = /\bmemo:[^\s<>()[\]]*/g;
const FILE_PATH_RE = /\b(?:[a-zA-Z]:[\\/]|\.{1,2}[\\/])[^\s<>()[\]]+/g;

/** Scans one line of plain text (with `lineFrom` its document offset) for bare auto-link tokens:
 * URLs, emails, file paths, and bare `memo:...` tokens. Does NOT detect `[text](url)` Markdown-link
 * syntax — that is `markdownDecorations.ts`'s job, driven off the syntax tree's `Link` nodes, since
 * distinguishing the label span from the URL span needs the parsed node structure, not regex. */
export function scanAutoLinks(lineText: string, lineFrom: number): DetectedLink[] {
  const results: DetectedLink[] = [];
  const push = (re: RegExp, kind: LinkKind, stripLeadingHash = false) => {
    re.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(lineText))) {
      const target = stripLeadingHash ? m[0].replace(/^#/, "") : m[0];
      results.push({ from: lineFrom + m.index, to: lineFrom + m.index + m[0].length, kind, target });
    }
  };
  push(BARE_MEMO_RE, "memo");
  push(URL_RE, "external");
  push(EMAIL_RE, "external");
  push(FILE_PATH_RE, "external");
  return results;
}

export function isMemoScheme(url: string): boolean {
  return url.startsWith("memo:");
}

export function isHeadingAnchor(url: string): boolean {
  return url.startsWith("#");
}

/** Stub — `link/PLAN.md`'s `memo:` resolution command (`memo:MemoName` / `memo:TabName/MemoName` /
 * `memo:TabName/`) does not exist yet. Per `[spec §2.8]`, an unresolved link "does nothing" — this
 * stub always behaves that way, which is externally indistinguishable from a real "not found"
 * result, so it is a safe placeholder rather than a broken one. Replace this function's body with
 * a real `ipc/link.ts` call once `link/PLAN.md` is implemented. */
export async function resolveAndOpenMemoLink(target: string): Promise<void> {
  console.info(`[markdown-editing] memo: link resolution not implemented yet (see link/PLAN.md); target=${target}`);
}

/** Stub — `table-of-contents/PLAN.md`'s heading-anchor scan/resolution does not exist yet. Real
 * behavior (`[spec §2.8, §4.2]`): re-scan the current memo's headings at click time and scroll to
 * a match; no match is a silent no-op. This stub always no-ops, which is externally
 * indistinguishable from "no matching heading" — safe placeholder, not broken behavior. */
export function resolveAndScrollToHeadingAnchor(view: EditorView, slug: string): void {
  console.info(`[markdown-editing] #slug heading-anchor resolution not implemented yet (see table-of-contents/PLAN.md); slug=${slug}`);
  void view;
}

export async function openDetectedLink(view: EditorView, link: DetectedLink): Promise<void> {
  if (link.kind === "external") {
    void linkOpenExternal(link.target);
    return;
  }
  if (link.kind === "memo") {
    await resolveAndOpenMemoLink(link.target);
    return;
  }
  resolveAndScrollToHeadingAnchor(view, link.target);
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
