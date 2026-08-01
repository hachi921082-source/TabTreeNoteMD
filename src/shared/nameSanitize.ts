// Client-side mirror of `src-tauri/src/fs/path_safety.rs` /
// `src-tauri/src/util/name_sanitize.rs`'s Strict validation mode, for
// instant inline feedback while typing a new tab/memo name (`docs/spec.md`
// §5.11 "real-time validation display, replacement suggestions").
// The backend's `path_safety::validate_name` call is authoritative; this
// module only avoids an obviously-doomed round-trip. Keep in sync with the
// Rust rules if either changes.

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
export function validateNameStrict(name: string): NameValidationResult {
  if (name.length === 0) {
    return { valid: false, reason: "名前を入力してください" };
  }
  if (hasForbiddenChar(name)) {
    return { valid: false, reason: '使用できない文字が含まれています ( \\ / : * ? " < > | )' };
  }
  if (isReservedName(name)) {
    return { valid: false, reason: "この名前は予約されているため使用できません" };
  }
  if (name.startsWith(" ") || name.endsWith(" ")) {
    return { valid: false, reason: "先頭または末尾に空白は使用できません" };
  }
  if (name.endsWith(".")) {
    return { valid: false, reason: "末尾にピリオドは使用できません" };
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
