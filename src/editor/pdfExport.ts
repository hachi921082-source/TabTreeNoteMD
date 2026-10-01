// PDF export (`docs/plans/markdown/pdf-export/PLAN.md` §3, `docs/MDEditor-spec.md` §6, issue
// #256). Reuses the exact same `renderMarkdownToHtml` call the on-screen preview pane uses
// (`preview/previewRenderer.ts`) — never a separately-configured render pass — and the same shared
// image-loading pipeline (`imageLoader.ts`) the preview and live-view already share.
//
// Read-only with respect to the main editor: `exportMemoToPdf` only ever READS `view.state`
// (`renderMarkdownToHtml(view.state)`, a pure function of the document text). It never calls
// `view.dispatch` and never touches `view.contentDOM` or any of the main window's own DOM — the
// print content is built as a plain string and handed to the backend (`pdfExportWrite`, below), so
// this carries none of the CM6-transaction-safety risk the live-view's `ImageWidget` had to work
// around (`image-handling/PLAN.md`'s own implementation report) — there is simply no CodeMirror
// `contentDOM` anywhere in this file's reach.
//
// Native `PrintToPdfAsync`, not `window.print()` (issue #256's final fix): two earlier versions of
// this module routed through `window.print()` — first from a hidden `<iframe>`, then from a
// genuine separate top-level `WebviewWindow` navigated via `window.print()`/`afterprint`. Both
// rendered the print preview correctly in a packaged release build, but clicking "印刷" never
// produced the OS "Save As" file picker and no PDF was ever written, regardless of which
// window/document triggered `print()` — a control test confirmed the OS print-to-PDF pipeline
// itself works fine for an ordinary top-level page in a different Chromium-based browser, so the
// failure was specific to how this app's packaged WebView2 hosting reaches the native Save-As step
// from `window.print()`'s browser-style print dialog. This version instead calls the backend's
// `pdf_export_write` command (`commands/pdf_export.rs`), which shows its own native save-file
// dialog and writes the PDF directly via WebView2's `ICoreWebView2.PrintToPdfAsync` — a separate,
// dedicated, dialog-free API Microsoft documents for exactly this use case, with no native print
// UI/Save-As-picker dependency at all. All frontend DOM work below (`inlineImages` etc.) is
// unchanged from the previous versions — only how the final HTML reaches a saved PDF changed.
//
// Sanitization: `renderMarkdownToHtml`'s returned `html` is already DOMPurify-sanitized, including
// the `memo:`-scheme fix to `ALLOWED_URI_REGEXP` (`previewRenderer.ts`'s own comment on that fix).
// This module does no further string-level processing of that HTML beyond parsing it with
// `DOMParser` (to resolve `<img>` sources — see `inlineImages` below) and serializing it back out
// via `.innerHTML`, which does not reintroduce a stripping bug like that one (no regex/string
// rewriting of the markup itself).
import type { EditorView } from "@codemirror/view";
import { renderMarkdownToHtml } from "./preview/previewRenderer";
import { getOrLoadImage, peekCachedBytes } from "./imageLoader";
import { pdfExportWrite } from "../ipc/pdfExport";
import type { Result } from "../shared/result";

/** Self-contained print stylesheet (`PLAN.md` §3) — independent of the rest of the app's CSS (the
 * backend's print window — `commands/pdf_export.rs` — is a separate document with no access to
 * `theme.css`'s `data-theme` custom properties, and print output should look sane regardless of
 * the app's current light/dark theme), so colors here are plain literals, not `var(--ttn-...)`
 * references. Mirrors the preview pane's own base typography/table/code/image rules (`Editor.css`'s
 * `.ttn-editor-preview-pane` rules, `markdown-preview/RENDERING.md` §10) closely enough that the
 * printed output visually matches what the preview showed. Sets margins only — no explicit `@page`
 * size (`docs/MDEditor-spec.md` §6: paper size is left to the OS/printer default). */
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
 * `getOrLoadImage` IPC round-trip below, not image decoding (the backend's `NavigateToString` +
 * `NavigationCompleted` wait — `commands/pdf_export.rs` — handles waiting for the already-inlined
 * `data:`-URI images to finish loading on its own side). */
const IMAGE_FETCH_TIMEOUT_MS = 3000;

/** Builds the same rendered HTML the on-screen preview shows (images inlined as `data:` URIs — see
 * `inlineImages` below) and hands it to the backend's native `PrintToPdfAsync` save flow
 * (`docs/MDEditor-spec.md` §6, issue #256). Never mutates `view` or any of CodeMirror's own DOM —
 * see this file's header comment. Returns the backend's raw `Result` (rather than swallowing it)
 * so the caller (`EditorPane.tsx`'s `handlePdfExport`) can surface a failure visibly — an earlier
 * version of this function only `console.error`'d a failure, which left a user who clicked "保存"
 * in the native save dialog with no visible feedback at all when the write failed. `{ ok: true,
 * value: null }` means the user cancelled the save dialog — not an error, callers should not treat
 * a `null` value as a failure. */
export async function exportMemoToPdf(view: EditorView, tabId: string, memoName: string, memoKey: string): Promise<Result<string | null>> {
  const { html } = renderMarkdownToHtml(view.state);
  const inlinedHtml = await inlineImages(html, tabId, memoKey);
  const documentHtml = buildPrintDocumentHtml(inlinedHtml);

  const result = await pdfExportWrite(documentHtml, memoName);
  if (!result.ok) console.error("PDF export failed", result.error);
  return result;
}

/** Wraps `bodyHtml` into a full standalone HTML document, including this module's own print
 * stylesheet. No inline `<script>` here (unlike this module's two earlier, since-superseded
 * versions) — the backend's `NavigateToString`/`PrintToPdfAsync` flow needs no
 * `window.print()`/`afterprint`/`window.close()` choreography from inside the page itself. */
function buildPrintDocumentHtml(bodyHtml: string): string {
  return `<!DOCTYPE html><html><head><meta charset="utf-8"><style>${PRINT_STYLESHEET}</style></head><body>${bodyHtml}</body></html>`;
}

/** Resolves every `img[data-relative-src]` in `html` (both this app's standard `![alt](path)`
 * syntax and the allowlisted raw `<img>` tag form emit `data-relative-src` —
 * `preview/markdownIt.ts`'s `renderImageWithDeferredSrc`, `previewRenderer.ts`'s image-tag
 * handling — so this single selector covers every resolvable image; there is no other `<img>`
 * shape in this app's rendered output) to inline `data:` URIs, using a detached `DOMParser`
 * document — never touches the main window's own DOM, consistent with this file's read-only
 * guarantee. Baking images in as `data:` URIs here (rather than reusing/minting `blob:` URLs)
 * sidesteps `blob:` URL cross-document/cross-process sharing entirely — the backend's print
 * target is a wholly separate WebView2 instance, not reachable from the main window's own `blob:`
 * URL registry at all. Runs all images concurrently. */
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
 * broken-image state" behavior the earlier iframe/window versions had. */
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
