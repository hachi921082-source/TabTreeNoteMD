// App-wide settings state (`docs/plans/project-architecture/PLAN.md` §2's `state/useSettings.ts`;
// `docs/plans/persistence-fs/PLAN.md` §6). `settings_get`/`settings_set` and their `ipc/settings.ts`
// wrappers already existed with no frontend consumer; `docs/plans/editor-save/PLAN.md` is the
// first feature that actually needs `AppSettings` on the frontend (autosave debounce,
// `editorDefaults` fallback for `MemoEditorSettings`-backed per-memo overrides, `[spec §3.4]`), so
// this hook is added here, scoped to exactly that: load once, expose the value, and a generic
// `patch` for later features (window position, theme, etc.) to reuse rather than re-inventing.
import { useCallback, useEffect, useState } from "react";
import { settingsGet, settingsSet } from "../ipc/settings";
import type { AppSettings, AppSettingsPatch } from "../shared/models";
import type { ErrorCode } from "../shared/result";

export interface UseSettingsResult {
  /** `null` until the initial `settings_get` resolves. */
  settings: AppSettings | null;
  loading: boolean;
  lastErrorCode: ErrorCode | null;
  clearLastError: () => void;
  /** Deep-partial patch; `settings_set` always returns the full post-merge `AppSettings`, which
   * replaces the local value wholesale (no separate optimistic-merge logic needed here). */
  patch: (patch: AppSettingsPatch) => Promise<void>;
  /** Replaces the local value wholesale with an `AppSettings` obtained from some *other* command
   * that also returns the full post-merge settings (currently just
   * `ipc/autostart.ts`'s `autostartSetEnabled`, `docs/plans/window-operation/PLAN.md` §4 — OS
   * registration has to go through that command, not the generic `settings_set` patch, but the
   * frontend's cached settings still need to reflect the result the same way `patch` does). */
  applyFetched: (settings: AppSettings) => void;
}

export function useSettings(): UseSettingsResult {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [lastErrorCode, setLastErrorCode] = useState<ErrorCode | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const result = await settingsGet();
      if (cancelled) return;
      if (result.ok) setSettings(result.value);
      else setLastErrorCode(result.error.code);
      setLoading(false);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const patch = useCallback(async (p: AppSettingsPatch) => {
    const result = await settingsSet(p);
    if (result.ok) setSettings(result.value);
    else setLastErrorCode(result.error.code);
  }, []);

  const clearLastError = useCallback(() => setLastErrorCode(null), []);
  const applyFetched = useCallback((next: AppSettings) => setSettings(next), []);

  return { settings, loading, lastErrorCode, clearLastError, patch, applyFetched };
}
