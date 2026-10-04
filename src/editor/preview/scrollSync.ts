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

// GitHub issue #279 / spec.v1.2.0 §12: エディタ末尾側スクロール時のプレビュー追従用の純関数群。
// エディタの最後の1ビューポート分 (max(0, maxScroll - viewportHeight) .. maxScroll) を
// 0..1 のブレンド係数に写像する。区間外は 0 (=従来の行合わせのみ)。
// エディタがスクロール不能 (maxScroll <= 0) なら常に 0 で、プレビューを末尾へ強制しない。
export function computeEndBlendFactor(scrollTop: number, maxScroll: number, viewportHeight: number): number {
  if (maxScroll <= 0) return 0;
  // 区間開始は maxScroll を超えない (viewportHeight <= 0 の異常値でも zoneStart <= maxScroll)
  const zoneStart = Math.min(maxScroll, Math.max(0, maxScroll - Math.max(0, viewportHeight)));
  const span = maxScroll - zoneStart;
  if (span <= 0) return scrollTop >= maxScroll ? 1 : 0;
  return Math.min(1, Math.max(0, (scrollTop - zoneStart) / span));
}

// 行合わせ位置 matched からプレビュー末尾 previewMax へ係数 t で線形補間した scrollTop を返す。
// previewMax < matched (行合わせ位置が既に末尾を超える) の場合は末尾へクランプし、
// 補間で逆方向 (上方向) へ引き戻すことはしない。結果は常に 0..previewMax に収める。
export function blendPreviewScrollTarget(matched: number, previewMax: number, t: number): number {
  const max = Math.max(0, previewMax);
  const base = Math.min(max, Math.max(0, matched));
  return base + (max - base) * Math.min(1, Math.max(0, t));
}

// エディタ→プレビュー同期の本体 (一方向のまま)。ブレンド区間外では従来どおり
// scrollPreviewToLine (行合わせ) と同一の動作。区間内のみ行合わせ位置とプレビュー末尾を補間する。
export function syncPreviewToEditorScroll(
  contentContainer: HTMLElement,
  scrollContainer: HTMLElement,
  targetLine: number,
  editorScrollDOM: HTMLElement,
): void {
  const editorMax = editorScrollDOM.scrollHeight - editorScrollDOM.clientHeight;
  const t = computeEndBlendFactor(editorScrollDOM.scrollTop, editorMax, editorScrollDOM.clientHeight);
  if (t <= 0) {
    scrollPreviewToLine(contentContainer, targetLine);
    return;
  }
  const blocks = contentContainer.querySelectorAll<HTMLElement>("[data-source-line]");
  let best: HTMLElement | null = null;
  for (const el of blocks) {
    if (Number(el.dataset.sourceLine) <= targetLine) best = el;
    else break;
  }
  // 行合わせ位置: scrollIntoView({ block: "start" }) が到達する scrollTop 相当
  const matched = best
    ? scrollContainer.scrollTop + (best.getBoundingClientRect().top - scrollContainer.getBoundingClientRect().top)
    : 0;
  const previewMax = scrollContainer.scrollHeight - scrollContainer.clientHeight;
  scrollContainer.scrollTop = blendPreviewScrollTarget(matched, previewMax, t);
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
