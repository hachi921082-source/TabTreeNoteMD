// Formats a raw epoch-ms timestamp for the Properties dialog (`docs/plans/memo-tree/PLAN.md`'s
// `memo_get_properties`/`MemoProperties`). Verbatim TS translation of the old app's
// `formatDateTime` (`PropertiesDialog.tsx`): local-timezone `Date` formatting with weekday +
// relative "n日前", rather than the backend pre-formatting a UTC string.
//
// The weekday abbreviation and relative-days suffix are translated via `useT()`, same as every
// other user-facing string in this app (`docs/plans/v1.2.0/i18n/PLAN.md` §6/§7 follow-up; folded in
// after the initial extraction pass since this module's location under `src/shared/` wasn't in
// `PLAN.md` §6's enumerated file list). `formatDateTime` takes `t` as a parameter (this module has
// no React context of its own) — both call sites (`TreeView.tsx`/`EditorPane.tsx`'s Properties
// dialogs) already have `t` in scope via their own `useT()`. The `{days}` template's surrounding
// punctuation (full-width parens in `ja`, a leading space + half-width parens in `en`) lives inside
// each locale's own `date.relativeDaysAgoTemplate` value, not hardcoded here.
import type { TranslationKey } from "../i18n";

/** Index matches `Date.prototype.getDay()` (0 = Sunday ... 6 = Saturday). */
const WEEKDAY_KEYS: TranslationKey[] = [
  "date.weekdaySun",
  "date.weekdayMon",
  "date.weekdayTue",
  "date.weekdayWed",
  "date.weekdayThu",
  "date.weekdayFri",
  "date.weekdaySat",
];

export function formatDateTime(ms: number, t: (key: TranslationKey) => string): string {
  const d = new Date(ms);
  const pad = (n: number): string => String(n).padStart(2, "0");
  const dateStr = `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} (${t(WEEKDAY_KEYS[d.getDay()])}) ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  const daysAgo = Math.max(0, Math.floor((Date.now() - ms) / 86400000));
  return `${dateStr}${t("date.relativeDaysAgoTemplate").replace("{days}", String(daysAgo))}`;
}
