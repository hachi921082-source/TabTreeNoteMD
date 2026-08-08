// Hidden-iframe + print flow for PDF export (`docs/plans/markdown/pdf-export/PLAN.md` §3,
// `docs/MDEditor-spec.md` §6). Reuses the exact same `renderMarkdownToHtml` call the on-screen
// preview pane uses (`preview/previewRenderer.ts`) — never a separately-configured render pass —
// and the same shared image-loading pipeline (`imageLoader.ts`) the preview and live-view already
// share.
//
// Read-only with respect to the main editor: `exportMemoToPdf` only ever READS `view.state`
// (`renderMarkdownToHtml(view.state)`, a pure function of the document text) and builds an entirely
// separate `<iframe>` document from that snapshot. It never calls `view.dispatch` and never touches
// `view.contentDOM` — none of this module's DOM writes (`doc.write`, `img.src = ...`) land inside
// CodeMirror's own DOM tree, so this carries none of the CM6-transaction-safety risk the live-view's
// `ImageWidget` had to work around (`image-handling/PLAN.md`'s own implementation report) — there is
// simply no CodeMirror `contentDOM` anywhere in this file's reach.
//
// Sanitization: `renderMarkdownToHtml`'s returned `html` is already DOMPurify-sanitized, including
// the `memo:`-scheme fix to `ALLOWED_URI_REGEXP` (`previewRenderer.ts`'s own comment on that fix).
// This module does no further string-level processing of that HTML — it is spliced verbatim into
// the iframe document's `<body>` via a single template-literal `doc.write` call, with no additional
// regex/string manipulation that could reintroduce a stripping bug like that one. The only DOM
// mutation performed here afterward is setting `<img>` `.src` attributes by element reference
// (`resolveOneImage` below), not by re-serializing/re-parsing the HTML string.
import type { EditorView } from "@codemirror/view";
import { renderMarkdownToHtml } from "./preview/previewRenderer";
import { getOrLoadImage, peekCachedBytes } from "./imageLoader";

/** Self-contained print stylesheet (`PLAN.md` §3) — independent of the rest of the app's CSS (the
 * iframe is a separate document with no access to `theme.css`'s `data-theme` custom properties, and
 * print output should look sane regardless of the app's current light/dark theme), so colors here
 * are plain literals, not `var(--ttn-...)` references. Mirrors the preview pane's own base
 * typography/table/code/image rules (`Editor.css`'s `.ttn-editor-preview-pane` rules,
 * `markdown-preview/RENDERING.md` §10) closely enough that the printed output visually matches what
 * the preview showed. Sets margins only — no explicit `@page` size (`docs/MDEditor-spec.md` §6:
 * paper size is left to the OS/printer default). */
const PRINT_STYLESHEET = `
  @page { margin: 20mm; }
  body {
    font-family: "Segoe UI", "Yu Gothic UI", sans-serif;
    color: #1a1a1a;
    background: #ffffff;
    margin: 0;
    padding: 0 8px;
    line-height: 1.5;
  }
  h1 { font-size: 1.8em; font-weight: 700; }
  h2 { font-size: 1.5em; font-weight: 700; }
  h3 { font-size: 1.3em; font-weight: 600; }
  h4 { font-size: 1.15em; font-weight: 600; }
  h5 { font-size: 1.05em; font-weight: 600; }
  h6 { font-size: 1em; font-weight: 600; color: #555555; }
  strong { font-weight: 700; }
  em { font-style: italic; }
  del, s { text-decoration: line-through; }
  blockquote {
    border-left: 3px solid #999999;
    padding-left: 8px;
    margin-left: 0;
    color: #555555;
  }
  pre, code {
    font-family: "Consolas", "Cascadia Code", monospace;
  }
  pre {
    background: #f0f0f0;
    padding: 8px;
    border-radius: 4px;
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-word;
  }
  code {
    background: #f0f0f0;
    border-radius: 3px;
    padding: 0 2px;
  }
  pre code {
    background: none;
    padding: 0;
  }
  .ttn-preview-table-wrap {
    overflow-x: auto;
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  th, td {
    border: 1px solid #999999;
    padding: 4px 8px;
  }
  img {
    /* Shrink-to-fit only, never upscale, aspect ratio preserved — matches the preview's own
       .ttn-editor-preview-pane img rule (Editor.css), so PDF export prints the same "final look"
       the preview showed. */
    max-width: 100%;
    height: auto;
  }
`;

/** Per-image decode-wait timeout (`docs/MDEditor-spec.md` §6: "a reasonable per-image timeout (e.g.
 * ~3 seconds) so one broken or slow image cannot hang PDF export indefinitely"). */
const IMAGE_DECODE_TIMEOUT_MS = 3000;

/** Fallback iframe-cleanup delay if `afterprint` never fires (`PLAN.md` §3 step 6) — generous so a
 * slow print dialog (the user leaving the native "Save as PDF" dialog open) doesn't get its iframe
 * yanked out from under it mid-print. */
const IFRAME_CLEANUP_FALLBACK_MS = 60000;

/** `HTMLIFrameElement.contentWindow` is typed as plain `Window` by TypeScript's DOM lib (aliased
 * `WindowProxy = Window`), which does not expose ambient globals like `URL` as instance members —
 * those are declared as top-level `declare var`s, only reachable off a `Window & typeof globalThis`-
 * typed reference. At runtime `iframe.contentWindow.URL` works fine (every `Window` implements the
 * global scope of its own document); this alias only widens the TYPE for the fallback object-URL
 * mint below (`resolveOneImage`'s `win.URL.createObjectURL` call). */
type IframeWindow = Window & typeof globalThis;

/** Builds a hidden iframe holding the same rendered HTML the on-screen preview shows, resolves its
 * images, and triggers the OS print dialog (`docs/MDEditor-spec.md` §6). Never mutates `view` or any
 * of CodeMirror's own DOM — see this file's header comment. */
export async function exportMemoToPdf(view: EditorView, tabId: string, memoName: string, memoKey: string): Promise<void> {
  const { html } = renderMarkdownToHtml(view.state);

  const iframe = document.createElement("iframe");
  Object.assign(iframe.style, { position: "fixed", width: "0", height: "0", border: "0", visibility: "hidden" });
  document.body.appendChild(iframe);

  let cleanedUp = false;
  const cleanup = () => {
    if (cleanedUp) return;
    cleanedUp = true;
    iframe.remove();
  };

  try {
    const doc = iframe.contentDocument;
    const win = iframe.contentWindow as IframeWindow | null;
    if (!doc || !win) {
      cleanup();
      return;
    }
    // Single `doc.write` call, no further string manipulation of `html` beyond this template-literal
    // splice (see this file's header comment on why that matters for the inherited DOMPurify fix).
    doc.open();
    doc.write(`<!DOCTYPE html><html><head><meta charset="utf-8"><style>${PRINT_STYLESHEET}</style></head><body>${html}</body></html>`);
    doc.close();
    // Filename hint (`docs/MDEditor-spec.md` §6) — the OS "Save as PDF" dialog suggests this instead
    // of the app's generic window title.
    doc.title = memoName;

    await resolveImages(doc, win, tabId, memoKey);

    win.addEventListener("afterprint", cleanup, { once: true });
    window.setTimeout(cleanup, IFRAME_CLEANUP_FALLBACK_MS);

    win.print();
  } catch {
    cleanup();
  }
}

/** Resolves every `img[data-relative-src]` inside the PRINT IFRAME's own document (not the main
 * window's DOM) — both this app's standard `![alt](path)` syntax and the allowlisted raw `<img>` tag
 * form emit `data-relative-src` (`preview/markdownIt.ts`'s `renderImageWithDeferredSrc`,
 * `previewRenderer.ts`'s image-tag handling), so this single selector covers every resolvable image;
 * there is no other `<img>` shape in this app's rendered output. Runs all images concurrently. */
async function resolveImages(doc: Document, win: IframeWindow, tabId: string, memoKey: string): Promise<void> {
  const images = Array.from(doc.querySelectorAll<HTMLImageElement>("img[data-relative-src]"));
  await Promise.all(images.map((img) => resolveOneImage(img, win, tabId, memoKey)));
}

/** Resolves one `<img>`'s `src` and waits for it to finish decoding before returning, so the
 * subsequent `print()` call never races a still-loading image (`docs/MDEditor-spec.md` §6).
 *
 * Cross-document `blob:` URL handling (`PLAN.md` §3's flagged open item, `MDEditor-spec.md` §9):
 * primary approach reuses the shared image cache's existing `blob:` URL — created in the MAIN
 * window — directly, since same-origin `blob:` URLs are spec'd to resolve across same-origin
 * documents (this iframe, created via `document.createElement`/`doc.write` rather than a cross-
 * origin `src` navigation, stays same-origin with the main window). This was NOT empirically
 * verified against the app's actual WebView2 runtime at implementation time (no interactive GUI
 * available in that environment — see this feature's implementation report) — instead of picking one
 * static answer on faith, this function tries the reused URL first and automatically falls back to
 * minting a FRESH object URL from the cache's already-retained raw bytes via the iframe's OWN
 * `contentWindow.URL` if the reused URL's image fails to decode within the timeout. Cheap either way
 * (no extra IPC round-trip — the bytes are already resident), and this keeps export working
 * regardless of which behavior the actual WebView2 build exhibits, including a hypothetical future
 * regression. Still worth an explicit manual smoke test once a GUI is available, per §3's open item. */
async function resolveOneImage(img: HTMLImageElement, win: IframeWindow, tabId: string, memoKey: string): Promise<void> {
  const relativePath = img.dataset.relativeSrc;
  if (!relativePath) return;

  const primaryUrl = await getOrLoadImage(tabId, relativePath, memoKey);
  if (primaryUrl) {
    img.src = primaryUrl;
    const primaryOk = await waitForImageDecode(img);
    if (primaryOk) return;
  }

  // Fallback: mint a fresh object URL from the same cache entry's raw bytes, via the iframe's own
  // `contentWindow.URL` (not the main window's `URL`).
  const bytes = peekCachedBytes(tabId, relativePath, memoKey);
  if (!bytes) return; // no bytes available at all (e.g. `getOrLoadImage` itself failed) — leave broken
  const freshUrl = win.URL.createObjectURL(new Blob([bytes] as BlobPart[]));
  img.src = freshUrl;
  await waitForImageDecode(img);
}

/** Races `<img>.decode()` against `IMAGE_DECODE_TIMEOUT_MS` — resolves `true` only if decode
 * actually succeeded in time, `false` on either failure or timeout, never rejects. */
function waitForImageDecode(img: HTMLImageElement): Promise<boolean> {
  return Promise.race([
    img.decode().then(
      () => true,
      () => false,
    ),
    new Promise<boolean>((resolve) => window.setTimeout(() => resolve(false), IMAGE_DECODE_TIMEOUT_MS)),
  ]);
}
