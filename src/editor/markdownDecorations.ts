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
import { findColorSpans, findImageTags } from "./markdownSpanParser";
import type { DetectedLink, LinkKind } from "./links";
import { linkClickMatchesTrigger, looksLikeEmail, openDetectedLink, scanBareFilePathTokens, scanBareMemoTokens } from "./links";
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
   * form. */
  onOpenMemo: (tabId: string, memoName: string | null) => void;
  /** Shows the shared non-blocking confirm dialog and resolves to whether the user confirmed;
   * only awaited when `confirmBeforeOpen` is true (`docs/plans/link/PLAN.md` §4). */
  requestExternalConfirm: (kind: ExternalLinkKind, value: string) => Promise<boolean>;
  /** Identifies the currently-open memo for image-cache scoping (§5.4 of
   * `docs/plans/markdown/image-handling/PLAN.md`). `null` only when no memo is open at all, in
   * which case no `Image`/`<img>` node can be reached anyway (there is no document to decorate). */
  memoKey: string | null;
}

const DEFAULT_CONFIG: MarkdownDecorationConfig = {
  openTrigger: "singleClick",
  readOnly: false,
  currentTabId: null,
  confirmBeforeOpen: false,
  onOpenMemo: () => {},
  requestExternalConfirm: async () => true,
  memoKey: null,
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
            addReplace(node.from, node.to, checkboxDecoration(checked, node.from, node.to, config.readOnly));
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
                addReplace(node.from, node.to, Decoration.replace({ widget: buildImageWidget(urlText, altText, config) }));
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
            const text = state.sliceDoc(node.from, node.to);
            const kind: LinkKind = looksLikeEmail(text) ? "email" : "url";
            links.push({ from: node.from, to: node.to, kind, target: text });
            addMark(node.from, node.to, Decoration.mark({ class: "cm-md-link" }));
            break;
          }
          case "Autolink": {
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
    for (let lineNo = 1; lineNo <= state.doc.lines; lineNo++) {
      const line = state.doc.line(lineNo);
      for (const link of [...scanBareMemoTokens(line.text, line.from), ...scanBareFilePathTokens(line.text, line.from)]) {
        if (overlapsAny(bareTokenSkipRanges, link.from, link.to)) continue;
        links.push(link);
        addMark(link.from, link.to, Decoration.mark({ class: "cm-md-link" }));
      }
    }
    for (const span of findColorSpans(docText)) {
      if (overlapsAny(codeSkipRanges, span.openFrom, span.closeTo)) continue;
      if (span.style) addMark(span.contentFrom, span.contentTo, Decoration.mark({ attributes: { style: span.style } }));
      if (!touchesSelection(state, span.openFrom, span.closeTo)) {
        addReplace(span.openFrom, span.openTo, Decoration.replace({}));
        addReplace(span.closeFrom, span.closeTo, Decoration.replace({}));
      }
    }
    for (const img of findImageTags(docText)) {
      if (overlapsAny(codeSkipRanges, img.from, img.to)) continue;
      if (!touchesSelection(state, img.from, img.to)) {
        addReplace(img.from, img.to, Decoration.replace({ widget: buildImageWidget(img.src, "", config) }));
      }
    }

    links.sort((a, b) => a.from - b.from);
    this.links = links;
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
  handleMouseEvent(event: MouseEvent, view: EditorView, eventType: "click" | "dblclick"): boolean {
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
}

/** The Markdown live-view decoration extension bundle — pass `config` and reconfigure the
 * enclosing compartment (`codeMirrorSetup.ts`'s `liveViewCompartment`) whenever it changes, same
 * pattern as `editor/textCommands.ts`'s other compartmentalized settings. */
export function markdownDecorationExtension(config: MarkdownDecorationConfig): Extension {
  return [
    markdownConfigFacet.of(config),
    ViewPlugin.fromClass(MarkdownDecorationsPlugin, {
      decorations: (plugin) => plugin.decorations,
      eventHandlers: {
        mousedown(event, view) {
          return this.handleMouseEvent(event, view, "click");
        },
        dblclick(event, view) {
          return this.handleMouseEvent(event, view, "dblclick");
        },
      },
    }),
  ];
}
