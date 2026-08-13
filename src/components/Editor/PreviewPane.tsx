// Read-only rendered-HTML preview pane (`docs/plans/markdown/markdown-preview/PLAN.md` §5,
// `RENDERING.md` §4). Mounted by `EditorPane.tsx` whenever `paneMode !== "edit"`. Owns the
// debounced re-render effect, the async image-resolution pass, the editor->preview scroll-sync
// listener, and the click-delegation handlers (`previewInteractions.ts`).
//
// Async-into-CodeMirror interaction points in this file, and why each is safe (see this feature's
// implementation report for the full write-up):
//   - The debounced re-render (`setTimeout` -> `renderMarkdownToHtml(view.state)`) only READS
//     `view.state`; it never dispatches a transaction or touches `view.contentDOM`.
//   - The image-resolution effect mutates `<img>` elements living inside THIS component's own
//     `dangerouslySetInnerHTML` subtree (`containerRef.current`), never inside CodeMirror's
//     `contentDOM` — CodeMirror's `DOMObserver` does not watch this subtree at all, so a direct
//     `img.src = url` assignment here carries none of the corruption risk the live-view's
//     `ImageWidget` had to work around (that widget's DOM node lives INSIDE `contentDOM`).
//   - The scroll-sync listener only reads `view.state`/`view.scrollDOM.scrollTop` and writes to
//     this component's own container (`scrollIntoView`) — no CodeMirror mutation either.
//   - Checkbox write-back and `memo:` link navigation (`previewInteractions.ts`) are the only paths
//     that reach back into CodeMirror, and both go through existing, already-transaction-safe
//     primitives (`toggleCheckboxRange`'s `view.dispatch`, and the shared `onOpenMemoLink` callback
//     that ultimately triggers a normal React-driven memo switch) — see that module's own header
//     comment.
import { useEffect, useRef, useState } from "react";
import type { EditorView } from "@codemirror/view";
import { renderMarkdownToHtml } from "../../editor/preview/previewRenderer";
import { attachPreviewInteractions } from "../../editor/preview/previewInteractions";
import type { PreviewInteractionsConfig } from "../../editor/preview/previewInteractions";
import { scrollPreviewToLine } from "../../editor/preview/scrollSync";
import { getOrLoadImage } from "../../editor/imageLoader";
import { computeTopVisibleLine } from "../../editor/textCommands";
import type { AppSettings, ExternalLinkKind, PaneMode } from "../../shared/models";
import { Dialog } from "../dialogs/Dialog";
import "./Editor.css";

export interface PreviewPaneProps {
  getView: () => EditorView | null;
  /** Bumped by `EditorPane.tsx`'s existing CodeMirror `updateListener` — reused as the
   * re-render trigger rather than duplicating a second doc-change listener. */
  renderTick: number;
  debounceMs: number;
  /** `"preview"` constrains the rendered content to a page-like column (GitHub issue #115) so its
   * wrap points roughly approximate PDF export's own paper width — `"split"` (the only other value
   * this component is ever mounted for) keeps stretching to the full available pane width,
   * unchanged. */
  paneMode: PaneMode;
  tabId: string | null;
  /** Image-cache scoping key (`docs/plans/markdown/image-handling/PLAN.md` §5.4) — `""` in the
   * (defensive-only; a memo is always open by the time this component mounts) case where no tab is
   * open. */
  memoKey: string;
  readOnly: boolean;
  openTrigger: AppSettings["links"]["openTrigger"];
  confirmBeforeOpen: boolean;
  requestExternalConfirm: (kind: ExternalLinkKind, value: string) => Promise<boolean>;
  onOpenMemoLink: (tabId: string, memoName: string | null) => void;
}

export function PreviewPane({
  getView,
  renderTick,
  debounceMs,
  paneMode,
  tabId,
  memoKey,
  readOnly,
  openTrigger,
  confirmBeforeOpen,
  requestExternalConfirm,
  onOpenMemoLink,
}: PreviewPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [html, setHtml] = useState("");

  // Click-to-enlarge lightbox (issue #83, preview pane only). Same wiring shape as
  // `EditorPane.tsx`'s `requestExternalConfirm`/`pendingExternalLink`, minus the Promise-resolver
  // part — an image click needs no confirmation, so it's a plain state write.
  const [lightboxSrc, setLightboxSrc] = useState<string | null>(null);

  // Debounced re-render on every doc change (`RENDERING.md` §4) — two independent debounce knobs
  // in this app (`AppSettings.preview.debounceMs` here vs. `AppSettings.autosave.plainDebounceMs`)
  // are never tied together; this one is a plain doc-changed timer, not a save-completion event.
  useEffect(() => {
    const t = window.setTimeout(() => {
      const view = getView();
      if (view) setHtml(renderMarkdownToHtml(view.state).html);
    }, debounceMs);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [renderTick, debounceMs]);

  // Click-delegation config, read live via a ref so `attachPreviewInteractions` (below) can be
  // mounted exactly once (RENDERING.md §4's "attached once via useEffect, not re-bound every
  // render") while still always seeing this render's latest props at click time.
  const configRef = useRef<PreviewInteractionsConfig>({
    getView,
    tabId,
    readOnly,
    openTrigger,
    confirmBeforeOpen,
    requestExternalConfirm,
    onOpenMemoLink,
    onImageClick: setLightboxSrc,
  });
  configRef.current = {
    getView,
    tabId,
    readOnly,
    openTrigger,
    confirmBeforeOpen,
    requestExternalConfirm,
    onOpenMemoLink,
    onImageClick: setLightboxSrc,
  };
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    return attachPreviewInteractions(container, {
      getView: () => configRef.current.getView(),
      get tabId() {
        return configRef.current.tabId;
      },
      get readOnly() {
        return configRef.current.readOnly;
      },
      get openTrigger() {
        return configRef.current.openTrigger;
      },
      get confirmBeforeOpen() {
        return configRef.current.confirmBeforeOpen;
      },
      requestExternalConfirm: (kind, value) => configRef.current.requestExternalConfirm(kind, value),
      onOpenMemoLink: (targetTabId, memoName) => configRef.current.onOpenMemoLink(targetTabId, memoName),
      // `setLightboxSrc` is a React state setter — referentially stable across renders, so (unlike
      // the props-derived fields above) it needs no `configRef` indirection to stay fresh.
      onImageClick: (src) => setLightboxSrc(src),
    });
  }, []);

  // Async image resolution (`RENDERING.md` §5) — runs after every `html` change. Mutates `<img>`
  // elements inside this component's own container subtree directly (see this file's header
  // comment on why that's safe here, unlike the live-view's `contentDOM`-hosted widgets).
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !tabId) return;
    let cancelled = false;
    const images = container.querySelectorAll<HTMLImageElement>("img[data-relative-src]");
    images.forEach((img) => {
      const relativePath = img.dataset.relativeSrc;
      if (!relativePath) return;
      void getOrLoadImage(tabId, relativePath, memoKey).then((url) => {
        if (cancelled) return;
        if (url) {
          img.src = url;
          img.classList.remove("ttn-preview-image-failed");
        } else {
          img.classList.add("ttn-preview-image-failed");
        }
      });
    });
    return () => {
      cancelled = true;
    };
  }, [html, tabId, memoKey]);

  // Editor -> preview scroll sync (`RENDERING.md` §6, one-directional only). Re-bound on every
  // `renderTick` change (not just once) so a memo switch's freshly-(re)created `EditorView` (a new
  // `scrollDOM` element — `EditorPane.tsx` §4.2 never unmounts this component across a mere memo
  // switch while pane-mode stays split/preview) gets its own listener promptly; `renderTick` is
  // bumped immediately after every view (re)creation in `EditorPane.tsx`. Slightly more churn than
  // strictly necessary (also re-binds on every plain keystroke), accepted as a simple/correct
  // tradeoff over a more surgical view-identity-diffing scheme.
  useEffect(() => {
    const view = getView();
    if (!view) return;
    let raf = 0;
    const handler = () => {
      if (raf) return;
      raf = window.requestAnimationFrame(() => {
        raf = 0;
        const container = containerRef.current;
        if (container) scrollPreviewToLine(container, computeTopVisibleLine(view));
      });
    };
    view.scrollDOM.addEventListener("scroll", handler);
    return () => {
      view.scrollDOM.removeEventListener("scroll", handler);
      if (raf) window.cancelAnimationFrame(raf);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [renderTick]);

  return (
    <>
      <div
        className="ttn-editor-preview-pane"
        // Fully suppressed (`docs/MDEditor-spec.md` §2: "right-click shows no context menu at all...
        // fully suppressed, not auto-reveal-then-show"). `stopPropagation` is required in addition to
        // `preventDefault` — without it, the click would still bubble to `EditorPane.tsx`'s own
        // `.ttn-editor-pane` container, whose `onContextMenu` handler opens this app's OWN
        // Convert/Check/Search-with-Google/Properties context menu (not the native one) regardless of
        // this pane's `preventDefault` call, contradicting the spec's "no context menu at all"
        // (deviates from `RENDERING.md` §4.3's literal `onContextMenu={(e) => e.preventDefault()}`
        // snippet, which alone would leave that outer app-level menu still showing — see this
        // feature's implementation report).
        onContextMenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        {/* Outer div above is the scroll container (full pane width/background regardless of pane
            mode); this inner div is the actual content column — only it gets width-constrained in
            preview-only mode (GitHub issue #115), so the pane's own background/scrollbar still span
            the full width either way. */}
        <div
          className={`ttn-editor-preview-pane-content${paneMode === "preview" ? " ttn-editor-preview-pane-content-page" : ""}`}
          ref={containerRef}
          // eslint-disable-next-line react/no-danger
          dangerouslySetInnerHTML={{ __html: html }}
        />
      </div>
      {lightboxSrc && (
        <Dialog title="画像" className="ttn-lightbox-dialog" onClose={() => setLightboxSrc(null)}>
          <img className="ttn-lightbox-image" src={lightboxSrc} alt="" />
        </Dialog>
      )}
    </>
  );
}
