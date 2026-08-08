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
