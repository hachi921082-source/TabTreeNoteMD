// Click-to-open handling for auto-links and `memo:`/`[text](memo:...)` links rendered by the
// Markdown decoration layer (`docs/plans/markdown-editing/PLAN.md` §2/§7). Detection of *which*
// text spans are clickable is split between here and `markdownDecorations.ts`: bare
// `http(s)://`/`www.`/email tokens are already recognized by `@lezer/markdown`'s GFM autolink
// extension as `URL` syntax-tree nodes (so `markdownDecorations.ts` reads those directly from the
// tree rather than re-detecting them with regex here); this module supplies the two token shapes
// the tree parser does NOT know about (bare `memo:...` and bare file paths), plus the
// classification/open-trigger/resolution/opening logic shared by every link kind regardless of
// how its range was detected (`docs/plans/link/PLAN.md` §5).
import { EditorView } from "@codemirror/view";
import { linkOpenExternal, linkResolve } from "../ipc/link";
import type { ExternalLinkKind } from "../ipc/link";
import type { AppSettings } from "../shared/models";
import type { MarkdownDecorationConfig } from "./markdownDecorations";
import { scanHeadings } from "./tocGenerator";

export type LinkKind = "url" | "email" | "filePath" | "memo" | "headingAnchor";

export interface DetectedLink {
  /** Clickable region — the whole bare token for an auto-link, or the link's visible label span
   * for `[text](...)` syntax (`[detailed-spec §5.8]`). */
  from: number;
  to: number;
  kind: LinkKind;
  /** The URL/email/path/`memo:...` target, or (for `headingAnchor`) the slug without its leading
   * `#`. `memo` targets keep their `memo:` prefix (stripped by `resolveAndOpenMemoLink`). */
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
const BARE_MEMO_RE = /\bmemo:[^\s<>()[\]]+/g;
// The drive-letter branch keeps a `\b` boundary (so e.g. `abcC:\foo` doesn't match mid-word), but
// the relative-path branch must NOT require one: a leading `.` is a non-word char, so `\b` would
// only match when the dot is directly touching a preceding word char (no boundary exists after
// whitespace, punctuation, or line-start) — which is how virtually every real relative path is
// preceded. The `[\\/]` required immediately after the 1-2 leading dots already prevents this
// branch from matching ordinary prose (e.g. a sentence-ending `.` followed by a word).
const FILE_PATH_RE = /\b[a-zA-Z]:[\\/][^\s<>()[\]]+|\.{1,2}[\\/][^\s<>()[\]]+/g;

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

/** `memo:` link click resolution (`docs/plans/link/PLAN.md` §2, §5): strips the leading `memo:`
 * scheme, calls `link_resolve`, and on a `"memo"` result opens it via `onOpenMemo` — the caller's
 * hook into the shared "open memo" path (`docs/plans/memo-tree/PLAN.md`, the same one tree click
 * and search-result click use). `"unresolved"` (or a failed IPC call) is a silent no-op, matching
 * `[spec §2.8]`'s "unresolved links do nothing" — no toast, no error surfaced. */
export async function resolveAndOpenMemoLink(currentTabId: string | null, rawTarget: string, onOpenMemo: (tabId: string, memoName: string | null) => void): Promise<void> {
  if (!currentTabId) return;
  const target = rawTarget.startsWith("memo:") ? rawTarget.slice("memo:".length) : rawTarget;
  const result = await linkResolve(currentTabId, target);
  if (!result.ok || result.value.kind !== "memo") return;
  onOpenMemo(result.value.tabId, result.value.memoName);
}

/** `#slug` heading-anchor click resolution (`docs/plans/table-of-contents/PLAN.md` §4,
 * `[detailed-spec §4.6, §5.8]`): re-scans the *current* memo's headings fresh at click time via
 * `tocGenerator.ts`'s `scanHeadings` — never cached from the last "Update" click — so a link still
 * resolves correctly even if headings were added/removed/reworded since. No match is a silent
 * no-op, same as this app's other unresolved-link cases; scoped to the current memo only, no
 * cross-tab/cross-memo resolution. */
export function resolveAndScrollToHeadingAnchor(view: EditorView, slug: string): void {
  const match = scanHeadings(view.state).find((h) => h.slug === slug);
  if (!match) return;
  view.dispatch({
    selection: { anchor: match.from },
    effects: EditorView.scrollIntoView(match.from, { y: "start" }),
  });
  view.focus();
}

/** Opens a URL/email/file-path via `link_open_external`, honoring `AppSettings.links.
 * confirmBeforeOpen` gating on the caller side (`docs/plans/link/PLAN.md` §2, §4) — the confirm
 * dialog itself is `config.requestExternalConfirm`'s job (an injected async prompt, since this
 * click handler runs deep inside a CodeMirror `ViewPlugin`, not a React component). Declining the
 * confirm is treated the same as any other unresolved/cancelled link: a silent no-op. */
async function openExternalLink(kind: ExternalLinkKind, value: string, config: MarkdownDecorationConfig): Promise<void> {
  if (config.confirmBeforeOpen) {
    const confirmed = await config.requestExternalConfirm(kind, value);
    if (!confirmed) return;
  }
  await linkOpenExternal(kind, value, config.currentTabId ?? undefined);
}

export async function openDetectedLink(view: EditorView, link: DetectedLink, config: MarkdownDecorationConfig): Promise<void> {
  switch (link.kind) {
    case "url":
      await openExternalLink("url", link.target, config);
      return;
    case "email":
      await openExternalLink("email", link.target, config);
      return;
    case "filePath":
      await openExternalLink("filePath", link.target, config);
      return;
    case "memo":
      await resolveAndOpenMemoLink(config.currentTabId, link.target, config.onOpenMemo);
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
