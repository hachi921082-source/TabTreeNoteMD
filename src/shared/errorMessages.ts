// Per-language user-facing text for each `ErrorCode` (`docs/spec.md` §5.1's `code`/`message`/
// `path?` wire shape sends `message` to the renderer too, but that string is written for internal
// diagnostic logging in English — UI surfaces that need to show the user something derive their
// localized text from `code` via this function instead of displaying `message` verbatim).
//
// `docs/plans/v1.2.0/i18n/PLAN.md` §7 shape (a): the code->message mapping lives in `i18n/locales/
// ja.ts`/`en.ts` under an `errors.<CODE>` key namespace, looked up via the same `useT()` hook every
// other UI string uses — this function is now a thin `t(`errors.${code}`)` wrapper taking the
// caller's own `t` rather than owning a private, single-language `MESSAGES` map.
import type { ErrorCode } from "./result";
import type { TranslationKey } from "../i18n";

export function errorMessageFor(code: ErrorCode, t: (key: TranslationKey) => string): string {
  return t(`errors.${code}` as TranslationKey);
}
