// Client-side mirror of `src-tauri/src/fs/path_safety.rs` /
// `src-tauri/src/util/name_sanitize.rs`'s Strict validation mode, for
// instant inline feedback while typing a new tab/memo name (`docs/spec.md`
// §5.11 "real-time validation display, replacement suggestions").
// The backend's `path_safety::validate_name` call is authoritative; this
// module only avoids an obviously-doomed round-trip. Keep in sync with the
// Rust rules if either changes.
//
// `validateNameStrict`'s `reason` strings are inline UI-chrome text (shown under a name-input
// field, `dialogs/PromptDialog.tsx`'s `ttn-dialog-error`) — translated via `useT()`, same as every
// other user-facing string in this app (`docs/plans/v1.2.0/i18n/PLAN.md` §6/§7 follow-up; folded in
// after the initial extraction pass since this module's location under `src/shared/` wasn't in
// `PLAN.md` §6's enumerated file list). `validateNameStrict` takes `t` as a parameter (this module
// has no React context of its own) — every call site already has `t` in scope via its own `useT()`.
import type { TranslationKey } from "../i18n";

const FORBIDDEN_CHARS = ["\\", "/", ":", "*", "?", '"', "<", ">", "|"];

const RESERVED_BASE_NAMES = [
  "CON",
  "PRN",
  "AUX",
  "NUL",
  "COM1",
  "COM2",
  "COM3",
  "COM4",
  "COM5",
  "COM6",
  "COM7",
  "COM8",
  "COM9",
  "LPT1",
  "LPT2",
  "LPT3",
  "LPT4",
  "LPT5",
  "LPT6",
  "LPT7",
  "LPT8",
  "LPT9",
];

function isControlChar(ch: string): boolean {
  const code = ch.codePointAt(0) ?? 0;
  return code < 0x20 || code === 0x7f;
}

function hasForbiddenChar(name: string): boolean {
  return [...name].some((ch) => FORBIDDEN_CHARS.includes(ch) || isControlChar(ch));
}

function isReservedName(name: string): boolean {
  const base = name.split(".")[0] ?? name;
  return RESERVED_BASE_NAMES.some((reserved) => reserved.toLowerCase() === base.toLowerCase());
}

export type NameValidationResult = { valid: true } | { valid: false; reason: string };

/** Mirrors `ValidationMode::Strict` (new-name entry: tab create/rename, memo
 * create/rename) — rejects forbidden characters, control characters,
 * reserved device names, leading/trailing spaces, and a trailing period. */
export function validateNameStrict(name: string, t: (key: TranslationKey) => string): NameValidationResult {
  if (name.length === 0) {
    return { valid: false, reason: t("validation.nameRequired") };
  }
  if (hasForbiddenChar(name)) {
    return { valid: false, reason: t("validation.forbiddenChars") };
  }
  if (isReservedName(name)) {
    return { valid: false, reason: t("validation.reservedName") };
  }
  if (name.startsWith(" ") || name.endsWith(" ")) {
    return { valid: false, reason: t("validation.leadingTrailingSpace") };
  }
  if (name.endsWith(".")) {
    return { valid: false, reason: t("validation.trailingPeriod") };
  }
  return { valid: true };
}

/** Best-effort replacement suggestion, mirrors
 * `util::name_sanitize::suggest_sanitized`. */
export function suggestSanitized(name: string): string {
  const replaced = [...name]
    .map((ch) => {
      switch (ch) {
        case "/":
        case "\\":
          return "-";
        case ":":
          return "_";
        case "*":
          return "_";
        case "?":
          return "_";
        case '"':
          return "'";
        case "<":
          return "(";
        case ">":
          return ")";
        case "|":
          return "_";
        default:
          return isControlChar(ch) ? " " : ch;
      }
    })
    .join("");
  return trimEdgesWhitespaceOrPeriod(replaced);
}

function trimEdgesWhitespaceOrPeriod(s: string): string {
  const isTrimmable = (ch: string) => /\s/.test(ch) || ch === ".";
  let start = 0;
  let end = s.length;
  while (start < end && isTrimmable(s[start])) start++;
  while (end > start && isTrimmable(s[end - 1])) end--;
  return s.slice(start, end);
}
