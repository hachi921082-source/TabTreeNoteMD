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
  // ローカル部・ドメイン部とも非 ASCII の文字(`\p{L}\p{N}\p{M}`)を許す(#301)。ASCII のみの文字列の判定結果は従来と同じ
  return /^[\p{L}\p{N}\p{M}_.+-]+@[\p{L}\p{N}\p{M}_-]+\.[\p{L}\p{N}\p{M}_.-]+$/u.test(text);
}

// Bare-token shapes `@lezer/markdown`'s GFM autolink extension does NOT recognize on its own —
// `[detailed-spec §5.8]`: "exact regexes are an implementation detail, not a spec-level ambiguity".
// Exported (not module-private) so `docs/plans/markdown/markdown-preview/RENDERING.md` §9's preview
// pipeline can reuse the exact same patterns for its own bare-token markdown-it inline detection,
// rather than reimplementing them — both call sites must stay in agreement on what counts as a bare
// `memo:`/file-path token.
export const BARE_MEMO_RE = /\bmemo:[^\s<>()[\]]+/g;
// The drive-letter branch keeps a `\b` boundary (so e.g. `abcC:\foo` doesn't match mid-word), but
// the relative-path branch must NOT require one: a leading `.` is a non-word char, so `\b` would
// only match when the dot is directly touching a preceding word char (no boundary exists after
// whitespace, punctuation, or line-start) — which is how virtually every real relative path is
// preceded. The `[\\/]` required immediately after the 1-2 leading dots already prevents this
// branch from matching ordinary prose (e.g. a sentence-ending `.` followed by a word).
export const FILE_PATH_RE = /\b[a-zA-Z]:[\\/][^\s<>()[\]]+|\.{1,2}[\\/][^\s<>()[\]]+/g;

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

// --- 非 ASCII 文字を含む URL の検出補正 (#300) ---
// ライブビュー(`@lezer/markdown` GFM autolink)とプレビュー(`linkify-it`)の両方が、ここの関数を通して
// 同じリンク範囲に揃える。純 ASCII の文字列は一切触らない(ASCII のリンク検出の挙動は従来のまま)。
// 非 ASCII を含むメール(#301)は `findNonAsciiEmailTokens` が同じ形で扱う。

const NON_ASCII_RE = /[^\x00-\x7f]/;
// 非 ASCII の句読点・記号・全角括弧(`。、，．！？）」』】〉》`、アラビア語の `،` `؟` 等)。URL はこの直前で切る
// ただし `・`(U+30FB)`〜`(U+301C)`～`(U+FF5E)は URL の文字として残す(Wikipedia 等のパスで使われるため)。
// 既知の限界: `https://x.jp/日本語・次の文` のように `・` の後に文が続く場合、`・` 以降も URL に含まれる
const NON_ASCII_PUNCT_RE = /(?=[^\x00-\x7f])(?![・〜～])[\p{P}\p{S}]/u;
// 補正対象は `http(s)://` と `www.` で始まるトークンのみ(メール等は対象外)
const URL_PREFIX_RE = /^(?:https?:\/\/|www\.)/i;
// 非 ASCII ホスト名の URL 候補(`https://` 直後〜最初の `/?#` までに非 ASCII 文字を含むもの)。
// 直前が ASCII の英数字・`_` なら語の途中とみなす(`@lezer/markdown` の autolink と同じ判定)
const NON_ASCII_URL_CANDIDATE_RE = /(?<![A-Za-z0-9_])https?:\/\/(?=[^\s<\/?#]*[^\x00-\x7f])[^\s<]+/gi;
// 補正後の URL がドメイン名(`.` 区切りの非 ASCII 対応ラベル)+任意のポート+任意のパスの形か
const NON_ASCII_URL_VALID_RE = /^https?:\/\/(?:[\p{L}\p{N}\p{M}_-]+\.)+[\p{L}\p{N}\p{M}_-]+(?::\d+)?(?:\/.*)?$/u;

function countChar(text: string, ch: string): number {
  let n = 0;
  for (const c of text) if (c === ch) n++;
  return n;
}

/** `@lezer/markdown` の autolink と同じ規則で、URL 末尾の ASCII 記号(`?!.,:*_~`、対応のない `)`、
 * 文字参照 `&...;`)を外す。 */
function trimAsciiUrlTail(url: string): string {
  let out = url;
  for (;;) {
    const last = out[out.length - 1];
    let m: RegExpExecArray | null;
    if (last !== undefined && (/[?!.,:*_~]/.test(last) || (last === ")" && countChar(out, ")") > countChar(out, "("))))
      out = out.slice(0, -1);
    else if (last === ";" && (m = /&(?:#\d+|#x[a-f\d]+|\w+);$/i.exec(out))) out = out.slice(0, m.index);
    else break;
  }
  return out;
}

/** 検出済み URL トークン(`http(s)://...`/`www....`)の末尾を補正して、リンクにする部分だけを返す。
 * 1. 最初の非 ASCII 句読点・記号・全角括弧の直前で切る。
 * 2. ホスト名の最後のラベルが ASCII の直後に非 ASCII 文字が続く形(`example.com日本語`)なら、ドメインの終わりで切る
 *    (ポートの直後に非 ASCII が続く場合も同様。パスは `/` で始まるときだけ残す)。
 * 3. 切ったときだけ、末尾の ASCII 記号(`?!.,:*_~`、対応のない `)`)を `@lezer/markdown` と同じ規則で外す。
 * 純 ASCII、または対象外の形式なら入力をそのまま返す。リンクにならない形になった場合は空文字を返す。 */
export function correctUrlRange(text: string): string {
  if (!NON_ASCII_RE.test(text)) return text;
  const prefix = URL_PREFIX_RE.exec(text);
  if (!prefix) return text;
  const punct = NON_ASCII_PUNCT_RE.exec(text);
  const rest = text.slice(prefix[0].length, punct ? punct.index : text.length);
  const hostMatch = /^([^/?#:\s]*)(:\d+)?/.exec(rest)!;
  const hostname = hostMatch[1];
  const after = rest.slice(hostMatch[0].length);
  const lastDot = hostname.lastIndexOf(".");
  const lastLabelTail = /^[\x00-\x7f]+(?=[^\x00-\x7f])/.exec(hostname.slice(lastDot + 1));
  let kept: string;
  if (lastLabelTail) kept = hostname.slice(0, lastDot + 1) + lastLabelTail[0];
  else if (after && !after.startsWith("/")) kept = hostMatch[0];
  else kept = hostMatch[0] + after;
  let out = prefix[0] + kept;
  if (out.length === text.length) return text;
  out = trimAsciiUrlTail(out);
  return out.length > prefix[0].length ? out : "";
}

/** テキストから、非 ASCII ホスト名(国際化ドメイン、`https://日本語.jp/abc` 等)を持つ `http(s)://` URL を探す。
 * `@lezer/markdown`/`linkify-it` が前者は ASCII ホストのみ、後者は直前が文字だと検出できない分を補う。
 * 補正後のホストが ASCII だけになるもの(`https://example.com日本語`)は対象外(既存の検出が扱う)。 */
export function findNonAsciiUrlTokens(text: string): { index: number; raw: string }[] {
  const results: { index: number; raw: string }[] = [];
  NON_ASCII_URL_CANDIDATE_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = NON_ASCII_URL_CANDIDATE_RE.exec(text))) {
    // 切り詰めが起きなかった場合も、GFM と同じ末尾 ASCII 記号の除去をかける(`correctUrlRange` は切ったときだけ除去する)
    const raw = trimAsciiUrlTail(correctUrlRange(m[0]));
    if (!raw || !NON_ASCII_URL_VALID_RE.test(raw)) continue;
    const host = /^https?:\/\/([^/?#]*)/i.exec(raw)![1];
    if (!NON_ASCII_RE.test(host)) continue;
    results.push({ index: m.index, raw });
  }
  return results;
}

/** 1 行のテキストから非 ASCII ホスト名の URL を探す(`scanBareFilePathTokens` と同じ呼び出し側の責務:
 * コード範囲の除外と、ツリーで検出済みの URL/Link 範囲との重複排除は呼び出し側で行う)。 */
export function scanNonAsciiUrlTokens(lineText: string, lineFrom: number): DetectedLink[] {
  return findNonAsciiUrlTokens(lineText).map((t) => ({ from: lineFrom + t.index, to: lineFrom + t.index + t.raw.length, kind: "url", target: t.raw }));
}

// --- 非 ASCII 文字を含むメールアドレスの検出 (#301) ---
// ローカル部またはドメイン部に非 ASCII を含むものだけが対象(ASCII のみのメールは従来どおり GFM/linkify-it が扱う)。
// 文字クラスは `\p{L}\p{N}\p{M}` のみなので、`。` `、` 全角括弧などの非 ASCII 句読点では自然に切れる。
// 直前が(ローカル部になりうる文字・`@`・`/`)なら語の途中・URL のパスとみなして開始しない。
const NON_ASCII_EMAIL_CANDIDATE_RE =
  /(?<![\p{L}\p{N}\p{M}_.+@\/-])[\p{L}\p{N}\p{M}_][\p{L}\p{N}\p{M}_.+-]*@(?:[\p{L}\p{N}\p{M}_-]+\.)+[\p{L}\p{N}\p{M}_-]*[\p{L}\p{N}\p{M}]/gu;

/** メールアドレスの最後のドメインラベルが ASCII の直後に非 ASCII が続く形(`user@example.com日本語`)なら、
 * ASCII の部分で切る(URL の `correctUrlRange` と同じ規則)。それ以外はそのまま返す。 */
export function correctEmailRange(raw: string): string {
  if (!NON_ASCII_RE.test(raw)) return raw;
  const lastDot = raw.lastIndexOf(".");
  const tail = /^[ -]+(?=[^ -])/.exec(raw.slice(lastDot + 1));
  return tail ? raw.slice(0, lastDot + 1) + tail[0] : raw;
}

/** テキストから、ローカル部またはドメイン部に非 ASCII 文字を含むメールアドレスを探す。
 * 最後のラベルが ASCII の直後に非 ASCII が続く形(`ユーザー@example.com日本語`)なら、URL(`correctUrlRange`)と同じく
 * ASCII の部分で切る。切った結果が ASCII のみになるものは対象外(既存の検出が扱う)。
 * URL 内の `@` などとの重複排除は呼び出し側の責務(コード範囲・既存リンク範囲との重複排除)。 */
export function findNonAsciiEmailTokens(text: string, includeAsciiAfterNonAscii = false): { index: number; raw: string }[] {
  const results: { index: number; raw: string }[] = [];
  NON_ASCII_EMAIL_CANDIDATE_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = NON_ASCII_EMAIL_CANDIDATE_RE.exec(text))) {
    let raw = correctEmailRange(m[0]);
    let index = m.index;
    // ローカル部に「非 ASCII → ASCII」の境界があるときは、最後の境界より後ろ(ASCII 側)だけをメールとして取る
    // (`連絡先はuser@example.com` は `user@...` のみ。日本語を含めない)。境界直後が `.` `+` `-` など
    // ローカル部の先頭になれない文字(`日本語.john@example.com`)なら、このトークンは対象外にする
    const at = raw.indexOf("@");
    let boundary = -1;
    for (const b of raw.slice(0, at).matchAll(/[^ -](?=[ -])/g)) boundary = b.index! + 1;
    if (boundary >= 0) {
      if (!/^[\p{L}\p{N}\p{M}_]/u.test(raw.slice(boundary))) continue;
      raw = raw.slice(boundary);
      index += boundary;
    }
    raw = raw.replace(/[-_]+$/, "");
    // #306: 日本語の直後に空白なしで続く ASCII のみのメール(`連絡先はuser@example.com`)は、GFM(ライブビュー)は
    // リンクにするが `linkify-it`(プレビュー)は見つけない。プレビューだけが `includeAsciiAfterNonAscii` で補う
    // (ライブビューは GFM が検出済みなので不要)。直前が非 ASCII のときだけ対象にし、通常の ASCII メールは従来どおり `linkify-it` に任せる
    // 直前が非 ASCII の句読点・括弧(`（user@example.com）`)の場合も同様。GFM は直前が `_` のときは開始しない
    const afterNonAscii = boundary >= 0 || (m.index > 0 && NON_ASCII_RE.test(text[m.index - 1]));
    if (!NON_ASCII_RE.test(raw)) {
      if (!includeAsciiAfterNonAscii || !afterNonAscii || raw.startsWith("_")) continue;
    }
    if (!looksLikeEmail(raw)) continue;
    results.push({ index, raw });
  }
  return results;
}

/** 1 行のテキストから非 ASCII を含むメールアドレスを探す(呼び出し側の責務は `scanNonAsciiUrlTokens` と同じ)。 */
export function scanNonAsciiEmailTokens(lineText: string, lineFrom: number): DetectedLink[] {
  return findNonAsciiEmailTokens(lineText).map((t) => ({ from: lineFrom + t.index, to: lineFrom + t.index + t.raw.length, kind: "email", target: t.raw }));
}

export function isMemoScheme(url: string): boolean {
  return url.startsWith("memo:");
}

export function isHeadingAnchor(url: string): boolean {
  return url.startsWith("#");
}

/** `memo:` link click resolution (`docs/plans/link/PLAN.md` §2, §5; `#slug` anchor suffix added by
 * `docs/plans/v1.2.0/cross-tab-anchor-links/PLAN.md` §2): strips the leading `memo:` scheme, splits
 * off an optional trailing `#slug` anchor BEFORE calling `link_resolve` (so the backend only ever
 * sees the memo-name portion, unaware `#slug` ever existed), and on a `"memo"` result opens it via
 * `onOpenMemo` — the caller's hook into the shared "open memo" path (`docs/plans/memo-tree/
 * PLAN.md`, the same one tree click and search-result click use), now also passed the parsed
 * `anchorSlug` (`null` when the target had no `#`). `"unresolved"` (or a failed IPC call) is a
 * silent no-op, matching `[spec §2.8]`'s "unresolved links do nothing" — no toast, no error
 * surfaced; the anchor is never read/passed in that case either, since there is no memo to scroll
 * within. No `decodeURIComponent` step here — `rawTarget` reaching this function is already clean
 * of `normalizeLink` encoding by the time it arrives (`preview-link-encoding-fix/PLAN.md` §3
 * decodes the preview pane's own `href` upstream; a live-view-sourced `rawTarget` was never encoded
 * to begin with), so this function stays agnostic to which pane called it. */
export async function resolveAndOpenMemoLink(
  currentTabId: string | null,
  rawTarget: string,
  onOpenMemo: (tabId: string, memoName: string | null, anchorSlug: string | null) => void,
): Promise<void> {
  if (!currentTabId) return;
  const stripped = rawTarget.startsWith("memo:") ? rawTarget.slice("memo:".length) : rawTarget;
  const hashIndex = stripped.indexOf("#");
  const target = hashIndex === -1 ? stripped : stripped.slice(0, hashIndex);
  const anchorSlug = hashIndex === -1 ? null : stripped.slice(hashIndex + 1);
  const result = await linkResolve(currentTabId, target);
  if (!result.ok || result.value.kind !== "memo") return;
  onOpenMemo(result.value.tabId, result.value.memoName, anchorSlug);
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
async function openExternalLink(kind: ExternalLinkKind, value: string, config: MarkdownDecorationConfig, extendedSearchText?: string): Promise<void> {
  if (config.confirmBeforeOpen) {
    const confirmed = await config.requestExternalConfirm(kind, value);
    if (!confirmed) return;
  }
  await linkOpenExternal(kind, value, config.currentTabId ?? undefined, extendedSearchText);
}

export async function openDetectedLink(view: EditorView, link: DetectedLink, config: MarkdownDecorationConfig): Promise<void> {
  switch (link.kind) {
    case "url":
      await openExternalLink("url", link.target, config);
      return;
    case "email":
      await openExternalLink("email", link.target, config);
      return;
    case "filePath": {
      // Issue #235: widen the click-time search to the rest of the source line (never the
      // decoration's own extent — see this module's `FILE_PATH_RE` comment) so a real path
      // containing a space or `(`/`)` still resolves; the backend trims this back down word by
      // word against the real filesystem (`link_open_external`'s `resolve_file_path_widened`).
      const line = view.state.doc.lineAt(link.from);
      const extendedSearchText = line.text.slice(link.from - line.from);
      await openExternalLink("filePath", link.target, config, extendedSearchText);
      return;
    }
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
