// Applies `AppSettings.theme`/`uiFontScale`/`appearance` to the document root
// (`docs/plans/appearance-theme/PLAN.md` §2): theme switching is a single `data-theme` attribute
// toggle on `document.documentElement`, and the scale/customization fields are reassigned as
// inline CSS custom properties on that same element — both are plain DOM mutations in a `useEffect`,
// never a re-render/remount of the tree, and `src/theme.css`'s stylesheet rules pick up the change
// immediately since they're all `var(...)`-driven. `null` `appearance.accentColor`/`uiFontFamily`
// removes the inline override so the element falls back to `theme.css`'s built-in default for that
// property, rather than setting an inline empty string (which would instead compute to the
// initial/inherited value, not necessarily the same thing).
import { useEffect } from "react";
import type { AppSettings } from "../shared/models";

export function useAppearance(settings: AppSettings | null): void {
  useEffect(() => {
    if (!settings) return;
    const root = document.documentElement;
    root.dataset.theme = settings.theme;
    root.style.setProperty("--ui-font-scale", String(settings.uiFontScale));
    root.style.setProperty("--ttn-tree-row-line-height", String(settings.appearance.treeRowLineHeight));
    if (settings.appearance.accentColor) {
      root.style.setProperty("--ttn-accent", settings.appearance.accentColor);
    } else {
      root.style.removeProperty("--ttn-accent");
    }
    if (settings.appearance.uiFontFamily) {
      root.style.setProperty("--ttn-ui-font-family", settings.appearance.uiFontFamily);
    } else {
      root.style.removeProperty("--ttn-ui-font-family");
    }
  }, [settings]);
}
