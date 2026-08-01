# PLAN: Tab Lock — Crash-Mid-Lock Recovery

> Status: planning complete, not yet implemented. Companion to [`PLAN.md`](PLAN.md) (read that file
> first) — split into its own file because it is a large, self-contained sub-algorithm only relevant to
> the crash-recovery path, not to `tab_lock`'s/`tab_unlock`'s ordinary happy-path flow. Elaborates
> `[detailed-spec §5.9]`'s "Recovery on tab-folder access" subsection and `DECISIONS.md`'s Option-B
> resolution of former `QA.md` item Q1. Resolves the three items `detailed-spec.md` explicitly left to
> this plan: marker format, trigger point, and finish-vs-rollback tie-break rule.

Sources: `docs/detailed-spec.md` §5.9; `docs/DECISIONS.md` "Tab Lock: Crash-Mid-Lock Recovery via
Explicit Marker + Resume/Rollback".

---

## Context

If the process is interrupted anywhere in `PLAN.md` §3's `tab_lock` steps 6–9 (after the in-progress
marker is written, before it is deleted), the marker is left on disk. This file specifies how the app
resolves that the next time the tab's folder is accessed, before treating the tab as normally
lockable/unlockable again.

## Marker File Format

`<tab folder>/.tabtreenote.lock-inprogress` (hidden, sibling to `.tabtreenote.json`), plain JSON (not
itself encrypted — see rationale below):

```jsonc
{
  "formatVersion": 1,
  "kdf": { "algo": "scrypt", "N": 32768, "r": 8, "p": 1, "keyLen": 32, "saltB64": "..." },
  "fileMap": [
    { "memoName": "Diary 2026-07", "token": "9f2c7a1e4b3d0f8c6a5e2d1b0c9a8f7e" }
  ]
}
```

- `kdf`/`saltB64` are the **same** salt+params that will end up in the eventual lock blob (`PLAN.md`
  §2) — generated once at `tab_lock` step 5, reused for both the marker and (if the operation
  completes) the final `lock.kdf` field. This is what lets recovery re-derive the *same* key from a
  re-entered password without needing anything else persisted, since the session key itself is never
  persisted (`[spec §5.10]`, security invariant — the marker holds only public KDF parameters, never a
  key).
- `fileMap` is the complete, final planned mapping (every `.md` file that existed when locking began),
  not a partial/incremental log — this is a **correctness requirement**, not a style choice
  (`[detailed-spec §5.9]`): it's what lets recovery determine each entry's state deterministically from
  on-disk evidence alone ("Per-Entry State Check" below), with no separate "how far did the loop get"
  counter to keep in sync.
- **Why the marker is not itself encrypted**: at the point the marker is written (before any renaming
  has happened), every listed `memoName` is still plainly visible as an actual `.md` filename in the
  same folder — the marker's plaintext content reveals nothing an onlooker couldn't already see by
  listing the folder. There is no privacy regression versus the pre-lock state, and encrypting the
  marker would create a chicken-and-egg problem (recovery would need a password just to know which
  files the marker even refers to, before it could offer the user anything to decide between).
- Written/deleted via the same `atomic_write`/self-write-registry/`with_retry` mechanisms as everything
  else in this app (`persistence-fs/PLAN.md` §3, §4, §7) — no special-cased I/O path.

## Trigger Point

**Lazy, per-tab, on `tree_open`** (`memo-tree/PLAN.md` §2) — not a full-startup scan across all
registered tabs. `tree_open`'s very first step, *before* its existing §3.5 reconciliation logic, checks
whether `.tabtreenote.lock-inprogress` exists in the target tab's folder:

- **Absent** → proceed with `tree_open` exactly as already specified in `memo-tree/PLAN.md` §2 (no
  change to that flow).
- **Present** → do **not** run normal reconciliation. Instead return a distinct state to the frontend
  (e.g. `TreeSnapshot { nodes: [], tab_name, locked: false, recovery_pending: true }` — exact field
  addition is this plan's call, not a new command) so the frontend shows the recovery dialog
  ("Frontend" below) instead of the tab's normal tree/empty-state view. This tab still appears as an
  ordinary **unlocked** tab in the tab bar in the meantime (`AppSettings.tabs[i].locked` was never set
  to `true` — that only happens at the very end of a *successful* `tab_lock`, `PLAN.md` §3 step 10 — so
  there is nothing inconsistent about the tab bar continuing to show its real name until recovery
  resolves).
- This reuses the existing per-tab-open code path (cheap, proportional to "was this specific tab just
  selected," per `DECISIONS.md`'s cost-justification writeup) rather than adding a scan whose cost
  scales with the total number of registered tabs or memos.
- `tab_lock` (`PLAN.md` §3 step 3) also defensively re-checks this at its own entry point, but the
  primary and expected trigger is always this `tree_open` path, since a tab must be selected (and
  therefore `tree_open`ed) before the user could reach the "Lock tab" menu action on it again.

## Per-Entry State Check and Finish-vs-Rollback Tie-Break Rule

On detecting a marker, before presenting anything to the user, classify every `{ memoName, token }`
entry in its `fileMap` by on-disk evidence (`[detailed-spec §5.9]`'s per-entry state check):
`renamed` (token file exists, `<memoName>.md` does not) or `not_renamed` (`<memoName>.md` exists, token
file does not). (The neither/both anomalous cases from `[detailed-spec §5.9]` are surfaced as a
distinct error state to the user rather than auto-resolved — not expected outside deliberate external
interference during the crash window, per that section's own note.)

- **Zero entries `renamed`** (crash happened at or immediately after the marker write, before any
  rename began): **automatic rollback, no password needed** — nothing was ever touched, so recovery
  simply deletes the marker and the tab proceeds to normal `tree_open` reconciliation immediately, with
  no dialog shown at all.
- **At least one entry `renamed`** (partial or fully complete rename loop): a password is required
  either way — finishing needs the key to encrypt the remaining `not_renamed` entries and build the
  lock blob; rolling back needs the key to decrypt the already-`renamed` entries back to plaintext.
  Show a dedicated **"Resume interrupted lock"** dialog (shared custom dialog component, distinct copy
  from the ordinary unlock prompt, but reusing the same password-input UI), explaining that a previous
  lock attempt on this tab did not finish, with two actions: **"Finish Locking"** and **"Restore
  (undo lock attempt)"**. Both require the password field to be filled before they can be invoked.
  - On submit, derive the key from the marker's `kdf`/`saltB64` + the entered password, then **verify**
    it by attempting an AES-GCM decrypt of exactly one already-`renamed` entry's content (at least one
    is guaranteed to exist in this branch). Auth-tag failure → `DecryptFailed` ("password incorrect"),
    dialog stays open for retry, **no file is touched** by a failed verification attempt.
  - **Tie-break rule for which action is pre-selected as the default button** (UI default only, not a
    forced choice — user can still pick the other one): default to **"Finish Locking"** when `renamed`
    entries are a strict majority of `fileMap.len()`; default to **"Restore"** when they are a strict
    minority or an exact tie.
  - **Finish**: with the verified key, rename+encrypt every remaining `not_renamed` entry (`PLAN.md` §3
    step 7 continued), then resume `PLAN.md` §3 steps 8–12 (build+write lock blob, delete marker,
    update `AppSettings`, purge index, stop-then-restart the watcher in its locked configuration).
  - **Restore**: with the verified key, for every `renamed` entry: decrypt content, rename back to
    `<memoName>.md` (self-write-registered, `with_retry`). Once every entry is back to plaintext `.md`
    form, delete the marker. No `lock` blob is ever written; `AppSettings.tabs[i].locked` was never
    changed, so no settings rollback is needed either — the tab is exactly as if `tab_lock` had never
    been called.
- **Idempotency**: both Finish and Restore recompute the per-entry classification fresh each time they
  run, so if recovery itself is interrupted again mid-way, re-opening the tab re-triggers this same
  recovery flow against the now-smaller set of remaining entries — safe to retry any number of times
  (`[detailed-spec §5.9]`'s explicit idempotency requirement).

## Frontend

`components/dialogs/`'s shared dialog component, in a "resume interrupted lock" variant (previous
section). This dialog is **not** one of the Esc-disabled exceptions (`[detailed-spec §6.5]`) in the
sense that Esc may close it, but closing it without choosing an action leaves the marker in place — the
tab remains unusable as a normal tab (still shown as `recovery_pending: true`) until the user completes
recovery on a later visit to that tab; this is acceptable since nothing is lost or further damaged by
deferring the choice, unlike the external-change-conflict dialog's Esc-disabled rationale (which
protects against silently discarding a choice about live data).

## Open Items

None. Fully specified and ready for implementation alongside `PLAN.md`.
