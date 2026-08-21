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
import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import type { EditorView } from "@codemirror/view";
import { renderMarkdownToHtml } from "../../editor/preview/previewRenderer";
import { attachPreviewInteractions } from "../../editor/preview/previewInteractions";
import type { PreviewInteractionsConfig } from "../../editor/preview/previewInteractions";
import { scrollPreviewToLine } from "../../editor/preview/scrollSync";
import { getOrLoadImage } from "../../editor/imageLoader";
import { computeTopVisibleLine } from "../../editor/textCommands";
import type { AppSettings, ExternalLinkKind, PaneMode } from "../../shared/models";
import { useT } from "../../i18n";
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
  /** Preview-pane zoom (`docs/plans/v1.2.0/preview-zoom/PLAN.md` §2/§3) — 70-180%, applied via CSS
   * `zoom` below. Owned by `EditorPane.tsx`; that owner is also responsible for resetting it to 100
   * whenever `paneMode` leaves `"preview"`, so this component never needs to itself gate applying it
   * on `paneMode === "preview"` — it's always already 100 while `paneMode === "split"`. */
  previewZoomPercent: number;
  tabId: string | null;
  /** Image-cache scoping key (`docs/plans/markdown/image-handling/PLAN.md` §5.4) — `""` in the
   * (defensive-only; a memo is always open by the time this component mounts) case where no tab is
   * open. */
  memoKey: string;
  readOnly: boolean;
  openTrigger: AppSettings["links"]["openTrigger"];
  confirmBeforeOpen: boolean;
  requestExternalConfirm: (kind: ExternalLinkKind, value: string) => Promise<boolean>;
  /** `anchorSlug` is the parsed `#slug` half of a `memo:...#slug` link (`docs/plans/v1.2.0/
   * cross-tab-anchor-links/PLAN.md` §2), `null` when the link carried no anchor. */
  onOpenMemoLink: (tabId: string, memoName: string | null, anchorSlug: string | null) => void;
}

/** Shared "preview-render-readiness gate" (`docs/plans/v1.2.0/cross-tab-anchor-links/PLAN.md` §4.3)
 * — owned by that plan, reused verbatim by `docs/plans/v1.2.0/preview-scroll-fix/PLAN.md`'s restore
 * path. `PreviewPane`'s own `html` state updates only after a debounce, so a caller that just
 * switched memos and immediately wants to query/read the rendered DOM (this feature's §4.2
 * `#slug` anchor jump; that other plan's scroll-position restore) needs to wait until `html`
 * actually reflects the target memo first, rather than racing the debounce and reading stale
 * markup. */
export interface PreviewPaneHandle {
  /** Resolves `true` once the currently-rendered `html` reflects `targetMemoKey` (already true most
   * of the time — no memo switch is in flight), or `false` if `timeoutMs` elapses first (e.g. the
   * memo was switched away again before its preview ever rendered) — callers treat `false` as this
   * app's standard "silently skip the jump/restore" edge-case fallback. Never rejects. */
  waitForRender: (targetMemoKey: string, timeoutMs?: number) => Promise<boolean>;
  /** The preview's own SCROLLABLE container (the outer `.ttn-editor-preview-pane` div — the one
   * with `overflow-y: auto`, not the inner `.ttn-editor-preview-pane-content` div that merely holds
   * the rendered HTML), for callers (post-`waitForRender`) needing direct DOM access — e.g. this
   * feature's own `#slug` `querySelector`/`scrollIntoView` (subtree search/scroll both work
   * identically starting from either div), and `docs/plans/v1.2.0/preview-scroll-fix/PLAN.md`'s
   * `computeTopVisiblePreviewLine`, which specifically needs the element whose `scrollTop` actually
   * changes when the user scrolls (fix for a bug caught by reviewer-lite: this used to return the
   * inner content div, whose `scrollTop` never moves, so the save-path's read was always `null`).
   * `null` before first mount / after unmount. */
  getContainer: () => HTMLElement | null;
}

/** Bounded wait for `waitForRender` below — mirrors PDF export's existing per-image wait precedent
 * (`[MDEditor-spec §6]`) rather than waiting forever if the target memo's preview never renders. */
const DEFAULT_RENDER_WAIT_TIMEOUT_MS = 3000;
const RENDER_WAIT_POLL_INTERVAL_MS = 50;

export const PreviewPane = forwardRef<PreviewPaneHandle, PreviewPaneProps>(function PreviewPane(
  {
    getView,
    renderTick,
    debounceMs,
    paneMode,
    previewZoomPercent,
    tabId,
    memoKey,
    readOnly,
    openTrigger,
    confirmBeforeOpen,
    requestExternalConfirm,
    onOpenMemoLink,
  },
  ref,
) {
  const t = useT();
  const containerRef = useRef<HTMLDivElement>(null);
  // Outer scrollable div (`overflow-y: auto` — see `Editor.css`'s `.ttn-editor-preview-pane`), kept
  // as a SEPARATE ref from `containerRef` (the inner content div) specifically so `getContainer()`
  // below can expose the element whose `scrollTop` actually reflects user scrolling — see this
  // ref's own `PreviewPaneHandle.getContainer` doc comment for why that distinction matters.
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [html, setHtml] = useState("");
  // Which `memoKey` the CURRENT `html` was actually rendered for — set only inside the debounced
  // re-render effect's timeout callback below, alongside `setHtml` itself (`PLAN.md` §4.3).
  const renderedForMemoKeyRef = useRef("");

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
      if (view) {
        setHtml(renderMarkdownToHtml(view.state).html);
        renderedForMemoKeyRef.current = memoKey;
      }
    }, debounceMs);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [renderTick, debounceMs]);

  // `PLAN.md` §4.3's readiness gate: polls `renderedForMemoKeyRef` rather than subscribing to `html`
  // directly, so a caller awaiting a render that already happened (the common case — no memo switch
  // in flight) resolves synchronously-ish without waiting a full poll tick.
  const waitForRender = useCallback((targetMemoKey: string, timeoutMs = DEFAULT_RENDER_WAIT_TIMEOUT_MS): Promise<boolean> => {
    if (renderedForMemoKeyRef.current === targetMemoKey) return Promise.resolve(true);
    return new Promise<boolean>((resolve) => {
      const deadline = Date.now() + timeoutMs;
      const poll = () => {
        if (renderedForMemoKeyRef.current === targetMemoKey) {
          resolve(true);
          return;
        }
        if (Date.now() >= deadline) {
          resolve(false);
          return;
        }
        window.setTimeout(poll, RENDER_WAIT_POLL_INTERVAL_MS);
      };
      poll();
    });
  }, []);

  useImperativeHandle(ref, () => ({ waitForRender, getContainer: () => scrollContainerRef.current }), [waitForRender]);

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
      onOpenMemoLink: (targetTabId, memoName, anchorSlug) => configRef.current.onOpenMemoLink(targetTabId, memoName, anchorSlug),
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
        ref={scrollContainerRef}
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
          // CSS `zoom` (not `transform: scale()` — `DECISIONS.md` "Preview-Pane Zoom CSS Mechanism")
          // triggers a real layout reflow at the new size (text rewraps, images/tables resize within
          // their own containers), matching the spec's "affects... text size, images, tables"
          // description of a genuine content-size change; `transform: scale()` doesn't reflow and
          // would need extra width/height compensation math. Universally supported across every
          // Chromium-based engine — this app's one and only rendering target (Tauri+WebView2) — so no
          // cross-browser concern applies.
          style={{ zoom: `${previewZoomPercent}%` }}
          // eslint-disable-next-line react/no-danger
          dangerouslySetInnerHTML={{ __html: html }}
        />
      </div>
      {lightboxSrc && (
        <Dialog title={t("previewPane.imageDialogTitle")} className="ttn-lightbox-dialog" onClose={() => setLightboxSrc(null)}>
          <img className="ttn-lightbox-image" src={lightboxSrc} alt="" />
        </Dialog>
      )}
    </>
  );
});
