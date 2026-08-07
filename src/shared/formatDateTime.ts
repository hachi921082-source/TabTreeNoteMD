// Formats a raw epoch-ms timestamp for the Properties dialog (`docs/plans/memo-tree/PLAN.md`'s
// `memo_get_properties`/`MemoProperties`). Verbatim TS translation of the old app's
// `formatDateTime` (`PropertiesDialog.tsx`): local-timezone `Date` formatting with weekday +
// relative "n日前", rather than the backend pre-formatting a UTC string.
const WEEKDAYS = ["日", "月", "火", "水", "木", "金", "土"];

export function formatDateTime(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number): string => String(n).padStart(2, "0");
  const dateStr = `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} (${WEEKDAYS[d.getDay()]}) ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  const daysAgo = Math.max(0, Math.floor((Date.now() - ms) / 86400000));
  return `${dateStr}（${daysAgo}日前）`;
}
