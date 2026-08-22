# Session Handoff

> Written to carry context into a fresh conversation when the previous one has grown too large.
> Read this once at the start of the new session, then treat it as disposable — once its content is
> either acted on or absorbed back into `spec.v1.2.0.md` / `detailed-spec.v1.2.0.md` / `DECISIONS.md` /
> `QA.md` / commits, this file can be deleted. It is not a permanent part of the documentation set
> (unlike `spec.md`, `spec.v1.2.0.md`, `DECISIONS.md`, `detailed-spec.md`, `detailed-spec.v1.2.0.md`,
> `QA.md`, which `docs/README.md` and `CLAUDE.md` already index).
>
> **Before acting on anything below, verify it against actual current state** (`git status`, `git log`,
> `git branch`, whether files described here still exist/match) — this file is a snapshot and can go
> stale before it's even read.

## Index

1. Where the project stands
2. This session's work: 6 issues resolved (#197/#200 x2/#201/#205/#206)
3. Process incident this session — read before touching the dev app or any background process
4. Dev-server startup gotcha: port 1420 conflict from a stray leftover process
5. Open issues: full list, recommended order
6. Issue #189: split into #209–212, branching/order decision (unchanged from prior session)

## 1. Where the project stands

- `V1.2.0` is at `87f3865`, in sync with `origin/V1.2.0` (pushed).
- `"CLAUDE - コピー.md"` (untracked, confirmed-stale backup) still sitting in the repo root, still
  unaddressed. Ask the user before touching it.
- The dev app was running earlier this session (`npm run tauri dev`) but **the user has since closed
  it themselves** — nothing is running as of this writing. Don't assume otherwise; check before any
  live-testing.

## 2. This session's work: 6 issues resolved

Fixed the 5 "trivial" issues from the prior session's recommended list, plus a same-day follow-up:

| Issue | PR | Summary |
|---|---|---|
| #197 | #213 | Tab bar's 📂 button vertical misalignment/clipping — added flex-centering to `.ttn-tab-create` |
| #201 | #214 | Preview zoom % readout shifted the ±buttons as digit count changed — fixed width instead of `min-width` |
| #205 | #215 | Font-size toolbar presets widened 12/20/28px → 12/24/36px; synced 2 stale doc tables |
| #200 | #216 | `ConfirmDialog` footer button order swapped to confirm-first/cancel-second (all 6 call sites) |
| #206 | #217 | `ConfirmDialog` Enter-key handler added; a focus-unaware first pass **broke Tab→Cancel→Enter native keyboard nav** (would confirm/delete instead of cancel) — caught by reviewer-lite's live test, fixed with a `document.activeElement` guard |
| #200 (reopened) | #218 | User tested live, found button *text* itself still said 削除/キャンセル — wanted はい/いいえ specifically for the single-memo delete-confirm dialog. Added `common.yes`/`common.no` i18n keys; **only** `TreeView.tsx`'s `deleteConfirm` call site changed — the other 5 `ConfirmDialog` usages (trash-failed-permanent-delete, permanent-unlock x2, tab remove, external-link-open) intentionally still say 削除/キャンセル or their own labels |

All 6 followed the `fix/<slug>` branch → PR → merge → issue-close → `RESOLVED-ISSUES.md` flow. Note:
GitHub's auto-close-on-merge (`Closes #N` in the PR body) does **not** fire here, because the repo's
GitHub-registered default branch is `V1.0.0`, not `V1.2.0` — every issue in this batch had to be closed
manually with `gh issue close`. Keep doing that; don't rely on the PR body alone.

**#200/#206 branch conflict**: both touch `ConfirmDialog.tsx`. Merged #200 first, then had to locally
merge `origin/V1.2.0` into the `fix/confirm-dialog-enter-key` branch, resolve a real conflict (kept
#200's button order, kept #206's new refs on both buttons), push, then merge via `gh pr merge`. If a
similar same-file overlap comes up again, merge one first and expect to do the same local-merge dance
for the second rather than assuming GitHub will fast-forward cleanly.

**#205's spec drift**: reviewer-lite's first pass on #205 found the code fix was correct but
`docs/detailed-spec.md` and `docs/plans/markdown-editing/PLAN.md` still documented the old 20px/28px
values — a good example of why the review step checks docs, not just code. Both were fixed in a
follow-up commit before merging.

## 3. Process incident this session — read before touching the dev app or any background process

While debugging why the app "wouldn't launch" for the user's manual real-device test, this session (as
orchestrator) started `npm run tauri dev` in the background via the Bash tool (`run_in_background`-style
`&`, redirected to a log file). The user then said **「stop」** mid-investigation. What actually happened:
only the *current tool call* (an unrelated file-read) was halted — the already-launched background
`npm`/`cargo`/`tabtreenotemd.exe` process tree kept running untouched. The user caught this a message
later ("それはもうストップしたはずだ" — "that should already have been stopped") when it turned out the
app process was still alive.

**The lesson**: "stop" from the user must be read as "stop everything you set in motion," not just "stop
the tool call in front of you." If you've started a detached/background process (dev server, app
instance, long-running script) and the user says stop, that process needs to actually be killed (or you
need to immediately ask "including the background process I started?" if there's any ambiguity) — don't
just let it keep running silently while you go quiet. This is a sibling lesson to the already-established
CDP/live-target rules from the prior session ([[feedback_verify_live_target_before_cdp_mutation]]), but
specifically about **process lifecycle**, not data mutation: a stray running process is itself a form of
unintended persistent side effect.

In this instance, the user resolved it by restarting/closing the app themselves rather than asking the
assistant to do it — respect that; don't reflexively re-launch or kill dev-server processes without
being asked, once a user has taken over managing the app's lifecycle for the session.

## 4. Dev-server startup gotcha: port 1420 conflict from a stray leftover process

Root cause of the original "won't launch" complaint: a bare `npm run dev` (plain Vite, frontend-only,
**not** `npm run tauri dev`) was left running from an earlier point in the day, bound to
`[::1]:1420` — Tauri's `tauri.conf.json` `beforeDevCommand` also tries to start Vite on the fixed port
1420 (required so Tauri's webview knows where to load from), and Vite's own dev server fails outright
(`Error: Port 1420 is already in use`) instead of finding a free port, which aborts `tauri dev` entirely
before cargo even starts building — so **no window ever appears**, with no cargo/rustc process at all
as the visible symptom.

**If "won't launch" recurs**: check `netstat -ano | findstr :1420` (or the PowerShell
`Get-NetTCPConnection`/`netstat` equivalent) for a stale listener before assuming a code-level bug in the
Rust/Tauri layer. Kill the stale process, then re-run `npm run tauri dev`.

## 5. Open issues: full list, recommended order

Same list as the prior session's HANDOFF minus the 5 (+1 reopened) closed above. Re-verify scale
estimates against current code before starting any of these — no new investigation was done on them
this session.

| # | Title (short) | Scale (from prior session's estimate, unverified since) |
|---|---|---|
| 196 | Scroll-position restore (#195-class bug) | Near-zero — fix already merged into `V1.2.0`; just needs live re-verification and closing |
| 202 | Locked-tab distinguishing label | Small/medium — spec fully decided (see `docs/DECISIONS.md`, "Locked Tab Distinguishing Label") |
| 203 | Right-click paste doesn't handle images | Small/medium — reuse `saveAndInsertImage`, add `clipboard.read()` |
| 208 | Preview-only double-click jump-to-start doesn't scroll | Medium — `paneMode` branch + wire `PreviewPane.getContainer()` |
| 198 | Title bar buttons unreachable at min window size | Medium — likely needs `TitleBarControls` moved out of the scrolling tab-bar container |
| 190 | TOC `<mark>` tag + live-view decoration overlap | Medium — `<mark>` part is the same pattern already fixed for `<span>`; decoration-overlap part unverified |
| 204 | Whole-line-span-wrapped heading vanishes from TOC | Medium/large — structural (lezer syntax-tree level) |
| 191 | Self-referential same-memo `#slug` anchor doesn't jump | Medium — uninvestigated, adjacent to already-understood anchor-jump logic |
| 207 | Settings shortcut rebinding doesn't work | Unknown — code reading found no cause; needs live reproduction first |
| 199 | UI font-size setting doesn't affect tooltips | Large — structural (native `title` attribute outside app CSS reach) |
| 189 | External-change watcher wiring (umbrella) | Large — **split into #209–212, see §6** |

## 6. Issue #189: split into #209–212, branching/order decision (unchanged from prior session)

`docs/plans/persistence-fs/PLAN.md` §5 already designed this; backend watcher/conflict logic is already
implemented and unit-tested, just never wired to any lifecycle trigger.

- **#209** — watcher lifecycle for plain (unlocked) tabs. No dependency.
- **#210** — watcher lifecycle for locked tabs, session-scoped. Depends on #209's mechanism.
- **#211** — pipe detected changes through `watch/conflict.rs`'s composite check, emit events.
  Practically depends on #209.
- **#212** — verify/implement the frontend's handling of those events. Depends on #211.

**Recommended order**: #209 → #211 → #212 → #210. **Branching**: each gets its own `fix/<slug>` branch
+ PR (one issue = one branch = one PR), same as every other issue this session.
