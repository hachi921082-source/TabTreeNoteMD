// Generic "is some modal-ish surface currently open" gate
// (`docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md` §5) — NOT scoped to Tab-Lock itself.
// `components/dialogs/Dialog.tsx` registers/unregisters via `enterModal`/`exitModal` on mount/
// unmount; every other dialog in this app (`PasswordDialog`, `ConfirmDialog`, `SettingsDialog`,
// etc.) is built on top of `Dialog.tsx`, so this one wiring point automatically covers all of them
// with no per-dialog code — including `components/Settings/SettingsDialog.tsx` (`[detailed-spec
// v1.1.0 §3.5]` Q5-1, confirming the Settings screen counts as a pause condition too), which itself
// renders via `Dialog.tsx` internally.
//
// Plain module-level counter, not React state: `state/useTabLockAutoRelock.ts` polls
// `isModalActive()` from a `setInterval` tick rather than needing a re-render subscription, so no
// pub/sub mechanism is needed here.
let count = 0;

/** Call on mount of any modal-ish surface (`Dialog.tsx`'s own effect). */
export function enterModal(): void {
  count += 1;
}

/** Call on unmount, paired 1:1 with `enterModal`. */
export function exitModal(): void {
  count = Math.max(0, count - 1);
}

/** `true` while at least one modal-ish surface is open — the auto-relock timer's pause condition
 * (`docs/plans/v1.1.0/tab-lock-auto-relock/PLAN.md` §5). */
export function isModalActive(): boolean {
  return count > 0;
}
