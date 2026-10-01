// Print-to-PDF flow for PDF export (`docs/plans/markdown/pdf-export/PLAN.md` §3,
// `docs/MDEditor-spec.md` §6, issue #256). Reuses the exact same `renderMarkdownToHtml` call the
// on-screen preview pane uses (`preview/previewRenderer.ts`) — never a separately-configured render
// pass — and the same shared image-loading pipeline (`imageLoader.ts`) the preview and live-view
// already share.
//
// Read-only with respect to the main editor: `exportMemoToPdf` only ever READS `view.state`
// (`renderMarkdownToHtml(view.state)`, a pure function of the document text). It never calls
// `view.dispatch` and never touches `view.contentDOM` or any of the main window's own DOM — the
// print content is built as a plain string and handed to a brand-new, separate top-level
// `WebviewWindow` (see below), so this carries none of the CM6-transaction-safety risk the
// live-view's `ImageWidget` had to work around (`image-handling/PLAN.md`'s own implementation
// report) — there is simply no CodeMirror `contentDOM` anywhere in this file's reach.
//
// Genuine top-level window, not a nested iframe (issue #256 follow-up investigation): an earlier
// version of this module built a hidden `<iframe>` in the main window and called
// `iframe.contentWindow.print()`. Packaged-release-build testing against the actual WebView2
// runtime showed that, while the print preview itself rendered correctly, clicking "印刷" in the
// native print dialog never produced the OS "Save As" file picker — no PDF was ever written. A
// control test (printing a normal top-level page in a different Chromium-based browser) confirmed
// the OS "Microsoft Print to PDF" pipeline itself works fine, so the failure was specific to this
// app's WebView2-hosted *nested iframe* print flow. This version instead creates a real, separate
// top-level `WebviewWindow` (Tauri's `@tauri-apps/api/webviewWindow`) and navigates it to a
// self-contained `data:` HTML document that triggers its own `window.print()` once loaded — a
// genuine top-level navigated page, which is what WebView2's native print-to-PDF Save-As dialog is
// documented/expected to support.
//
// Sanitization: `renderMarkdownToHtml`'s returned `html` is already DOMPurify-sanitized, including
// the `memo:`-scheme fix to `ALLOWED_URI_REGEXP` (`previewRenderer.ts`'s own comment on that fix).
// This module does no further string-level processing of that HTML beyond parsing it with
// `DOMParser` (to resolve `<img>` sources — see `inlineImages` below) and serializing it back out
// via `.innerHTML`, which does not reintroduce a stripping bug like that one (no regex/string
// rewriting of the markup itself).
import type { EditorView } from "@codemirror/view";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { renderMarkdownToHtml } from "./preview/previewRenderer";
import { getOrLoadImage, peekCachedBytes } from "./imageLoader";

/** Self-contained print stylesheet (`PLAN.md` §3) — independent of the rest of the app's CSS (the
 * print window is a separate top-level document with no access to `theme.css`'s `data-theme`
 * custom properties, and print output should look sane regardless of the app's current light/dark
 * theme), so colors here are plain literals, not `var(--ttn-...)` references. Mirrors the preview
 * pane's own base typography/table/code/image rules (`Editor.css`'s `.ttn-editor-preview-pane`
 * rules, `markdown-preview/RENDERING.md` §10) closely enough that the printed output visually
 * matches what the preview showed. Sets margins only — no explicit `@page` size
 * (`docs/MDEditor-spec.md` §6: paper size is left to the OS/printer default). */
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

/** Per-image byte-fetch timeout (`docs/MDEditor-spec.md` §6: "a reasonable per-image timeout (e.g.
 * ~3 seconds) so one broken or slow image cannot hang PDF export indefinitely") — bounds the
 * `getOrLoadImage` IPC round-trip below, not image decoding (decoding of the already-inlined
 * `data:` URIs happens inside the print window itself; see `PRINT_TRIGGER_SCRIPT`'s own
 * image-readiness wait). */
const IMAGE_FETCH_TIMEOUT_MS = 3000;

/** Fallback print-window-close delay if neither `afterprint` nor the window's own destruction ever
 * fires — generous so a slow print dialog (the user leaving the native "Save as PDF" dialog open)
 * doesn't get its window yanked out from under it mid-print. */
const PRINT_WINDOW_CLEANUP_FALLBACK_MS = 60000;

/** Inline script baked directly into the print window's own `data:` HTML document — runs in that
 * window's own JS realm (a separate top-level `WebviewWindow`, not reachable from this module's own
 * `window`/`document`), so image-readiness waiting and the `print()`/`window.close()` calls all have
 * to live here rather than being driven from the main window the way the old hidden-iframe version
 * could. Every `<img>` source is already a `data:` URI by the time this runs (see `inlineImages`
 * below) — no network/IPC fetch happens inside the print window itself, so `document.images[i]
 * .complete` becomes reliably true very quickly once `load` fires; the `setTimeout` fallback below
 * exists only so a pathologically slow/broken individual image can't hang the print trigger
 * indefinitely, mirroring `IMAGE_FETCH_TIMEOUT_MS`'s spec-mandated intent one level up. */
const PRINT_TRIGGER_SCRIPT = `
(function () {
  var printed = false;
  function triggerPrint() {
    if (printed) return;
    printed = true;
    window.print();
  }
  function allImagesComplete() {
    var imgs = document.images;
    for (var i = 0; i < imgs.length; i++) {
      if (!imgs[i].complete) return false;
    }
    return true;
  }
  window.addEventListener("afterprint", function () {
    window.close();
  }, { once: true });
  if (document.readyState === "complete" && allImagesComplete()) {
    triggerPrint();
  } else {
    window.addEventListener("load", triggerPrint, { once: true });
  }
  window.setTimeout(triggerPrint, 5000);
})();
`;

/** Builds a real, separate top-level print `WebviewWindow` holding the same rendered HTML the
 * on-screen preview shows, and triggers the OS print dialog from inside it (`docs/MDEditor-spec.md`
 * §6, issue #256). Never mutates `view` or any of CodeMirror's own DOM — see this file's header
 * comment. */
export async function exportMemoToPdf(view: EditorView, tabId: string, memoName: string, memoKey: string): Promise<void> {
  const { html } = renderMarkdownToHtml(view.state);
  const inlinedHtml = await inlineImages(html, tabId, memoKey);
  const documentHtml = buildPrintDocumentHtml(inlinedHtml, memoName);

  let printWindow: WebviewWindow | null = null;
  let cleanedUp = false;
  const cleanup = () => {
    if (cleanedUp) return;
    cleanedUp = true;
    printWindow?.close().catch(() => {
      // Already closed (e.g. the window's own `afterprint` → `window.close()` already tore it
      // down) or never finished creating — either way there is nothing left to clean up.
    });
  };

  try {
    // Real on-screen size (not a hidden/zero-size surface — the whole lesson of issue #256's first
    // fix attempt) but positioned far off the visible desktop area, so the user never sees it.
    printWindow = new WebviewWindow(printWindowLabel(), {
      url: `data:text/html;charset=utf-8,${encodeURIComponent(documentHtml)}`,
      x: -10000,
      y: 0,
      width: 820,
      height: 1160,
      decorations: false,
      visible: true,
      skipTaskbar: true,
    });

    printWindow.once("tauri://error", cleanup);
    printWindow.once("tauri://destroyed", () => {
      cleanedUp = true;
    });

    window.setTimeout(cleanup, PRINT_WINDOW_CLEANUP_FALLBACK_MS);
  } catch {
    cleanup();
  }
}

/** Unique per-call window label — export can be triggered again (e.g. from a different memo) before
 * a previous print window has finished closing, and Tauri window labels must be unique among
 * currently-open windows. */
function printWindowLabel(): string {
  return `ttn-pdf-print-${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
}

/** Wraps `bodyHtml` into a full standalone HTML document for the print window, including this
 * module's own print stylesheet and the inline print-trigger script (`PRINT_TRIGGER_SCRIPT`). The
 * `<title>` becomes the OS "Save as PDF" dialog's suggested file name, matching the old iframe
 * version's `doc.title = memoName` behavior. `memoName` is escaped since it lands in HTML text
 * content (memo names are user-controlled free text, not markdown-sanitized by `renderMarkdownToHtml`
 * the way `bodyHtml` already is). */
function buildPrintDocumentHtml(bodyHtml: string, memoName: string): string {
  return `<!DOCTYPE html><html><head><meta charset="utf-8"><title>${escapeHtml(memoName)}</title><style>${PRINT_STYLESHEET}</style></head><body>${bodyHtml}<script>${PRINT_TRIGGER_SCRIPT}</script></body></html>`;
}

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/** Resolves every `img[data-relative-src]` in `html` (both this app's standard `![alt](path)`
 * syntax and the allowlisted raw `<img>` tag form emit `data-relative-src` —
 * `preview/markdownIt.ts`'s `renderImageWithDeferredSrc`, `previewRenderer.ts`'s image-tag
 * handling — so this single selector covers every resolvable image; there is no other `<img>`
 * shape in this app's rendered output) to inline `data:` URIs, using a detached `DOMParser`
 * document — never touches the main window's own DOM, consistent with this file's read-only
 * guarantee. Baking images in as `data:` URIs here (rather than reusing/minting `blob:` URLs the
 * way the old iframe version did) sidesteps `blob:` URL cross-document/cross-webview sharing
 * entirely — a genuinely separate top-level `WebviewWindow` is a different WebView2 instance from
 * the main window, so a `blob:` URL minted in the main window would not be expected to resolve
 * there the way it might (same-realm) inside a same-window iframe. Runs all images concurrently. */
async function inlineImages(html: string, tabId: string, memoKey: string): Promise<string> {
  const doc = new DOMParser().parseFromString(`<!DOCTYPE html><html><body>${html}</body></html>`, "text/html");
  const images = Array.from(doc.querySelectorAll<HTMLImageElement>("img[data-relative-src]"));
  await Promise.all(images.map((img) => inlineOneImage(img, tabId, memoKey)));
  return doc.body.innerHTML;
}

/** Inlines one `<img>`'s `src` as a `data:` URI from the shared image cache's already-retained raw
 * bytes (`imageLoader.ts`), bounding the `getOrLoadImage` IPC round-trip by
 * `IMAGE_FETCH_TIMEOUT_MS`. Leaves `src` unset (broken-image fallback) on timeout, load failure, or
 * a since-evicted cache entry — the same "fall back silently, let the browser render its own
 * broken-image state" behavior the old iframe version had. */
async function inlineOneImage(img: HTMLImageElement, tabId: string, memoKey: string): Promise<void> {
  const relativePath = img.dataset.relativeSrc;
  if (!relativePath) return;

  const loaded = await Promise.race([
    getOrLoadImage(tabId, relativePath, memoKey).then((url) => url !== null),
    new Promise<boolean>((resolve) => window.setTimeout(() => resolve(false), IMAGE_FETCH_TIMEOUT_MS)),
  ]);
  if (!loaded) return;

  const bytes = peekCachedBytes(tabId, relativePath, memoKey);
  if (!bytes) return; // no bytes available at all (e.g. the cache entry was evicted mid-export)
  img.src = await bytesToDataUrl(bytes);
}

/** Converts raw image bytes to a `data:` URI via `FileReader` (no explicit MIME type — `<img>`
 * rendering content-sniffs the actual image format regardless of a declared/absent MIME, the same
 * way the main window's own `blob:`-URL image cache already relies on — `imageLoader.ts`'s
 * `loadAndCache` also constructs its `Blob` with no `type`). */
function bytesToDataUrl(bytes: Uint8Array): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error ?? new Error("FileReader failed to read image bytes"));
    reader.readAsDataURL(new Blob([bytes] as BlobPart[]));
  });
}
