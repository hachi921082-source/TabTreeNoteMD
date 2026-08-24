// Click-delegation handlers for the preview pane's own rendered-HTML container
// (`docs/plans/markdown/markdown-preview/RENDERING.md` §7-§9): checkbox write-back, heading-anchor
// jump, and link-click open handling. Mounted once per `PreviewPane` instance (not re-bound every
// render) via `attachPreviewInteractions`.
//
// Transaction-safety note (this feature's whole reason for the extra scrutiny requested — see the
// implementation report): the ONE place this module reaches back INTO the CodeMirror editor
// (checkbox write-back, below) does so exclusively through `checkboxClick.ts`'s existing exported
// `toggleCheckboxRange`, which dispatches a real CodeMirror transaction (`view.dispatch({ changes:
// {...} })`) — the same, already-reviewed dispatch primitive the live-view's own checkbox widget
// uses. Nothing here ever mutates `view.contentDOM` (or any of its descendants) directly; the only
// direct DOM mutations in this module target the PREVIEW's own container subtree (heading-anchor
// `scrollIntoView`), which is a plain React-rendered `dangerouslySetInnerHTML` subtree that
// CodeMirror's `DOMObserver` never watches, so it carries none of the corruption-bug risk that
// applied to the live-view's in-`contentDOM` image-widget fallback.
import type { EditorView } from "@codemirror/view";
import { toggleCheckboxRange } from "../checkboxClick";
import { resolveAndOpenMemoLink, linkClickMatchesTrigger, FILE_PATH_RE } from "../links";
import { linkOpenExternal } from "../../ipc/link";
import type { AppSettings, ExternalLinkKind } from "../../shared/models";

export interface PreviewInteractionsConfig {
  /** Reads the currently-mounted `EditorView` at call time — never stored, so a memo switch (which
   * replaces `viewRef.current` in `EditorPane.tsx` without remounting `PreviewPane`) is always
   * seen live. */
  getView: () => EditorView | null;
  tabId: string | null;
  /** Gates checkbox click-to-toggle only (`RENDERING.md` §8 item 1) — matches the live-view's own
   * `cm-md-checkbox-readonly` gating, checked here in JS rather than relying on the rendered
   * `<input>`'s own `disabled` attribute (more robust against the task-list plugin's exact markup
   * shape, `markdownIt.ts`'s own empirically-verified caveat). */
  readOnly: boolean;
  openTrigger: AppSettings["links"]["openTrigger"];
  confirmBeforeOpen: boolean;
  requestExternalConfirm: (kind: ExternalLinkKind, value: string) => Promise<boolean>;
  /** `anchorSlug` is the parsed `#slug` half of a `memo:...#slug` link (`docs/plans/v1.2.0/
   * cross-tab-anchor-links/PLAN.md` §2), `null` when the link carried no anchor. */
  onOpenMemoLink: (tabId: string, memoName: string | null, anchorSlug: string | null) => void;
  /** Click-to-enlarge lightbox (issue #83, preview pane only — live-view's own image click
   * behavior, #80, is untouched by this). Called with the clicked `<img>`'s current `src`, which by
   * click time is already the resolved `blob:` URL from `getOrLoadImage`'s cache (`PreviewPane.tsx`'s
   * async image-resolution effect populates it before the user can click) — no new loading needed. */
  onImageClick: (src: string) => void;
}

/** GFM task-marker shape, matched against the start of a source line immediately after its
 * list-item bullet/number prefix — same pattern `checkboxClick.ts`'s own `TASK_MARKER_RE` uses,
 * inlined here with the leading-prefix capture group `RENDERING.md` §8 step 4 requires. */
const LIST_ITEM_TASK_RE = /^(\s*(?:[-*+]|\d+[.)])\s+)\[([ xX])\]/;

function handleCheckboxClick(e: MouseEvent, checkbox: HTMLInputElement, container: HTMLElement, config: PreviewInteractionsConfig): void {
  // Always prevent the browser's own native checkbox toggle, read-only or not — otherwise a
  // read-only click would still flip the rendered glyph for a moment before the next debounced
  // re-render reverts it (RENDERING.md §8 item 1's "no-op", applied without a visible flash).
  e.preventDefault();
  if (config.readOnly) return;
  const view = config.getView();
  if (!view) return;
  const host = checkbox.closest("[data-source-line]") as HTMLElement | null;
  if (!host || !container.contains(host)) return;
  const lineIndex0 = Number(host.dataset.sourceLine);
  if (!Number.isFinite(lineIndex0)) return;
  const lineNumber = lineIndex0 + 1; // CodeMirror lines are 1-indexed; markdown-it's `.map` is 0-indexed.
  if (lineNumber < 1 || lineNumber > view.state.doc.lines) return;
  const line = view.state.doc.line(lineNumber);
  const match = LIST_ITEM_TASK_RE.exec(line.text);
  if (!match) return;
  const from = line.from + match[1].length;
  const to = from + 3; // the literal `[ ]`/`[x]`/`[X]` marker is always exactly 3 characters.
  toggleCheckboxRange(view, from, to, /[xX]/.test(match[2]));
}

function classifyExternalLink(href: string): { kind: ExternalLinkKind; value: string } {
  if (href.startsWith("mailto:")) return { kind: "email", value: href.slice("mailto:".length) };
  // Fresh non-global copy: `FILE_PATH_RE` carries a `g` flag with caller-managed `lastIndex` state
  // elsewhere (`links.ts`'s own scanners) — a one-shot containment check here must not share or
  // mutate that state.
  if (new RegExp(FILE_PATH_RE.source).test(href)) return { kind: "filePath", value: href };
  return { kind: "url", value: href };
}

async function openExternal(kind: ExternalLinkKind, value: string, config: PreviewInteractionsConfig, extendedSearchText?: string): Promise<void> {
  if (config.confirmBeforeOpen) {
    const confirmed = await config.requestExternalConfirm(kind, value);
    if (!confirmed) return;
  }
  await linkOpenExternal(kind, value, config.tabId ?? undefined, extendedSearchText);
}

function handleAnchorClick(e: MouseEvent, anchor: HTMLAnchorElement, container: HTMLElement, config: PreviewInteractionsConfig): void {
  // Always prevent the default anchor navigation first, regardless of kind — letting it proceed
  // would navigate the app's own window away from itself (RENDERING.md §9).
  e.preventDefault();
  const href = anchor.getAttribute("href") ?? "";
  if (href.startsWith("memo:")) {
    // markdown-it's `normalizeLink` percent-encodes non-ASCII hrefs (mdurl); decode before resolving
    // so the memo-name portion matches the raw (unencoded) tab/memo names. A malformed percent-sequence
    // degrades to a silent no-op, matching this app's "unresolved link = do nothing" convention.
    let decodedHref: string;
    try {
      decodedHref = decodeURIComponent(href);
    } catch {
      return;
    }
    void resolveAndOpenMemoLink(config.tabId, decodedHref, config.onOpenMemoLink);
    return;
  }
  if (href.startsWith("#")) {
    // Same encoding mismatch as the `memo:` branch above: the heading `id` attribute is set from
    // the raw (unencoded) slug, but this href was percent-encoded by markdown-it's `normalizeLink`.
    let slug: string;
    try {
      slug = decodeURIComponent(href.slice(1));
    } catch {
      return;
    }
    // Scoped to the preview's own container (not `document.getElementById`), matching
    // RENDERING.md §7's "within the preview pane's own internal scroll container" — avoids
    // colliding with an unrelated same-id element elsewhere in the app.
    const target = container.querySelector(`#${CSS.escape(slug)}`);
    target?.scrollIntoView({ block: "start" });
    return;
  }
  if (!linkClickMatchesTrigger(e, config.openTrigger, "click")) return;
  const { kind, value } = classifyExternalLink(href);
  // Issue #235: `data-extended-search-text` is only ever set for a `filePath`-shaped bare token
  // (`markdownIt.ts`'s `bareLinkCoreRule`); undefined for every other anchor shape (a real
  // `[text](url)`/`[text](memo:...)` link, `linkify`'s own bare URL/email detection), where it's
  // correctly ignored by `linkOpenExternal`/the backend.
  const extendedSearchText = kind === "filePath" ? (anchor.dataset.extendedSearchText ?? undefined) : undefined;
  void openExternal(kind, value, config, extendedSearchText);
}

/** Attaches one delegated `click` listener to `container`. Returns a cleanup function removing it
 * — call from a `useEffect` with an empty dependency array (`PreviewPane.tsx`), reading the latest
 * config via a ref-backed accessor object rather than re-binding on every render. */
export function attachPreviewInteractions(container: HTMLElement, config: PreviewInteractionsConfig): () => void {
  const handler = (e: MouseEvent) => {
    const target = e.target as HTMLElement | null;
    if (!target) return;
    const checkbox = target.closest('input[type="checkbox"]') as HTMLInputElement | null;
    if (checkbox && container.contains(checkbox)) {
      handleCheckboxClick(e, checkbox, container, config);
      return;
    }
    const anchor = target.closest("a") as HTMLAnchorElement | null;
    if (anchor && container.contains(anchor)) {
      handleAnchorClick(e, anchor, container, config);
      return;
    }
    const img = target.closest("img") as HTMLImageElement | null;
    // `img.src` is only ever set once `getOrLoadImage` resolves successfully (`PreviewPane.tsx`'s
    // async image-resolution effect) — before that (still carrying `data-relative-src` only) or
    // after a permanent failure (`ttn-preview-image-failed`, `src` never set), `img.src` is empty.
    // No-op rather than opening a lightbox with nothing to show (GitHub issue #103).
    if (img && container.contains(img) && img.src && !img.classList.contains("ttn-preview-image-failed")) {
      config.onImageClick(img.src);
    }
  };
  container.addEventListener("click", handler);
  return () => container.removeEventListener("click", handler);
}
