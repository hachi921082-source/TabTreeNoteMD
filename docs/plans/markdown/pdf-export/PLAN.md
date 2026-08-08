# PLAN: PDF Export

> Status: planning complete, not yet implemented. Additive on top of
> [`markdown-preview/PLAN.md`](../markdown-preview/PLAN.md) and
> [`image-handling/PLAN.md`](../image-handling/PLAN.md).

**Dependencies**:
- [`markdown-preview/PLAN.md`](../markdown-preview/PLAN.md)'s `renderMarkdownToHtml` function
  (`src/editor/preview/previewRenderer.ts`) is reused **verbatim** here, not reimplemented — this plan
  assumes that function already exists.
- [`image-handling/PLAN.md`](../image-handling/PLAN.md)'s shared image-loading pipeline
  (`src/editor/imageLoader.ts`'s cache) is required for the cross-document `blob:` URL fallback below,
  which needs the cache's retained raw bytes.

Sources: `docs/MDEditor-spec.md` §6, §8 (primary/authoritative source for this plan); `docs/spec.md`
§4.2; `docs/detailed-spec.md` §5.10; `docs/plans/markdown-preview/PLAN.md` (renderer reuse);
`docs/plans/image-handling/PLAN.md` (shared image cache reuse).

---

## 1. Scope

This plan originated as one section of a combined plan (`docs/plans/markdown-preview/PLAN.md`'s
earlier, pre-split revision) covering the preview pane, PDF export, and live-view image rendering
together. This file now covers **only PDF export**. It elaborates `docs/MDEditor-spec.md` §6: printing
the same rendered HTML the preview pane shows.

## 2. Module Layout

New file (frontend):

| File | Purpose |
|---|---|
| `src/editor/pdfExport.ts` | Hidden-iframe + print flow (§3). |

Modified files:

| File | Change |
|---|---|
| `src/components/Editor/EditorPane.tsx` | New global shortcut branch for PDF export (§4), passes `memoKey`/`tabId`/`memoName` context to the export call. |
| `src/components/Editor/Toolbar/MarkdownToolbar.tsx` | New header button (§4). |
| `src/shared/shortcuts.ts` | `markdown.pdfExport` entry (§4). |

## 3. PDF Export Flow

Elaborates `docs/MDEditor-spec.md` §6. `src/editor/pdfExport.ts`:

```ts
export async function exportMemoToPdf(view: EditorView, tabId: string, memoName: string, memoKey: string): Promise<void>
```

1. `const { html } = renderMarkdownToHtml(view.state);` — **the exact same renderer call** the
   on-screen preview uses (`markdown-preview/PLAN.md` §5.2), not a separately-configured render pass.
2. Build a hidden iframe:
   ```ts
   const iframe = document.createElement("iframe");
   Object.assign(iframe.style, { position: "fixed", width: "0", height: "0", border: "0", visibility: "hidden" });
   document.body.appendChild(iframe);
   const doc = iframe.contentDocument!;
   doc.open();
   doc.write(`<!DOCTYPE html><html><head><meta charset="utf-8"><style>${PRINT_STYLESHEET}</style></head><body>${html}</body></html>`);
   doc.close();
   doc.title = memoName;   // filename hint (§6) — OS "Save as PDF" suggests this instead of the app's window title
   ```
3. **Resolve every `img[data-relative-src]` inside `doc`** (not the main window's DOM): for each,
   attempt `img.src = <the shared cache's existing blob: URL>` first (primary design — reuse the
   identical `blob:` URL string across documents, per `docs/MDEditor-spec.md` §5/§6's "same Blob-URL
   cache" wording, i.e. `image-handling/PLAN.md`'s shared cache).

   **Cross-document `blob:` URL verification** (open item — see bottom of this document for tracking):
   confirm a `blob:` URL created in the main window's origin actually resolves when set as an `<img
   src>` inside this separate hidden iframe's document (WebView2-version-dependent, untested as of this
   plan). **Fallback if it fails**: since the cache entry (`image-handling/PLAN.md`'s cache) retains the
   raw `bytes: Uint8Array`, construct a *fresh* Blob URL from those already-in-memory bytes via the
   iframe's own `iframe.contentWindow!.URL.createObjectURL(new Blob([entry.bytes]))` — no extra IPC
   round-trip needed either way, since the bytes are already resident; "re-fetch bytes fresh" (as worded
   in `docs/MDEditor-spec.md` §9) can mean either re-calling `image_read_bytes` or reusing the
   already-cached bytes to mint a new object URL — both are cheap, and which one is used is this plan's
   implementation discretion once the primary approach's actual behavior is observed.
4. **Wait for every image to finish loading** before printing: `await Promise.all([...doc.images].map(img => Promise.race([img.decode().catch(() => {}), new Promise((r) => setTimeout(r, 3000))])))` — a per-image ~3s timeout so one broken/slow image can't hang export indefinitely (`docs/MDEditor-spec.md` §6). This is a distinct concern from step 3's cross-document verification and applies regardless of how that resolves.
5. `iframe.contentWindow!.print();`
6. Remove the iframe from the DOM shortly after (`afterprint` event if fired reliably by WebView2, else
   a generous fallback timeout, e.g. 60s, so a lingering hidden iframe doesn't accumulate across
   repeated exports within one session) — exact cleanup timing is implementation discretion, not
   spec-mandated.

**Print stylesheet** (`PRINT_STYLESHEET`, a self-contained constant string, independent of the rest of
the app's CSS): sets margins only (e.g. `@page { margin: 20mm; }`), mirrors the preview's own base
typography/table/code/image rules (`markdown-preview/PLAN.md` §5.10) so the printed output visually
matches what the preview showed, and does **not** set an explicit `@page` size — paper size is left to
the OS/printer default (`docs/MDEditor-spec.md` §6).

**Rationale for the iframe mechanism** (load-bearing, restated from spec for future readers): this
app's many independent floating/absolute-positioned UI elements (the toolbar, tree pane, tab bar,
dialogs, context menu) would otherwise all need individual `@media print` hiding rules, forever, as new
UI is added — the iframe is immune to this by construction since none of that markup is ever fed to it.

**Availability**: the PDF-export toolbar-header button and its `Ctrl+P` shortcut (§4) must **not** be
gated on `readOnly` — available unconditionally regardless of edit-lock/read-only/conflict-dialog
state, since printing never mutates the document (`docs/MDEditor-spec.md` §6). This is a specific thing
to double-check against the 16-slot-grid pattern's existing `disabled={readOnly}` convention, which
must **not** be copied onto this button.

## 4. Keyboard Shortcut and New Toolbar-Header Button

Elaborates `docs/MDEditor-spec.md` §8. `src/shared/shortcuts.ts`'s `DEFAULT_SHORTCUTS` gains:

```ts
"markdown.pdfExport": "Ctrl+P",
```

Confirmed unused in the existing table.

**Dispatch**: not part of `ToolbarActionId`/`toolbarSlots.ts`'s 16-slot registry (lives in the
toolbar's header, not the fixed grid — `docs/MDEditor-spec.md` §8), and must work regardless of focus
location within the editor pane (same focus-unconstrained requirement as
`markdown-preview/PLAN.md` §6.3's pane-mode-cycle shortcut). Added as a branch in the same, or a
sibling, `keydown` effect in `EditorPane.tsx`:

```ts
useEffect(() => {
  const handler = (e: KeyboardEvent) => {
    const shortcuts = resolveShortcuts(appSettings?.shortcuts);
    const combo = comboFromEvent(e);
    if (combo === shortcuts["markdown.pdfExport"]) {
      e.preventDefault();   // required: suppresses WebView2/Chromium's native Ctrl+P print dialog
      const view = viewRef.current;
      if (view && memoName) void exportMemoToPdf(view, tabId, memoName, memoKey);
    }
  };
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}, [appSettings?.shortcuts, tabId, memoName, memoKey]);
```

**New toolbar-header button** (`MarkdownToolbar.tsx`'s header, alongside
`markdown-preview/PLAN.md` §6.3's pane-mode button, the existing live/source toggle button, and the
minimize button):

```tsx
<button type="button" className="ttn-md-toolbar-pdf-export" onClick={onPdfExport} disabled={!memoName}>
  PDF出力
</button>
```

`onPdfExport` is a new `MarkdownToolbarProps` field, passed down from `EditorPane.tsx` (the same
call-through pattern `liveViewEnabled`/`onToggleLiveView` already use). The PDF button is **never**
gated on `readOnly` (§3's availability requirement) — only on whether a memo is open at all.

## Open Items

1. **§3's cross-document `blob:` URL verification**: confirm early whether a `blob:` URL created in the
   main window resolves inside the hidden PDF-export iframe's separate document; the fallback (mint a
   fresh object URL from already-cached bytes via the iframe's own `contentWindow.URL`) is specified
   in §3 and cheap to apply if the primary approach doesn't work.
