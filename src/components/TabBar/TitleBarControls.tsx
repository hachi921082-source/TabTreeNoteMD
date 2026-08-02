// Self-drawn minimize/maximize/close buttons, replacing the native title bar removed by
// `tauri.conf.json`'s `decorations: false` (`docs/plans/window-operation/PLAN.md` §1). Rendered as
// the rightmost group in `TabBar.tsx`'s toolbar row — the actual title bar drag affordance is
// `TabBar.tsx`'s own `data-tauri-drag-region` on the bar background, not owned by this component.
//
// `close()` intentionally does NOT bypass the app's own exit handling: it calls the same
// `Window.close()` the OS "X" button would, which fires the `tauri://close-requested` event that
// `state/useEditor.ts` already intercepts to flush pending saves before actually destroying the
// window (`docs/plans/editor-save/PLAN.md` §6) — no separate flush logic needed here.
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./TitleBarControls.css";

export function TitleBarControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void appWindow.isMaximized().then((v) => {
      if (!cancelled) setMaximized(v);
    });

    // No dedicated "maximize changed" event in Tauri v2 — `onResized` also fires on
    // maximize/restore, so re-querying there keeps this in sync (same pattern as
    // `state/useEditor.ts`'s minimize-flush listener).
    void appWindow
      .onResized(() => {
        void appWindow.isMaximized().then((v) => {
          if (!cancelled) setMaximized(v);
        });
      })
      .then((un) => {
        if (cancelled) un();
        else unlisten = un;
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return (
    <div className="ttn-titlebar-controls">
      <button type="button" className="ttn-titlebar-btn" title="最小化" aria-label="最小化" onClick={() => void getCurrentWindow().minimize()}>
        &#x2015;
      </button>
      <button
        type="button"
        className="ttn-titlebar-btn"
        title={maximized ? "元のサイズに戻す" : "最大化"}
        aria-label={maximized ? "元のサイズに戻す" : "最大化"}
        onClick={() => void getCurrentWindow().toggleMaximize()}
      >
        {maximized ? "❒" : "□"}
      </button>
      <button
        type="button"
        className="ttn-titlebar-btn ttn-titlebar-btn-close"
        title="閉じる"
        aria-label="閉じる"
        onClick={() => void getCurrentWindow().close()}
      >
        &#x2715;
      </button>
    </div>
  );
}
