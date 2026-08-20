# TabTreeNoteMD v1.2.0 Detailed Specification (detailed-spec.v1.2.0.md)

> Elaboration of [spec.v1.2.0.md](spec.v1.2.0.md) to a level usable by `developer-lite` when writing
> implementation plans (`docs/plans/<feature-name>/PLAN.md`) for the nine v1.2.0 items (§1–§9). This
> document plays the same role for `spec.v1.2.0.md` that
> [detailed-spec.v1.1.0.md](detailed-spec.v1.1.0.md) plays for `spec.v1.1.0.md`, and that
> [detailed-spec.md](detailed-spec.md) plays for `spec.md` — it does not introduce new product decisions;
> it translates and refines what `spec.v1.2.0.md` already decided, down to concrete file/function touch
> points, data-model fields, and edge cases, grounded in direct inspection of the currently-shipping
> `src/`/`src-tauri/` code (not just the prior spec documents, several of which lag slightly behind what
> has actually shipped via the GitHub-issue fix workflow — e.g. `MemoEditorSettings.cursorAnchor`/
> `cursorHead`/`previewLocked` exist in `src/shared/models.ts` today but are undocumented in
> `spec.md`/`detailed-spec.md`/`MDEditor-spec.md`, and are load-bearing context for this document's own
> elaboration). A genuine ambiguity `spec.v1.2.0.md` did not decide or delegate is never decided here — it
> is logged in [QA.md](QA.md) instead (see §4.4/§9.2/§12 for the one case this pass found, since resolved
> by the user — rationale in `DECISIONS.md`).
>
> **Citation convention** (matches `detailed-spec.v1.1.0.md`'s own convention, extended one more level):
> - `[spec §x]` — cites `docs/spec.md` (the frozen v1.0.0 baseline).
> - `[spec v1.1.0 §x]` — cites `docs/spec.v1.1.0.md`.
> - `[MDEditor-spec §x]` — cites `docs/MDEditor-spec.md` (split-pane preview / PDF export / live-image
>   detailed spec, itself an elaboration of `[spec §4.2]`).
> - `[spec v1.2.0 §x]` — cites `docs/spec.v1.2.0.md` (the v1.2.0 feature additions this file elaborates).
> - `` `detailed-spec.md` §x `` / `` `detailed-spec.v1.1.0.md` §x `` (no brackets) — cite the two earlier
>   detailed-spec files, for mechanisms and conventions this file reuses rather than redefines.
> - A bare file path (e.g. `src/editor/links.ts`) cites the actual source file inspected while writing
>   this document — used throughout, since most of v1.2.0's nine items are surgical changes to specific,
>   already-identified call sites rather than new subsystems.
>
> Read `spec.v1.2.0.md` first; this document assumes familiarity with it and does not repeat rationale
> already written inline there. Decision rationale, comparative analysis, and rejected alternatives
> belong in `DECISIONS.md`, not here.

---

## Index

Keep this in sync whenever a section is added, renamed, or moved below.

- **§0 How to Use This Document**
- **§1 UI Internationalization (i18n) — Elaboration** (elaborates `[spec v1.2.0 §1]`)
  - §1.1 Mechanism Choice (Implementation-Discretion Call Made In This Pass)
  - §1.2 Data Model and Settings-UI Touch Point
  - §1.3 String-Extraction Scope and Sequencing
  - §1.4 Error-Message Translation Touch Point
- **§2 Math/TeX Rendering — Elaboration** (elaborates `[spec v1.2.0 §2]`)
  - §2.1 Preview Pipeline Touch Point and Plugin Choice
  - §2.2 False-Positive Heuristic (Implementation-Discretion Call Made In This Pass)
  - §2.3 Live-View Marker (Implementation-Discretion Call Made In This Pass)
  - §2.4 Source-View Marker — Concrete Reuse of `SourceImageMarkerPlugin`
  - §2.5 Preview Rendering Pipeline Tie-In (Debounce/Scroll-Sync Reuse, Wide-Formula Wrapping, Asset
    Bundling)
- **§3 Pane-Mode / View-Mode Toolbar Button Semantics — Elaboration** (elaborates `[spec v1.2.0 §3]`)
- **§4 Cross-Tab Heading Anchor Links, `memo:TabName/MemoName#slug` — Elaboration** (elaborates
  `[spec v1.2.0 §4]`)
  - §4.1 Target-String Parsing (No Backend Change Needed)
  - §4.2 Cross-Tab Navigation State Machine — New `pendingLinkFocus.anchorSlug`
  - §4.3 Anchor-Jump Execution, Pane-Mode-Dependent
  - §4.4 Adjacent `memo:`-Branch Encoding Bug — Bundled Into §9's Fix (resolved; see `DECISIONS.md`)
  - §4.5 Tab Lock and Unresolved-Cases Behavior (elaborates `[spec v1.2.0 §4.4]`; note its numbering here
    diverges from `spec.v1.2.0.md`'s own §4.4, see §4.5's own note)
- **§5 Double-Click Active Tab — Jump-to-Position — Elaboration** (elaborates `[spec v1.2.0 §5]`)
  - §5.1 Trigger Wiring (`TabBar.tsx` → `App.tsx` → `EditorPaneHandle`)
  - §5.2 Saved-Position Shape and Position-Mapping Mechanism
  - §5.3 Reset Conditions
  - §5.4 Discoverability Tooltip Touch Point
- **§6 Preview-Pane Zoom — Elaboration** (elaborates `[spec v1.2.0 §6]`)
  - §6.1 State Placement and Reset Wiring
  - §6.2 CSS Mechanism (Implementation-Discretion Call Made In This Pass)
  - §6.3 Button Placement and Bounds-Disabled Rendering
  - §6.4 Keyboard Shortcut Wiring — the `Ctrl+NumpadSubtract`/`Ctrl+-` Collision Finding
- **§7 Quick Open Discoverability Hint — Elaboration** (elaborates `[spec v1.2.0 §7]`)
- **§8 Preview-Only-Mode Scroll-Position Tracking Fix — Elaboration** (elaborates `[spec v1.2.0 §8]`)
  - §8.1 New Function: Reverse of `scrollPreviewToLine`
  - §8.2 Save-Path and Restore-Path Touch Points
  - §8.3 Shared Timing Concern With §4 — See §10.1
- **§9 Preview-Pane Link-Encoding Fix — `#slug` Anchor Navigation and `memo:` Link Navigation —
  Elaboration** (elaborates `[spec v1.2.0 §9]`, including its `memo:`-branch bundled follow-up)
  - §9.1 `#slug` Branch (as Originally Decided)
  - §9.2 `memo:` Branch (Bundled Follow-Up, Resolves `Q-v1.2.0-1`)
- **§10 Cross-Cutting Additions**
  - §10.1 New Shared Primitive: Preview-Render-Readiness Gate (needed by §4 and §8 alike)
  - §10.2 `schemaVersion` / Migration Handling
  - §10.3 Error Handling — No New `ErrorCode`s
- **§11 Deferred-to-Implementation Details (Not Ambiguities — No User Decision Needed)**
- **§12 Open Questions Requiring User Decision** — none remaining; `Q-v1.2.0-1` resolved (§4.4/§9.2)

---

## 0. How to Use This Document

- `spec.v1.2.0.md` = authoritative "what is decided" for the nine v1.2.0 items. This document = "how
  that decided behavior plays out precisely enough to plan and build it," reusing every mechanism already
  established by `spec.md`/`detailed-spec.md`/`spec.v1.1.0.md`/`detailed-spec.v1.1.0.md`/
  `MDEditor-spec.md` rather than redefining it. Neither document dictates Rust module layout or Tauri
  command naming — that stays `PLAN.md`-level.
- `spec.v1.2.0.md` itself already reaches a very high level of detail (its own §0 notes "all nine
  items... fully decided; no open points remain"). This document's job is therefore **grounding** each
  decided behavior against the actual, current `src/`/`src-tauri/` implementation — concrete function
  names, concrete data flow, and edge cases that only become visible once a decided behavior is mapped
  onto real code. One gap was found this way (§4.4/§9.2, resolved — rationale in `DECISIONS.md`, resolution
  recorded in §12).
- **Implementation-discretion calls made in this pass** (§1.1, §2.2, §2.3, §6.2): `spec.v1.2.0.md` itself
  explicitly delegated these to "`developer-lite`/`designer-lite`'s detailed-spec pass" (see each
  section's own citation). Each is stated with its reasoning inline, same "starting default, not a fixed
  rule" framing `detailed-spec.md` §0 and `detailed-spec.v1.1.0.md` §0 both already use — `developer-lite`
  may adjust within the same spirit during `PLAN.md` writing if a concrete implementation reason
  surfaces.
- Every mechanism already specified for the baseline or v1.1.0 (atomic write, self-write registry, sync
  order, the shared dialog component, the `resolveShortcuts`/`comboFromEvent` shortcut system, the
  `pendingXxxFocus` per-trigger-source cross-tab-navigation state-machine convention, etc.) is reused
  verbatim by v1.2.0's items unless stated otherwise; this document cites the relevant prior section
  rather than re-deriving those mechanisms.

---

## 1. UI Internationalization (i18n) — Elaboration

Elaborates `[spec v1.2.0 §1]` in full.

### 1.1 Mechanism Choice (Implementation-Discretion Call Made In This Pass)

`[spec v1.2.0 §1.4]` explicitly delegates both the exact folder location and the exact library/mechanism
to this pass. Decisions made here, with reasoning:

- **Folder location: `src/i18n/`**, with one file per language under `src/i18n/locales/` (e.g.
  `src/i18n/locales/ja.ts`, `src/i18n/locales/en.ts`), plus `src/i18n/index.ts` exporting the
  hook/provider described below. Reasoning: matches this codebase's existing flat, feature-named
  top-level folder convention directly under `src/` (`src/editor/`, `src/ipc/`, `src/state/`, `src/
  shared/` — confirmed by inspection, no nested `src/lib/` or `src/utils/` catch-all exists anywhere in
  the tree), so `src/i18n/` sits at the same level as `src/shared/` for the same reason `src/shared/
  shortcuts.ts` and `src/shared/models.ts` do: cross-cutting, not owned by one feature/component.
- **Mechanism: a small hand-rolled React Context + hook**, not the `react-i18next` package — matches this
  app's deliberate "plain hooks, no external state library" architecture (see `DECISIONS.md`, "i18n
  Mechanism: Hand-Rolled React Context, Not `react-i18next` Package," for the full rationale).
- **Concrete shape**: `src/i18n/index.ts` exports (a) a `translations: Record<"ja" | "en", TranslationMap>`
  object assembled from the two locale files, (b) an `I18nProvider` wrapping `<App />` (mounted once in
  `src/main.tsx`) that reads the resolved language (§1.2 below) from `useSettings()`'s already-loaded
  `AppSettings` and provides it via `React.createContext`, and (c) a `useT()` hook returning a `t(key:
  TranslationKey) => string` function that looks up the current language's map, falling back to `ja` (per
  `[spec v1.2.0 §1.5]`'s own resolution rule) if a key is somehow missing from a language file (a
  translator-error safety net, not a spec-mandated behavior — `developer-lite`'s call whether to also
  `console.warn` in dev builds).
- **Satisfies `[spec v1.2.0 §1.3]`'s "effective immediately, no restart required" requirement by
  construction**: since `I18nProvider`'s context value is derived directly from `useSettings()`'s live
  `AppSettings.language`, an ordinary React Context re-render already propagates to every mounted `useT()`
  consumer the instant `patchSettings({ language: ... })` resolves and `useSettings()`'s own state updates
  — no reload, no remount, no separate "apply" step. This is a direct, standard consequence of the Context
  design chosen above, not additional code that needs to be written.
- **`TranslationKey` as a union type derived from `ja.ts`'s own keys** (e.g. `type TranslationKey =
  keyof typeof ja`), so `en.ts` (and any future language file) is type-checked at compile time to have
  every key `ja.ts` has — catches a missing translation as a TypeScript error rather than a silent
  runtime fallback, cheap to add and directly useful given `[spec v1.2.0 §1.2]`'s scope is "every hardcoded
  string in `src/`" (a large, error-prone extraction).

### 1.2 Data Model and Settings-UI Touch Point

Elaborates `[spec v1.2.0 §1.3]`.

- **`AppSettings.language: string | null`** — new field, sibling of `theme`/`uiFontScale` in
  `src/shared/models.ts`'s `AppSettings` interface (confirmed by inspection: `theme: "light" | "dark";`
  and `uiFontScale: number;` sit at lines ~12–13 of that interface today — `language` goes directly
  alongside these, not nested under `appearance`, since language is not a visual-styling concern the way
  `AppearanceSettings` is). Matching `AppSettingsPatch` gains `language?: string | null`.
- Bumps `AppSettings.schemaVersion` once — see §10.2.
- **Settings UI**: new row in the existing 一般 (General) tab of `SettingsDialog.tsx`, placed near the
  existing `theme`/`uiFontScale` rows (both already in that tab per the file's own header comment,
  "General houses Appearance... theme, uiFontScale..."). Concrete pattern, copied from the adjacent
  `theme` `<select>` (`src/components/Settings/SettingsDialog.tsx`, the `<select value={settings.theme}
  onChange={(e) => void patchSettings({ theme: ... })}>` block already in that file):

  ```tsx
  <select
    value={settings.language ?? resolvedDefaultLanguage}
    onChange={(e) => void patchSettings({ language: e.target.value })}
  >
    <option value="ja">日本語</option>
    <option value="en">English</option>
  </select>
  ```

  where `resolvedDefaultLanguage` is whatever §1.2's OS-locale-detection default (below) resolves to when
  `settings.language` is `null` — the dropdown must show the *effective* language, not a blank/undefined
  selection, when the user has never explicitly chosen one.
- **OS-locale detection** (`[spec v1.2.0 §1.5]`'s "detect the OS UI locale at first read"): the simplest
  available signal in a Tauri+WebView2 app is the WebView's own `navigator.language` (WebView2 mirrors the
  OS UI language for this value unless the OS itself is configured with an override) — no new Tauri
  plugin/permission needed. `developer-lite` may instead use a Tauri-side OS-locale API
  (`@tauri-apps/plugin-os` or equivalent) if `navigator.language` proves unreliable in practice; either
  satisfies `[spec v1.2.0 §1.5]`'s "detect... fall back to `ja` for anything not `ja`" rule, which only
  needs a single yes/no check ("does the detected locale start with `ja`?"), not fine-grained locale
  parsing.
- **Where the default-resolution runs**: once, at `I18nProvider` mount, computed from
  `settings.language ?? (detectedLocale.startsWith("ja") ? "ja" : "ja")` — collapsing to a constant `"ja"`
  either way per `[spec v1.2.0 §1.5]`'s own literal rule ("otherwise... fall back to `ja` as well"), so the
  detection step is really only meaningful the day a second non-`ja` default is ever added — but the
  detection call itself should still exist now (not stubbed to always return `ja`), since a future third
  language's default-selection story depends on it already being wired.

### 1.3 String-Extraction Scope and Sequencing

- **In-scope inventory, concretely** (per `[spec v1.2.0 §1.2]`'s scope rule, "menu items, dialog
  titles/labels/buttons, toolbar tooltips/labels, toast/notification messages, status bar text, settings
  screen labels, context menu entries, error messages"): every literal Japanese string currently inline in
  every file under `src/components/`, plus `src/shared/errorMessages.ts` (§1.4 below), plus scattered
  Japanese literals inside hook files that construct user-facing strings directly (e.g. `src/state/
  useToast.ts` callers building toast copy, `src/editor/tocGenerator.ts`'s marker text is NOT in scope —
  it is inert document content, never shown as UI chrome).
- **Sequencing is `PLAN.md`-level**, not a spec-level ambiguity (`[spec v1.2.0 §1.1]`'s own framing: "the
  *scope*... is unambiguous... the size/sequencing of the refactor is an implementation-planning
  concern"). One reasonable approach worth noting for `developer-lite`'s planning, not binding: extract
  component-by-component behind the `useT()` hook, verifying each component still renders identical
  Japanese text before moving to the next (a mechanical, low-risk-per-step refactor, given the volume of
  call sites this touches across the whole `src/components/` tree) — plausible split points already exist
  along this codebase's own boundaries (`TabBar`, `TreeView`+`TreeRow`, `EditorPane`+`MarkdownToolbar`,
  `SettingsDialog`, the `dialogs/` folder, `Toast`).

### 1.4 Error-Message Translation Touch Point

Elaborates `[spec v1.2.0 §1.2]`'s "frontend-only concern" note.

- **Single touch point**: `src/shared/errorMessages.ts`'s `MESSAGES: Record<ErrorCode, string>` map and
  its `errorMessageFor(code: ErrorCode): string` export — currently a single hardcoded Japanese map (12
  entries as of inspection: `NAME_DUPLICATE`, `NAME_INVALID`, `NOT_FOUND`, `IO_LOCKED`, `TRASH_FAILED`,
  `DECRYPT_FAILED`, `CONFLICT`, `META_CORRUPT`, `LONG_PATH`, `TAB_LOCKED`, `ANOMALOUS_FILE_STATE`,
  `MEMO_EDIT_LOCKED`, `UNKNOWN`). Becomes per-language: either (a) folded directly into each language
  file's own translation map under an `errors.<CODE>` key namespace, looked up via the same `useT()` hook
  wherever `errorMessageFor` is currently called, or (b) kept as its own small `errorMessageFor(code,
  lang)` function reading from a per-language `MESSAGES` map, mirroring its current shape most closely.
  Either satisfies `[spec v1.2.0 §1.2]`'s "add the code→message mapping to each language file" — pure
  implementation-detail choice, not spec-level.
- **No backend/Rust change**, confirmed by re-reading `[spec §3.4]`'s `Result<T>` shape and `[spec §5.10]`
  ("send `code`/`message`/`path?` only externally... details logged internally only") — the Rust side
  never sends a language-dependent string to begin with; `message` in the wire payload is already
  English-oriented internal-diagnostic text per `errorMessages.ts`'s own file-header comment ("that string
  is written for internal diagnostic logging in English"), never displayed to the user as-is. This
  confirms `[spec v1.2.0 §1.2]`'s own claim with a concrete code citation.

---

## 2. Math/TeX Rendering — Elaboration

Elaborates `[spec v1.2.0 §2]` in full. Confirmed by inspection: `package.json` has no `katex`/`mathjax`
dependency and no math-adjacent plugin for `markdown-it`/`@lezer/markdown` today — greenfield, as
`[spec v1.2.0 §2.1]` itself already states.

### 2.1 Preview Pipeline Touch Point and Plugin Choice

- **Renderer construction site**: `src/editor/preview/markdownIt.ts` — the module that configures the
  shared `markdown-it` instance (`html: false`, GFM/task-lists plugin registration per
  `[MDEditor-spec §4]`). A KaTeX-rendering step is added here, either via a `markdown-it`-ecosystem math
  plugin (e.g. `markdown-it-texmath` or `@vscode/markdown-it-katex`-style, configured with the decided
  `$...$`/`$$...$$` delimiters, `[spec v1.2.0 §2.2]`) or a small custom inline/block rule following the
  same registration pattern the task-lists plugin already uses in that file — either is
  implementation-discretion (`[spec v1.2.0 §2.2]`'s own framing), since the observable requirement is only
  "delimiter syntax in, typeset HTML out, through the same pipeline."
- **Sanitizer ordering unchanged**: whichever plugin is chosen, its output HTML still flows through
  `src/editor/preview/previewRenderer.ts`'s existing DOMPurify pass (`[MDEditor-spec §4]`'s "defense-in-
  depth backstop, not the primary control") — KaTeX's own generated markup (nested `<span
  class="katex-...">` structure) must be checked against DOMPurify's current allowlist configuration.
  Confirmed by direct inspection (`previewRenderer.ts` lines ~186–195): this **is** a strict, explicit
  custom allowlist (`DOMPurify.sanitize(html, { ALLOWED_TAGS: [...], ALLOWED_ATTR: [...] })`), not
  DOMPurify's own broad default — and, concretely, `"span"` is already in `ALLOWED_TAGS` and both
  `"style"`/`"class"` are already in `ALLOWED_ATTR` today (added for the existing `<span style>`
  color/font-size allowlist construct and preview-internal classes like `ttn-preview-table-wrap`). Since
  KaTeX's default HTML-only rendering output is almost entirely `<span class="katex-..." style="...">`
  markup, **little or no new allowlist entry may actually be needed** for that output mode. The one
  concrete addition likely required: if the chosen KaTeX configuration keeps its default `output:
  "htmlAndMathml"` (or `"mathml"`) mode rather than pinning `output: "html"`, KaTeX also emits a parallel
  `<math>`/`<semantics>`/`<mrow>`/`<mo>`/etc. MathML tree, none of which are in today's `ALLOWED_TAGS` —
  `developer-lite` should either extend `ALLOWED_TAGS` with the specific MathML tag names KaTeX emits, or
  simply set `output: "html"` in the KaTeX renderer call to skip generating that parallel tree at all
  (avoiding the extra sanitizer surface area and the extra bytes, at the cost of the MathML tree's
  accessibility/copy-paste benefits) — either is a mechanical, implementation-level choice, not a new
  sanitization *policy* decision (the two existing allowlisted constructs, `<span style>` and `<img>`, are
  pre-extracted before DOMPurify runs per `[MDEditor-spec §4]`; KaTeX's own output is different in kind —
  it is the renderer's OWN generated markup, analogous to how markdown-it's own `<h1>`/`<table>`/etc.
  output is already trusted, not user-authored HTML being allowlisted through).
- **Where math nodes attach `data-source-line`**: KaTeX's own block/inline nodes must still carry a
  `data-source-line` attribute (or be wrapped in a parent element that does) consistent with `[MDEditor-
  spec §4]`'s scroll-sync mechanism (`token.map[0]`-derived attribute on every top-level rendered block) —
  whichever math plugin is chosen must expose a `token.map` on its generated tokens (most
  `markdown-it`-ecosystem block plugins do, following the same convention as every other block rule) so
  this falls out for free from the existing scroll-sync code rather than needing new plugin-specific
  wiring.

### 2.2 False-Positive Heuristic (Implementation-Discretion Call Made In This Pass)

`[spec v1.2.0 §2.2]` explicitly delegates the currency-`$`-false-positive heuristic to this pass ("no
special escaping rule beyond whatever false-positive-avoidance heuristic the implementation applies...
is specified here"). Decision made here:

- **Adopt the standard, widely-used heuristic already built into the common `markdown-it` math plugins**
  (`markdown-it-texmath`, the Pandoc/Typora convention this app's own delimiter choice already mirrors,
  `[spec v1.2.0 §2.2]`): an inline `$...$` span only matches when there is **no whitespace immediately
  inside either delimiter** (`$5` alone never opens a valid span since there's no closing `$`; a run like
  "It costs $5, but not $10 total" is not treated as inline math even though it contains two unescaped
  `$` characters, because the character immediately before the second `$` is a space — the standard
  heuristic requires the character immediately preceding a closing `$` to be non-whitespace). This is not
  a new design — it's the same heuristic essentially every implementation of this Pandoc-style convention
  already uses, which is precisely why it is a safe default to just adopt rather than invent: it is the
  behavior a `markdown-it-texmath`-class plugin already gives "for free" if that's the plugin chosen in
  §2.1, and is simple enough to replicate in a hand-rolled rule if a different plugin/custom
  implementation is chosen instead.

### 2.3 Live-View Marker (Implementation-Discretion Call Made In This Pass)

`[spec v1.2.0 §2.3]` delegates the exact glyph ("exact glyph implementation discretion"). Decision made
here: **reuse the `Sigma` icon from `lucide-react`** (already a direct dependency, `package.json`,
`^1.28.0` — the exact icon library `detailed-spec.md` §5.10 already documents as this app's confirmed,
bundled-locally toolbar icon set: "Icon set: Lucide... bundled locally into the app — no CDN/network
dependency"). Reasoning: reusing an icon already inside the existing dependency, already bundled,
already matching this app's established icon-set choice, is strictly simpler than introducing any new
glyph source (a Unicode character, a custom SVG, or a second icon package) purely for this one marker,
and keeps the math marker visually consistent with every other decoration-widget icon in the app (e.g.
`ImagePlaceholderWidget`'s own icon-adjacent styling).

- **Concrete widget shape**, following `ImagePlaceholderWidget`'s own precedent in
  `src/editor/markdownDecorations.ts` (a `WidgetType` subclass rendering a small inert `<span>`): a new
  `MathPlaceholderWidget extends WidgetType` rendering `<span class="cm-md-math-placeholder"><Sigma
  size={12}/> formula</span>`-equivalent (React icon components can't be used directly inside a
  CodeMirror-owned DOM node built by `toDOM()`, so this must render the icon's raw SVG markup or use
  `lucide`'s vanilla-JS/SVG-string export rather than the `lucide-react` component wrapper — the same
  constraint any CodeMirror `WidgetType.toDOM()` already has for any icon, not new to this feature).
- **Non-interactive** (`[spec v1.2.0 §2.3]`): unlike `ImageWidget`'s click-to-cursor-position handling
  (`markdownDecorations.ts`'s `MarkdownDecorationsPlugin`'s `mousedown` handler), this widget registers no
  click handler at all — a plain `Decoration.replace({ widget: new MathPlaceholderWidget(...) })` range,
  wired into the same `liveViewCompartment`-governed `MarkdownDecorationsPlugin.build()` pass that already
  builds `ImageWidget`/other live-view decorations, scanning for `$...$`/`$$...$$` spans using the same
  code-fence/inline-code exclusion already applied to `findImageTags` (`SourceImageMarkerPlugin.build()`'s
  own `codeSkipRanges` pattern, §2.4 below — reused for both the live-view and source-view scans so a
  literal `$...$`-looking string inside a fenced code block is never marked in either mode).

### 2.4 Source-View Marker — Concrete Reuse of `SourceImageMarkerPlugin`

Elaborates `[spec v1.2.0 §2.4]`, which itself already correctly identifies the precedent
(`SourceImageMarkerPlugin`, `sourceImageMarkerCompartment`, GitHub issue #81). Confirmed by direct
inspection of `src/editor/markdownDecorations.ts` (lines ~527–605) and `src/editor/codeMirrorSetup.ts`:

- **Exact mirrored shape**: a new `SOURCE_MATH_MARKER_MARK = Decoration.mark({ class:
  "cm-md-source-math-marker" })` constant (mirroring `SOURCE_IMAGE_MARKER_MARK`'s own declaration one-for-
  one), and either (a) a new case branch inside the *existing* `SourceImageMarkerPlugin.build()`'s
  `syntaxTree(state).iterate` switch (renaming the class/exports to something less image-specific if
  extended this way — a naming/scope decision left to `developer-lite`), or (b) a sibling
  `SourceMathMarkerPlugin` class following the identical `PluginValue` shape (`decorations: DecorationSet`,
  a `build(state)` method gathering `Range<Decoration>[]` from a syntax-tree walk plus a regex scan,
  `docChanged`-gated rebuild), wired through its own new `sourceMathMarkerCompartment` in
  `codeMirrorSetup.ts` (mirroring `sourceImageMarkerCompartment`'s own declaration and its
  `.of(sourceImageMarkerViewExtension(!liveViewEnabled))` wiring one-for-one) — `[spec v1.2.0 §2.4]` leaves
  this (a) vs (b) choice to `developer-lite` explicitly.
- **CSS**: `.cm-md-source-math-marker { background: var(--ttn-bg-math-marker); border-radius: 3px; }`
  plus a `::before { content: "..." }` icon (reusing §2.3's glyph choice, per `[spec v1.2.0 §2.4]`'s own
  "reusing whichever glyph §2.3's live-view badge uses" instruction) — added to `src/components/Editor/
  Editor.css` immediately after the existing `.cm-md-source-image-marker` block (lines ~314–319 of that
  file), and a new `--ttn-bg-math-marker` CSS custom property added to `src/theme.css` alongside the
  existing three `--ttn-bg-image-marker` definitions (one per theme variant — confirmed by inspection,
  `src/theme.css` currently defines `--ttn-bg-image-marker` three times, lines 57/99/135, once per theme
  context) — a **new, distinct** color value per `[spec v1.2.0 §2.4]`'s explicit "distinct color... per
  direct user request," not a reuse of the image marker's own RGBA value.
- **Detection scan target**: `$...$`/`$$...$$` spans are not lezer syntax-tree nodes (unlike `Image`,
  which `@lezer/markdown`'s parser already produces as a first-class node type) — so, unlike the image
  marker's `syntaxTree(state).iterate` branch, math-span detection is necessarily a **regex scan** over
  the raw document text (mirroring `findImageTags`'s own regex-based approach for raw `<img>` tags, not
  the `Image`-lezer-node branch), applying the same `codeSkipRanges` exclusion built from `InlineCode`/
  `FencedCode`/`CodeBlock` node ranges that `SourceImageMarkerPlugin.build()` already computes — this is
  the one piece of genuinely new scan logic this feature needs (the delimiter-boundary heuristic from
  §2.2, reused identically here for the source-view scan so both marker locations and the live-view
  widget locations agree exactly on what counts as "a math span").

### 2.5 Preview Rendering Pipeline Tie-In (Debounce/Scroll-Sync Reuse, Wide-Formula Wrapping, Asset Bundling)

Elaborates `[spec v1.2.0 §2.5]` in full — covers all three of that spec section's points (debounce/
scroll-sync reuse, wide-formula wrapping, asset bundling), not asset bundling alone.

- **No debounce/scroll-sync/image-pipeline change, restated concretely**: math rendering is registered as
  an ordinary `markdown-it` rule/plugin inside the same `createPreviewMarkdownIt()` instance §2.1 already
  targets (`src/editor/preview/markdownIt.ts`) and produces its HTML inline within the same single
  `md.render()` call `previewRenderer.ts` already makes on every debounced re-render — there is no second
  render pass, no second debounce timer, and no new image-loading path introduced anywhere by this
  feature; §2.1's own `data-source-line` bullet already covers why scroll-sync in particular needs no new
  code (math's `data-source-line` attribute falls out of the same `token.map[0]` convention every other
  block already uses).
- **Wide/overflowing formulas — horizontal-scroll wrapping, concrete reuse**: `[spec v1.2.0 §2.5]` calls
  for reusing the existing wide-table horizontal-scroll-container treatment. Confirmed by direct
  inspection, that treatment is `src/editor/preview/markdownIt.ts`'s `wrapTableOpen`/`wrapTableClose`
  renderer-rule overrides (`md.renderer.rules.table_open = wrapTableOpen; md.renderer.rules.table_close =
  wrapTableClose;`), which wrap the rendered `<table>` in `<div class="ttn-preview-table-wrap">...</div>`
  — a plain CSS `overflow-x: auto` container, per `[MDEditor-spec §4]`'s "wrap in a horizontal-scroll
  container... never auto-shrink font size to fit." For KaTeX block math (`$$...$$`, rendered as a
  block-level `.katex-display` element), the identical technique applies: whichever plugin is chosen in
  §2.1, its block-math renderer rule (or a post-processing wrap step, if the plugin doesn't expose a
  rule-override point the way markdown-it's own `table_open`/`table_close` do) should wrap the rendered
  output in an analogous container — e.g. `<div class="ttn-preview-math-wrap">...</div>` with the same
  `overflow-x: auto` CSS rule `.ttn-preview-table-wrap` already declares (confirmed by inspection:
  `src/components/Editor/Editor.css` line ~118, `.ttn-preview-table-wrap { overflow-x: auto; }` —
  reusing that exact declaration (e.g. a shared `.ttn-preview-overflow-x-wrap`-style class both wrappers
  apply, or simply adding `.ttn-preview-math-wrap` as a second selector on the same CSS rule) rather than
  writing a second, duplicate horizontal-scroll CSS rule. `"class"` is already in `previewRenderer.ts`'s
  DOMPurify `ALLOWED_ATTR` list (§2.1 above), so no sanitizer-allowlist change is needed for this new
  wrapper class. **Inline math (`$...$`) is not wrapped this way** — only block math, mirroring how the
  wide-table treatment itself only applies to block-level `<table>` elements, never inline content; an
  overlong inline formula simply reflows/wraps or overflows its containing paragraph like any other long
  inline content (no spec text calls for special handling of that narrower case, and none is invented
  here).
- **Asset bundling (must-bundle, not CDN-loaded)**: this is a Tauri desktop app, expected to work fully
  offline. KaTeX ships its own CSS (`katex.min.css`) and font files (`woff2`/`woff`/`ttf` glyph sets) as
  static assets — these must be imported/copied into the Vite build output (e.g. a static `import
  "katex/dist/katex.min.css"` inside `src/editor/preview/markdownIt.ts` or `PreviewPane.tsx`, which Vite
  already bundles/inlines for any other CSS import in this codebase — confirmed by inspection,
  `EditorPane.tsx` already does exactly this for `import "./Editor.css";`, so no new build-config work is
  implied) rather than referenced via a `<link href="https://cdn...">` tag anywhere — confirmed there is
  no existing CDN-link pattern anywhere in `index.html`/`src/main.tsx` to accidentally follow, so this is a
  "don't introduce one," not a "remove an existing one," instruction. Otherwise formula rendering silently
  breaks (missing glyphs/unstyled output) whenever the app is used without an internet connection.

---

## 3. Pane-Mode / View-Mode Toolbar Button Semantics — Elaboration

Elaborates `[spec v1.2.0 §3]` in full — a pure UI-text/label-computation change to two already-shipped
buttons in `src/components/Editor/Toolbar/MarkdownToolbar.tsx`, confirmed by direct inspection (lines
~59–65, ~599–610):

- **`PANE_MODE_LABEL` stays as the raw `Record<PaneMode, string>` lookup table it already is** — no
  change to its 3 entries (`edit`/`split`/`preview` → "編集のみ"/"分割表示"/"プレビューのみ"). What
  changes is **which key is used to look it up**: today, `PANE_MODE_LABEL[paneMode]` (current state);
  after this change, `PANE_MODE_LABEL[nextPaneMode]` where `nextPaneMode` is the state clicking would
  advance to. `nextPaneMode` is already trivially computable — `docs/plans/markdown/markdown-preview/
  PLAN.md` §4.1's existing `PANE_MODE_CYCLE` map (cited directly by `[spec v1.2.0 §3]` itself) already
  gives `PANE_MODE_CYCLE[paneMode]` as the next state; this map already exists for the click-handler's own
  `onCyclePaneMode` advancement logic, so the label computation is a **new read of an existing value**,
  not new logic.
- **Concrete edit, pane-mode button** (`MarkdownToolbar.tsx` line ~609): `{PANE_MODE_LABEL[paneMode]}` →
  `{PANE_MODE_LABEL[PANE_MODE_CYCLE[paneMode]]}`. The `aria-label` (line ~607) currently reads
  `` `ペイン表示モードを切り替え (現在: ${PANE_MODE_LABEL[paneMode]})` `` — per `[spec v1.2.0 §3]`'s
  explicit instruction ("removed, not reworded to additionally describe the next state"), this becomes
  simply a next-state-only phrasing, e.g. `` `${PANE_MODE_LABEL[PANE_MODE_CYCLE[paneMode]]}に切替` ``
  (exact copy is a minor wording call, not spec-mandated beyond "next-state-only, no current-state
  suffix").
- **Concrete edit, live/source toggle button** (`MarkdownToolbar.tsx` line ~600): `{liveViewEnabled ?
  "ライブ表示" : "ソース表示"}` — today this literally names the *current* mode. Per `[spec v1.2.0 §3]`'s
  next-state rule, this **inverts**: `{liveViewEnabled ? "ソース表示" : "ライブ表示"}` (when currently in
  live view, clicking switches TO source view, so the label should read "ソース表示"; when currently in
  source view, clicking switches TO live view, so the label should read "ライブ表示") — the simplest
  possible fix for the 2-state case, exactly matching `[spec v1.2.0 §3]`'s own "for the 2-state toggle,
  'next state' is simply 'the other state' — trivial to compute" framing. This button currently has no
  `aria-label`/`title` at all (confirmed by inspection — only the pane-mode button and the
  preview-lock/PDF/minimize buttons have one); `[spec v1.2.0 §3]` does not require adding one, only
  changing what's already there for the pane-mode button's existing `aria-label`.
- **No other touch points**: `previewLocked` (GitHub issue #149, `disabled={previewLocked}` on the
  pane-mode button) is unaffected — `[spec v1.2.0 §3]` changes only which label text is shown, not the
  button's enabled/disabled logic, which stays governed by `previewLocked` exactly as it is today.
- **No new state, no new props**: both buttons already receive `paneMode`/`liveViewEnabled` as props;
  this is a pure render-expression change at the two cited lines, no new prop threading through
  `EditorPane.tsx`.

---

## 4. Cross-Tab Heading Anchor Links, `memo:TabName/MemoName#slug` — Elaboration

Elaborates `[spec v1.2.0 §4]` in full.

### 4.1 Target-String Parsing (No Backend Change Needed)

- **Detection needs no change at all**: `src/editor/links.ts`'s `BARE_MEMO_RE = /\bmemo:[^\s<>()[\]]+/g`
  already matches a full `memo:TabName/MemoName#slug` token as a single unit, since `#` is not in the
  regex's excluded-character set — confirmed by inspection. The same is true for the `[text](memo:...)`
  markdown-link-syntax case, since the parenthetical URL content is captured whole regardless of an
  embedded `#`. So neither live-view's bare-token scan nor the parenthetical-link case needs any change to
  recognize the extended syntax — only **resolution** (below) needs to change.
- **Resolution split happens client-side, not in the Rust backend**: `src/ipc/link.ts`'s `linkResolve
  (currentTabId, target)` invokes the Rust `link_resolve` command, which matches `target` against real
  tab/memo names — passing it an un-split `"TabName/MemoName#slug"` string would make it try (and fail) to
  match a memo literally named `"MemoName#slug"`. The `#slug` suffix must therefore be split off **before**
  calling `linkResolve`, inside `src/editor/links.ts`'s `resolveAndOpenMemoLink` (the single function both
  live-view and preview-pane click handling already funnel through — confirmed: `previewInteractions.ts`'s
  `handleAnchorClick`'s `href.startsWith("memo:")` branch already calls this same function, so a change
  here covers both panes' `memo:` click paths at once). Concretely:

  ```ts
  export async function resolveAndOpenMemoLink(
    currentTabId: string | null,
    rawTarget: string,
    onOpenMemo: (tabId: string, memoName: string | null, anchorSlug: string | null) => void, // signature grows one param
  ): Promise<void> {
    if (!currentTabId) return;
    const stripped = rawTarget.startsWith("memo:") ? rawTarget.slice("memo:".length) : rawTarget;
    const hashIndex = stripped.indexOf("#");
    const target = hashIndex === -1 ? stripped : stripped.slice(0, hashIndex);
    const anchorSlug = hashIndex === -1 ? null : stripped.slice(hashIndex + 1);
    const result = await linkResolve(currentTabId, target);
    if (!result.ok || result.value.kind !== "memo") return;
    onOpenMemo(result.value.tabId, result.value.memoName, anchorSlug);
  }
  ```

  This means **no Rust/`src-tauri/` change at all** for this feature — `link_resolve` keeps matching
  exactly the memo-name portion it already does today, unaware `#slug` ever existed. Reasoning for doing
  the split client-side rather than teaching the backend to tolerate/strip a `#` suffix: the whole
  anchor-resolution mechanism (heading scan, slugify) already lives entirely in the frontend
  (`src/editor/tocGenerator.ts`'s `scanHeadings`/`slugifyHeading`, per `[spec v1.2.0 §4.3]`'s own "no new
  heading-scan or slugify implementation is introduced" instruction) — there is no reason for the backend
  to ever see or care about the anchor half of the target string.
- **`memo:TabName/#slug` (tab-only form with a stray anchor)**: per `[spec v1.2.0 §4.2]`'s own rule ("not
  applicable... treated as any other unresolved/malformed link"), the split logic above still runs (an
  anchor is extracted whenever a `#` is present, regardless of the memo-name portion), but since `target`
  here is `"TabName/"` (trailing slash, empty memo-name portion) — `link_resolve`'s existing behavior for
  that shape (`[detailed-spec §5.8]`: "`memo:TabName/`... opens the target tab without selecting a
  specific memo") already returns a `memoName: null` result. §4.2's step-2 anchor resolution (below) is
  gated on a specific memo actually being opened, so a `null` `memoName` naturally means step 2 never
  runs — no extra special-casing needed; the existing `memoName === null` path already produces the
  correct "anchor never applied" outcome as a side effect of the existing type shape, not new logic.

### 4.2 Cross-Tab Navigation State Machine — New `pendingLinkFocus.anchorSlug`

Elaborates `[spec v1.2.0 §4.3]`'s step-1/step-2 ordering guarantee, grounded against the actual
cross-tab-navigation architecture found in `src/App.tsx`/`src/state/useTabView.ts`.

- **Existing architecture, confirmed by inspection**: this app already has exactly the "step 1 completes,
  then step 2 runs against the now-open memo" ordering machinery `[spec v1.2.0 §4.3]` calls for — three
  independent `pending<Source>Focus` state machines already exist side-by-side in `App.tsx`
  (`pendingQuickOpenFocus`, `pendingLinkFocus`, plus `useTabView.ts`'s own `requestRestore`/
  `pendingTabIdRef` for startup/tab-switch restore), each following the identical shape: set a pending
  `{tabId, memoName}` record when a same-tick tab switch is needed, then a `useEffect` watching
  `treeState.activeTabId`/`treeState.flatOrder` fires `treeState.focusOnMemo(memoName)` once the target
  tab's tree has actually finished loading and contains that memo — consumed exactly once, dropped
  silently if the memo is never found (mirrors `[spec v1.2.0 §4.4]`'s own "silent no-op" rule for a memo
  that no longer exists, already true today for reasons unrelated to this feature).
- **Extension**: `pendingLinkFocus`'s type grows one field: `{ tabId: string; memoName: string; anchorSlug:
  string | null }`. `openLinkTarget` (`App.tsx`, the function passed as `EditorPane`'s `onOpenMemoLink`
  prop and `previewInteractions.ts`'s `config.onOpenMemoLink`) receives the extra `anchorSlug` parameter
  from §4.1's updated `resolveAndOpenMemoLink` signature and threads it into the `setPendingLinkFocus` call
  when a tab switch is needed. **Same-tab case** (`tabId === treeState.activeTabId`, no tab switch needed —
  the existing early-return branch in `openLinkTarget`): the anchor cannot simply be applied "now" either,
  since `treeState.focusOnMemo(memoName)` itself is what triggers `EditorPane.tsx`'s memo-load effect
  asynchronously (IPC `editor_load` round-trip) — so **both** branches of `openLinkTarget` need the same
  deferred-anchor treatment; the existing `if (tabId === treeState.activeTabId) { ...; return; }` early
  return must also route through the same pending-anchor mechanism (a same-tab-different-memo `memo:...#slug`
  link is not a same-tick no-op today either, since opening a *different* memo within the same tab is
  already async).
- **Where the anchor is actually consumed**: NOT inside `App.tsx`'s `pendingLinkFocus`-consuming
  `useEffect` (that effect only has `treeState` — tree/metadata state, no access to the mounted
  `EditorView`/preview-DOM the anchor jump needs). Instead, `pendingLinkFocus`'s effect, once it calls
  `treeState.focusOnMemo(memoName)`, additionally stashes `anchorSlug` into a new ref threaded down into
  `EditorPane.tsx` as a prop (e.g. `pendingAnchorSlug: string | null`, alongside the existing
  `focusEditorToken`/`onOpenMemoLink` props) — consumed inside `EditorPane.tsx`'s own view-(re)creation
  effect, at the exact point (line ~501–525, right after `viewRef.current = view;` and the existing
  `focusEditorTokenRef`-consumption block) where the newly-loaded memo's `EditorView` has just been
  created and is guaranteed to belong to the correct memo — mirroring the **exact same** "one-shot signal,
  consumed via a ref comparison, at the moment a fresh view exists for the just-navigated-to memo"
  pattern `focusEditorToken`/`consumedFocusTokenRef` already establishes there for auto-focus. A sibling
  `consumedAnchorSlugRef` (or reusing a single generation-token approach covering both auto-focus and
  anchor-jump in one signal) avoids re-applying a stale anchor jump on a later, unrelated re-render.

### 4.3 Anchor-Jump Execution, Pane-Mode-Dependent

Elaborates `[spec v1.2.0 §4.3]`'s "resolved... using the exact same slugify/scan logic" instruction, split
by the current `paneMode` of the memo that was just opened — a distinction `[spec v1.2.0 §4.3]` does not
itself spell out mechanically, but which follows necessarily from `[MDEditor-spec §2]`'s "editable-pane
show/hide is CSS-only" design (`display:none`, not unmounted) once the two candidate execution paths are
compared against actual scroll-sync wiring:

- **`edit`/`split` pane mode** (edit sub-pane visible, `display` not `none`): reuse
  `src/editor/links.ts`'s existing `resolveAndScrollToHeadingAnchor(view, slug)` **unchanged** — the exact
  same function the same-memo case already calls (`links.ts` line ~104, dispatching `{selection: {anchor:
  match.from}, effects: EditorView.scrollIntoView(...)}`). This works correctly for `split` mode too
  without any preview-specific code, because `PreviewPane.tsx`'s own scroll-sync listener (`view.scrollDOM
  .addEventListener("scroll", ...)`, confirmed by inspection lines ~171–189) fires from the real DOM
  `scroll` event `EditorView.scrollIntoView`'s dispatch produces, driving `scrollPreviewToLine` on the
  preview side automatically — i.e. `split` mode's preview half is a **side effect** of the existing
  editor-side jump, not a separate code path.
- **`preview` pane mode** (edit sub-pane `display:none`, 0×0): the `edit`/`split` path above is
  **unreliable here** and must not be relied on — dispatching a `scrollIntoView` effect against a
  CodeMirror view whose `scrollDOM` has zero rendered height (per `[MDEditor-spec §2]`'s CSS-only-hide
  design) cannot compute a meaningful scroll offset, which is exactly the same underlying mechanism `[spec
  v1.2.0 §8]`'s bug report already establishes independently for the reverse (save) direction ("the edit
  sub-pane is `display:none`... an action... never touches the editor's `scrollDOM`"). Instead, when the
  just-opened memo's resolved `paneMode` (`MemoEditorSettings.paneMode ?? AppSettings.editorDefaults.
  paneMode`) is `"preview"`, the anchor jump must act **directly on the preview container**, reusing §9's
  now-fixed `handleAnchorClick` `#`-branch logic (`previewInteractions.ts`) verbatim — i.e. `container.
  querySelector('#' + CSS.escape(slug))?.scrollIntoView({block: "start"})` — but triggered programmatically
  from §4.2's ref-consumption point instead of from a real click event. **No `decodeURIComponent` needed
  on this programmatic path** (unlike §9.1/§9.2's own click-handler fixes): the slug here comes from
  `resolveAndOpenMemoLink`'s own string-splitting (§4.1), operating on a `rawTarget` that is *already*
  clean of `normalizeLink` encoding by the time it reaches that function — regardless of which of the two
  possible trigger paths produced it. If the original click was on a **preview-pane** `<a href="memo:...">`
  (the click-handler entry point §9.2 fixes), that `href` genuinely did pass through markdown-it's
  `normalizeLink` when rendered, but §9.2's own `decodeURIComponent(href)` call already reverses that
  encoding *before* `resolveAndOpenMemoLink` is ever invoked (§9.2's own "decode point stays upstream"
  reasoning). If the original click was on a **live-view** bare `memo:...` token or `[text](memo:...)`
  link instead, `rawTarget` was read straight from the raw CodeMirror source buffer and was never encoded
  in the first place (§9.1's "Live view is unaffected" finding, restated for the `memo:` case by §9.2).
  Either way, `resolveAndOpenMemoLink`'s own splitting logic — and therefore this section's programmatic
  anchor-jump slug — never needs its own decode step; §9.1/§9.2 together are what make that always true,
  not merely one specific branch of them.
- **Required wait condition (new finding, not stated in `spec.v1.2.0.md`)**: the preview-mode path above
  can only succeed once the preview container's rendered `<h1>`–`<h6>` `id` attributes actually belong to
  the just-opened memo — `PreviewPane.tsx`'s `html` state does **not** update synchronously on a memo
  switch; it is recomputed after `AppSettings.preview.debounceMs` (default 150ms, `[MDEditor-spec §3]`)
  from the doc-change event the new `EditorView`'s creation itself triggers, and until that timer fires,
  `html` still holds the **previous** memo's rendered markup (confirmed by inspection: `PreviewPane.tsx`'s
  `useState("")` for `html` is not reset synchronously on memo switch — `PreviewPane` itself is never
  unmounted across a memo switch while `paneMode !== "edit"`, so the old HTML lingers in the DOM until the
  debounced re-render replaces it). Firing the `querySelector` lookup immediately upon consuming §4.2's
  pending anchor would therefore, in the common case, either match nothing or match a stale heading from
  the memo just switched away from. **This same wait condition also applies to §8's restore path** (§8.3)
  — see §10.1 for the shared primitive proposed to solve both at once, rather than each implementing its
  own ad hoc polling/timeout.

### 4.4 Adjacent `memo:`-Branch Encoding Bug — Bundled Into §9's Fix (resolved; see `DECISIONS.md`)

`memo:TabName/MemoName` hrefs with **non-ASCII tab/memo names** are *also* percent-encoded by markdown-
it's `normalizeLink` when rendered in the preview pane — the identical mechanism `[spec v1.2.0 §9]` fixes
for bare `#slug` hrefs, but on the **`memo:`-prefixed branch**. This is a pre-existing bug in today's
baseline `memo:` cross-tab links (`[spec §2.8]`), not introduced by this feature, but it directly
undermines §4 for any Japanese-named tab/memo when clicked from the preview or split pane — the
memo-target half of a `memo:TabName/MemoName#slug` link fails to resolve in that case, regardless of
whether the anchor half is even present.

Per the user's decision (rationale and rejected "track separately" alternative in `DECISIONS.md`,
"Preview-Pane `memo:` Link Non-ASCII Name Encoding Bug: Bundled into v1.2.0's §9 Fix, Not Deferred"), the
fix is bundled into this same v1.2.0 pass, using the identical pattern already decided for the `#slug`
branch. `spec.v1.2.0.md` §9 records this as a decided, in-scope bundled follow-up; the concrete fix is
§9.2 below (same shape as §9.1's `#slug`-branch fix, sibling branch of the identical function).

### 4.5 Tab Lock and Unresolved-Cases Behavior (Elaborates `[spec v1.2.0 §4.4]`)

Note: this document's own §4.4 (above) covers a different topic (the bundled `memo:`-branch encoding
bug) than `spec.v1.2.0.md`'s §4.4 ("Tab Lock and Unresolved Cases") — the section numbering below tracks
this document's own order, not a 1:1 mapping to `spec.v1.2.0.md`'s subsection numbers.

All three of `[spec v1.2.0 §4.4]`'s requirements turn out to be satisfied automatically by the exact
mechanisms §4.1 and §4.3 already specify — confirmed by direct inspection, not merely asserted:

- **Locked target tab (no active session) → entire link (memo **and** anchor) unresolved**: confirmed by
  reading `src-tauri/src/commands/link.rs`'s `find_tab_id_by_display_name` — for a tab with
  `entry.locked.unwrap_or(false) == true` and no active session (`sessions.with_session(&entry.id, ...)`
  returns `None`), the function returns `None`, causing `link_resolve` to return `ResolvedLink::Unresolved`
  before ever reaching the memo-tree/heading logic. Since §4.1's `resolveAndOpenMemoLink` (this document's
  own code snippet) checks `if (!result.ok || result.value.kind !== "memo") return;` immediately after
  calling `linkResolve`, a locked-no-session target already short-circuits to a full no-op **before**
  `anchorSlug` is ever read or `onOpenMemo` is ever called — i.e. "the entire link, including any `#slug`
  suffix, resolves as unresolved" is true by construction, with **zero new code required** for this case.
  This was already true for the memo-only baseline case before this feature; the `#slug` extension changes
  nothing about it, exactly as `[spec v1.2.0 §4.4]` requires.
- **Target tab locked with an active unlock session → resolves normally, memo and anchor alike**: same
  `find_tab_id_by_display_name` function, confirmed by inspection — for a locked tab **with** an active
  session, it matches against `sessions.with_session(&entry.id, |s| s.real_tab_name.clone())` instead of
  the placeholder `entry.name`, so `link_resolve` resolves normally (returns `ResolvedLink::Memo {...}`
  exactly as for an unlocked tab). §4.1's `resolveAndOpenMemoLink` and §4.3's pane-mode-dependent
  anchor-jump logic have no lock-awareness of their own at all — they operate purely on the `ResolvedLink`
  result and the (now-open, real) memo's content, so an active-session target flows through both steps
  identically to an unlocked tab. Again, **zero new code required** — this is a direct, unmodified
  consequence of `link_resolve`'s existing session-aware matching, which this feature's frontend-only
  changes (§4.1) never touch.
- **Memo resolves but no heading matches `slug` → silent no-op for the anchor only, memo switch still
  happens**: confirmed by inspection of both anchor-jump code paths §4.3 already specifies —
  `edit`/`split` mode's `resolveAndScrollToHeadingAnchor(view, slug)` (`src/editor/links.ts` lines
  ~104–112) does `const match = scanHeadings(view.state).find((h) => h.slug === slug); if (!match) return;`
  — a plain no-op on no match, never touching the already-completed memo switch from step 1. `preview`
  mode's direct `container.querySelector('#' + CSS.escape(slug))?.scrollIntoView(...)` (§4.3's own code)
  uses optional chaining specifically so a `null` query result (no matching heading `id`) is *also* already
  a silent no-op with no further code needed. Both paths independently already satisfy this requirement by
  construction — the memo the target tab/memo just switched to stays open and visible either way, only the
  scroll/cursor jump is skipped.

**Net conclusion for `developer-lite`**: no additional tab-lock-checking or no-match-guarding code needs to
be written anywhere in this feature specifically for `[spec v1.2.0 §4.4]`'s three requirements — all three
already fall out of code this document's §4.1/§4.3 already call for reusing unmodified. The only
actionable item here is **test coverage**: `developer-lite` should include these three cases explicitly in
this feature's test plan (locked-no-session `memo:...#slug` link; locked-with-session `memo:...#slug`
link; a `memo:...#slug` link whose memo resolves but whose slug matches no heading) precisely because they
are easy to silently break with an unrelated future change to `link_resolve` or the anchor-jump functions,
even though no new code is needed to make them pass today.

---

## 5. Double-Click Active Tab — Jump-to-Position — Elaboration

Elaborates `[spec v1.2.0 §5]` in full. Confirmed by inspection: `src/components/TabBar/TabBar.tsx`'s tab
`<div>` (line ~337–357) currently has only `onMouseDown`/`onMouseEnter`/`onClick`/`onContextMenu` — no
`onDoubleClick` handler exists anywhere on it today, and no double-click gesture exists on any tab
anywhere in the codebase — a clean addition, confirming `[spec v1.2.0 §5]`'s own "no existing baseline
behavior to reconcile with" claim.

### 5.1 Trigger Wiring (`TabBar.tsx` → `App.tsx` → `EditorPaneHandle`)

- **New `TabBarProps` field**: `onActiveTabDoubleClick?: () => void`. Wired at the tab `<div>` (line
  ~348's `onClick={() => selectTab(tab.id)}` sibling): `onDoubleClick={() => { if (tab.id === activeTabId)
  onActiveTabDoubleClick?.(); }}` — the guard against `tab.id === activeTabId` is what implements `[spec
  v1.2.0 §5]`'s "double-clicking an inactive tab is unaffected" rule (an inactive tab's double-click still
  produces two ordinary `onClick` selections in a row via native browser event sequencing — `dblclick`
  fires *in addition to*, not instead of, the two `click` events, so no special-casing is needed to
  preserve today's plain-click-select behavior for that case).
- **`App.tsx` wiring**: reuses the already-existing `editorRef` (`useRef<EditorPaneHandle>(null)`,
  confirmed at `App.tsx` line ~55, already used for `flushSaveNow`) — `EditorPaneHandle` gains a new
  imperative method, e.g. `toggleJumpToStart: () => void`, exposed via `EditorPane.tsx`'s existing
  `useImperativeHandle(ref, () => ({ flushSaveNow: ed.flushSaveNow }))` call (line ~134), extended to `{
  flushSaveNow: ed.flushSaveNow, toggleJumpToStart }}`. `App.tsx` passes `onActiveTabDoubleClick={() =>
  editorRef.current?.toggleJumpToStart()}` down to `<TabBar>`. This mirrors the existing
  `flushSaveNow`-via-imperative-handle pattern exactly — no new cross-component plumbing style introduced.
- **No-op if no memo is open**: `toggleJumpToStart`'s implementation must early-return if `viewRef.current`
  is `null` (no memo currently open in the active tab) — trivially true already, since the gesture is
  meaningless with nothing to jump within; `[spec v1.2.0 §5]` does not need to state this explicitly since
  it follows from "there is no cursor/scroll position to jump from or to."

### 5.2 Saved-Position Shape and Position-Mapping Mechanism

Elaborates `[spec v1.2.0 §5]`'s "position-mapping... `ChangeDesc.mapPos()`" requirement — confirmed by
inspection that **no existing code in this app uses `ChangeDesc`/`mapPos()` anywhere today**; this is
new, self-contained logic, living entirely inside `EditorPane.tsx` (or a small extracted helper module,
`developer-lite`'s call) alongside the view's own lifecycle.

- **Saved shape**: a ref, e.g. `jumpToggleRef = useRef<{ anchor: number; head: number; topLinePos: number
  } | null>(null)`, reset to `null` on memo/tab switch (§5.3). `anchor`/`head` mirror `EditorSelection.
  main.{anchor,head}` (character offsets — the exact same pair `MemoEditorSettings.cursorAnchor`/
  `cursorHead` already persist for the unrelated feature GitHub issue #28 added, confirmed present in
  `src/shared/models.ts` today though undocumented in `spec.md`/`detailed-spec.md`; that field is
  persisted long-term scroll/cursor memory and is **not** reused or touched by this feature — this is a
  separate, transient, in-memory-only pair, per `[spec v1.2.0 §5]`'s own explicit distinction from
  `[spec v1.1.0 §2]`'s persisted scroll memory). `topLinePos` is stored as a **document character offset**
  (the `from` of whichever line `computeTopVisibleLine(view)` currently reports as topmost — i.e.
  `view.state.doc.line(computeTopVisibleLine(view)).from`), **not** a raw line *number* — storing a
  position (which `ChangeDesc.mapPos()` operates on directly) rather than a number avoids a second lookup
  step and is the natural unit for this specific remap mechanism, unlike `MemoEditorSettings.
  scrollPosition`'s own persisted-line-number convention (`detailed-spec.md` §3.3), which serves a
  different purpose (a cross-session anchor, not intra-session remapping) and is intentionally not reused
  here.
- **`toggleJumpToStart()` logic**:
  1. If `jumpToggleRef.current` is `null` (first double-click, or state was cleared per §5.3): capture the
     current position into `jumpToggleRef.current` (`{anchor, head}` from `view.state.selection.main`,
     `topLinePos` from `computeTopVisibleLine`/`doc.line(...).from` as above), then dispatch a jump to
     document start — `view.dispatch({ selection: { anchor: 0, head: 0 }, effects: EditorView.
     scrollIntoView(0, { y: "start" }) })` (reusing `EditorView.scrollIntoView`, the exact same effect
     primitive `resolveAndScrollToHeadingAnchor`/`scrollToTopVisibleLine` already use elsewhere in this
     codebase, not a new scroll primitive).
  2. If `jumpToggleRef.current` is non-`null` (second double-click, toggle-back): dispatch a restore —
     `view.dispatch({ selection: { anchor: saved.anchor, head: saved.head }, effects: EditorView.
     scrollIntoView(saved.topLinePos, { y: "start" }) })` — using the **already-remapped** values (see
     below), then set `jumpToggleRef.current = null` (a further double-click starts fresh, per `[spec
     v1.2.0 §5]`'s "a third double-click... jumps to document start again, capturing the then-current
     position anew").
- **Continuous remapping across edits**: a new branch inside `EditorPane.tsx`'s existing `EditorView.
  updateListener.of((update) => {...})` (already present at line ~496 for an unrelated purpose, char/line/
  cursor status-bar refresh) — extended so that whenever `update.docChanged` is true **and**
  `jumpToggleRef.current` is non-`null`, every one of the three saved values is remapped through
  `update.changes` (a `ChangeDesc`) before being written back: `jumpToggleRef.current = { anchor:
  update.changes.mapPos(jumpToggleRef.current.anchor), head: update.changes.mapPos(jumpToggleRef.current.
  head), topLinePos: update.changes.mapPos(jumpToggleRef.current.topLinePos) }`. `ChangeDesc.mapPos()`'s
  own built-in behavior (position after an insertion before it shifts forward; position inside a deleted
  range collapses to the deletion's edge, both standard CodeMirror semantics) is exactly the "adjusted for
  lines/characters inserted or removed before it" guarantee `[spec v1.2.0 §5]` itself describes — no
  custom remapping math is needed beyond calling the built-in primitive three times per doc change.
- **Extra care needed for the multi-transaction case**: `updateListener` can fire with a `ViewUpdate`
  representing more than one combined change in a single dispatch (e.g. an autocomplete-style
  multi-region edit, or this app's own multi-cursor-adjacent text-conversion commands) — `update.changes`
  already represents the *combined* `ChangeDesc` for the whole update regardless of how many individual
  changes composed it, so a single `mapPos()` call per saved value already handles this correctly; no
  per-sub-change loop is needed (a detail worth flagging since it's easy to assume incorrectly that
  multiple `mapPos()` calls would be needed).
- **Not a bug, but worth stating explicitly for `developer-lite`**: jumping to document start moves the
  real CodeMirror selection to position 0, exactly as any other cursor movement would — this is
  necessarily also visible to the pre-existing, unrelated `MemoEditorSettings.cursorAnchor`/`cursorHead`
  persistence (GitHub issue #28, §5.2's own citation above) via that feature's own debounced-scroll/
  memo-switch flush (`persistEditorViewState`/`flushOutgoingEditorViewState`, `EditorPane.tsx` lines
  ~389–424), which reads `view.state.selection.main` directly and has no awareness of `jumpToggleRef`.
  Concretely: if the user double-clicks to jump to document start and then switches tabs/memos **without**
  double-clicking a second time to toggle back, position 0 gets persisted as this memo's long-term
  `cursorAnchor`/`cursorHead` (and `scrollPosition`) for next app restart — but this is not a special case
  introduced by this feature; it is the exact same outcome as a user manually pressing Ctrl+Home and then
  leaving the memo without moving the cursor back, which already persists identically today. `[spec
  v1.2.0 §5]`'s own "transient, in-memory-only, non-persisted" framing describes the **toggle-back
  mechanism's own saved value** (`jumpToggleRef`, which indeed never touches disk) — it does not, and was
  never intended to, prevent the ordinary, pre-existing, always-on cursor-position persistence from
  observing wherever the cursor happens to be at its own normal flush points, the same as it does for
  every other means of moving the cursor. No special interaction code is needed between the two features.

### 5.3 Reset Conditions

Per `[spec v1.2.0 §5]`'s "discarded... whenever the active tab is switched away from, or the open memo
within the tab is switched" (and explicitly **not** discarded by an edit alone, per the position-mapping
mechanism above):

- **Concrete reset point**: `EditorPane.tsx`'s existing view-(re)creation `useEffect` (the one gated on
  `ed.loadGeneration`, which bumps on every memo switch — confirmed this effect already resets
  `liveViewOverride`/`paneModeOverride`/`previewLockOverride` to `null` at lines ~514–516 for the exact
  same "session-only state must not leak into a newly-opened memo" reason) — `jumpToggleRef.current =
  null;` is added as a fourth reset alongside those three, at the same point in the same effect. This
  effect already fires on both "memo switched within the same tab" and "tab switched, causing a different
  memo (or no memo) to load" — both of `[spec v1.2.0 §5]`'s discard triggers collapse to this single reset
  point, since both necessarily go through a `loadGeneration` bump (a tab switch to a tab with no open
  memo also correctly clears the ref, since the effect's early-return branch for `!ed.content` — line
  ~448 — is reached after the reset-refs branch would need to run too; `developer-lite` should confirm
  the reset happens before that early return, not after, so it isn't skipped for the "switched to an
  empty tab" case).
- **A tab switch back to the SAME tab/memo does not need special-casing**: switching away discards the
  ref (per the point above) regardless of whether the user later switches back to the very same memo —
  `[spec v1.2.0 §5]` does not carve out an exception for "switched away and immediately back," so the
  straightforward "any `loadGeneration` bump clears it" rule already produces the specified behavior with
  no extra logic.

### 5.4 Discoverability Tooltip Touch Point

Elaborates `[spec v1.2.0 §5]`'s tooltip requirement. `TabBar.tsx`'s tab `<div>` already sets `title=
{displayName}` (line ~354, the tab's only tooltip today) — becomes conditional on active state:

```tsx
title={tab.id === activeTabId ? `${displayName} (ダブルクリックで先頭へ)` : displayName}
```

No new element, no new CSS — a plain native-tooltip (`title` attribute) extension of an already-present
attribute, consistent with how every other tooltip in this app (`title="検索"`, etc., §7 below) is
implemented as a plain `title` attribute rather than a custom tooltip component.

---

## 6. Preview-Pane Zoom — Elaboration

Elaborates `[spec v1.2.0 §6]` in full.

### 6.1 State Placement and Reset Wiring

- **New state**: `const [previewZoomPercent, setPreviewZoomPercent] = useState(100);` inside
  `EditorPane.tsx`, alongside the existing `paneModeOverride`/`liveViewOverride` session-scoped `useState`
  hooks (same component, same lifecycle scope — `EditorPane.tsx` is the natural owner since it already
  owns `paneMode` itself, per `[MDEditor-spec §2]`'s own architecture).
- **Reset conditions, three independent triggers, per `[spec v1.2.0 §6]`**:
  1. Active tab switched, 2. open memo switched — both collapse to the exact same `loadGeneration`-gated
     reset point already used for §5.3's `jumpToggleRef` reset and the pre-existing `paneModeOverride`/
     `liveViewOverride`/`previewLockOverride` resets — `setPreviewZoomPercent(100)` added as a fifth reset
     at that same point (all five are the identical "session-only state, cleared on fresh memo load"
     category).
  3. `paneMode` leaves `"preview"` — this trigger is **not** the memo-switch reset point (`paneMode` can
     change without any memo switch, via the pane-mode-cycle button/shortcut) — needs its own small
     `useEffect`: `useEffect(() => { if (paneMode !== "preview") setPreviewZoomPercent(100); }, [paneMode]);`
     — a plain watch-and-reset effect, the same shape already used elsewhere in this file for
     `paneMode`-driven side effects (e.g. `MarkdownToolbar.tsx`'s own auto-minimize-on-`"preview"` effect,
     `[MDEditor-spec §7]`).
- **Passed down to `PreviewPane`/`MarkdownToolbar`**: `previewZoomPercent` (read) and a setter/stepper
  callback are threaded as new props to both — `PreviewPane.tsx` to apply the zoom CSS (§6.2), and
  `MarkdownToolbar.tsx` to render the ± buttons and drive the value (§6.3) — mirroring how `paneMode`/
  `onCyclePaneMode` are already threaded to both components today.

### 6.2 CSS Mechanism (Implementation-Discretion Call Made In This Pass)

`[spec v1.2.0 §6]` specifies the *range/steps/scope* (70–180%, 10% steps, `paneMode === "preview"` only)
but not the CSS mechanism. Decision made here: **use the CSS `zoom` property** (e.g. `style={{ zoom:
`${previewZoomPercent}%` }}` on `PreviewPane.tsx`'s `.ttn-editor-preview-pane-content` element, §6.1's
prop threaded in) rather than `transform: scale(...)` — `zoom` triggers real layout reflow at the new
size, matching `[spec v1.2.0 §6]`'s "affects... text size, images, tables" description of a genuine
content-size change; see `DECISIONS.md`, "Preview-Pane Zoom CSS Mechanism: `zoom` Property, Not
`transform: scale()`," for the full comparative rationale.
- **Not yet visually confirmed**: whether the existing wide-table horizontal-scroll container and image
  `max-width` rule actually look right at the range's upper bound (180%) in a real running preview pane is
  a visual/subjective judgment call, not something `reviewer-lite` should adjudicate on its own — leave it
  as a manual check for the user during/after implementation.

### 6.3 Button Placement and Bounds-Disabled Rendering

- **Placement**: `MarkdownToolbar.tsx`'s header `<div className="ttn-md-toolbar-header">` (confirmed by
  inspection, lines ~590–641 — already houses the live/source toggle, pane-mode toggle, preview-lock,
  PDF-export, and minimize buttons in sequence) — the new ± / percentage-readout button group is added as
  a further sibling in that same header row, **conditionally rendered only while `paneMode === "preview"`**
  (a `developer-lite`-level rendering choice, not itself spec-mandated beyond "toolbar header area, ...
  stays visible" — conditional rendering rather than always-rendered-but-disabled is chosen here because,
  unlike the pane-mode toggle which is always meaningful, the zoom control has literally no effect and no
  current value worth displaying outside `"preview"` mode, per `[spec v1.2.0 §6]`'s own scope rule).
- **Concrete markup**, mirroring the existing preview-lock button's `disabled`/icon-swap pattern (lines
  ~615–624):

  ```tsx
  {paneMode === "preview" && (
    <div className="ttn-md-toolbar-zoom">
      <button type="button" disabled={previewZoomPercent <= 70} onClick={onZoomOut}
        title={`ズームアウト (${zoomOutCombo})`} aria-label="プレビューをズームアウト">－</button>
      <button type="button" onClick={onZoomReset} title="100%にリセット"
        aria-label="ズームを100%にリセット">{previewZoomPercent}%</button>
      <button type="button" disabled={previewZoomPercent >= 180} onClick={onZoomIn}
        title={`ズームイン (${zoomInCombo})`} aria-label="プレビューをズームイン">＋</button>
    </div>
  )}
  ```

  `zoomOutCombo`/`zoomInCombo` are computed via `resolveShortcuts(appSettings?.shortcuts)["preview.
  zoomOut"/"preview.zoomIn"]` — the exact same "tooltip shows the live-bound shortcut" pattern `[spec
  v1.2.0 §6]` itself specifies, and the same `resolveShortcuts` call already used elsewhere in this file
  (e.g. the 18-toolbar-action keydown handler, `EditorPane.tsx` line ~626) and in `TabBar.tsx` (per `[spec
  v1.2.0 §6]`'s own citation).
- **Disabled-at-bounds check**: `previewZoomPercent <= 70`/`>= 180` — a plain numeric comparison against
  the fixed range constants (§10.2 area — worth defining as named constants, e.g. `PREVIEW_ZOOM_MIN = 70`,
  `PREVIEW_ZOOM_MAX = 180`, `PREVIEW_ZOOM_STEP = 10`, alongside `PANE_MODE_LABEL` near the top of
  `MarkdownToolbar.tsx`, or in `src/shared/models.ts` if `developer-lite` prefers co-locating with other
  numeric-range constants like `uiFontScale`'s 80–150% bounds, which — confirmed by inspection of
  `SettingsDialog.tsx`'s own `<input type="range" min={80} max={150}>` — are themselves inlined at the JSX
  call site today, not named constants, so either placement is consistent with this codebase's existing,
  mixed practice).

### 6.4 Keyboard Shortcut Wiring — the `Ctrl+NumpadSubtract`/`Ctrl+-` Collision Finding

Elaborates `[spec v1.2.0 §6]`'s "both registered" shortcut requirement, grounded against
`src/shared/shortcuts.ts`'s actual `comboFromEvent` implementation (confirmed by inspection):

- **New `DEFAULT_SHORTCUTS` entries**: `"preview.zoomIn": "Ctrl+="`, `"preview.zoomOut": "Ctrl+-"` —
  ordinary, fully rebindable entries added to `src/shared/shortcuts.ts`'s existing table, following the
  same convention every other action-id/combo pair there already does (e.g. `"markdown.pdfExport":
  "Ctrl+P"`).
- **Dispatch handler**: a new, **focus-unconstrained** `useEffect` in `EditorPane.tsx`, following the
  exact same pattern already established for `markdown.paneModeCycle`/`markdown.pdfExport` (lines
  ~655–682 — both are separate `useEffect`s outside the focus-scoped 18-toolbar-action dispatcher,
  specifically because those two "must fire regardless of focus location within the editor pane, including
  while focus is inside the preview pane's own DOM subtree" — the identical requirement applies here,
  since the zoom buttons are meant to work while the user's focus is inside the very preview pane being
  zoomed, not just when the (possibly hidden, in `paneMode === "preview"`) CodeMirror `contentDOM` happens
  to have focus).
- **Important, non-obvious finding from directly reading `comboFromEvent`'s implementation**
  (`src/shared/shortcuts.ts` lines ~118–127: builds a combo string from `e.ctrlKey`/`e.altKey`/
  `e.shiftKey` plus `e.key` — **not** `e.code`): standard `KeyboardEvent.key` values for the numpad
  operator keys are the literal printed symbol (`"+"` for `NumpadAdd`, `"-"` for `NumpadSubtract`),
  identical to what `e.key` reports for the corresponding non-numpad key **only for the minus/subtract
  case** — confirmed empirically consistent with the DOM `UI Events` spec's own defined `key` values.
  Concretely:
  - **Zoom OUT**: `Ctrl+NumpadSubtract`'s `e.key` is `"-"`, exactly the same as the main-row `Ctrl+-`'s
    `e.key`. `comboFromEvent` therefore already produces the identical string `"Ctrl+-"` for **both**
    physical keys, with **zero extra code** — the existing generic `resolveShortcuts`/`comboFromEvent`
    reverse-lookup dispatcher already handles the numpad alias automatically, once `"preview.zoomOut":
    "Ctrl+-"` is registered. `[spec v1.2.0 §6]`'s framing of the numpad alias as something needing its own
    "fixed, always-on... not exposed as its own Settings entry" special-casing is **only actually true for
    zoom-in**, not zoom-out.
  - **Zoom IN**: `Ctrl+NumpadAdd`'s `e.key` is `"+"`, but the main-row `Ctrl+=`'s `e.key` (no Shift held)
    is `"="` — these are **different** strings, so `comboFromEvent` does **not** already alias them; this
    is the one case that genuinely needs the separate, hardcoded, non-rebindable check `[spec v1.2.0 §6]`
    describes: `if (e.ctrlKey && e.code === "NumpadAdd") { e.preventDefault(); onZoomIn(); }`, checked via
    `e.code` (not `e.key`, and not routed through `resolveShortcuts` at all, matching `[spec v1.2.0 §6]`'s
    "not exposed as its own separate entry in Settings" requirement) inside the same focus-unconstrained
    handler, alongside the primary `resolveShortcuts`-driven check.
  - **Net effect**: the zoom-out numpad alias needs no special code (it already works once the primary
    shortcut is registered); the zoom-in numpad alias needs exactly one extra `e.code === "NumpadAdd"`
    branch. `developer-lite` should still document both as intentional aliases in code comments (mirroring
    this codebase's own dense-comment convention) so a future reader doesn't mistake the zoom-out case's
    "no extra code" for an oversight.
- **`e.preventDefault()` required**: same reasoning as `markdown.pdfExport`'s own native-print-dialog
  conflict note (`[MDEditor-spec §8]`) — `Ctrl+=`/`Ctrl+-` are also WebView2/Chromium's native
  page-zoom shortcuts (zooming the whole app window's UI, not this app's own preview-content zoom); both
  must be intercepted before the native browser-zoom behavior fires.

---

## 7. Quick Open Discoverability Hint — Elaboration

Elaborates `[spec v1.2.0 §7]` in full — a one-line tooltip-text change, fully grounded by inspection.

- **Exact touch point**: `src/components/TabBar/TabBar.tsx` line ~389, `<button type="button"
  className="ttn-toolbar-btn" title="検索" aria-label="検索" onClick={crossTabSearch.open}>` — both
  `title` and `aria-label` change from the static `"検索"` to a dynamically-built string incorporating
  Quick Open's live-resolved shortcut:

  ```tsx
  title={`検索 ／ クイックオープン (${resolveShortcuts(settings?.shortcuts)["search.quickOpen"]}) でメモ名検索`}
  ```

  (exact copy per `[spec v1.2.0 §7]`'s own suggested phrasing; `aria-label` may reasonably stay shorter,
  e.g. keep `"検索"` alone, or match `title` verbatim — `[spec v1.2.0 §7]` does not distinguish the two,
  unlike §3's explicit aria-label instruction, so this is a minor implementation call).
- **`resolveShortcuts` already imported/available in this file**: confirmed by inspection —
  `TabBar.tsx` already imports both `comboFromEvent` and `resolveShortcuts` from `../../shared/shortcuts`
  and already calls `resolveShortcuts(settings?.shortcuts)` twice elsewhere in the same component (per
  `[spec v1.2.0 §6]`'s own citation, "the same `resolveShortcuts(settings?.shortcuts)` lookup already used
  elsewhere, e.g. `src/components/TabBar/TabBar.tsx`"), so this change needs no new import, no new prop —
  `settings` (the component's actual prop name; **not** `appSettings`, which is the analogous but
  differently-named prop `MarkdownToolbar.tsx` uses instead, per §6.3/§6.4 above) is already a value this
  component has access to.
- **No other change**: `crossTabSearch.open` (the button's `onClick`) is untouched — this button still
  only opens in-memo/cross-tab **search** (`Ctrl+F`-equivalent UI), not Quick Open itself; the tooltip
  merely now also *tells* the user Quick Open exists and its shortcut, it does not make this button
  trigger Quick Open.

---

## 8. Preview-Only-Mode Scroll-Position Tracking Fix — Elaboration

Elaborates `[spec v1.2.0 §8]` in full.

### 8.1 New Function: Reverse of `scrollPreviewToLine`

- **New function, `src/editor/preview/scrollSync.ts`** (the same file already housing the one-directional
  `scrollPreviewToLine(container, targetLine)`, confirmed by inspection — this is a natural sibling
  export, same file, same module's concern): a `computeTopVisiblePreviewLine(container: HTMLElement):
  number` returning a **1-indexed document line number**, matching `computeTopVisibleLine`'s own
  documented convention (`src/editor/textCommands.ts`) exactly, so both feed `MemoEditorSettings.
  scrollPosition` (`detailed-spec.md` §3.3, "1-indexed document line number") identically regardless of
  which pane mode produced the value.

  ```ts
  export function computeTopVisiblePreviewLine(container: HTMLElement): number | null {
    const blocks = container.querySelectorAll<HTMLElement>("[data-source-line]");
    let best: HTMLElement | null = null;
    for (const el of blocks) {
      if (el.offsetTop <= container.scrollTop) best = el; // same "last match wins" scan shape as scrollPreviewToLine
      else break;
    }
    if (!best) return null; // no rendered block yet (e.g. empty memo, or html not yet caught up — see §8.3/§10.1)
    return Number(best.dataset.sourceLine) + 1; // +1: data-source-line is 0-indexed (token.map[0]), scrollPosition is 1-indexed
  }
  ```

  The `+ 1` conversion is not invented here — it is the **exact same** 0-indexed-to-1-indexed conversion
  `previewInteractions.ts`'s existing `handleCheckboxClick` already performs for an unrelated purpose
  (`const lineNumber = lineIndex0 + 1; // CodeMirror lines are 1-indexed; markdown-it's .map is
  0-indexed.`) — reusing the identical, already-battle-tested convention rather than inventing a new one,
  and worth a code comment cross-referencing that existing site so a future reader sees the two are
  deliberately consistent, not coincidentally similar.
  - **Scan-comparison basis differs slightly from `scrollPreviewToLine`'s own**: `scrollPreviewToLine`
    compares `data-source-line` values directly against a target *line number* (an approximate match, per
    `[MDEditor-spec §4]`'s "closest value ≤..." description, §10.1's own note on this existing tolerance).
    `computeTopVisiblePreviewLine` above instead compares each block's `offsetTop` (a pixel position)
    against `container.scrollTop` (also a pixel position) — the correct basis for "which block is
    currently topmost in the viewport," mirroring `computeTopVisibleLine`'s own `view.lineBlockAtHeight
    (view.scrollDOM.scrollTop)` pixel-based lookup on the CodeMirror side, not a line-number-based one.

### 8.2 Save-Path and Restore-Path Touch Points

- **Save path**: `EditorPane.tsx`'s `persistEditorViewState`/`flushOutgoingEditorViewState` (lines
  ~389–424, confirmed by inspection — both currently call `computeTopVisibleLine(view)` unconditionally)
  each need a `paneMode === "preview"` branch: when true, call `computeTopVisiblePreviewLine(previewContainerRef
  .current)` instead (requiring `EditorPane.tsx` to hold a ref to the mounted preview container — either
  threaded up from `PreviewPane.tsx` via a new imperative handle/callback ref, mirroring how `EditorPane`
  already exposes its own handle to `App.tsx`, or `PreviewPane.tsx` itself exposes a `getTopVisibleLine()`
  method for `EditorPane.tsx` to call at flush time — either shape is `developer-lite`'s call). If
  `computeTopVisiblePreviewLine` returns `null` (no rendered blocks yet — e.g. flushing before the debounced
  preview render ever completed for this memo), fall back to not overwriting the previously-saved
  `scrollPosition` at all (skip the write) rather than persisting a wrong/zero value — a reasonable,
  `developer-lite`-level edge-case call not explicitly spelled out in `[spec v1.2.0 §8]` but consistent
  with `[spec §2.6]`'s general "don't write if no change" persistence philosophy.
- **Restore path**: the `savedScrollLine`-driven `requestAnimationFrame(() => scrollToTopVisibleLine(view,
  savedScrollLine))` block (`EditorPane.tsx` lines ~527–531, confirmed by inspection) needs an analogous
  `paneMode === "preview"` branch calling `scrollPreviewToLine(previewContainer, savedScrollLine - 1)`
  instead (note the **inverse** `- 1` conversion, undoing §8.1's `+ 1`, since `scrollPreviewToLine`'s own
  `targetLine` parameter is compared directly against the 0-indexed `data-source-line` attribute, per
  `scrollPreviewToLine`'s own source, §8.1's citation) — timing-gated on the preview container actually
  reflecting the target memo first (§8.3/§10.1), not merely on the container existing.
- **`edit`/`split` unaffected**: both branches' existing, non-`"preview"` code path is untouched — `[spec
  v1.2.0 §8]`'s "edit/split mode unaffected" requirement is satisfied by construction, since the new logic
  is purely an added `if (paneMode === "preview") { ... } else { /* existing code, unchanged */ }` branch
  at each of the two touch points, not a rewrite of the existing path.
- **"Not in scope" boundary confirmed honored, not just asserted**: `[spec v1.2.0 §8]` explicitly excludes
  "live bidirectional syncing... at the moment the user switches pane modes mid-session" from this fix's
  scope. The two touch points above (§8.2's save/restore paths) are gated on `ed.loadGeneration` (a fresh
  memo load/switch) and on unmount/outgoing-memo flush — **not** on `paneMode` itself changing while the
  same memo stays open. Confirmed by inspection: `EditorPane.tsx`'s existing debounced-scroll `scrollHandler`
  is bound to the **editor's own** `scrollDOM`, which the user never touches directly while `paneMode ===
  "preview"` (the edit sub-pane is `display:none`), so no live sync/flush fires purely from toggling
  `paneMode` back and forth on an already-open memo — only this fix's save-path branch, itself only invoked
  at the pre-existing scroll-debounce/switch-away/unmount trigger points `[spec v1.1.0 §2]` already
  established, ever runs. This fix therefore does not, even inadvertently, introduce the excluded
  live-bidirectional-sync behavior.

### 8.3 Shared Timing Concern With §4 — See §10.1

The restore-path timing gate above needs the exact same "wait until the preview container's rendered
content actually belongs to the just-opened memo" condition §4.3 already identified independently for its
own cross-tab anchor jump (both read from `[data-source-line]`/`id` attributes inside a `PreviewPane`
whose `html` state lags a memo switch by up to `debounceMs`). Rather than each feature inventing its own
ad hoc wait/poll logic, §10.1 proposes one shared primitive both can call.

---

## 9. Preview-Pane Link-Encoding Fix — `#slug` Anchor Navigation and `memo:` Link Navigation — Elaboration

Elaborates `[spec v1.2.0 §9]` in full, including its `memo:`-branch bundled follow-up (resolving `QA.md`
`Q-v1.2.0-1`, per the user's decision — see `DECISIONS.md` for rationale).

### 9.1 `#slug` Branch (as Originally Decided)

`[spec v1.2.0 §9]`'s own root-cause writeup already reaches
implementation-planning depth (concrete file: `src/editor/preview/previewInteractions.ts`, concrete line:
the `handleAnchorClick` `#`-prefixed branch, concrete fix: wrap the existing `href.slice(1)` lookup in
`decodeURIComponent(...)`) — re-verified correct by direct inspection of that file (confirmed at lines
~97–104, the exact `if (href.startsWith("#")) { const slug = href.slice(1); ... }` block `[spec v1.2.0
§9]` describes). Nothing further to add beyond:

- **Concrete one-line diff**:

  ```diff
  -    const slug = href.slice(1);
  +    const slug = decodeURIComponent(href.slice(1));
  ```

  at `src/editor/preview/previewInteractions.ts`'s `handleAnchorClick`, line ~98.
- **No change needed to `tocGenerator.ts`/`previewRenderer.ts`**: `[spec v1.2.0 §9]`'s own root-cause
  analysis already establishes the heading `id` attribute (`previewRenderer.ts`'s `token.attrSet("id",
  headings[headingIndex].slug)`) is set from the raw, unencoded slug and must **stay** that way — the fix
  is entirely on the read/lookup side (`handleAnchorClick`), not the write side. Confirmed consistent with
  §4.3's own programmatic (non-click) anchor-resolution path (§4 above), which reads the slug directly
  from `resolveAndOpenMemoLink`'s string-splitting and therefore needs **no** `decodeURIComponent` of its
  own — the two code paths (real click vs. programmatic post-navigation jump) each need exactly one
  decode operation between them, at the one point encoding was actually introduced (markdown-it's
  `normalizeLink`, which only the real-`<a href>`-click path ever passes through).
- **`decodeURIComponent` failure mode**: a malformed percent-sequence in a hand-typed link would make
  `decodeURIComponent` throw (`URIError`) rather than silently no-op — `[spec §2.8]`'s "no match = silent
  no-op" convention for an unresolved anchor means this call should be wrapped in a `try { ... } catch {
  return; }` (or equivalent) so a malformed link degrades to a silent no-op rather than throwing an
  uncaught exception into the click handler — a defensive-coding detail not explicitly named in `[spec
  v1.2.0 §9]` but directly implied by the app's own established "unresolved link = do nothing, never an
  error surfaced to the user" convention it repeatedly cites elsewhere.

### 9.2 `memo:` Branch (Bundled Follow-Up, Resolves `Q-v1.2.0-1`)

Same root cause as §9.1, same file, sibling branch of the same function — confirmed by direct inspection
of `previewInteractions.ts`'s `handleAnchorClick` (lines ~92–96):

```ts
if (href.startsWith("memo:")) {
  void resolveAndOpenMemoLink(config.tabId, href, config.onOpenMemoLink);
  return;
}
```

- **Concrete one-line diff**, mirroring §9.1's own fix exactly:

  ```diff
  -    void resolveAndOpenMemoLink(config.tabId, href, config.onOpenMemoLink);
  +    void resolveAndOpenMemoLink(config.tabId, decodeURIComponent(href), config.onOpenMemoLink);
  ```

  Decoding the **whole** `href` (not just a slice past a fixed-length prefix, unlike §9.1's `href.slice
  (1)`) is correct and safe here: the literal ASCII substring `"memo:"` contains no characters
  `decodeURIComponent` would ever transform, so decoding the full string first and then letting §4.1's
  existing `rawTarget.startsWith("memo:") ? rawTarget.slice("memo:".length) : rawTarget` scheme-stripping
  logic run against the now-decoded string (unchanged from §4.1's own snippet) produces the same
  end result as decoding only the post-`memo:` remainder — simpler to write, with no behavioral
  difference.
- **Decode point stays upstream of `resolveAndOpenMemoLink`, not inside it** — deliberately, to avoid
  double-decoding or an incorrect decode on the live-view call path: `src/editor/links.ts`'s
  `resolveAndOpenMemoLink` (§4.1's own code snippet) is shared by **both** the preview pane's
  `handleAnchorClick` and live view's own bare-`memo:`-token/parenthetical-link click handling
  (`markdownDecorations.ts`'s `handleMouseEvent`/`links.ts` call sites) — live view's `rawTarget` is read
  straight from the raw CodeMirror source buffer and was **never** percent-encoded to begin with (§9.1's
  own "Live view is unaffected" finding, restated here for the `memo:` case: it applies identically, for
  the identical reason — live view never passes through markdown-it's `normalizeLink`). Adding a decode
  step inside `resolveAndOpenMemoLink` itself would therefore incorrectly attempt to decode an
  already-plain live-view target (harmless for most real tab/memo names, but wrong in principle, and
  actively wrong for the rare case of a real name containing a literal `%` character followed by two hex
  digits, e.g. `"50% Done"` — decoding a name that was never encoded could corrupt it). Fixing the encoding
  problem at its one actual source (the preview pane's own click-handler branch, exactly where §9.1 already
  fixed the sibling `#`-branch) keeps the shared `resolveAndOpenMemoLink` function agnostic to which pane
  called it, matching §4.1's own "single code path... covers both panes' `memo:` click paths at once"
  design once the upstream decode is in place.
- **Interaction with §4's `#slug` splitting**: because the decode happens *before* `resolveAndOpenMemoLink`
  ever runs, §4.1's own `hashIndex`/`target`/`anchorSlug`-splitting logic (this document's §4.1 code
  snippet) operates on an already-decoded string for both the memo-name portion and any `#slug` suffix —
  i.e. this fix, applied once at the `handleAnchorClick` call site, transparently also fixes decoding for
  §4's new `memo:TabName/MemoName#slug` cross-tab links' memo-name half, with no separate code needed in
  §4.1/§4.2/§4.3 beyond what those sections already specify. §4.3's preview-mode anchor-jump path (the
  `container.querySelector('#' + CSS.escape(slug))` lookup) already correctly needs no decode of its own,
  as §9.1 already established for the same reason — the slug it receives comes from this same
  already-decoded split, never re-encoded.
- **Same `try`/`catch` treatment as §9.1**: a malformed percent-sequence anywhere in the href — not just
  after `#` — would make this `decodeURIComponent(href)` call throw too; the same defensive `try { ... }
  catch { return; }` wrapping (or a single shared wrapping around both branches' decode calls) applies
  here for the identical reason.

---

## 10. Cross-Cutting Additions

### 10.1 New Shared Primitive: Preview-Render-Readiness Gate (needed by §4 and §8 alike)

Both §4.3 and §8.3 independently need the same underlying condition: "wait until `PreviewPane`'s rendered
`html` actually reflects the target memo before reading/querying its DOM." Rather than each feature
polling or timing out on its own, a single shared mechanism is proposed here (a genuinely new piece of
design this document adds, beyond restating `spec.v1.2.0.md` — grounded in, but not itself decided by,
`spec.v1.2.0.md`'s existing text; flagged as such per this document's own §0 convention, not escalated to
`QA.md` since it is a *mechanical* implementation detail of already-decided behavior, not a new product
decision):

- **Mechanism**: `PreviewPane.tsx`'s existing debounced-render effect (§4.3's citation, `setTimeout(() =>
  { if (view) setHtml(renderMarkdownToHtml(view.state).html); }, debounceMs)`) already has access to
  `memoKey` (`${tabId}:${memoName}`, confirmed already computed in `EditorPane.tsx` and passed down as a
  prop, per §10's own earlier citations). Track which `memoKey` the **current** `html` was actually
  rendered for, e.g. a new `renderedForMemoKeyRef` ref set at the same moment `setHtml(...)` is called
  inside that timeout callback.
- **Consumption**: both §4.3's post-navigation anchor jump and §8.3's preview-mode scroll restore, instead
  of firing immediately, poll (or subscribe via a small `useEffect` watching `html`/`renderTick`) until
  `renderedForMemoKeyRef.current === targetMemoKey`, then perform their respective DOM query. A bounded
  timeout (e.g. a few seconds, mirroring this codebase's own existing "reasonable per-image timeout"
  precedent for PDF export's image-wait step, `[MDEditor-spec §6]`) should guard against waiting forever
  if something else goes wrong (e.g. the memo was closed again before its preview ever rendered) — falling
  back to simply not performing the jump/restore in that case, consistent with this app's general
  "silent no-op on an edge case, never an error dialog" convention for these two features specifically.
- **Not a new component/module requirement**: this can be implemented as a small addition inside
  `PreviewPane.tsx` itself (exposing the readiness signal via an imperative handle or a callback prop) —
  `developer-lite`'s call on exact shape, same as every other "which exact React plumbing pattern" choice
  already left open throughout this document.

### 10.2 `schemaVersion` / Migration Handling

Elaborates the one new persisted field this whole v1.2.0 pass introduces: `AppSettings.language` (§1.2).
None of the other eight items add any persisted field — §5's toggle-back state and §6's zoom level are
both explicitly session-only/transient (`[spec v1.2.0 §5]`, `[spec v1.2.0 §6]`), and §2/§3/§4/§7/§8/§9
are rendering/behavior/bug-fix changes with no new schema field at all (§4's `#slug` suffix is parsed,
never stored).

- `AppSettings.schemaVersion` increments once, covering `language` alone (unlike `detailed-spec.v1.1.0.md`
  §4.1's two-fields-in-one-bump case, there is only one new `AppSettings` field this time, and no new
  `TabMeta`/`MetaTreeNode` field at all — `TabMeta.schemaVersion` does **not** need to bump for v1.2.0).
- **No destructive migration logic required**: `language: string | null`, absent on a pre-upgrade file,
  resolves to `null` on read (the existing generic "optional field, absent means not-yet-set" JSON-parsing
  behavior already used for every other optional `AppSettings` field, e.g. `tabLockAutoRelockMinutes`,
  `detailed-spec.v1.1.0.md` §1.2) — `[spec v1.2.0 §1.5]`'s own default-resolution rule (§1.2 above) is
  exactly what runs for that `null` case; no field-migration step beyond the schema bump itself.
- The concrete new integer value `AppSettings.schemaVersion` bumps **to** depends on whatever value is
  currently shipping at v1.2.0 implementation time — not hardcoded here, same convention
  `detailed-spec.v1.1.0.md` §4.1 and `detailed-spec.md` §9 both already establish.

### 10.3 Error Handling — No New `ErrorCode`s

None of the nine v1.2.0 items introduce a new `ErrorCode` or change any existing `Result<T>` error
handling:

- §1 (i18n) is a pure frontend string-lookup change; §1.4 above already confirms no backend/`message`-
  wire-shape change.
- §4's `link_resolve` call is unchanged (§4.1); an unresolved anchor is a silent no-op, reusing `[spec
  §2.8]`'s existing "no match = silent no-op" convention verbatim, same as the pre-existing same-memo case.
- §5/§6 have no backend IPC calls at all (both are pure frontend/CodeMirror-state features).
- §2's math rendering has no failure mode requiring a new error code — malformed `$...$` syntax simply
  fails to match the delimiter heuristic (§2.2) and renders as plain literal text, the same "graceful
  fallback to literal text" behavior CommonMark rendering already has for any unrecognized syntax.
- §8/§9 are bug fixes to existing, already-error-code-free code paths (scroll-position persistence,
  anchor-click resolution) — neither touches `Result<T>`/`ErrorCode` machinery at all.

---

## 11. Deferred-to-Implementation Details (Not Ambiguities — No User Decision Needed)

Mirrors the role of `detailed-spec.md` §9 and `detailed-spec.v1.1.0.md` §5, scoped to v1.2.0's nine items.
Every item below is either explicitly delegated by `spec.v1.2.0.md` itself, or a pure "which exact React/
CSS/Rust plumbing shape" choice with no observable behavioral consequence either way:

- Exact translation-key naming convention and per-language file internal structure (flat vs. nested keys)
  for i18n (§1.1) — beyond the `src/i18n/locales/` folder location and `TranslationKey` type-safety
  approach already decided in this pass.
- Exact `markdown-it` math-plugin package chosen, or hand-rolled-rule alternative (§2.1).
- Exact DOMPurify allowlist extension needed for KaTeX's generated markup (§2.1) — mechanical once the
  plugin is chosen.
- Whether the source-view math marker extends `SourceImageMarkerPlugin` in place or is a sibling class/
  compartment (§2.4) — `[spec v1.2.0 §2.4]`'s own explicit discretion callout.
- Exact wording of the pane-mode toggle's next-state `aria-label` phrasing (§3) — only "next-state-only,
  no current-state suffix" is spec-level.
- Exact shape of the `EditorPane.tsx` ↔ `PreviewPane.tsx` handle/callback used to read the preview
  container's scroll state (§8.2) and to expose the render-readiness signal (§10.1).
- `aria-label` copy for §6's zoom buttons, and whether it matches `title` verbatim or is a shorter variant
  (§6.3) — same non-decision `[spec v1.2.0 §7]` itself leaves for that section's own aria-label.
- Exact bounded-timeout duration for §10.1's render-readiness wait, and its user-visible failure behavior
  (silent no-op vs. some other fallback) — a `PLAN.md`-level constant choice, same class as this app's
  other already-established "reasonable timeout" constants (e.g. `[MDEditor-spec §6]`'s ~3-second
  per-image PDF-export wait).
- The concrete new integer value `AppSettings.schemaVersion` bumps to (§10.2).
- Exact CSS class name for block-math's wide-formula horizontal-scroll wrapper (§2.5 — `.ttn-preview-
  math-wrap` is this document's own suggested spelling, not binding, mirroring `.ttn-preview-table-wrap`'s
  own naming, same discretion class as `detailed-spec.v1.1.0.md` §5's "exact action-ID string spellings").
- Whether KaTeX is configured with `output: "html"` (skipping the parallel MathML tree) or its default
  `output: "htmlAndMathml"`/`"mathml"` (§2.1) — a genuine, if minor, accessibility/copy-paste-fidelity
  vs. sanitizer-surface-area tradeoff, left to `developer-lite`'s ordinary engineering judgment since
  `spec.v1.2.0.md` does not distinguish between the two output modes anywhere.

---

## 12. Open Questions Requiring User Decision

**None remaining.** `Q-v1.2.0-1` (whether to bundle the §4.4 `memo:`-branch encoding-bug fix into this
v1.2.0 pass) was resolved by the user — bundle it, using `[spec v1.2.0 §9]`'s already-decided `#slug`-
branch fix pattern. Reflected in `spec.v1.2.0.md` §9 and this document's §4.4/§9.2; full rationale in
`DECISIONS.md` ("Preview-Pane `memo:` Link Non-ASCII Name Encoding Bug: Bundled into v1.2.0's §9 Fix, Not
Deferred").

Every other point in `spec.v1.2.0.md`'s nine items was either already fully decided there, or was an
implementation-discretion item this pass resolved directly (§1.1, §2.2, §2.3, §6.2 above).
