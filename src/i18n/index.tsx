// UI internationalization — hand-rolled React Context, not `react-i18next` (`docs/DECISIONS.md`
// "i18n Mechanism: Hand-Rolled React Context, Not `react-i18next` Package"). `docs/plans/v1.2.0/
// i18n/PLAN.md` §3.
//
// Mounting note (deviation from `PLAN.md` §3.2's literal sketch): the plan's own code sample shows
// `I18nProvider` calling `useSettings()` internally and being mounted in `src/main.tsx` "inside
// whatever provider already makes `useSettings()` available." This codebase has no such provider —
// `App.tsx` is the sole caller of `state/useSettings.ts`'s hook and threads the resulting
// `AppSettings` down as a prop to every consumer (`docs/plans/project-architecture/PLAN.md`'s
// existing convention, no pre-existing Context anywhere in `src/`). Having `I18nProvider` call
// `useSettings()` a second time here would create a SECOND, independent settings-fetch/state
// instance that never observes `patchSettings` calls made through `App.tsx`'s own instance — i.e.
// changing the language in Settings would never actually update `useT()`'s consumers, breaking the
// "no reload, no remount" requirement this feature is built on. Instead, `I18nProvider` takes
// `settings: AppSettings | null` as a plain prop and is mounted by `App.tsx` itself (wrapping its
// returned JSX), reusing `App.tsx`'s single `settingsState.settings` — reactivity is preserved by
// ordinary prop-flow instead of a second Context/hook layer.
import { createContext, useCallback, useContext, useMemo } from "react";
import type { ReactNode } from "react";
import { ja } from "./locales/ja";
import { en } from "./locales/en";
import type { AppSettings } from "../shared/models";

export type TranslationKey = keyof typeof ja;
export type Language = "ja" | "en";

const translations: Record<Language, Record<TranslationKey, string>> = { ja, en };

interface I18nContextValue {
  lang: Language;
  t: (key: TranslationKey) => string;
}

const defaultContextValue: I18nContextValue = {
  lang: "ja",
  t: (key) => translations.ja[key],
};

const I18nContext = createContext<I18nContextValue>(defaultContextValue);

/** `[spec v1.2.0 §1.5]`'s literal rule ("if Japanese, use `ja`; otherwise... fall back to `ja` as
 * well") — both branches currently return `"ja"`. The `navigator.language` detection call still
 * runs (not stubbed to a bare constant): a future third non-`ja` default's story depends on the
 * detection step already being wired (`[detailed-spec v1.2.0 §1.2]`). */
export function resolveDefaultLanguage(): Language {
  const detected = navigator.language;
  return detected.toLowerCase().startsWith("ja") ? "ja" : "ja";
}

/** `settings.language` is `"ja"`/`"en"` once explicitly chosen from Settings, `null` before that
 * choice is ever made, or (a hand-edited settings file) some other unrecognized string — the last
 * two cases both fall through to `resolveDefaultLanguage()`, this app's general "silent, safe
 * fallback over error dialog" convention for a malformed persisted value (`PLAN.md` §5). */
export function resolveLanguage(settings: AppSettings | null): Language {
  if (settings?.language === "ja" || settings?.language === "en") return settings.language;
  return resolveDefaultLanguage();
}

export interface I18nProviderProps {
  settings: AppSettings | null;
  children: ReactNode;
}

export function I18nProvider({ settings, children }: I18nProviderProps) {
  const lang = resolveLanguage(settings);
  const t = useCallback(
    (key: TranslationKey): string => {
      // Missing-key fallback: defensive only (the `Record<TranslationKey, string>` annotation on
      // `en.ts` makes this unreachable for `en`), but guards against a future third language file
      // shipped incomplete.
      return translations[lang][key] ?? translations.ja[key];
    },
    [lang],
  );
  const value = useMemo<I18nContextValue>(() => ({ lang, t }), [lang, t]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useT(): (key: TranslationKey) => string {
  return useContext(I18nContext).t;
}

export function useLang(): Language {
  return useContext(I18nContext).lang;
}
