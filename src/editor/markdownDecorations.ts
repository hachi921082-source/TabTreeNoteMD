// The core live-view decoration engine (`docs/plans/markdown-editing/PLAN.md` §2). A `ViewPlugin`
// that recomputes a `DecorationSet` from the current document + selection on every relevant
// update, driven by `@lezer/markdown`'s (via `@codemirror/lang-markdown`) real CommonMark+GFM
// syntax tree — headings, lists (bullet/numbered/checklist), blockquotes, code blocks/inline code,
// tables, bold/italic/strikethrough, horizontal rules, links (`memo:` scheme included), image
// placeholders, and the allowlisted `<span style="...">`/`<img>` HTML fragments
// (`markdownSpanParser.ts`).
//
// **Risk-reduction strategy for `[spec §8]`'s high-risk defect zone** (`docs/plans/markdown-
// editing/PLAN.md` §9): every block/inline-syntax decoration below (headings, emphasis,
// strikethrough, inline code, list markers, quote markers, blockquote/code-block/table line
// styling, link label styling) is implemented as a `Decoration.mark`/`Decoration.line` — a pure
// CSS-class overlay that never changes the visible-vs-source character count. Markers are never
// hidden. This sidesteps the classic "hidden syntax marker vs. visible text length mismatch"
// cursor-mapping bug entirely for that whole class of decoration, rather than mitigating it after
// the fact — there is nothing to map, since no text is ever replaced. `Decoration.replace`/widget
// decorations are used ONLY for: (a) task-list checkboxes (`checkboxClick.ts`, an atomic
// fixed-width `[ ]`/`[x]` replacement), (b) allowlisted `<span>`/`<img>` HTML tag markup and image
// placeholders, both of which additionally reveal their raw source text whenever the current
// selection touches the range (`touchesSelection` below) so editing at/inside them is always
// editing plain visible text, never a hidden decoration boundary.
import { Facet, StateEffect } from "@codemirror/state";
import type { EditorState, Extension, Range } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType } from "@codemirror/view";
import type { DecorationSet, PluginValue, ViewUpdate } from "@codemirror/view";
import { syntaxTree } from "@codemirror/language";
import type { SyntaxNodeRef } from "@lezer/common";
import { checkboxDecoration } from "./checkboxClick";
import { findColorSpans, findImageTags, findMarkSpans, findMathSpans } from "./markdownSpanParser";
import type { DetectedLink, LinkKind } from "./links";
import { correctUrlRange, linkClickMatchesTrigger, looksLikeEmail, openDetectedLink, scanBareFilePathTokens, scanBareMemoTokens, scanNonAsciiUrlTokens } from "./links";
import { getOrLoadImage, hasImageLoadFailed } from "./imageLoader";
import type { AppSettings, ExternalLinkKind } from "../shared/models";

/** Dispatched (as an empty-document-change, effect-only transaction) by `ImageWidget.toDOM()`'s
 * async load-failure branch below, in place of the raw `contentDOM` mutation this class used to do
 * (`img.replaceWith(...)`) — that mutation ran outside any CodeMirror transaction, so CM6's
 * `DOMObserver` (which watches `contentDOM` to reconcile IME/spellcheck/extension-driven external
 * edits back into document state) misread the placeholder's text content as a user edit and
 * corrupted the document on the next autosave. Dispatching this effect instead makes
 * `MarkdownDecorationsPlugin.update()` (below) rebuild the decoration set through CodeMirror's own
 * transaction-driven path; the rebuilt set consults `hasImageLoadFailed` and emits a real
 * `ImagePlaceholderWidget` decoration, which CodeMirror itself swaps into `contentDOM` — a change
 * `DOMObserver` correctly attributes to the editor, not the user. */
const imageLoadFailedEffect = StateEffect.define<void>();

export interface MarkdownDecorationConfig {
  openTrigger: AppSettings["links"]["openTrigger"];
  /** `true` while the memo is read-only (edit-locked, or a conflict dialog is open) — gates the
   * checkbox widget's click-to-toggle (`checkboxClick.ts`). Link clicking and all styling remain
   * active regardless (opening a link/viewing formatting isn't "editing"). */
  readOnly: boolean;
  /** Tab the currently-open memo belongs to (`docs/plans/link/PLAN.md` §2) — passed to
   * `link_resolve`/`link_open_external` for `memo:` resolution and relative file-path base
   * resolution. `null` only in contexts with no tab open at all. */
  currentTabId: string | null;
  /** `AppSettings.links.confirmBeforeOpen` (`[spec §2.8]`) — gates `requestExternalConfirm` below
   * before every external (URL/email/file-path) open; `memo:`/`#slug` links never go through this
   * gate since they never leave the app (`docs/plans/link/PLAN.md` §2, §4). */
  confirmBeforeOpen: boolean;
  /** Reuses the shared "open memo" path (`docs/plans/memo-tree/PLAN.md`, the same one tree click
   * and search-result click use) — `memoName: null` for the `memo:TabName/` (tab-only) resolved
   * form. `anchorSlug` is the parsed `#slug` half of a `memo:...#slug` link (`docs/plans/v1.2.0/
   * cross-tab-anchor-links/PLAN.md` §2), `null` when the link carried no anchor. */
  onOpenMemo: (tabId: string, memoName: string | null, anchorSlug: string | null) => void;
  /** Shows the shared non-blocking confirm dialog and resolves to whether the user confirmed;
   * only awaited when `confirmBeforeOpen` is true (`docs/plans/link/PLAN.md` §4). */
  requestExternalConfirm: (kind: ExternalLinkKind, value: string) => Promise<boolean>;
  /** Identifies the currently-open memo for image-cache scoping (§5.4 of
   * `docs/plans/markdown/image-handling/PLAN.md`). `null` only when no memo is open at all, in
   * which case no `Image`/`<img>` node can be reached anyway (there is no document to decorate). */
  memoKey: string | null;
  /** Already-translated checkbox-widget tooltip strings (`checkboxClick.ts`'s `CheckboxWidget`) —
   * resolved via `useT()` by the React layer (this module/`checkboxClick.ts` has no React context
   * of its own) and reconfigured on `liveViewCompartment` whenever `markdownConfig` changes,
   * including a language change (`docs/plans/v1.2.0/i18n/PLAN.md` §6/§7 follow-up). Shown while the
   * box IS checked ("click to uncheck") / is NOT checked ("click to check") respectively. */
  checkboxUncheckTooltip: string;
  checkboxCheckTooltip: string;
}

const DEFAULT_CONFIG: MarkdownDecorationConfig = {
  openTrigger: "singleClick",
  readOnly: false,
  currentTabId: null,
  confirmBeforeOpen: false,
  onOpenMemo: () => {},
  requestExternalConfirm: async () => true,
  memoKey: null,
  checkboxUncheckTooltip: "",
  checkboxCheckTooltip: "",
};

export const markdownConfigFacet = Facet.define<MarkdownDecorationConfig, MarkdownDecorationConfig>({
  combine: (values) => (values.length ? values[values.length - 1] : DEFAULT_CONFIG),
});

type Range2 = { from: number; to: number };

function touchesSelection(state: EditorState, from: number, to: number): boolean {
  return state.selection.ranges.some((r) => r.from <= to && r.to >= from);
}

function overlapsAny(ranges: Range2[], from: number, to: number): boolean {
  return ranges.some((r) => from < r.to && to > r.from);
}

function forEachLine(state: EditorState, from: number, to: number, cb: (line: { from: number; to: number }) => void): void {
  let pos = from;
  // Defensive bound: a malformed/huge single "line" (no newline) still terminates in one
  // iteration since `line.to >= to` is checked before advancing.
  for (;;) {
    const line = state.doc.lineAt(Math.min(pos, state.doc.length));
    cb(line);
    if (line.to >= to || line.to >= state.doc.length) break;
    pos = line.to + 1;
  }
}

class ImagePlaceholderWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly alt: string,
  ) {
    super();
  }

  eq(other: ImagePlaceholderWidget): boolean {
    return other.src === this.src && other.alt === this.alt;
  }

  toDOM(): HTMLElement {
    // Real `<img>` rendering (`docs/plans/markdown/image-handling/PLAN.md` §6.2, `ImageWidget`
    // below) now handles the normal case. This widget is reached only as its fallback: a
    // genuinely missing/unloadable image (`imageLoader.ts`'s `hasImageLoadFailed`) or a null
    // tab/memo context (`ImageWidget.toDOM()`'s early-return branch) — hence the "not found"
    // wording rather than a "not implemented yet" placeholder.
    const el = document.createElement("span");
    el.className = "cm-md-image-placeholder";
    el.textContent = `🖼 ${this.alt || this.src}`;
    el.title = `image not found: ${this.src}`;
    return el;
  }

  ignoreEvent(): boolean {
    return false;
  }
}

/** Real `<img>` rendering for the live view (`docs/plans/markdown/image-handling/PLAN.md` §6.2),
 * replacing `ImagePlaceholderWidget` at both of its call sites below. `ImagePlaceholderWidget`
 * itself is kept (not deleted) as the read-failure/no-context fallback's DOM builder. */
class ImageWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly alt: string,
    readonly tabId: string | null,
    readonly memoKey: string | null,
  ) {
    super();
  }

  eq(other: ImageWidget): boolean {
    return other.src === this.src && other.alt === this.alt && other.tabId === this.tabId && other.memoKey === this.memoKey;
  }

  toDOM(view: EditorView): HTMLElement {
    // No tab/memo context at all — nothing to resolve against; render the fallback immediately
    // rather than attempting a call that can't succeed. This is a synchronous, pre-mount build of
    // the returned element (no later mutation of an already-attached `contentDOM` node), so it's
    // unrelated to the transaction-safety concern below.
    if (!this.tabId || !this.memoKey) {
      return new ImagePlaceholderWidget(this.src, this.alt).toDOM();
    }
    const img = document.createElement("img");
    img.className = "cm-md-image cm-md-image-loading";
    img.alt = this.alt;
    img.title = this.src;
    const tabId = this.tabId;
    const memoKey = this.memoKey;
    const src = this.src;
    // Lazy by construction: this fetch only starts once CodeMirror actually calls toDOM() for a
    // widget that entered the viewport, not eagerly during decoration-set build.
    void getOrLoadImage(tabId, src, memoKey).then((blobUrl) => {
      if (blobUrl) {
        // Safe in-place attribute mutation — no text nodes involved, so `DOMObserver` (which
        // watches `contentDOM` for `childList`/`characterData` changes to reconcile external edits
        // like IME/spellcheck) has nothing to misinterpret here.
        img.src = blobUrl;
        img.classList.remove("cm-md-image-loading");
      } else {
        // Read-failure fallback. Deliberately does NOT mutate `img`/`contentDOM` directly here
        // (that was the corruption bug — see `imageLoadFailedEffect` above): `getOrLoadImage`
        // resolving to `null` means `imageLoader.ts` already recorded this path as failed
        // (`failedKeys`), so all that's needed is to ask CodeMirror to rebuild decorations; the
        // rebuild itself (in `build()` below, via `hasImageLoadFailed`) is what actually swaps in
        // `ImagePlaceholderWidget`, through CodeMirror's own transaction-driven DOM reconciliation.
        view.dispatch({ effects: imageLoadFailedEffect.of() });
      }
    });
    return img;
  }

  ignoreEvent(): boolean {
    return false;
  }
}

/** Path data copied from `lucide-react`'s own `Sigma` icon (`node_modules/lucide-react/dist/esm/
 * icons/sigma.mjs`, `lucide-react@1.28.0` as pinned in `package.json` at implementation time) — not
 * imported directly, since that per-icon module is an untyped internal build-output path (no sibling
 * `.d.ts`, unlike the package's public `import { Sigma } from "lucide-react"` React-component
 * surface `toolbarSlots.ts`/`MarkdownToolbar.tsx` already use elsewhere). `WidgetType.toDOM()`
 * builds a raw CodeMirror-owned DOM node that React cannot render into (`docs/plans/v1.2.0/
 * math-rendering/PLAN.md` §3's own constraint), so `createSigmaIconSvg` below hand-assembles the
 * identical `<svg>` markup the `<Sigma />` component would produce via plain DOM APIs instead —
 * same icon, same already-bundled dependency's own path data, zero new dependency. Re-copy this
 * path data (and `SIGMA_ICON_VIEWBOX_ATTRS` below) if `lucide-react` is ever upgraded and the icon's
 * shape changes upstream. */
const SIGMA_ICON_PATH_D =
  "M18 7V5a1 1 0 0 0-1-1H6.5a.5.5 0 0 0-.4.8l4.5 6a2 2 0 0 1 0 2.4l-4.5 6a.5.5 0 0 0 .4.8H17a1 1 0 0 0 1-1v-2";

/** `lucide-react`'s own `defaultAttributes` (`node_modules/lucide-react/dist/esm/
 * defaultAttributes.mjs`) that every icon's `<svg>` root carries, reproduced here verbatim (minus
 * `width`/`height`, sized down separately below for this badge's own small inline footprint). */
const SIGMA_ICON_VIEWBOX_ATTRS: Record<string, string> = {
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  "stroke-width": "2",
  "stroke-linecap": "round",
  "stroke-linejoin": "round",
};

function createSigmaIconSvg(): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("width", "12");
  svg.setAttribute("height", "12");
  for (const [key, value] of Object.entries(SIGMA_ICON_VIEWBOX_ATTRS)) svg.setAttribute(key, value);
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("cm-md-math-placeholder-icon");
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("d", SIGMA_ICON_PATH_D);
  svg.appendChild(path);
  return svg;
}

/** Live-view "this is a math formula" badge (`docs/plans/v1.2.0/math-rendering/PLAN.md` §3) —
 * follows `ImagePlaceholderWidget`'s own precedent one-for-one: a short, non-interactive
 * `Decoration.replace` widget standing in for the raw `$...$`/`$$...$$` source text while the
 * current selection doesn't touch it (see `buildMathWidgetRanges`'s call site in `build()` below,
 * same `touchesSelection`-gated reveal-on-cursor-touch pattern every other `Decoration.replace`
 * widget in this file already follows — this file's own header comment explains why: hidden text
 * must always be revealed the moment the selection touches it, so there is never a hidden-vs-visible
 * character-count mismatch to reason about). Unlike `ImageWidget`, genuinely non-interactive: no
 * `ignoreEvent()` override (the `WidgetType` base class default, `return true`, means CodeMirror
 * treats clicks on this widget as ordinary cursor placement — no special click handling registered
 * anywhere for it, matching `PLAN.md` §3's explicit "clicking the math badge does nothing special"
 * requirement). */
class MathPlaceholderWidget extends WidgetType {
  constructor(readonly raw: string) {
    super();
  }

  eq(other: MathPlaceholderWidget): boolean {
    return other.raw === this.raw;
  }

  toDOM(): HTMLElement {
    const el = document.createElement("span");
    el.className = "cm-md-math-placeholder";
    el.appendChild(createSigmaIconSvg());
    el.appendChild(document.createTextNode(" formula"));
    el.title = this.raw;
    return el;
  }
}

/** Collects a `Link`/`Image` node's `LinkMark` children (in document order) and its `URL` child,
 * if any. Reference-style links (`[ref][1]`, no `URL` child) and any other shape this returns
 * `null` `url` for are left entirely undecorated — safe default, raw text still renders normally. */
function collectLinkParts(node: SyntaxNodeRef): { marks: Range2[]; url: Range2 | null } {
  const marks: Range2[] = [];
  let url: Range2 | null = null;
  let child = node.node.firstChild;
  while (child) {
    if (child.type.name === "LinkMark") marks.push({ from: child.from, to: child.to });
    else if (child.type.name === "URL") url = { from: child.from, to: child.to };
    child = child.nextSibling;
  }
  return { marks, url };
}

/** Picks which image widget a decoration should use, consulting `imageLoader.ts`'s known-failed-path
 * record (`hasImageLoadFailed`) first. A path already known to have failed is given a real
 * `ImagePlaceholderWidget` decoration directly here — no `ImageWidget`/no fresh async attempt — so
 * the corrupting-DOM-mutation bug this replaces (see `imageLoadFailedEffect` above) can't recur even
 * on the very rebuild that's reacting to the failure: by the time `build()` runs again,
 * `hasImageLoadFailed` is already true for `src`, and this function returns the placeholder outright
 * rather than constructing another `ImageWidget` that would just fail the same async load again. */
function buildImageWidget(src: string, alt: string, config: MarkdownDecorationConfig): WidgetType {
  if (config.currentTabId && config.memoKey && hasImageLoadFailed(config.currentTabId, src, config.memoKey)) {
    return new ImagePlaceholderWidget(src, alt);
  }
  return new ImageWidget(src, alt, config.currentTabId, config.memoKey);
}

function classifyUrl(target: string): LinkKind {
  if (target.startsWith("memo:")) return "memo";
  if (target.startsWith("#")) return "headingAnchor";
  if (looksLikeEmail(target)) return "email";
  return "url";
}

const HEADING_CLASS: Record<number, string> = { 1: "cm-md-h1", 2: "cm-md-h2", 3: "cm-md-h3", 4: "cm-md-h4", 5: "cm-md-h5", 6: "cm-md-h6" };

class MarkdownDecorationsPlugin implements PluginValue {
  decorations: DecorationSet;
  private links: DetectedLink[] = [];
  // Ranges of currently-*rendered* images (`Image` lezer nodes and raw `<img>` tags alike) —
  // populated in `build()` below only in the `!touchesSelection` branch, i.e. only while the widget
  // decoration is actually active. Consulted by `handleImageMouseDown` (GitHub issue #80) so a
  // plain click can special-case "the cursor landed on a rendered image" without affecting clicks
  // on the same span once it's already showing raw text for editing (at that point this list
  // simply omits the range, so normal cursor placement applies).
  private imageRanges: Range2[] = [];

  constructor(view: EditorView) {
    this.decorations = this.build(view);
  }

  update(update: ViewUpdate): void {
    // `imageLoadFailedEffect` (see above `ImageWidget.toDOM()`) carries no document/selection/
    // viewport change of its own — it's an empty-change, effect-only transaction dispatched purely
    // to trigger this rebuild in a transaction-safe way, so it needs its own explicit check here.
    const imageLoadJustFailed = update.transactions.some((tr) => tr.effects.some((e) => e.is(imageLoadFailedEffect)));
    if (update.docChanged || update.selectionSet || update.viewportChanged || imageLoadJustFailed) {
      this.decorations = this.build(update.view);
    }
  }

  private build(view: EditorView): DecorationSet {
    const { state } = view;
    const config = state.facet(markdownConfigFacet);
    const ranges: Range<Decoration>[] = [];
    const links: DetectedLink[] = [];
    const imageRanges: Range2[] = [];
    // Ranges to exclude the color-span/image-tag regex scans from (fenced/indented code, inline
    // code) — so a `<span>`/`<img>` shown literally as example text inside a code block isn't
    // treated as a live decoration. Deliberately does NOT include Link/Image node ranges: a
    // `<span style="color:...">` commonly WRAPS a `[label](url)` link (§9 item 5's own required
    // nested-decoration test case), and excluding on Link/Image overlap would wrongly suppress the
    // color span whenever it contains one.
    const codeSkipRanges: Range2[] = [];
    // Superset of `codeSkipRanges` plus every Link/Image node's full range — used only by the bare
    // `memo:`/file-path token scan, so a bare token that happens to sit inside a `[label](url)`'s
    // parenthesized URL isn't separately, redundantly detected as its own bare auto-link.
    const linkNodeRanges: Range2[] = [];
    // ツリーで検出した裸 URL(範囲補正後)と `<url>` 形式のオートリンクの範囲。非 ASCII ドメイン URL の
    // 走査(`scanNonAsciiUrlTokens`)だけがこれを除外に使う(`linkNodeRanges` に混ぜると既存の
    // memo:/ファイルパス走査の挙動が変わるため別リストにしている)
    const urlNodeRanges: Range2[] = [];

    const addMark = (from: number, to: number, deco: Decoration) => {
      if (from < to) ranges.push(deco.range(from, to));
    };
    const addLine = (line: Range2, deco: Decoration) => {
      ranges.push(deco.range(line.from));
    };
    const addReplace = (from: number, to: number, deco: Decoration) => {
      ranges.push(deco.range(from, to));
    };

    syntaxTree(state).iterate({
      enter: (node) => {
        switch (node.name) {
          case "ATXHeading1":
          case "ATXHeading2":
          case "ATXHeading3":
          case "ATXHeading4":
          case "ATXHeading5":
          case "ATXHeading6": {
            const level = Number(node.name.slice(-1));
            addMark(node.from, node.to, Decoration.mark({ class: HEADING_CLASS[level] }));
            break;
          }
          case "Emphasis":
            addMark(node.from, node.to, Decoration.mark({ class: "cm-md-em" }));
            break;
          case "StrongEmphasis":
            addMark(node.from, node.to, Decoration.mark({ class: "cm-md-strong" }));
            break;
          case "Strikethrough":
            addMark(node.from, node.to, Decoration.mark({ class: "cm-md-strike" }));
            break;
          case "InlineCode":
            addMark(node.from, node.to, Decoration.mark({ class: "cm-md-inline-code" }));
            codeSkipRanges.push({ from: node.from, to: node.to });
            break;
          case "ListMark":
            addMark(node.from, node.to, Decoration.mark({ class: "cm-md-list-mark" }));
            break;
          case "QuoteMark":
            addMark(node.from, node.to, Decoration.mark({ class: "cm-md-quote-mark" }));
            break;
          case "HorizontalRule":
            addLine(state.doc.lineAt(node.from), Decoration.line({ class: "cm-md-hr" }));
            break;
          case "Blockquote":
            forEachLine(state, node.from, node.to, (line) => addLine(line, Decoration.line({ class: "cm-md-blockquote" })));
            break;
          case "FencedCode":
          case "CodeBlock":
            codeSkipRanges.push({ from: node.from, to: node.to });
            forEachLine(state, node.from, node.to, (line) => addLine(line, Decoration.line({ class: "cm-md-codeblock" })));
            break;
          case "Table":
            forEachLine(state, node.from, node.to, (line) => addLine(line, Decoration.line({ class: "cm-md-table-line" })));
            break;
          case "TaskMarker": {
            const text = state.sliceDoc(node.from, node.to);
            const checked = /\[[xX]\]/.test(text);
            addReplace(
              node.from,
              node.to,
              checkboxDecoration(checked, node.from, node.to, config.readOnly, config.checkboxUncheckTooltip, config.checkboxCheckTooltip),
            );
            break;
          }
          case "Link":
          case "Image": {
            linkNodeRanges.push({ from: node.from, to: node.to });
            const { marks, url } = collectLinkParts(node);
            if (marks.length < 2 || !url) break; // reference-style / malformed — leave undecorated
            const labelFrom = marks[0].to;
            const labelTo = marks[1].from;
            const urlText = state.sliceDoc(url.from, url.to);
            if (node.name === "Image") {
              if (!touchesSelection(state, node.from, node.to)) {
                const altText = state.sliceDoc(labelFrom, labelTo);
                // `@lezer/markdown` の URL ノードは山括弧形 `<新 メモ/001.png>` の `<` `>` を含むので外す(GitHub issue #266)
                const src = urlText.length >= 2 && urlText.startsWith("<") && urlText.endsWith(">") ? urlText.slice(1, -1) : urlText;
                addReplace(node.from, node.to, Decoration.replace({ widget: buildImageWidget(src, altText, config) }));
                imageRanges.push({ from: node.from, to: node.to });
              }
              break;
            }
            const kind = classifyUrl(urlText);
            const target = kind === "headingAnchor" ? urlText.replace(/^#/, "") : urlText;
            links.push({ from: labelFrom, to: labelTo, kind, target });
            addMark(labelFrom, labelTo, Decoration.mark({ class: "cm-md-link" }));
            addMark(node.from, labelFrom, Decoration.mark({ class: "cm-md-link-syntax" }));
            addMark(labelTo, node.to, Decoration.mark({ class: "cm-md-link-syntax" }));
            break;
          }
          case "URL": {
            const parentName = node.node.parent?.type.name;
            if (parentName === "Link" || parentName === "Image") break; // handled via the parent above
            // 非 ASCII の句読点・全角括弧・ドメイン直後の非 ASCII 文字までリンクに含めないよう範囲を補正する(#300)。
            // 純 ASCII の URL・メールは correctUrlRange がそのまま返すので挙動は変わらない
            const text = correctUrlRange(state.sliceDoc(node.from, node.to));
            if (!text) break;
            const to = node.from + text.length;
            const kind: LinkKind = looksLikeEmail(text) ? "email" : "url";
            links.push({ from: node.from, to, kind, target: text });
            addMark(node.from, to, Decoration.mark({ class: "cm-md-link" }));
            urlNodeRanges.push({ from: node.from, to });
            break;
          }
          case "Autolink": {
            urlNodeRanges.push({ from: node.from, to: node.to });
            const { marks, url } = collectLinkParts(node);
            if (marks.length < 1 || !url) break;
            const text = state.sliceDoc(url.from, url.to);
            const kind: LinkKind = looksLikeEmail(text) ? "email" : "url";
            links.push({ from: url.from, to: url.to, kind, target: text });
            addMark(url.from, url.to, Decoration.mark({ class: "cm-md-link" }));
            break;
          }
        }
      },
    });

    // Bare `memo:...`/file-path tokens (`links.ts`) and the allowlisted `<span>`/`<img>` HTML
    // fragments (`markdownSpanParser.ts`) — regex-scanned over the raw text since neither is part
    // of the Lezer grammar.
    const docText = state.doc.toString();
    const bareTokenSkipRanges = [...codeSkipRanges, ...linkNodeRanges];
    const nonAsciiUrlSkipRanges = [...bareTokenSkipRanges, ...urlNodeRanges];
    for (let lineNo = 1; lineNo <= state.doc.lines; lineNo++) {
      const line = state.doc.line(lineNo);
      for (const link of [...scanBareMemoTokens(line.text, line.from), ...scanBareFilePathTokens(line.text, line.from)]) {
        if (overlapsAny(bareTokenSkipRanges, link.from, link.to)) continue;
        links.push(link);
        addMark(link.from, link.to, Decoration.mark({ class: "cm-md-link" }));
      }
      // 非 ASCII ドメインの URL(`https://日本語.jp/abc` 等)は GFM autolink が検出しないので別途走査する(#300)
      for (const link of scanNonAsciiUrlTokens(line.text, line.from)) {
        if (overlapsAny(nonAsciiUrlSkipRanges, link.from, link.to)) continue;
        links.push(link);
        addMark(link.from, link.to, Decoration.mark({ class: "cm-md-link" }));
      }
    }
    for (const span of findColorSpans(docText)) {
      if (overlapsAny(codeSkipRanges, span.openFrom, span.closeTo)) continue;
      if (span.style) addMark(span.contentFrom, span.contentTo, Decoration.mark({ attributes: { style: span.style } }));
      // Check open/close tag ranges independently — a cursor merely sitting inside the styled
      // content (not touching either tag) must not reveal the tags. See issue #89.
      if (!touchesSelection(state, span.openFrom, span.openTo)) {
        addReplace(span.openFrom, span.openTo, Decoration.replace({}));
      }
      if (!touchesSelection(state, span.closeFrom, span.closeTo)) {
        addReplace(span.closeFrom, span.closeTo, Decoration.replace({}));
      }
    }
    for (const span of findMarkSpans(docText)) {
      if (overlapsAny(codeSkipRanges, span.openFrom, span.closeTo)) continue;
      addMark(span.contentFrom, span.contentTo, Decoration.mark({ class: "cm-md-mark", attributes: span.style ? { style: span.style } : undefined }));
      // Same open/close-tag-only reveal-on-touch behavior as the color-span loop above (issue #89).
      if (!touchesSelection(state, span.openFrom, span.openTo)) {
        addReplace(span.openFrom, span.openTo, Decoration.replace({}));
      }
      if (!touchesSelection(state, span.closeFrom, span.closeTo)) {
        addReplace(span.closeFrom, span.closeTo, Decoration.replace({}));
      }
    }
    for (const img of findImageTags(docText)) {
      if (overlapsAny(codeSkipRanges, img.from, img.to)) continue;
      if (!touchesSelection(state, img.from, img.to)) {
        addReplace(img.from, img.to, Decoration.replace({ widget: buildImageWidget(img.src, "", config) }));
        imageRanges.push({ from: img.from, to: img.to });
      }
    }
    // Live-view math badge (`PLAN.md` §3) — same code-fence exclusion and touch-to-reveal gating as
    // every other `Decoration.replace` scan above.
    for (const math of findMathSpans(docText)) {
      if (overlapsAny(codeSkipRanges, math.from, math.to)) continue;
      const matchedText = docText.slice(math.from, math.to);
      // A multi-line `$$...$$` block formula's matched range spans a line break — CM6 throws
      // `RangeError: Decorations that replace line breaks may not be specified via plugins` when a
      // `ViewPlugin` (as opposed to a `StateField`) issues such a replace, which crashes the whole
      // app with no error boundary to catch it (issue #194). Skip the badge for those; the raw
      // `$$...$$` source just stays visible as plain text in live view instead.
      if (matchedText.includes("\n")) continue;
      if (!touchesSelection(state, math.from, math.to)) {
        addReplace(math.from, math.to, Decoration.replace({ widget: new MathPlaceholderWidget(matchedText) }));
      }
    }

    links.sort((a, b) => a.from - b.from);
    this.links = links;
    this.imageRanges = imageRanges;
    // `sort: true` — ranges above are gathered in tree-iteration + regex-scan order, not
    // necessarily position order; `Decoration.set` handles the from/side ordering rules that a
    // manual `RangeSetBuilder` would otherwise require the caller to get exactly right, which
    // matters a great deal given this feature's own flagged high-risk-of-mis-mapping status.
    return Decoration.set(ranges, true);
  }

  // Not `private` — CM6's `ViewPlugin.fromClass(..., { eventHandlers })` methods are plain object
  // properties defined outside this class body but called with `this` bound to the plugin
  // instance at runtime; TypeScript's `private` is a compile-time-only, lexically-scoped check
  // that would reject that access pattern even though it's safe.
  linkAtPos(pos: number): DetectedLink | null {
    return this.links.find((l) => pos >= l.from && pos <= l.to) ?? null;
  }

  handleMouseEvent(event: MouseEvent, view: EditorView, eventType: "click" | "dblclick"): boolean {
    // 右クリック・中クリックではリンクを開かない(右クリックメニュー「リンクをコピー」のため)。
    if (event.button !== 0) return false;
    const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
    if (pos == null) return false;
    const link = this.links.find((l) => pos >= l.from && pos <= l.to);
    if (!link) return false;
    const config = view.state.facet(markdownConfigFacet);
    if (!linkClickMatchesTrigger(event, config.openTrigger, eventType)) return false;
    event.preventDefault();
    void openDetectedLink(view, link, config);
    return true;
  }

  // GitHub issue #80 (and its follow-up regression): CodeMirror's default click-to-cursor behavior
  // places the cursor at a boundary of the clicked `Decoration.replace` range, which immediately
  // satisfies `touchesSelection()` (a *closed*-interval touch test — see its definition above, both
  // `from` and `to` count as touching) and reveals the raw `![alt](src)`/`<img ...>` text on the
  // very next rebuild — so a plain click to just look at a rendered image had the same effect as
  // intentionally starting to edit it.
  //
  // The original #80 fix tried to dodge this by moving the cursor to `img.to` ("just past" the
  // image) on every mousedown, on the theory that `to` sits outside the range. It doesn't:
  // `touchesSelection()` treats `to` as touching just like `from`, so that dispatch re-revealed the
  // image on its own — on a double-click's *first* mousedown, before the second mousedown/dblclick
  // ever ran. That reveal can reflow the surrounding text (raw Markdown source is usually wider
  // than the rendered widget), shifting later lines under the still-stationary physical pointer, so
  // by the time the second click/dblclick fired it could land on unrelated content (e.g. a heading
  // that scrolled into the click point) — this was the reported "cursor ends up on a following
  // heading" bug. A separate `dblclick` handler trying to "undo" `mousedown`'s move afterward only
  // added an ordering dependency on top of the underlying reveal bug.
  //
  // Fixed by using `event.detail` (native click-count: 1 for a plain click, >=2 for a double/triple
  // click) to decide the outcome in this single handler, with no dispatch at all for a plain click:
  // `img` is only ever in `imageRanges` while the current selection does NOT touch it (`build()`
  // above only pushes into `imageRanges` inside the `!touchesSelection` branch), so simply leaving
  // the selection untouched provably keeps it that way — there is no position, including `img.to`,
  // that's guaranteed to stay outside the closed-interval touch test, so "don't move it" is the only
  // reliably-safe choice. `img.from` for a double-click is unambiguous: it's the image's own left
  // edge, always inside the closed interval, so it always reveals. This removes the mousedown/
  // dblclick ordering dependency entirely — no separate `dblclick` handler for images is needed.
  handleImageMouseDown(event: MouseEvent, view: EditorView): boolean {
    const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
    if (pos == null) return false;
    const img = this.imageRanges.find((r) => pos >= r.from && pos <= r.to);
    if (!img) return false;
    event.preventDefault();
    if (event.detail >= 2) {
      view.dispatch({ selection: { anchor: img.from } });
    }
    return true;
  }
}

/** The Markdown live-view decoration extension bundle — pass `config` and reconfigure the
 * enclosing compartment (`codeMirrorSetup.ts`'s `liveViewCompartment`) whenever it changes, same
 * pattern as `editor/textCommands.ts`'s other compartmentalized settings. */
export function markdownDecorationExtension(config: MarkdownDecorationConfig): Extension {
  return [markdownConfigFacet.of(config), markdownDecorationsViewPlugin];
}

/** 画面座標に位置するリンクを返す。クリック判定と同じ検出結果(this.links)を再利用する。 */
export function findLinkAtCoords(view: EditorView, x: number, y: number): DetectedLink | null {
  const pos = view.posAtCoords({ x, y });
  if (pos == null) return null;
  return view.plugin(markdownDecorationsViewPlugin)?.linkAtPos(pos) ?? null;
}

const markdownDecorationsViewPlugin = ViewPlugin.fromClass(MarkdownDecorationsPlugin, {
      decorations: (plugin) => plugin.decorations,
      eventHandlers: {
        mousedown(event, view) {
          // Checked first: an image click is never also a link click (`Image` nodes and `<img>`
          // tags never populate `this.links`), so there's no ordering conflict — this just avoids
          // an unnecessary `this.links` scan on every image click. `handleImageMouseDown` itself
          // uses `event.detail` to distinguish single vs. double/triple click, so both cases for
          // images are decided right here — no separate image handling on `dblclick` below.
          if (this.handleImageMouseDown(event, view)) return true;
          return this.handleMouseEvent(event, view, "click");
        },
        dblclick(event, view) {
          // Link-only at this point (see `mousedown` above): an image click never reaches here with
          // anything left to do, since `handleImageMouseDown` already consumed both mousedowns of a
          // double-click via `event.detail` and called `preventDefault()` on each.
          return this.handleMouseEvent(event, view, "dblclick");
        },
      },
});

/** Source-view-only "this is an image reference" marker (GitHub issue #81). Source view shows raw
 * Markdown text with no decoration extension active at all (`liveViewCompartment` swaps in `[]`
 * there — `codeMirrorSetup.ts`), so there is otherwise no visual hint that a `![alt](src)` span (or
 * an allowlisted raw `<img>` tag) points at an image. This is intentionally a standalone plugin, not
 * a branch inside `MarkdownDecorationsPlugin` above: it runs specifically while live view is OFF
 * (live view already shows the real image via `ImageWidget`, so marking it there too would just
 * double up), wired through its own `sourceImageMarkerCompartment` (`codeMirrorSetup.ts`) rather
 * than `liveViewCompartment`. `Decoration.mark` only — never `Decoration.replace` — so the
 * underlying characters are never hidden/substituted: "source view = raw text" stays intact, and
 * there is no cursor-position-mapping risk to reason about (nothing is ever inserted into or
 * removed from the rendered length). Reuses the same `Image` lezer-node detection
 * (`collectLinkParts`, above) and raw-`<img>`-tag detection (`findImageTags`,
 * `markdownSpanParser.ts`) that `MarkdownDecorationsPlugin.build()` already uses for live view's
 * real `<img>` rendering — just building a mark decoration from the same ranges instead of a
 * replace/widget one. */
const SOURCE_IMAGE_MARKER_MARK = Decoration.mark({ class: "cm-md-source-image-marker" });

class SourceImageMarkerPlugin implements PluginValue {
  decorations: DecorationSet;

  constructor(view: EditorView) {
    this.decorations = this.build(view.state);
  }

  update(update: ViewUpdate): void {
    // No selection/viewport dependency, unlike `MarkdownDecorationsPlugin` above: this decoration
    // never reveals raw text on selection touch (it doesn't hide anything to begin with) and is a
    // plain mark rather than a viewport-relevant widget, so only document edits require a rebuild.
    if (update.docChanged) this.decorations = this.build(update.state);
  }

  private build(state: EditorState): DecorationSet {
    const ranges: Range<Decoration>[] = [];
    // Same code-fence/inline-code exclusion `MarkdownDecorationsPlugin.build()` applies to its
    // `findImageTags` scan, applied here for the same reason (a literal `<img ...>` shown as example
    // text inside a code block isn't a real image reference). Not needed for the `Image` lezer-node
    // case below: the parser never produces an `Image` node inside code-block/inline-code content in
    // the first place (that content is literal text to it).
    const codeSkipRanges: Range2[] = [];

    syntaxTree(state).iterate({
      enter: (node) => {
        switch (node.name) {
          case "InlineCode":
          case "FencedCode":
          case "CodeBlock":
            codeSkipRanges.push({ from: node.from, to: node.to });
            break;
          case "Image": {
            const { marks, url } = collectLinkParts(node);
            if (marks.length < 2 || !url) break; // reference-style / malformed — leave undecorated
            ranges.push(SOURCE_IMAGE_MARKER_MARK.range(node.from, node.to));
            break;
          }
        }
      },
    });

    for (const img of findImageTags(state.doc.toString())) {
      if (overlapsAny(codeSkipRanges, img.from, img.to)) continue;
      ranges.push(SOURCE_IMAGE_MARKER_MARK.range(img.from, img.to));
    }

    // `sort: true` — ranges above are gathered in tree-iteration order followed by regex-scan
    // order, not necessarily position order (same reasoning as `MarkdownDecorationsPlugin.build()`
    // above).
    return Decoration.set(ranges, true);
  }
}

/** Source-view marker extension (see `SourceImageMarkerPlugin` above). Takes no config, unlike
 * `markdownDecorationExtension` — this decoration has no click handling and no tab/memo/settings
 * dependency; it only needs the document's syntax tree and raw text. */
export function sourceImageMarkerExtension(): Extension {
  return ViewPlugin.fromClass(SourceImageMarkerPlugin, {
    decorations: (plugin) => plugin.decorations,
  });
}

/** Source-view "this is a math formula" marker (`docs/plans/v1.2.0/math-rendering/PLAN.md` §4) —
 * a sibling plugin rather than a case folded into `SourceImageMarkerPlugin` above (`PLAN.md` §4's
 * own explicit "two implementation shapes, developer-lite discretion" delegation): math spans have
 * no first-class lezer syntax-tree node at all (unlike `Image`), so this plugin's `build()` is a
 * pure regex scan (`findMathSpans`) with no `syntaxTree(state).iterate` node-type case to add —
 * keeping it separate avoids renaming/touching the already-shipped, already-tested image-marker
 * plugin (GitHub issue #81) just to fold in an unrelated detection mechanism, at the modest,
 * already-established cost (`MarkdownDecorationsPlugin.build()` above does the same) of one extra
 * `syntaxTree(state).iterate` pass per rebuild to (re)compute `codeSkipRanges` independently. Wired
 * through its own `sourceMathMarkerCompartment` (`codeMirrorSetup.ts`), active with the exact same
 * polarity as `sourceImageMarkerCompartment` (on while live view is off) but otherwise fully
 * independent of it — a memo can show both an image marker and a math marker at once, each with its
 * own distinct color (`theme.css`'s `--ttn-bg-math-marker`, deliberately different from
 * `--ttn-bg-image-marker`, per the direct user request for visual distinguishability). */
const SOURCE_MATH_MARKER_MARK = Decoration.mark({ class: "cm-md-source-math-marker" });

class SourceMathMarkerPlugin implements PluginValue {
  decorations: DecorationSet;

  constructor(view: EditorView) {
    this.decorations = this.build(view.state);
  }

  update(update: ViewUpdate): void {
    if (update.docChanged) this.decorations = this.build(update.state);
  }

  private build(state: EditorState): DecorationSet {
    const ranges: Range<Decoration>[] = [];
    // Same code-fence/inline-code exclusion `SourceImageMarkerPlugin.build()` computes for its own
    // `findImageTags` scan, computed independently here for `findMathSpans` (see this plugin's own
    // header comment on why it's a separate tree walk rather than a shared one).
    const codeSkipRanges: Range2[] = [];

    syntaxTree(state).iterate({
      enter: (node) => {
        switch (node.name) {
          case "InlineCode":
          case "FencedCode":
          case "CodeBlock":
            codeSkipRanges.push({ from: node.from, to: node.to });
            break;
        }
      },
    });

    for (const math of findMathSpans(state.doc.toString())) {
      if (overlapsAny(codeSkipRanges, math.from, math.to)) continue;
      ranges.push(SOURCE_MATH_MARKER_MARK.range(math.from, math.to));
    }

    return Decoration.set(ranges, true);
  }
}

/** Source-view math-marker extension (see `SourceMathMarkerPlugin` above) — same no-config shape as
 * `sourceImageMarkerExtension`. */
export function sourceMathMarkerExtension(): Extension {
  return ViewPlugin.fromClass(SourceMathMarkerPlugin, {
    decorations: (plugin) => plugin.decorations,
  });
}
