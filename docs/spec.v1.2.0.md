# TabTreeNoteMD v1.2.0 Specification Additions (spec.v1.2.0.md)

> Version-suffixed spec file for v1.2.0, following the convention started by `spec.v1.1.0.md`
> (see `docs/README.md`'s File Structure section). `spec.md` and `spec.v1.1.0.md` stay frozen and are
> never edited for post-v1.1.0 feature work. This file is the authoritative "what is decided" record for
> v1.2.0-only additions/changes, consumed the same way by `designer-lite` (in `docs/detailed-spec.v1.2.0.md`,
> which elaborates this file to implementation-planning detail) and downstream by
> `developer-lite` (`docs/plans/<feature-name>/PLAN.md`). Every item below that builds on or modifies
> existing baseline/prior-version behavior is cited as `[spec §x]` / `[spec v1.1.0 §x]` /
> `[MDEditor-spec §x]`. Decision rationale, comparative analysis, and rejected alternatives belong in
> `DECISIONS.md`, not here. As of 2026-08-20 for §1–§9; §10 added 2026-08-22.
>
> **Status**: all six requested features (§1–§6), plus a discoverability follow-up (§7) and two bug
> fixes surfaced during review (§8 preview-only-mode scroll tracking, §9 preview-pane link-encoding —
> `#slug` anchor navigation **and**, per a decision made after implementation planning began, the
> adjacent `memo:` link navigation branch too), are fully decided; no open points remain. A tenth item,
> the Locked Tab Distinguishing Label (§10), was added 2026-08-22 after GitHub issue #202 surfaced that
> multiple locked tabs are visually indistinguishable — also fully decided, no open points.

---

## Index

Keep this in sync whenever a section is added, renamed, or moved below.

- **§0 Scope** — relationship to `spec.md`/`spec.v1.1.0.md` and to the version-suffixed-spec-file convention.
- **§1 UI Internationalization (i18n) — Runtime Language Switching (New)** — extract UI chrome text
  behind a translation mechanism, switchable at runtime from Settings.
  - §1.1 Current State (No Existing i18n Mechanism)
  - §1.2 Scope: What Gets Translated
  - §1.3 Settings / Data Model
  - §1.4 Language File Location and Mechanism
  - §1.5 Initial Language Set and Default Selection
- **§2 Math/TeX Rendering — Preview-Pane-Only First Pass (New)** — elaborates `[spec §4.2]` /
  `[MDEditor-spec §4]`.
  - §2.1 Scope: Preview Full Render Only, Non-Rendered Elsewhere
  - §2.2 Delimiter Syntax and Renderer — decided: `$...$`/`$$...$$` + KaTeX
  - §2.3 Live-View Treatment (Placeholder-Marker Pattern)
  - §2.4 Source-View Treatment — decided: mark-decoration precedent, reuses/mirrors the existing
    source-view image marker (GitHub issue #81) with a distinct color
  - §2.5 Preview Rendering Pipeline Tie-In
- **§3 Pane-Mode / View-Mode Toolbar Button Semantics (Modifies Existing v1.0.0/Shipped Behavior)** —
  both toggle buttons now show the *next* state on click; current-state display abolished, no separate
  indicator added. Decided.
- **§4 Cross-Tab/Cross-Memo Heading Anchor Links, `memo:TabName/MemoName#slug` (New)** — extends
  `[spec §2.8]`'s `memo:` link syntax with an optional anchor suffix.
  - §4.1 Existing `#slug` Scope (Same-Memo Only) — Baseline Recap
  - §4.2 Syntax Extension
  - §4.3 Resolution Behavior
  - §4.4 Tab Lock and Unresolved Cases
- **§5 Double-Click a Selected Tab — Jump-to-Position Behavior (New)** — jumps cursor+scroll to document
  start; toggle-back to prior position required, transient in-memory state survives edits via
  CodeMirror position-mapping, cleared only on tab/memo switch. Decided.
- **§6 Preview-Pane Zoom (New)** — elaborates `[MDEditor-spec §2]`. Toolbar-header ± buttons, 70–180%/
  10% steps, session-only (resets on tab/memo switch or leaving preview mode), `preview` mode only.
  Decided.
- **§7 Quick Open Discoverability Hint (New)** — elaborates `[spec v1.1.0 §3]`. Extends the 🔍 search
  button's tooltip to also name Quick Open (`Ctrl+O`), raised during v1.2.0 spec review, not one of the
  original six requests. Decided.
- **§8 Preview-Only-Mode Scroll-Position Tracking Fix (Bug Fix)** — `[spec v1.1.0 §2]`'s
  `scrollPosition` persistence never captured the user's actual reading position while
  `paneMode === "preview"` (one-directional editor→preview scroll sync means preview-pane scrolling
  never reaches the editor's own scroll state that `scrollPosition` reads from). Fixed by reading/
  restoring preview-mode scroll directly against the preview pane's own `[data-source-line]` markers.
  Decided.
- **§9 Preview-Pane Link-Encoding Fix — `#slug` Anchor Navigation and `memo:` Link Navigation (Bug Fix)**
  — elaborates `[spec §4.2]` / `[spec §2.8]`. Heading-anchor links (`#slug`) and `memo:`/`memo:TabName/
  MemoName` links alike silently fail to resolve in the preview pane whenever the slug or the tab/memo
  name contains a non-ASCII character (i.e. almost any Japanese heading, tab, or memo name), because
  markdown-it's `normalizeLink` percent-encodes the `href` while the corresponding lookup target (a
  heading's `id`, or the real tab/memo name) stays raw. Both branches of the preview pane's click handler
  get the identical `decodeURIComponent` fix. Also a prerequisite for §4's cross-tab anchor feature
  working correctly in `preview`/`split` mode — for both halves of a `memo:TabName/MemoName#slug` link,
  not just the anchor half. Decided (the `memo:`-branch half was added to this fix's scope after
  implementation planning began, resolving `QA.md`'s `Q-v1.2.0-1`; see `DECISIONS.md`).
- **§10 Locked Tab Distinguishing Label (New — GitHub issue #202, added 2026-08-22)** — elaborates
  `[spec §4.1]` Tab Lock. Supersedes `[spec §4.1]`'s "bare 🔒 glyph, no accompanying text" placeholder
  as the *only* option: an optional, user-authored, per-tab label, set/edited any time via a new tab
  right-click context-menu item, falling back to the bare glyph when unset. Decided.
- **Open Questions for User**

---

## 0. Scope

- `spec.md` and `spec.v1.1.0.md` remain frozen; this file records only what is new or changed for
  v1.2.0. A later `v1.3.0` (etc.) feature set would get its own `spec.v1.3.0.md`, following the same
  pattern.
- Six features were requested for v1.2.0 (§1–§6 above). All six (§1 i18n, §2 math preview, §3
  button-semantics flip, §4 anchor-link extension, §5 double-click-jump, §6 preview zoom) are now fully
  decided with no open points. §7, §8, and §9 were all added during spec review, not part of the
  original six requests: §7 is a small tooltip-only follow-up to `[spec v1.1.0 §3]`'s Quick Open
  feature, prompted by the same discoverability concern raised for §5's double-click gesture; §8 and §9
  are genuine bug fixes surfaced while investigating a question about Edit Lock, though neither turned
  out to be related to Edit Lock itself — §8 fixes `[spec v1.1.0 §2]`'s scroll-position restore for
  preview-only mode, §9 fixes preview-pane link-encoding for non-ASCII names, covering both the `#slug`
  anchor-navigation branch and (per a decision made after implementation planning began, once
  `developer-lite`/`designer-lite` found the identical bug on the adjacent branch while elaborating §4)
  the `memo:` link-navigation branch — making §9 a full prerequisite for §4 working correctly in
  `preview`/`split` mode, not just for its anchor half.
- None of the six modify data already persisted by `spec.md`/`spec.v1.1.0.md`. §3 is a pure UI-behavior
  change to an existing toolbar affordance (no schema change). §5's toggle-back state is transient,
  in-memory-only (no schema change, no persistence).
- §10 was added later still, after the other nine were already fully decided (2026-08-22, GitHub issue
  #202, reported against a since-shipped v1.2.0 preview build with several tabs locked at once). Unlike
  §7/§8/§9, it does add new persisted data (`AppSettings.tabs[i].lockLabel`, `[spec §3.4]`), and it
  supersedes part of `[spec §4.1]`'s existing Tab Lock placeholder-name behavior (see §10 itself and
  `DECISIONS.md`) rather than being a pure addition.

---

## 1. UI Internationalization (i18n) — Runtime Language Switching (New)

### 1.1 Current State (No Existing i18n Mechanism)

- Confirmed by inspection: no i18n/l10n library (`react-i18next`, `react-intl`, etc.) appears in
  `package.json`, and no `i18n`/`useTranslation`/`react-intl` usage appears anywhere under `src/`. All
  UI chrome text is hardcoded Japanese string literals inline in components (e.g.
  `src/components/Settings/SettingsDialog.tsx`'s `SHORTCUT_LABELS`, tab labels "一般"/"エディタ", every
  button label, dialog title, toast message, etc., across the whole `src/` tree).
  - Also confirmed: `spec.md`, `spec.v1.1.0.md`, and `detailed-spec.md` document no language/locale
    setting anywhere — this is a clean, non-conflicting addition, not a change to any existing
    documented setting.
- This means the feature is a genuine, ground-up extraction effort (every hardcoded string in `src/`
  needs to become a `t()`-style lookup) — the *scope* of that extraction (all UI chrome across every
  component) is unambiguous from the request, but the size/sequencing of the refactor is an
  implementation-planning concern for `developer-lite`, not a spec ambiguity.

### 1.2 Scope: What Gets Translated

- **In scope**: all UI chrome text — menu items, dialog titles/labels/buttons, toolbar tooltips/labels,
  toast/notification messages, status bar text, settings screen labels, context menu entries, and
  user-facing error messages surfaced from backend error codes.
  - Error messages are already architecturally friendly to this: IPC uses a `Result<T>` discriminated
    union with error **codes**, not raw backend strings (`[spec §5.1]`), and the frontend is already
    responsible for turning a code into a user-facing message. Translating error messages is therefore
    a frontend-only concern (add the code→message mapping to each language file) — no backend/Rust-side
    change is implied by this feature.
- **Out of scope** (not requested, and would be a separate, much larger feature if ever wanted):
  translating memo *content* itself, or any Markdown-body text. This feature only affects the app's own
  interface chrome, never the user's notes.

### 1.3 Settings / Data Model

- **New `AppSettings` field**: `language: string | null` (`[spec §3.4]` `AppSettings` type; schema
  addition, bumps `AppSettings.schemaVersion` per the existing convention, `[spec §3.2]`). Holds a
  language-tag value (e.g. `"ja"`, `"en"`); `null` means "not yet chosen — fall back to §1.5's
  auto-detect default."
- **Settings UI**: a new row/dropdown in the General (一般) tab of the Settings screen (same tab used
  by the existing Appearance/Links/Autostart sections per `src/components/Settings/SettingsDialog.tsx`'s
  own file-header note), listing the available languages by their own native names (e.g. "日本語",
  "English"). Selecting a value writes immediately via the same `patchSettings`/no-separate-Apply-step
  pattern every other Settings row already uses (`src/components/Settings/SettingsDialog.tsx`, "every
  field reads from the already-loaded `AppSettings` and writes immediately on change").
- **Effective immediately, no restart required**: switching the dropdown re-renders all currently
  mounted UI chrome in the new language in place — this is the "switchable structure," not a one-time
  translation pass, per the request.

### 1.4 Language File Location and Mechanism

- Language files (translation key→string maps, one file per language) live in an appropriate dedicated
  folder under `src/` (exact path, e.g. `src/i18n/locales/`, is implementation discretion for
  `developer-lite`/`designer-lite`'s detailed-spec pass — no existing convention in this codebase
  dictates a specific folder name, and this is the same class of "implementation detail, not a
  spec-level ambiguity" already used for other minutiae in this codebase, e.g. `[detailed-spec §5.8]`
  on auto-link regex exactness).
- Mechanism: a `react-i18next`-**like** mechanism was requested, not necessarily the `react-i18next`
  package itself by name — `t()`-style translation-function calls at every UI-text call site, backed by
  per-language key→string files, with a runtime language switch that re-renders without reload. Whether
  the actual library used is `react-i18next` or an equivalent (or a small hand-rolled context/hook, given
  this app already uses "plain hooks... no external state library," `docs/plans/project-architecture/
  PLAN.md`) is a technology-choice decision within `developer-lite`'s normal implementation-planning
  discretion, not a spec-level ambiguity — the observable behavior (switchable, key-based, per-file) is
  what's specified here.

### 1.5 Initial Language Set and Default Selection

- **Initial language set**: Japanese (`ja`) and English (`en`), per the request. The mechanism itself
  (per §1.4) is not hardcoded to exactly two languages — adding a third language later is just adding
  another language file, not a structural change — but only these two ship with v1.2.0.
- **Default selection when `language` is `null`** (fresh install, or an existing pre-v1.2.0 settings
  file that predates this field): detect the OS UI locale at first read; if it resolves to Japanese, use
  `ja`; otherwise (including any locale not in the shipped set) fall back to `ja` as well, since the
  app's entire existing user-facing text is Japanese today and this preserves current behavior for every
  existing user with zero UI-visible change until they explicitly open Settings and pick a language.
  This is a **default, not a fixed rule** — `AppSettings.language` is a normal persisted user
  preference, changeable at any time via §1.3's dropdown, the same "default, not hardcoded" framing
  already used elsewhere in this app's settings (e.g. `[spec v1.1.0 §3.1]`'s `Ctrl+O` shortcut default).

---

## 2. Math/TeX Rendering — Preview-Pane-Only First Pass (New)

Elaborates `[spec §4.2]` (CommonMark full support) and `[MDEditor-spec §4]` (Preview Rendering). Per the
request, this is an explicitly first-pass, exploratory addition: full math typesetting is added only to
the **preview** pane (`[MDEditor-spec §2]`'s `paneMode: "preview"`/`"split"`); source view and live view
get, at most, an at-a-glance indicator that a formula is present, not full rendering.

### 2.1 Scope: Preview Full Render Only, Non-Rendered Elsewhere

- **Preview pane (`[MDEditor-spec §4]`)**: math expressions render as fully typeset formulas, the same
  pane where full Markdown→HTML rendering already happens today. This reuses that pane's existing
  rendering pipeline (§2.5 below) rather than introducing a separate math-only rendering surface.
- **Live view and source view**: **no full inline math typesetting in this first pass** — this is an
  explicit, deliberate non-goal for v1.2.0's pass at this feature, not an oversight. Both instead get
  some form of "there's a formula here" indicator (§2.3 for live view; §2.4 for source view).
- No dependency on math rendering exists anywhere in the codebase today: confirmed by inspection, no
  `katex`/`mathjax`/`remark-math`/`rehype-katex`-family package appears in `package.json`, and no
  `tex`/`katex` string appears in `src-tauri/Cargo.toml`. This is greenfield — no existing math-adjacent
  code or partial implementation to reconcile with.

### 2.2 Delimiter Syntax and Renderer

**Decided**: `$...$` (inline) / `$$...$$` (block) delimiters (the common GitHub/Typora-style
convention), rendered via **KaTeX** (small footprint, fast, fits the "first pass, preview-only"
framing). The false-positive risk against literal `$` currency text already present in existing notes
(§2.2 originally flagged this as a tradeoff) is accepted as-is for this first pass — no special escaping
rule beyond whatever false-positive-avoidance heuristic the implementation applies (e.g. requiring no
whitespace immediately inside the delimiters) is specified here; refining that heuristic is left to
`developer-lite`'s implementation planning if real false positives turn up in practice.

- **Plug-in point**: the preview pane's Markdown→HTML rendering step (`[MDEditor-spec §4]`, "a full
  Markdown→HTML library... covering CommonMark+GFM equivalent") is where a math-rendering pass would be
  inserted — either as a plugin to that same renderer (if the chosen library supports one, e.g. a
  `markdown-it` math plugin) or as a post-processing pass over the rendered HTML before the
  `[MDEditor-spec §4]` DOMPurify backstop step, mirroring how the two existing allowlisted constructs
  (`<span style>`, `<img>`) are already handled outside the main renderer's own HTML parsing
  (`[MDEditor-spec §4]`, "pre-processed using the exact same regex-based extraction... converted directly
  into safe, code-constructed DOM/HTML elements").
- **Sanitizer boundary unchanged in principle**: whatever the renderer produces still passes through the
  same DOMPurify defense-in-depth backstop already specified for all preview HTML (`[MDEditor-spec §4]`).

### 2.3 Live-View Treatment (Placeholder-Marker Pattern)

- **Reuses an existing, already-shipped precedent**: before real `<img>` rendering was added to live
  view, images were shown as a text-only placeholder widget (`ImagePlaceholderWidget`, `🖼 alt-or-src`,
  `[MDEditor-spec §5]`'s "Replaces... a text-only... placeholder"). Math in live view for this first pass
  follows the **same pattern**: a CodeMirror decoration widget replaces the raw `$...$`/`$$...$$` (or
  whichever delimiter is decided, §2.2) source span with a short, non-interactive badge/marker (e.g. a
  small "∑" or "𝑓(x)"-style glyph, exact glyph implementation discretion) — not the literal source text,
  and not a fully typeset formula.
- **Non-interactive for this pass**: clicking the marker does nothing special (no click-to-edit, no
  popover preview) — consistent with the "first pass, preview-only" framing; richer live-view
  interaction (e.g. click-to-reveal-source, matching how other decorated constructs already toggle
  between rendered/raw on click elsewhere in live view) is left for a future pass if wanted.
- **Source-of-truth unaffected**: exactly like every other live-view decoration (`[spec §4.2]`, "live
  view is a decoration layer over that same buffer, so switching between live view and raw source never
  converts or regenerates the text"), the plain-text buffer still holds the literal delimiter syntax;
  the marker is a purely visual decoration layer, reversible and non-destructive.

### 2.4 Source-View Treatment

**Decided.** Correction to this section's earlier framing: source view is **not** actually a
zero-decoration mode without exception — `[MDEditor-spec §5]`'s "source mode disables the whole
decoration `Compartment`" describes `liveViewCompartment` specifically, but a **separate**, independent
compartment already adds a source-view-only marker for images (GitHub issue #81,
`src/editor/markdownDecorations.ts`'s `SourceImageMarkerPlugin`, wired via its own
`sourceImageMarkerCompartment` in `src/editor/codeMirrorSetup.ts`, active precisely while live view is
off). So a source-view math marker is not a first-of-its-kind exception — it follows an **existing
precedent**, not a new UI-semantics rule.

- **Same mechanism as the image marker**: a `Decoration.mark` (never `Decoration.replace`) applied over
  the raw `$...$`/`$$...$$` span — the underlying delimiter text stays fully visible, selectable, and
  editable; only a background tint and a small leading icon are layered on top (mirrors
  `.cm-md-source-image-marker`'s CSS, `src/components/Editor/Editor.css`: `background:
  var(--ttn-bg-image-marker); border-radius: 3px;` plus a `::before { content: "🖼" }` icon). "Source
  view = raw text" stays intact in the same sense it already does for the image marker: nothing is
  inserted into or removed from the document, and there is no cursor-position-mapping concern.
- **Distinct color from the image marker, per direct user request**: a new CSS custom property (e.g.
  `--ttn-bg-math-marker`) distinct from `--ttn-bg-image-marker`, so the two marker kinds are visually
  distinguishable in source view when both appear in the same memo. A distinct leading icon (reusing
  whichever glyph §2.3's live-view badge uses, for visual consistency between the two math markers)
  completes the distinction alongside the color.
- **Wiring**: reuses the existing `sourceImageMarkerCompartment`-style pattern — either extending that
  same plugin to also scan for math spans, or a sibling plugin/compartment following the identical
  structure, is an implementation-planning detail for `developer-lite`; the spec-level requirement is
  only that it activate under the same "live view off" condition and use the same mark-only, non-viewport-node
  detection approach already established.

### 2.5 Preview Rendering Pipeline Tie-In

- No change to `[MDEditor-spec §4]`'s existing re-render debounce (`AppSettings.preview.debounceMs`),
  scroll-sync (`data-source-line`), or image-loading pipeline mechanisms — math rendering is an
  additional content-transformation step within the same existing preview render cycle, not a parallel
  or competing pipeline.
- **Wide/overflowing formulas**: reuses the same horizontal-scroll-container treatment already specified
  for wide tables (`[MDEditor-spec §4]`, "wrap in a horizontal-scroll container... never auto-shrink font
  size to fit") — the same principle (don't shrink content to fit, let it scroll) applies naturally to a
  formula wider than the pane.
- **Implementation note for `developer-lite` (must-bundle, not CDN-loaded)**: this is a Tauri desktop
  app, expected to work fully offline. KaTeX's stylesheet and font assets must be bundled into the app
  build and loaded locally — never fetched from a CDN at runtime — otherwise formula rendering silently
  breaks (missing glyphs/unstyled output) whenever the app is used without an internet connection. Flagged
  explicitly here so this isn't missed during implementation planning.

---

## 3. Pane-Mode / View-Mode Toolbar Button Semantics (Modifies Existing v1.0.0/Shipped Behavior)

**Decided**: both buttons switch to showing the *next* state as their label, and the current-state
display is **abolished entirely** — no separate current-state indicator is added elsewhere (status bar,
etc.), and this includes the pane-mode toggle's `aria-label` (point 2 below): it also switches to
describing only the next state, rather than retaining a separate current-state description for
screen-reader users. The user infers the currently active pane-mode/view-mode from which pane(s) are
actually visible on screen — a deliberate, direct decision, not an oversight.

- **The two buttons in question**, both already shipped (`[MDEditor-spec §2]`, `[MDEditor-spec §8]`,
  `docs/plans/markdown/markdown-preview/PLAN.md` §6.3), both currently showing their **current** state
  as the visible button label:
  1. **Live/source view-mode toggle** (`src/components/Editor/Toolbar/MarkdownToolbar.tsx`, the
     `ttn-md-toolbar-view-toggle` button) — a 2-state boolean toggle. Label today: "ライブ表示" when
     currently in live view, "ソース表示" when currently in source view — i.e. the label names *where
     you are*, and clicking flips to the other state.
  2. **Pane-mode toggle** (`ttn-md-toolbar-pane-mode` button) — a 3-state cycling toggle
     (`edit → split → preview → edit → ...`, `docs/plans/markdown/markdown-preview/PLAN.md` §4.1/§6.3).
     Label today is `PANE_MODE_LABEL[paneMode]` — "編集のみ"/"分割表示"/"プレビューのみ" for whichever
     state is **current**; the `aria-label` separately also states "(現在: {current label})". Clicking
     advances to the next state in the fixed cycle.
- **Change, decided**: for both buttons, the visible label — **and** the pane-mode toggle's `aria-label`
  — always shows the label of the state clicking would move **to** (the *next* state), not the state
  currently active. The `aria-label`'s "(現在: {current label})" suffix is removed, not reworded to
  additionally describe the next state; it becomes the same next-state-only text as the visible label
  (e.g. "分割表示に切替"-style phrasing), with no separate current-state channel for screen readers
  either.
- **Mechanically straightforward for both buttons**: for the 2-state toggle, "next state" is simply
  "the other state" — trivial to compute. For the 3-state cycle, "next state" is already precomputed by
  the existing `PANE_MODE_CYCLE` map (`docs/plans/markdown/markdown-preview/PLAN.md` §4.1) — also
  trivial.

---

## 4. Cross-Tab/Cross-Memo Heading Anchor Links, `memo:TabName/MemoName#slug` (New)

Extends `[spec §2.8]`'s `memo:` link syntax (`memo:MemoName` / `memo:TabName/MemoName` /
`memo:TabName/`) with an optional `#slug` anchor suffix, so a `memo:` link can jump not just to a memo
but to a specific heading within it — closing the gap that today's `#slug` anchor only works within the
currently-open memo.

### 4.1 Existing `#slug` Scope (Same-Memo Only) — Baseline Recap

- Confirmed: today's `#slug` heading-anchor links (`[spec §2.8]`, "Same-document heading anchor links";
  `[spec §4.2]`, TOC feature) are **explicitly scoped to the current memo only** — resolution "re-scans
  its headings on click" within whatever memo is presently open (`[detailed-spec §5.8]`, "resolution is
  scoped to the currently open memo only, re-computed at click time from a fresh heading scan"). There is
  today no cross-memo or cross-tab form of `#slug` at all — this feature is a pure extension into that
  gap, not a modification of the existing same-memo case (which is untouched).
- **No match = silent no-op** is already the established baseline rule for unmatched `#slug` anchors
  (`[spec §2.8]`, "No match = silent no-op"; `[detailed-spec §5.8]`, "A non-matching slug does nothing —
  no error UI"). This feature reuses that precedent verbatim for the cross-memo case (§4.3 below); no new
  error-handling rule is introduced.

### 4.2 Syntax Extension

- **New form**: `memo:TabName/MemoName#slug` — an optional `#slug` suffix appended to the existing
  cross-tab `memo:TabName/MemoName` form (`[spec §2.8]`).
- **Symmetric extension to the same-tab bare form**: the existing `memo:MemoName` form (current tab,
  `[spec §2.8]`) also accepts the same `#slug` suffix — `memo:MemoName#slug` — for the same reason: both
  forms name a specific target memo, and the anchor suffix's meaning ("jump to this heading within the
  named memo") applies identically regardless of whether a tab prefix is present. This follows directly
  from treating `#slug` as an orthogonal addition to "which memo does this link name," not a special case
  of the cross-tab form specifically.
- **Not applicable to the tab-only form**: `memo:TabName/` (trailing slash, no memo name, `[spec §2.8]`)
  names no specific memo, so there is nothing for `#slug` to scan headings against — this form does not
  gain an anchor variant. (`memo:TabName/#slug` is therefore not a recognized/meaningful pattern; it is
  treated as any other unresolved/malformed link per the existing "unresolved links do nothing" rule,
  `[spec §2.8]`.)
- **Pure additive syntax extension, no conflict with existing usage**: every `memo:` link written today
  (with no `#` suffix) continues to resolve exactly as it does now — `#slug` is optional and its absence
  changes nothing about existing resolution behavior.

### 4.3 Resolution Behavior

- **See §9 of this file**: preview-pane `#slug` navigation currently fails for non-ASCII heading slugs
  regardless of how the anchor was reached (same-memo or, once this feature ships, cross-memo); the
  preview pane's `memo:TabName/MemoName` resolution independently fails the same way whenever the tab or
  memo name is non-ASCII. §9's fix now covers both — it is a prerequisite for **both** steps below to
  work correctly whenever the target memo is viewed in `preview`/`split` mode, not just the anchor-jump
  step — not a separate issue to solve here.
- **Step 1 — resolve the memo target**, exactly as today: `memo:MemoName` / `memo:TabName/MemoName`
  resolution against `AppSettings.tabs` and the target tab's tree, unchanged
  (`[detailed-spec §5.8]`, "`memo:` link resolution"). Read literally, "unchanged" describes the
  resolution *logic*; §9's fix corrects a preview-pane-only encoding defect in reaching that logic with
  the right, undamaged target string in the first place — it does not alter step 1's own matching rules.
- **Step 2 — if a `#slug` suffix is present, resolve the anchor within the now-identified target memo.**
  Since the target memo is not necessarily the currently-open memo (unlike today's same-memo-only
  `#slug` case), this requires the target memo's content to be available to scan for headings: after
  step 1's navigation (switching to the target tab and opening the target memo — the existing end effect
  of any `memo:` link click, `[detailed-spec §5.8]`), the now-open target memo is scanned for headings
  using the exact same slugify/scan logic already used for the same-memo case
  (`src/editor/tocGenerator.ts`'s `scanHeadings`, per `[MDEditor-spec §4]`'s "share the literal function
  rather than reimplementing it"), and the view scrolls to the matching heading — i.e. this is the
  existing same-memo `#slug` resolution, now performed against the memo that step 1 just navigated to,
  instead of against a memo that was already open. No new heading-scan or slugify implementation is
  introduced; this is the existing mechanism applied post-navigation rather than in-place.
- **Ordering guarantee**: the memo switch (step 1) must complete — the target memo loaded into the
  editor/preview — before the heading scan (step 2) runs against it, since the scan operates on the
  now-open memo's content.

### 4.4 Tab Lock and Unresolved Cases

- **Locked target tab (no active session)**: the entire link — including any `#slug` suffix — resolves
  as unresolved/no-op, exactly like today's rule for a memo-only `memo:` link to a locked tab
  (`[spec §2.8]`, "Links to locked tabs: ... always treated as 'unresolved'"). The presence of an anchor
  suffix does not change this — there is no partial-resolution case where the tab-switch is denied but
  the anchor is still attempted.
- **Target tab locked with an active unlock session**: resolves normally, memo and anchor alike — same
  precedent already established for the memo-only case (`[spec §2.8]`, "During unlock session, resolve
  normally").
- **Memo resolves but the named memo has no heading matching `slug`**: silent no-op for the anchor
  specifically — the memo-switch (step 1) still happens (the target memo opens), only the anchor-jump
  (step 2) is skipped, reusing the existing "no match = silent no-op" precedent (§4.1). This mirrors how
  today's same-memo case behaves when headings have changed since a stale link was written — the memo
  itself is already open in that case (trivially, since it's the same-memo case), so this is a direct
  analogy: link resolution and anchor resolution are two independent steps that can each independently
  succeed or silently no-op.

---

## 5. Double-Click a Selected Tab — Jump-to-Position Behavior (New)

- **No existing baseline behavior to reconcile with**: confirmed, no double-click gesture on a tab is
  specified anywhere in `spec.md`/`detailed-spec.md` today. `[spec §2.1]` only specifies single-click
  tab switching ("switch via click / shortcut"); `[detailed-spec §5.1]` elaborates tab management further
  (uniqueness checks, folder-picker behavior, drag-reorder) with no mention of a double-click gesture on
  a tab at all. This is a clean addition, not a change to any documented gesture.
- **Trigger**: double-clicking a tab that is **already the active/selected tab**. Double-clicking an
  inactive tab is unaffected by this feature — it behaves as single-click tab switching already does
  (`[spec §2.1]`); this feature only adds behavior for the already-active-tab case.
- **Discoverability — tooltip hint, decided**: since this gesture has no visible button and no
  Settings-configurable shortcut binding to surface it anywhere, hovering the active tab shows a tooltip
  (e.g. "ダブルクリックで先頭へ") naming the gesture, so it isn't entirely undiscoverable to a user who
  never reads release notes.
- **Target of the jump — decided: document start** ("column 1" = line 1, column 1 of the currently open
  memo — a "jump to document start" gesture, equivalent to Ctrl+Home). Both the **cursor** (text
  insertion point) and the **scroll position** move together to document start.
- **Toggle-back — decided: required.** Immediately before jumping to document start, the cursor position
  and scroll position at that moment are captured into a transient, **in-memory-only, non-persisted**
  value (no `MemoEditorSettings`/`TabMeta` schema field, no `schemaVersion` bump — this is session-local
  UI state, not saved data). A second double-click on the still-active tab, while that saved value still
  exists, restores the cursor and scroll position from it and clears the saved value — the pair of
  double-clicks behaves as a toggle, and a third double-click (with no saved value present) jumps to
  document start again, capturing the then-current position anew.
- **Saved value survives edits, via position-mapping (decided)**: the saved cursor position is not
  stored as a raw, static line/column pair. It is tracked through the CodeMirror 6 `EditorView`'s own
  change-mapping mechanism (`ChangeDesc.mapPos()` — the same primitive CodeMirror already uses internally
  to keep the live cursor/selection valid across document edits, `docs/MDEditor-spec.md` §5.10's
  "CodeMirror 6 decoration-engine architecture"), so edits made to the memo between the jump and the
  return double-click do not invalidate it — the saved position is remapped forward through each edit and
  still points at the same logical location (adjusted for lines/characters inserted or removed before
  it) when restored. This directly enables the common "jump to document start, add/edit something there,
  then double-click again to return to where I was" workflow, which a raw-position-plus-clear-on-edit
  approach would have broken.
- **Saved value is discarded (not restorable) whenever**: the active tab is switched away from, or the
  open memo within the tab is switched. An edit to the memo, by itself, does **not** discard the saved
  value (superseded by the position-mapping point above). Rationale for the tab/memo-switch case: once a
  different tab's or memo's content is displayed, "the original position" no longer has an unambiguous
  target to restore to (there's a different document on screen, not just a shifted version of the same
  one), so the saved value is dropped rather than risk restoring a stale/meaningless position — a further
  double-click in that state is treated as a fresh first double-click (jump to document start, capture
  current position).
- **Distinct from `[spec v1.1.0 §2]`'s existing scroll-position memory**: that feature persists and
  restores a **document line number** (`MemoEditorSettings.scrollPosition`, `[detailed-spec §3.3]`,
  "integer, 1-indexed document line number, not a raw pixel offset") per memo, across tab switches and
  app restarts. This feature's toggle-back state is unrelated and does not read from or write to that
  field — it is a separate, transient, in-session-only value that does not survive a tab switch or memo
  switch (see above), let alone an app restart.

---

## 6. Preview-Pane Zoom (New)

Elaborates `[MDEditor-spec §2]` (preview-only pane mode). **Decided** — all points below were confirmed
directly with the user.

- **No existing zoom mechanism to reconcile with, confirmed by inspection**: the only existing
  scale-like setting is `AppSettings.uiFontScale` (`[spec §2.5]`, 80–150%), which is explicitly scoped
  to "non-editor-body text" (`[spec §2.5]`) — i.e. general UI chrome (menus, dialogs, labels), not the
  editor or preview pane's own content. `[MDEditor-spec §4]`'s preview-pane image display rule
  (`max-width: 100%; height: auto`) is a per-image shrink-to-fit cap, not a user-controlled zoom of the
  pane's rendered content as a whole. Neither is a precedent this feature can directly extend; this is a
  new, independent zoom mechanism scoped to the preview pane's rendered output.
- **Scope: strictly `paneMode === "preview"`** (`[MDEditor-spec §2]`), exactly as literally requested —
  does **not** apply to the preview sub-pane while `paneMode === "split"`. Switching from `preview` into
  `split` or `edit` mode is, per the persistence rule below, itself one of the events that resets zoom to
  100%.
- **What "zoom" affects**: the preview pane's own rendered HTML content (text size, images, tables,
  and — if §2 ships in the same release — rendered math), not the surrounding toolbar/chrome, mirroring
  how `uiFontScale` already deliberately excludes editor-body-equivalent content in the opposite
  direction (`[spec §2.5]`).
- **Placement: toolbar header area** — the same place `[MDEditor-spec §8]`'s pane-mode toggle and PDF
  export buttons were added (outside the spec-closed 16-slot grid, `src/editor/toolbarSlots.ts`,
  `[detailed-spec §5.10]`). This follows that exact precedent, and stays visible because the toolbar
  auto-minimizes to header-only (not fully hidden) when `paneMode` becomes `"preview"`
  (`[MDEditor-spec §7]`).
- **Control: fixed ± buttons only** (no slider, no Ctrl+scroll) — a "－" / "＋" button pair plus a
  percentage readout between them; clicking the percentage readout resets to 100%. **At the 70%/180%
  bounds, the corresponding button (－ at 70%, ＋ at 180%) renders visually disabled** (greyed out,
  non-interactive) rather than silently no-oping past the limit, so the boundary is visually obvious.
- **Keyboard shortcut — both registered, per direct user decision**:
  - **Primary, user-configurable default**: `Ctrl+=` (zoom in) / `Ctrl+-` (zoom out) — the main-row keys,
    mirroring the common browser zoom convention, usable without a numpad. Registered as
    `preview.zoomIn`/`preview.zoomOut` in `src/shared/shortcuts.ts`'s `DEFAULT_SHORTCUTS`, following the
    same new-default-shortcut pattern already used for the pane-mode-cycle (`Ctrl+Shift+P`) and PDF
    export (`Ctrl+P`) buttons (`[MDEditor-spec §8]`) — configurable via `AppSettings.shortcuts`
    (`[spec §3.4]`/`[spec §2.7]`), listed/rebindable in Settings → Shortcuts like any other action.
  - **Secondary, fixed alias**: `Ctrl+NumpadAdd` / `Ctrl+NumpadSubtract` also trigger the identical
    zoom-in/zoom-out action, for numpad users. This is a fixed, always-on alias, **not** exposed as its
    own separate entry in Settings → Shortcuts (deliberately, to avoid a confusing "why are there two
    zoom-in rows" duplicate listing) — only the primary `Ctrl+=`/`Ctrl+-` entry is user-rebindable.
  - Ctrl+scroll is explicitly **not** added in this pass (would additionally require intercepting/
    `preventDefault()`-ing WebView2's native Ctrl+wheel page-zoom gesture, the same class of
    native-shortcut conflict already handled for `Ctrl+P`'s native print dialog, `[MDEditor-spec §8]` —
    deferred, not rejected, as a possible future addition).
- **Button tooltip shows the live-bound shortcut**: each ± button's `title` (tooltip) is built
  dynamically from the currently-resolved `AppSettings.shortcuts` value for the primary action (the same
  `resolveShortcuts(settings?.shortcuts)` lookup already used elsewhere, e.g.
  `src/components/TabBar/TabBar.tsx`), not a hardcoded string — e.g. "ズームイン (Ctrl+=)" — so the
  tooltip stays correct if the user rebinds the primary shortcut in Settings. The fixed numpad alias is
  not separately named in the tooltip (it has no Settings entry to go stale against, per the point
  above); mentioning only the primary/configurable combo is consistent with every other
  configurable-shortcut button in the app never hardcoding its own combo display.
- **Range and step: 70%–180%, in 10% steps, default 100%.** A separate axis from `uiFontScale`
  (`[spec §2.5]`, 80–150%, UI-chrome-only) — deliberately not reusing that range, since this is
  preview-content zoom, a different concern.
- **Persistence: session-only, not a persisted `AppSettings`/schema field.** The zoom level resets to
  100% whenever: the active tab is switched, the open memo is switched, or `paneMode` leaves
  `"preview"` (per the scope rule above). No `schemaVersion` bump — this mirrors §5's toggle-back state
  in being transient, in-memory UI state rather than saved data, though unlike §5 it is a plain reset
  (not a restore-on-return toggle).

---

## 7. Quick Open Discoverability Hint (New — elaborates `[spec v1.1.0 §3]`)

- **Gap**: Quick Open (`[spec v1.1.0 §3]`, default `Ctrl+O`) has no visible button anywhere in the UI —
  it is a keyboard-only overlay, discoverable today only via Settings → Shortcuts or release notes. The
  adjacent 🔍 cross-tab content-search toolbar button (`src/components/TabBar/TabBar.tsx`,
  `title="検索"`, bound to `editor.findReplace`/`Ctrl+F` for in-memo find, no default shortcut of its
  own for the cross-tab case) sits exactly where a user would naturally look for "find a memo," but its
  tooltip today says nothing about Quick Open — so most users only ever discover `Ctrl+F` (in-memo find)
  and never `Ctrl+O` (cross-tab filename jump-open), even though the latter is arguably more useful as
  tab/memo count grows (`[spec v1.1.0 §3.1]`).
- **Decided**: the 🔍 button's tooltip is extended to also name Quick Open and its current shortcut —
  e.g. "検索 ／ クイックオープン (Ctrl+O) でメモ名検索" — built dynamically from the resolved
  `AppSettings.shortcuts["search.quickOpen"]` value (`resolveShortcuts`, the same live-binding tooltip
  pattern just established in §6), so it stays accurate if the user rebinds Quick Open's shortcut.
- **Scope: tooltip-text change only**, on an existing v1.0.0 button. No new button, no schema change, no
  shortcut change. `[spec v1.1.0 §3]`'s Quick Open feature itself (trigger, matching, ordering, Tab Lock
  exclusion) is entirely unmodified — this only makes an already-shipped, easy-to-miss feature more
  discoverable.

---

## 8. Preview-Only-Mode Scroll-Position Tracking Fix (Bug Fix — `[spec v1.1.0 §2]` × Baseline Preview-Only Pane Mode)

**Gap, confirmed by inspection** (surfaced via a question about Edit Lock, but the gap itself is
unrelated to Edit Lock — it affects every preview-only-mode memo, locked or not): `[spec v1.1.0 §2]`'s
per-memo `scrollPosition`/`lastViewedMemo` restore feature reads/writes `scrollPosition` exclusively via
`computeTopVisibleLine(view)` — the CodeMirror editor's own scroll state
(`src/components/Editor/EditorPane.tsx`). The editor↔preview scroll sync is explicitly one-directional
by design (`docs/plans/markdown/markdown-preview/RENDERING.md` §6, "editor scroll drives preview
scroll, never the reverse"). While `paneMode === "preview"` (`[MDEditor-spec §2]`), the edit sub-pane is
`display:none` and the user scrolls the preview pane directly — an action that never touches the
editor's `scrollDOM`, so `computeTopVisibleLine(view)` stays frozen at wherever the editor was last
scrolled before pane-mode became `"preview"`. Consequence: reading through a memo in preview-only mode
never updates the persisted `scrollPosition`; on next open, the memo restores to a stale
pre-preview-mode position instead of where the user actually left off reading.

**Fix — decided**:

- **Save path, preview-only mode**: while `paneMode === "preview"`, `scrollPosition` is computed from
  the **preview pane's own scroll position** instead of the editor's — the topmost currently-visible
  `[data-source-line]` element within the preview container (`src/editor/preview/scrollSync.ts`) is read
  back to its source document line, the exact reverse operation of the existing
  `scrollPreviewToLine(container, targetLine)` function already used for editor→preview sync, reusing
  the same `data-source-line` attribute scheme already on every rendered block (no new data model, no
  new attribute). Saved into the same `MemoEditorSettings.scrollPosition` field (`[detailed-spec §3.3]`)
  — still a 1-indexed document line number, no schema change.
- **Restore path, preview-only mode**: when reopening a memo whose own `MemoEditorSettings.paneMode` is
  `"preview"` (`[MDEditor-spec §3]`), the saved `scrollPosition` is applied by calling
  `scrollPreviewToLine` against the newly-mounted preview container, instead of the
  editor-`scrollIntoView` path used for `edit`/`split` mode.
- **`edit`/`split` mode unaffected**: both the save and restore paths for those two pane modes are
  untouched — they continue to use the editor's own scroll state exactly as `[spec v1.1.0 §2]` already
  specified. The one-directional editor→preview **live**-sync design (`RENDERING.md §6`) is also
  untouched; this fix only changes which pane's scroll state is treated as the source of truth for the
  **persisted snapshot** when `paneMode === "preview"`, not the live-sync behavior while the user stays
  in one mode.
- **Not in scope**: live bidirectional syncing between preview and editor scroll at the moment the user
  switches pane modes mid-session (e.g. jumping the editor to match wherever preview last was, the
  instant the user switches from `preview` back to `split`/`edit`) — this fix is scoped strictly to the
  persist-on-leave/restore-on-open snapshot mechanism `[spec v1.1.0 §2]` already established, not a new
  live-sync feature.

---

## 9. Preview-Pane Link-Encoding Fix — `#slug` Anchor Navigation and `memo:` Link Navigation (Bug Fix — `[spec §4.2]` TOC / Heading-Anchor Links, `[spec §2.8]` `memo:` Links)

**Not an Edit Lock issue** — the original question that surfaced this ("do memo-internal links still work
under Edit Lock?") turned out to have an unrelated real cause: same-document heading-anchor navigation
via `#slug` links is currently broken in the **preview pane specifically**, for any heading whose text
contains a non-ASCII character (in practice, virtually every Japanese heading) — reproduced directly by
the user with a plain manually-written link, e.g. `[spec.mdの作成](#specmdの作成)`, clicking it in
preview does nothing; the identical link in live view correctly jumps to the heading.

**Root cause, confirmed by inspection of the installed `markdown-it` package**
(`node_modules/markdown-it/dist/markdown-it.cjs.js`): markdown-it's link-parsing pipeline runs every
parsed `href` — including a same-document `#slug` fragment — through its `normalizeLink()` rule, which
calls `mdurl.encode(mdurl.format(mdurl.parse(url)))`, percent-encoding non-ASCII characters. A heading
anchor link written as `[spec.mdの作成](#specmdの作成)` is therefore rendered into the preview pane's
HTML with `href="#specmd%E3%81%AE%E4%BD%9C%E6%88%90"` (percent-encoded). Meanwhile, the corresponding
heading's `id` attribute is set directly from `scanHeadings()`'s raw slug value
(`src/editor/preview/previewRenderer.ts`, `token.attrSet("id", headings[headingIndex].slug)`) —
bypassing `normalizeLink` entirely, so it stays **unencoded** (`id="specmdの作成"`).
`previewInteractions.ts`'s `handleAnchorClick` then looks up
`container.querySelector('#' + CSS.escape(href.slice(1)))` using the still-percent-encoded fragment,
which can never match the raw `id` — the click silently does nothing, no error, matching exactly what
was observed. **Live view is unaffected**: its own `#slug` click-resolution path
(`src/editor/links.ts`'s `resolveAndScrollToHeadingAnchor`, `[detailed-spec §5.8]`) reads the slug
straight from a fresh `scanHeadings(view.state)` call against the raw CodeMirror document — it never
passes through markdown-it's HTML rendering pipeline, so no encoding is ever introduced there.

**Fix — decided**: `handleAnchorClick`'s `#slug` branch (`src/editor/preview/previewInteractions.ts`)
decodes the href fragment (`decodeURIComponent(href.slice(1))`) before using it to build the
`querySelector` lookup, reversing markdown-it's `normalizeLink` encoding and restoring the original raw
slug — matching it back against the `id` attribute exactly as `scanHeadings` produced it. No change to
slug generation, `id` assignment, or the TOC-insertion logic (`tocGenerator.ts`) — this is an isolated
decode fix in the preview pane's click-handling path only.

**Interaction with §4 of this file**: this bug directly affects §4's new
`memo:TabName/MemoName#slug` cross-tab anchor feature once the target memo's `#slug` portion is resolved
and the view lands in `preview` mode (or `split` mode, whose preview sub-pane shares the identical
rendering/click-handling code) — §4.3's resolution logic reuses this same heading-scan/click mechanism,
so this fix is a **prerequisite** for §4 behaving correctly whenever the destination memo is viewed in
`preview`/`split` mode with a non-ASCII heading slug (i.e. almost always, for Japanese content). No
change to §4's own spec text is needed — it already specified reuse of the existing mechanism; this fix
is what makes that existing mechanism correct in every pane mode, not just live view.

**Bundled follow-up, decided after `Q-v1.2.0-1` (`docs/QA.md`) — the `memo:` link branch gets the
identical fix, same implementation pass**: while elaborating §4/§9 into `detailed-spec.v1.2.0.md`,
`designer-lite` re-ran this section's own root-cause investigation against a `memo:` target instead of a
bare `#slug` target and confirmed the identical `normalizeLink` percent-encoding defect also corrupts
`memo:TabName/MemoName` hrefs whenever the tab name or memo name (not just the anchor slug) contains a
non-ASCII character — e.g. `mdurl.encode(mdurl.format(mdurl.parse('memo:Work/日記#見出し')))` returns
`memo:Work/%E6%97%A5%E8%A8%98#%E8%A6%8B%E5%87%BA%E3%81%97`, corrupting the memo-name portion of the href
just as thoroughly as the anchor portion. This is a **pre-existing bug in today's baseline `memo:`
cross-tab links** (`[spec §2.8]`), not something introduced by §4 — but it directly undermines §4 for any
Japanese-named tab/memo when clicked from the preview or split pane, since the memo-target half of a
`memo:TabName/MemoName#slug` link would fail to resolve at all in that case, independent of whether the
anchor half is even present. The user decided to fix this in the same v1.2.0 implementation pass rather
than deferring it (see `DECISIONS.md` for the rationale and the rejected "track separately" alternative):
`handleAnchorClick`'s `memo:`-prefixed branch (`src/editor/preview/previewInteractions.ts`, the sibling
branch to the `#slug` branch fixed above) gets the identical `decodeURIComponent` treatment — decoding
the `href` (or its `memo:`-stripped remainder) before it is passed on to `memo:` link resolution, so a
non-ASCII tab/memo name round-trips correctly through the preview pane's rendering pipeline exactly as it
already does in live view. Same fix shape, same root cause, same file, applied to the one remaining
branch this section's original fix did not touch.

---

## 10. Locked Tab Distinguishing Label (New — GitHub issue #202, added 2026-08-22)

**Problem**: `[spec §4.1]`'s locked-tab placeholder is the bare 🔒 glyph alone, with no accompanying
text (`DECISIONS.md` "Tab-Lock Placeholder Display Name: 🔒 Glyph Alone"). With several locked tabs open
at once, every locked tab renders identically on the tab bar — no way to tell them apart without
unlocking each one to check (GitHub issue #202).

**Decided fix**: an optional, entirely user-authored, per-tab label — not an automatically generated
one:

- A new tab right-click context-menu item (e.g. "ロック名を設定" / "Set lock label") opens a
  single-text-input prompt to set or change the label, usable at any time, not only when first locking
  the tab.
- New persisted field, `AppSettings.tabs[i].lockLabel: string | null` (`[spec §3.4]`), keyed to the
  tab's internal `id` — stable across an external folder rename, unaffected by it.
- Until the user sets one, display falls back unchanged to the bare 🔒 glyph — additive only, not a new
  mandatory default for every locked tab.
- The label must never be auto-derived, auto-suggested, or hinted from the real tab/folder name — it is
  the user's own free text, entirely under their control, same privacy intent as the original
  glyph-only placeholder decision.

**Rejected alternative**: an automatically generated sequential label (e.g. "LockTabA"/"LockTabB"/...,
never reused even after a tab's deletion, continuing past "Z" spreadsheet-column-style). Rejected —
see `DECISIONS.md` for the full rationale.

Decided 2026-08-22 (GitHub issue #202 and its comment thread); no open points.

---

## Open Questions for User

None. All ten items (§1–§10) are fully decided.
