// Tree-row file-type icon (GitHub issue #7). Inline SVG line-art (stroke-based, not a flat-color
// emoji) so it can be colored via the `color` prop — matches a memo's label color, or falls back to
// the muted foreground color when unlabeled.
export interface FileIconProps {
  color: string;
  encrypted?: boolean;
}

export function FileIcon({ color, encrypted }: FileIconProps) {
  return (
    <svg
      width="1em"
      height="1em"
      viewBox="0 0 24 24"
      fill="none"
      stroke={color}
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className="ttn-tree-row-file-icon"
      aria-hidden="true"
    >
      {encrypted ? (
        <>
          <rect x="5" y="11" width="14" height="10" rx="2" />
          <path d="M8 11V7a4 4 0 0 1 8 0v4" />
        </>
      ) : (
        <>
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
          <path d="M14 2v6h6" />
        </>
      )}
    </svg>
  );
}
