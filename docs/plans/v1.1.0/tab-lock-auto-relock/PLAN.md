# PLAN: Tab-Lock Auto-Relock on Inactivity (v1.1.0)

> Status: planning complete. Timer scope is confirmed as **foreground-active-tab-only, single global
> timer** — see §2 below (previously blocked on `docs/QA.md`; resolved by the user and reflected into
> `docs/spec.v1.1.0.md` §5.1 and `docs/detailed-spec.v1.1.0.md` §3.5 as an explicit, deliberate scope
> boundary rather than an oversight; `docs/QA.md` is empty again). The backgrounded-session gap
> developer-lite identified during planning (a cross-tab-drop-created session that never becomes the
> active tab has no natural end-trigger, `tab-lock/PLAN.md` §5) is confirmed **out of scope** for this
> feature — a pre-existing condition of Tab Lock's multi-session model, unrelated to and not newly
> introduced by this feature, knowingly left unaddressed to keep this feature's scope small. No
> multi-session/per-session timer handling is designed here.
> Extends `docs/plans/tab-lock/PLAN.md` (session lifecycle, `tab_end_session`, `state/useTabLock.ts`) and
> `docs/plans/persistence-fs/PLAN.md` §6.1 (`AppSettings` schema). Introduces no new backend command —
> every backend call this feature makes (`tab_end_session`, the existing autosave flush path, `settings_set`)
> already exists. Reuses `docs/plans/v1.1.0/folder-import/PLAN.md` §7's new `components/Toast/Toast.tsx`
> shared primitive for its non-modal warning/notification surfaces.

Sources: `docs/spec.v1.1.0.md` §5; `docs/detailed-spec.v1.1.0.md` §1.4, §2.4, §3.5.

---

## 1. Data Model

New `AppSettings` field:

```rust
// src-tauri/src/model/app_settings.rs
pub struct AppSettings {
    // ...
    pub tab_lock_auto_relock_minutes: Option<u32>,   // NEW — [spec v1.1.0 §5.4]. None (or Some(0)) = disabled.
}
```

- **`schemaVersion` bump**: combined with `docs/plans/v1.1.0/tab-memo-memory/PLAN.md`'s `lastOpened`
  narrowing into a single `APP_SETTINGS_SCHEMA_VERSION` bump (`1` → `2`) — see that plan's §1 for the
  exact coordination note (first-implementer bumps, second verifies rather than re-bumps).
- Absent (pre-upgrade file, or a schemaVersion-2 file predating the user ever opening Settings) resolves
  to the documented default, `15` — not a third "disabled/undefined" state distinct from an explicit
  value (`[detailed-spec v1.1.0 §1.2]`). Whether the Settings UI writes `None` or `Some(0)` for "無効" is
  implementation discretion (both are valid per the type).
- Persisted via the existing generic `settings_set(patch: AppSettingsPatch)` deep-partial-merge command
  (`persistence-fs/PLAN.md` §6.2) — no new persistence plumbing.

## 2. Scope of the Timer — Confirmed: Foreground Active Tab Only

`[spec v1.1.0 §5.1]` (as now finalized) states this feature adds a **single global inactivity timer, not
one per tab** — it tracks and can discard only whichever Tab-Lock session (if any) belongs to the tab
that is currently active, resetting on activity within that tab (§5.2) and tearing that one session down
on timeout (§5.3).

**Backgrounded sessions are deliberately, explicitly out of scope.** During planning, developer-lite found
that Tab Lock's actual multi-session model (`tab-lock/PLAN.md`'s own "Implementation Notes";
`src/state/useTabLock.ts`) allows more than one session to be alive at once — a cross-tab drag-and-drop
onto a locked tab unlocks that tab in place (password entered at drop time) without the drag-source tab
ever losing focus, so the dropped-onto tab's session can stay alive in the background, never itself
becoming the active tab, until the user eventually visits and leaves it or the app exits. This was
escalated to the user via `docs/QA.md` rather than decided unilaterally; the resolution, now reflected in
`[spec v1.1.0 §5.1]` and `[detailed-spec v1.1.0 §3.5]`, is that this feature's single global timer does
**not** reach that backgrounded session — it watches only the foreground active tab. This is a deliberate,
accepted scope boundary, not an oversight: the backgrounded-session case is a pre-existing condition of
Tab Lock's multi-session model, unrelated to and not newly introduced by this feature, and is knowingly
left unaddressed here to keep this feature's scope small (`DECISIONS.md`). A backgrounded session remains
exactly as exposed after this feature ships as it was before it — no multi-session/per-session timer
bookkeeping is designed anywhere in this plan.

The timer mechanics in §3–§7 below are therefore a single timer instance, active only while `activeTabId`
currently holds a session — this is the final design, not an interim reading.

## 3. Constants

`extends detailed-spec.md §3.7`'s Constants Inventory (not editing that file — new constants for this
feature, defined in `src-tauri/src/model/app_settings.rs` or a new `state/tabLockAutoRelock.ts` for the
frontend-only ones):

| Constant | Value | Source |
|---|---|---|
| Warning lead time (before timeout) | 30 seconds, fixed — not configurable, not proportional to the configured timeout | `[detailed-spec v1.1.0 §1.4]` (Q5-2-A) |
| Configurable presets | 5 / 15 / 30 / 60 minutes, plus 無効 (disabled) | `[spec v1.1.0 §5.4]` |
| Default | 15 minutes | `[spec v1.1.0 §5.4]` |

30 seconds is always shorter than the smallest preset (5 min = 300s), so the warning window never needs
clamping for a short configured timeout.

## 4. Timer Lifecycle and Reset — `state/useTabLock.ts`

Extends the existing hook (§1's summary above: no new backend command, this is almost entirely a
frontend addition):

- **Created**: the moment `unlockTab` resolves successfully for the tab that is (per §2's confirmed scope)
  `activeTabId` at that moment. **Destroyed**: the moment that session ends, by whatever existing
  path already calls `tabEndSession` (leaving the tab — the existing `[activeTabId]`-keyed effect,
  §2 above — or this feature's own new triggers, §6/§7 below). If `activeTabId` changes to a *different*
  tab that also holds a session (e.g. the user switches directly from one session tab to another), the old
  timer is destroyed (that session ends via the existing `[activeTabId]`-keyed effect) and a fresh timer is
  created for the newly-active session — there is still only ever one timer instance at a time, per §2.
- **Reset ("activity")**: any keyboard or pointer input at the app-window level while a session is active
  and the timer is not currently paused (§5 below) updates the last-activity timestamp and, if the
  pre-timeout warning (§6 below) is currently showing, cancels/hides it and resets the main timer to its
  full configured duration (Q5-2-B — no separate "stay unlocked" button distinct from ordinary activity).
- If `tabLockAutoRelockMinutes` resolves to disabled (`None`/`0`, §1 above), no timer is ever started.

## 5. Pause Condition

The timer pauses (stops counting down, does not reset) while any of the following is true, and resumes
counting from where it left off once neither is true (`[spec v1.1.0 §5.2]`, `[detailed-spec v1.1.0 §3.5]`
Q5-1):

- Any dialog built from the shared `components/dialogs/Dialog.tsx` is currently open — including
  `docs/plans/v1.1.0/folder-import/PLAN.md` §4's `ImportCollisionDialog`, the concrete case the spec names
  as motivating this rule.
- The Settings screen (`components/Settings/SettingsDialog.tsx`, `window-operation/PLAN.md` §7) is open
  (Q5-1 — the spec's own text only says "any modal dialog," this confirms the Settings screen counts too).

**Mechanism**: a new small shared counter, e.g. `shared/modalActivityGate.ts` — `Dialog.tsx` and
`SettingsDialog.tsx` each increment it on mount and decrement on unmount; the timer's pause condition
reads "counter > 0." This is a generic gate, not scoped to Tab-Lock — any future modal-ish surface that
mounts through `Dialog.tsx` is automatically covered with no per-dialog wiring.

- **The pre-timeout warning (§6 below) is explicitly excluded from this pause list** — it is rendered via
  the non-modal `components/Toast/Toast.tsx` primitive (`docs/plans/v1.1.0/folder-import/PLAN.md` §7), not
  `Dialog.tsx`, specifically so showing it does not pause the very timer it exists to warn about.

## 6. Pre-Timeout Warning

When the timer reaches `(configured minutes) − 30s` with no activity since: show a warning via
`components/Toast/Toast.tsx` (`docs/plans/v1.1.0/folder-import/PLAN.md` §7) — non-blocking, ambient, not
a `Dialog.tsx` instance (§5 above explains why). Exact visual treatment (static text vs. a live countdown)
is implementation discretion (`[detailed-spec v1.1.0 §5]`). Any ordinary activity while it is showing
cancels it and resets the main timer (§4 above, Q5-2-B).

## 7. On-Timeout Teardown and App-Focus-Loss Trigger

Two independent triggers into the **same existing teardown** — no new teardown logic is introduced,
consistent with `[spec v1.1.0 §5.3]`:

- **Inactivity timeout elapses** (no activity since the warning, §6, for the remaining 30s): flush any
  pending unsaved edit first — reuses the exact "flush before changing active tab" call
  `editor-save/PLAN.md` §6's trigger table already makes at ordinary tab-switch time, just invoked here
  instead of at a click — then call `tabEndSession(tabId)` (`tab-lock/PLAN.md` §3, unchanged signature),
  same routine "leaving the tab" already uses. **After** teardown completes, show an auto-relock toast via
  `Toast.tsx` (e.g. "非アクティブのため自動的にロックされました," exact copy implementation discretion,
  Q5-3) — fires **only** for this trigger.
- **App-window loses OS focus** (Tauri window focus-lost event) while a session is active: immediately
  invokes the same flush-then-`tabEndSession` teardown, **independent of and not gated by** the timer's
  current pause/warning state (`[spec v1.1.0 §5.2]`'s broadening of `[spec §4.1 pt.5]` to also treat OS
  focus loss as "leaving the tab"). Implemented via Tauri v2's window focus-change listener (e.g.
  `getCurrentWindow().onFocusChanged(({ payload: focused }) => ...)`), mirroring the `onResized`-based
  minimize-detection precedent already established in `editor-save/PLAN.md`'s Implementation Notes for
  this same class of "OS-level window lifecycle signal" concern — exact API surface to confirm against
  the installed Tauri version at implementation time, same as that precedent did. **No warning** is shown
  for this trigger (it's instantaneous — there is no window of time to show one in) and **no toast** is
  shown after teardown (stays indistinguishable from any other ordinary session end, Q5-3).
- If the pre-teardown flush itself surfaces a save conflict, it reuses the existing `Conflict`
  resolution dialog (`editor-save/PLAN.md` §3) exactly as any other trigger into "leaving the tab"
  teardown already could — no new error handling (`[detailed-spec v1.1.0 §4.2]`'s table, "auto-relock's
  pre-teardown flush hits a save conflict → `CONFLICT` → existing conflict-resolution dialog").

## 8. Settings UI

New row in the Settings screen's existing 一般 tab (`SettingsDialog.tsx`, `window-operation/PLAN.md` §7)
— a dropdown with the five choices from §3 above (5 / 15 / 30 / 60 minutes / 無効), writing
`tab_lock_auto_relock_minutes` via the existing per-field/patch `settings_set` call already used by every
other 一般-tab control. Exact row styling matches whatever convention `developer-lite` establishes
elsewhere in that tab (`[spec v1.1.0 §5.4]`'s own explicit discretion callout — neither
`detailed-spec.md` nor `window-operation/PLAN.md` fixes one).

## Open Items

None.
