// Shared custom non-blocking dialog shell — replaces `window.confirm()` /
// `window.alert()` / `window.prompt()` (all prohibited, `docs/detailed-spec.md`
// §6.5 "No native blocking dialogs anywhere") app-wide. Every other dialog in
// the app (`PromptDialog`, `ConfirmDialog`, and future tab-lock/conflict
// dialogs) is built on top of this shell so the look stays unified
// (`docs/spec.md` §9.1 item 2).
import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import "./Dialog.css";

export interface DialogProps {
  title: string;
  /**
   * Called when the dialog should close (backdrop click or Esc). Omit
   * entirely to make this dialog Esc-disabled — the one named exception in
   * `docs/detailed-spec.md` §6.5 is the external-change conflict dialog,
   * which must not be dismissible without an explicit user choice.
   */
  onClose?: () => void;
  children: ReactNode;
  footer?: ReactNode;
  /** Appended to the inner `.ttn-dialog` box's className — an escape hatch for callers that need
   * more than the default `min-width: 320px; max-width: 480px` (e.g. the Settings screen,
   * `docs/plans/window-operation/PLAN.md` §7). */
  className?: string;
}

export function Dialog({ title, onClose, children, footer, className }: DialogProps) {
  const boxRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!onClose) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  return (
    <div
      className="ttn-dialog-overlay"
      onMouseDown={(e) => {
        if (onClose && e.target === e.currentTarget) onClose();
      }}
    >
      <div className={`ttn-dialog${className ? ` ${className}` : ""}`} role="dialog" aria-modal="true" aria-label={title} ref={boxRef}>
        <div className="ttn-dialog-title">{title}</div>
        <div className="ttn-dialog-body">{children}</div>
        {footer && <div className="ttn-dialog-footer">{footer}</div>}
      </div>
    </div>
  );
}
