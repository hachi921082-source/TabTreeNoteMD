# Session Handoff

> Written to carry context into a fresh conversation when the previous one has grown too large.
> Read this once at the start of the new session, then treat it as disposable — once its content is
> either acted on or absorbed back into `spec.v1.2.0.md` / `detailed-spec.v1.2.0.md` / `DECISIONS.md` /
> `QA.md` / commits, this file can be deleted. It is not a permanent part of the documentation set
> (unlike `spec.md`, `spec.v1.2.0.md`, `DECISIONS.md`, `detailed-spec.md`, `QA.md`, which
> `docs/plans/README.md` and `CLAUDE.md` already index).
>
> **Before acting on anything below, verify it against actual current state** (`git status`, `git log`,
> `git branch`, whether files described here still exist/match) — this file is a snapshot and can go
> stale before it's even read. The prior version of this file described a completed v1.1.0 session
> (branch `V1.1.0`, last feature `tab-lock-auto-relock`) — that work is fully merged (see §1) and this
> file has been entirely rewritten for the v1.2.0 spec-drafting session that followed it. Don't assume
> a fresh HANDOFF.md is safe to skim past its own verification instruction.

## Index

1. Where the project stands
2. `docs/spec.v1.2.0.md` — fully decided, 9 items, **uncommitted**
3. Item-by-item summary (§1–§9)
4. Two bugs found and fixed during this session (§8, §9) — not part of the original 6 requests
5. Not yet started: `docs/detailed-spec.v1.2.0.md`
6. Uncommitted state
7. Next step
8. Branch policy for v1.2.0 (differs from v1.1.0's, per direct user instruction)

## 1. Where the project stands

- v1.1.0 is fully complete: all 5 features (`folder-import`, `tab-memo-memory`, `favorites`,
  `quick-open`, `tab-lock-auto-relock`) merged into `V1.1.0`, per `8cf71e6`'s merge commit (now the tip
  of both `V1.1.0` and `V1.2.0` — confirmed no divergence between the two branches yet).
- Current branch: **`V1.2.0`**, cut from `V1.1.0` with zero commits of its own so far — this whole
  session was spec/design work in the working tree, no implementation branch created yet.
- This session's work was entirely **designer-lite-style spec alignment** (`.claude/agents/README.md`
  step 1: "User ⇔ designer-lite to finalize the spec"), conducted directly with the user (a designer-lite
  subagent was invoked once early on to do the first full draft pass; all refinement after that was done
  directly in conversation). **No developer-lite or reviewer-lite work has started.**

## 2. `docs/spec.v1.2.0.md` — fully decided, 9 items, **uncommitted**

- The file exists and its own "Open Questions for User" section says "None. All nine items (§1–§9) are
  fully decided" — but it is **untracked in git** (`git status` shows `?? docs/spec.v1.2.0.md`). It was
  never committed during this session; nobody was asked to commit it.
- Per `CLAUDE.md`'s commit convention (one file per commit, message describing the file's role), this
  should get its own commit before being treated as "done" in any durable sense. **Ask the user whether
  to commit it** before proceeding — don't assume yes.

## 3. Item-by-item summary (§1–§9)

Full detail lives in `docs/spec.v1.2.0.md` itself (it has its own Index near the top — read that first,
then jump to the section you need, per this repo's indexing convention). One line each here:

1. **i18n** — `t()`-style runtime ja/en switching, new `AppSettings.language`, no existing i18n library
   found in the codebase (greenfield).
2. **Math/TeX (preview-only first pass)** — KaTeX, `$...$`/`$$...$$`, full render in preview only, badge
   marker in live view, mark-decoration marker in source view (reusing the existing image-marker
   precedent, GitHub issue #81).
3. **Toolbar pane-mode/view-toggle buttons flip to "next state" label** — modifies existing v1.0.0
   behavior; current-state display (including `aria-label`) is abolished, not relocated.
4. **`memo:TabName/MemoName#slug`** — extends existing `memo:` link syntax with an anchor suffix; pure
   additive, reuses existing heading-scan/silent-no-op precedent.
5. **Double-click an active tab → jump to document start**, toggle back on a second double-click
   (position tracked via CodeMirror's own `ChangeDesc.mapPos`, survives edits, cleared only on tab/memo
   switch). Discoverability tooltip added on tab hover.
6. **Preview-only-mode zoom** — toolbar-header ± buttons, 70–180%/10% steps, `Ctrl+=`/`Ctrl+-` primary
   shortcut (configurable) + `Ctrl+NumpadAdd`/`Ctrl+NumpadSubtract` fixed alias, session-only (resets on
   tab/memo switch or leaving preview mode), `preview` mode only (not `split`).
7. **Quick Open discoverability hint** — extends the existing 🔍 search button's tooltip to also name
   Quick Open (`Ctrl+O`); tooltip-text-only change, `[spec v1.1.0 §3]` itself untouched.
8. **Bug fix**: preview-only-mode `scrollPosition` tracking — see §4 below.
9. **Bug fix**: preview-pane `#slug` anchor navigation for non-ASCII slugs — see §4 below.

## 4. Two bugs found and fixed during this session (§8, §9) — not part of the original 6 requests

Both surfaced while investigating an unrelated user question ("does Edit Lock block memo-internal
links?" — answer: no, it doesn't, that part was already fine) and turned out to be genuine, unrelated
bugs in already-shipped behavior:

- **§8**: `[spec v1.1.0 §2]`'s scroll-position restore only ever reads the CodeMirror editor's own scroll
  state. While `paneMode === "preview"`, the user scrolls the preview pane directly, which never touches
  the editor's scroll state (the editor↔preview sync is one-directional, editor→preview only, by
  design — `RENDERING.md §6`). Fix: read/restore preview-mode scroll against the preview pane's own
  `[data-source-line]` markers instead.
- **§9**: same-document `#slug` heading-anchor links silently fail to jump **in the preview pane**
  whenever the heading text contains a non-ASCII character (i.e. virtually every Japanese heading) —
  confirmed root cause by reading the installed `markdown-it` package source: its `normalizeLink()`
  percent-encodes non-ASCII characters in `href`, but the heading's `id` attribute is set from the raw,
  unencoded slug, so they never match. Live view is unaffected (different code path, no markdown-it
  involved). Fix: `decodeURIComponent()` the href fragment before the `querySelector` lookup in
  `previewInteractions.ts`. **This is a prerequisite for §4** (the new cross-tab anchor feature) working
  correctly whenever the destination memo is viewed in `preview`/`split` mode — §4.3 in the spec file
  cross-references this.

## 5. Not yet started: `docs/detailed-spec.v1.2.0.md`

Per `.claude/agents/README.md`'s step 2, the next stage is `designer-lite` elaborating the now-finalized
`spec.v1.2.0.md` into `docs/detailed-spec.v1.2.0.md` (implementation-planning-level detail), following
the same version-suffixed-file convention `detailed-spec.v1.1.0.md` already established. **This has not
been started at all** — the user's last two requests in this session were (a) a plain-English bullet
summary of the 9 items (delivered in-conversation, not written to a file) and (b) this handoff file.

## 6. Uncommitted state

```
?? .claude/files/
?? "CLAUDE - コピー.md"
?? docs/spec.v1.2.0.md
```

The first two are pre-existing and unrelated to this session (carried forward unverified from even
earlier sessions' notes — still presumed pre-existing, worth a fresh check by whichever session first
has reason to touch either). `docs/spec.v1.2.0.md` is this session's own output — see §2.

## 7. Next step

1. Commit `docs/spec.v1.2.0.md` (§2) and this file — **done this session**, pushed to
   `origin/V1.2.0`.
2. Proceed to `detailed-spec.v1.2.0.md` (§5), i.e. resume the `designer-lite` elaboration step — still
   open, ask the user.

## 8. Branch policy for v1.2.0 (differs from v1.1.0's, per direct user instruction)

**`V1.2.0` development does NOT merge back into `V1.1.0`.** Per the user: "v1.1.0とは別にv1.2.0として開発
を継続するのでv1.1.0へマージはしない" (development continues as v1.2.0, separate from v1.1.0; no merge
into v1.1.0 planned). This is a deliberate divergence from §7 of the old (pre-rewrite) version of this
file, which described a `feature/v1.1.0-<name>` → merge-into-`V1.1.0` flow for that version's work —
**that flow does not apply here.** Whatever branch strategy v1.2.0 features end up using (feature
branches off `V1.2.0`, or direct commits to `V1.2.0`), the merge target is not `V1.1.0`. If a
feature-branch-per-item flow is adopted for v1.2.0 too, confirm the actual merge target with the user
before assuming it's `V1.2.0` by default — this was stated once, in passing, not exhaustively specified.
