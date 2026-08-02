// Window geometry (position/size/maximized) restore-at-startup and persist-on-move/resize
// (`docs/plans/window-operation/PLAN.md` §2). Values are stored/read as physical pixels
// throughout (matching `Window.outerPosition()`/`outerSize()`'s native return type and
// `availableMonitors()`'s monitor rects), so no DPI scale-factor conversion is needed anywhere in
// this module.
//
// Also owns showing the window for the first time: the main window starts hidden
// (`tauri.conf.json`'s `visible: false`, `window-operation/PLAN.md` §4's tray-autostart flag) so
// the restored position/size/maximized state can be applied *before* the window becomes visible,
// avoiding a flash at the config-file default position.
import { useEffect, useRef } from "react";
import { availableMonitors, getCurrentWindow, PhysicalPosition, PhysicalSize } from "@tauri-apps/api/window";
import { windowIsTrayLaunch } from "../ipc/window";
import type { AppSettings, AppSettingsPatch } from "../shared/models";

/** `docs/plans/persistence-fs/PLAN.md` §6.2's "window-geometry note": debounce writes during
 * drag/resize rather than persisting every frame. */
const PERSIST_DEBOUNCE_MS = 500;

/** Minimum on-screen strip (physical px) a restored window must expose on some monitor before
 * it's considered "reachable" — below this, the position is treated as effectively off-screen
 * (e.g. after a monitor configuration change) and reset to a visible default
 * (`docs/plans/window-operation/PLAN.md` §2's defensive clamp). */
const MIN_VISIBLE_MARGIN = 80;

/** Smallest size (physical px) that still keeps the three-pane layout (tree/editor/toolbar)
 * usable — matches `tauri.conf.json`'s `minWidth`/`minHeight`, which stop the *live* window from
 * ever being resized below this. This constant instead guards a *persisted* `settings.json` value
 * that predates that constraint (or was otherwise corrupted) from restoring an unusable window at
 * startup, with no in-app way to recover since this app's users aren't assumed to edit JSON by
 * hand (`docs/spec.md`). */
const MIN_WINDOW_WIDTH = 640;
const MIN_WINDOW_HEIGHT = 480;

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Returns `rect`, clamped onto some monitor in `monitors` if it isn't already reachable there,
 * and with its size floored to `MIN_WINDOW_WIDTH`/`MIN_WINDOW_HEIGHT` (a persisted `settings.json`
 * below that size — e.g. from before that minimum existed, or otherwise corrupted — would
 * otherwise keep restoring an unusable window on every launch). */
function clampToVisibleArea(rect: Rect, monitors: { position: { x: number; y: number }; size: { width: number; height: number } }[]): Rect {
  const sized: Rect = { ...rect, width: Math.max(rect.width, MIN_WINDOW_WIDTH), height: Math.max(rect.height, MIN_WINDOW_HEIGHT) };
  if (monitors.length === 0) return sized;
  const reachable = monitors.some((m) => {
    const overlapLeft = Math.max(sized.x, m.position.x);
    const overlapTop = Math.max(sized.y, m.position.y);
    const overlapRight = Math.min(sized.x + sized.width, m.position.x + m.size.width);
    const overlapBottom = Math.min(sized.y + sized.height, m.position.y + m.size.height);
    return overlapRight - overlapLeft >= MIN_VISIBLE_MARGIN && overlapBottom - overlapTop >= MIN_VISIBLE_MARGIN;
  });
  if (reachable) return sized;
  const m = monitors[0];
  const width = Math.min(sized.width, m.size.width);
  const height = Math.min(sized.height, m.size.height);
  return {
    x: m.position.x + Math.max(0, Math.round((m.size.width - width) / 2)),
    y: m.position.y + Math.max(0, Math.round((m.size.height - height) / 2)),
    width,
    height,
  };
}

export function useWindowGeometry(settings: AppSettings | null, patch: (p: AppSettingsPatch) => Promise<void>): void {
  const appliedRef = useRef(false);
  const patchRef = useRef(patch);
  patchRef.current = patch;
  const debounceRef = useRef<number | null>(null);

  // Restore once, as soon as settings first become available, then reveal the window.
  useEffect(() => {
    if (!settings || appliedRef.current) return;
    appliedRef.current = true;
    const appWindow = getCurrentWindow();
    void (async () => {
      const { x, y, width, height, maximized } = settings.window;
      const monitors = await availableMonitors();
      const clamped = clampToVisibleArea(
        { x, y, width, height },
        monitors.map((m) => ({ position: { x: m.position.x, y: m.position.y }, size: { width: m.size.width, height: m.size.height } })),
      );
      await appWindow.setSize(new PhysicalSize(clamped.width, clamped.height));
      await appWindow.setPosition(new PhysicalPosition(clamped.x, clamped.y));
      if (maximized) await appWindow.maximize();

      // Launched via the tray-autostart Startup entry (`--tray`, `window-operation/PLAN.md` §4):
      // stay hidden. The window is only revealed later when the tray icon is clicked, at which
      // point `main.rs`'s tray event handler calls `show()`/`set_focus()` directly in Rust, not
      // through this hook.
      const trayLaunch = await windowIsTrayLaunch();
      if (trayLaunch.ok && trayLaunch.value) return;

      await appWindow.show();
      await appWindow.setFocus();
    })();
  }, [settings]);

  // Persist on move/resize, debounced. Skips overwriting x/y/width/height while maximized (that
  // would otherwise record the screen-filling size as the "restore" geometry) — only the
  // `maximized` flag itself is updated in that case.
  useEffect(() => {
    const appWindow = getCurrentWindow();
    let unlistenResized: (() => void) | undefined;
    let unlistenMoved: (() => void) | undefined;
    let cancelled = false;

    const schedulePersist = () => {
      if (debounceRef.current !== null) window.clearTimeout(debounceRef.current);
      debounceRef.current = window.setTimeout(() => {
        void (async () => {
          const maximized = await appWindow.isMaximized();
          if (maximized) {
            await patchRef.current({ window: { maximized: true } });
            return;
          }
          const pos = await appWindow.outerPosition();
          const size = await appWindow.outerSize();
          await patchRef.current({
            window: { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized: false },
          });
        })();
      }, PERSIST_DEBOUNCE_MS);
    };

    void appWindow.onResized(schedulePersist).then((un) => {
      if (cancelled) un();
      else unlistenResized = un;
    });
    void appWindow.onMoved(schedulePersist).then((un) => {
      if (cancelled) un();
      else unlistenMoved = un;
    });

    return () => {
      cancelled = true;
      unlistenResized?.();
      unlistenMoved?.();
      if (debounceRef.current !== null) window.clearTimeout(debounceRef.current);
    };
  }, []);
}
