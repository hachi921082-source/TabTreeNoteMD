// One-directional (editor -> preview) scroll sync (`docs/plans/markdown/markdown-preview/
// RENDERING.md` §6). Pure DOM reads/writes over the preview's own container subtree — never
// touches CodeMirror state or `contentDOM`, so it carries none of the transaction-safety concerns
// that apply to code reaching back INTO the CodeMirror editor pane.
export function scrollPreviewToLine(container: HTMLElement, targetLine: number): void {
  const blocks = container.querySelectorAll<HTMLElement>("[data-source-line]");
  let best: HTMLElement | null = null;
  for (const el of blocks) {
    const line = Number(el.dataset.sourceLine);
    if (line <= targetLine) best = el; // blocks are in document order, so the last match <= target wins
    else break;
  }
  best?.scrollIntoView({ block: "start" });
}

// Inverse of the editor->preview sync above: reads the preview's OWN scroll state (`docs/plans/
// v1.2.0/preview-scroll-fix/PLAN.md` §2) rather than driving it, for the save-path half of that
// plan's `paneMode === "preview"` scroll-position persistence fix -- the one-directional design
// note at the top of this file still holds (this function never mutates CodeMirror state either).
export function computeTopVisiblePreviewLine(container: HTMLElement): number | null {
  const blocks = container.querySelectorAll<HTMLElement>("[data-source-line]");
  let best: HTMLElement | null = null;
  for (const el of blocks) {
    // Scan basis differs from `scrollPreviewToLine` above: pixel position (`offsetTop` vs.
    // `scrollTop`) rather than a line-number comparison -- the correct basis for "which block is
    // currently topmost in the viewport," mirroring `computeTopVisibleLine`'s own
    // `view.lineBlockAtHeight(view.scrollDOM.scrollTop)` pixel-based lookup on the CodeMirror side.
    if (el.offsetTop <= container.scrollTop) best = el; // last-match-wins scan, same shape as scrollPreviewToLine
    else break;
  }
  if (!best) return null; // no rendered block yet -- caller treats null as "skip, don't persist a wrong value"
  // +1: `data-source-line` is 0-indexed (markdown-it's `.map`), `scrollPosition`/`computeTopVisibleLine`
  // are 1-indexed (CodeMirror lines) -- same convention `previewInteractions.ts`'s `handleCheckboxClick`
  // already applies (`lineIndex0 + 1`) for an unrelated purpose; kept consistent here on purpose.
  return Number(best.dataset.sourceLine) + 1;
}
