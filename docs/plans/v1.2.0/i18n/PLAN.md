# PLAN: UI Internationalization (i18n) — Runtime Language Switching (v1.2.0)

> Status: planning complete, not yet implemented. New feature, no existing v1.0.0/v1.1.0 plan owns this
> surface. Depends on `docs/plans/project-architecture/PLAN.md` (flat top-level `src/` module-layout
> convention, `Result<T>`/error-code shape), `docs/plans/persistence-fs/PLAN.md` §6 (`AppSettings`
> schema/`schemaVersion` bump convention), and `docs/plans/window-operation/PLAN.md` (Settings-screen
> shell, `SettingsDialog.tsx`'s existing 一般/General tab housing `theme`/`uiFontScale`). This plan
> touches nearly every other component file in `src/components/` for string extraction (§6), but owns no
> other feature's data model — every other v1.2.0 feature that needs UI-chrome copy of its own (`preview-
> zoom/PLAN.md`, `tab-double-click-jump/PLAN.md`, etc.) writes its Japanese strings directly and gets them
> extracted into the `src/i18n/locales/*.ts` maps this plan establishes, per the sequencing note in §6.

Sources: `docs/spec.v1.2.0.md` §1; `docs/detailed-spec.v1.2.0.md` §1, §10.2, §10.3; `docs/DECISIONS.md`
("i18n Mechanism: Hand-Rolled React Context, Not `react-i18next` Package").

---

## 1. Scope Recap

Extract every hardcoded Japanese UI-chrome string in `src/` (menus, dialog titles/labels/buttons,
toolbar tooltips/labels, toast/notification messages, status bar text, settings-screen labels, context
menu entries, backend-error-code-derived user-facing messages) behind a `t()`-style lookup, switchable at
runtime from Settings with no restart, no reload, and no remount step. Memo *content* (Markdown body
text) is explicitly out of scope — this only ever touches the app's own interface chrome.

Initial language set: `ja` (Japanese) and `en` (English). Default when `AppSettings.language` is `null`:
`ja` unconditionally (see §5) — this is a "starts at `ja`, no user-visible change until Settings is
opened" feature for every existing user.

## 2. Data Model

`src/shared/models.ts`, `AppSettings` interface — new field, sibling of `theme`/`uiFontScale`:

```ts
export interface AppSettings {
  // ...
  theme: "light" | "dark";
  uiFontScale: number;
  language: string | null; // NEW — [spec v1.2.0 §1.3]. null = "not yet chosen, auto-detect default (§5)".
  // ...
}

export interface AppSettingsPatch {
  // ...
  language?: string | null;
  // ...
}
```

- `AppSettings.schemaVersion` increments by one, covering `language` alone (the only new persisted field
  across all nine v1.2.0 items, per `[detailed-spec v1.2.0 §10.2]`). No `TabMeta`/`MetaTreeNode` field
  changes. The concrete integer value is decided at implementation time (whatever `schemaVersion` is
  currently shipping + 1), same convention as every prior schema bump.
- **No migration logic**: a pre-upgrade `AppSettings` JSON simply lacks the `language` key; the existing
  generic "optional field absent → parses to `null`" behavior (already used for e.g.
  `tabLockAutoRelockMinutes`) applies as-is. §5's default-resolution rule is what runs for that `null`
  case — no destructive/rewriting migration step.
- No backend/`src-tauri/` change: `language` is read/written through the existing generic
  `settings_get`/`settings_set` (or equivalent patch) commands exactly like every other `AppSettings`
  field — no new Tauri command, no Rust-side validation beyond whatever generic JSON round-trip already
  applies to every other field.

## 3. `src/i18n/` Module — Context, Hook, Types

New top-level folder, sibling of `src/shared/`, `src/state/`, `src/editor/` (matches this codebase's flat,
feature-named folder convention directly under `src/`, per `[detailed-spec v1.2.0 §1.1]`):

```
src/i18n/
  index.ts            # I18nProvider, useT(), translations map, TranslationKey type
  locales/
    ja.ts              # const ja = { ... } as const   (source of truth for TranslationKey)
    en.ts              # const en: Record<TranslationKey, string> = { ... }
```

### 3.1 `locales/ja.ts` / `locales/en.ts`

- `ja.ts` exports a flat `const ja = { "tabBar.searchTooltip": "検索", ... } as const` object. Flat
  dot-namespaced keys (e.g. `"settings.general.language"`, `"toast.saveFailed"`) rather than nested
  objects — simpler `TranslationKey` derivation (a plain `keyof typeof ja` union, no recursive key-path
  type needed) and simpler to `grep` a specific key across both locale files during review. Exact
  namespace-prefix convention (one segment per originating component/feature, e.g. `tabBar.*`,
  `settings.*`, `dialog.confirmDelete.*`, `toast.*`, `errors.*`) is `developer-lite` discretion at
  extraction time (`[detailed-spec v1.2.0 §11]`), not spec-level.
- `en.ts` exports `const en: Record<TranslationKey, string> = { ... }` — the `Record<TranslationKey, ...>`
  annotation (not `as const`) means TypeScript raises a compile error the moment `en.ts` is missing a key
  `ja.ts` has, or has an extra key `ja.ts` doesn't — a build-time safety net for the large, error-prone
  extraction this feature requires.

### 3.2 `index.ts`

```ts
export type TranslationKey = keyof typeof ja;
export type Language = "ja" | "en";

const translations: Record<Language, Record<TranslationKey, string>> = { ja, en };

const I18nContext = React.createContext<{ lang: Language; t: (key: TranslationKey) => string }>(
  { lang: "ja", t: (key) => translations.ja[key] },
);

export function I18nProvider({ children }: { children: React.ReactNode }) {
  const { settings } = useSettings();
  const lang = resolveLanguage(settings); // §5 — settings.language ?? detected-default, collapses to "ja" today
  const t = React.useCallback(
    (key: TranslationKey) => translations[lang][key] ?? translations.ja[key], // fallback net, see below
    [lang],
  );
  return <I18nContext.Provider value={{ lang, t }}>{children}</I18nContext.Provider>;
}

export function useT() {
  return React.useContext(I18nContext).t;
}
```

- Mounted once in `src/main.tsx`, wrapping `<App />` — inside (not outside) whatever provider already
  makes `useSettings()` available, since `I18nProvider` reads `AppSettings.language` at render time.
- **Reactive by construction**: `I18nProvider`'s own render depends on `useSettings()`'s live state, so
  the instant `patchSettings({ language: "en" })` resolves and `useSettings()`'s state updates, React
  re-renders `I18nProvider` with a new `lang`, and every mounted `useT()` consumer re-renders with the new
  `t`. No reload, no remount, no extra plumbing — a direct, standard consequence of Context, not
  additional code (`[detailed-spec v1.2.0 §1.1]`).
- **Missing-key fallback**: `translations[lang][key] ?? translations.ja[key]` — falls back to the `ja`
  string if a key is somehow absent at runtime (defensive only; `TranslationKey`'s compile-time check
  should make this unreachable for `en`, but guards against a future third language file shipped
  incomplete). `developer-lite` may additionally `console.warn` in dev builds on fallback — not spec-
  mandated, a debugging aid.

## 4. Settings UI

`src/components/Settings/SettingsDialog.tsx`, 一般 (General) tab — new row placed near the existing
`theme`/`uiFontScale` rows:

```tsx
const t = useT();
const resolvedLang = settings.language ?? resolveDefaultLanguage(); // §5

<label>{t("settings.general.language")}</label>
<select
  value={resolvedLang}
  onChange={(e) => void patchSettings({ language: e.target.value })}
>
  <option value="ja">日本語</option>
  <option value="en">English</option>
</select>
```

- Same "writes immediately via `patchSettings`, no separate Apply step" pattern every other Settings row
  in this file already uses.
- The `<select>` shows the **effective** language (resolved default, not a blank option) when
  `settings.language` is `null` — never an unselected/undefined dropdown state.
- Language names are shown in their own native form ("日本語", "English"), not translated relative to the
  currently-active language — a language picker conventionally names each option in that option's own
  language regardless of current UI language.

## 5. OS-Locale Default Detection

Elaborates `[spec v1.2.0 §1.5]`/`[detailed-spec v1.2.0 §1.2]`.

```ts
// src/i18n/index.ts
function resolveDefaultLanguage(): Language {
  const detected = navigator.language; // WebView2 mirrors OS UI language unless overridden
  return detected.toLowerCase().startsWith("ja") ? "ja" : "ja"; // collapses to "ja" either way, see below
}

function resolveLanguage(settings: AppSettings | null): Language {
  if (settings?.language === "ja" || settings?.language === "en") return settings.language;
  return resolveDefaultLanguage();
}
```

- Per `[spec v1.2.0 §1.5]`'s literal rule ("if Japanese, use `ja`; otherwise... fall back to `ja` as
  well"), both branches of `resolveDefaultLanguage` currently return `"ja"` — the detection call still
  runs (not stubbed to a bare constant), since a future third non-`ja` default's story depends on the
  detection step already being wired, per `[detailed-spec v1.2.0 §1.2]`'s explicit note.
- `navigator.language` (no new Tauri plugin/permission) is the default choice; `developer-lite` may swap
  to a Tauri-side OS-locale API (`@tauri-apps/plugin-os`) if `navigator.language` proves unreliable during
  implementation — either satisfies the spec's single yes/no check, implementation discretion.
- **Unknown/malformed `settings.language` value** (e.g. a hand-edited settings file with `"fr"`, a
  language not in the shipped set): falls through to `resolveDefaultLanguage()` exactly like `null` —
  not a hard error, consistent with this app's general "silent, safe fallback over error dialog"
  convention for malformed persisted values.

## 6. String-Extraction Scope and Sequencing

In-scope inventory (per `[spec v1.2.0 §1.2]`, grounded by `[detailed-spec v1.2.0 §1.3]`): every literal
Japanese UI string under `src/components/`, plus `src/shared/errorMessages.ts` (§7), plus scattered
Japanese literals inside hook files building user-facing strings directly (e.g. `src/state/useToast.ts`
callers). **Not** in scope: `src/editor/tocGenerator.ts`'s TOC marker text (inert document content, not
UI chrome).

- **Suggested (non-binding) extraction order**, component-by-component behind `useT()`, verifying each
  component still renders identical Japanese text (with `language` left at its default `ja`) before
  moving to the next — a mechanical, low-risk-per-step refactor given the volume of call sites:
  1. `TabBar` (incl. the §7/`quick-open-discoverability` and §... tooltip changes other v1.2.0 plans add —
     coordinate so those plans' new strings land as translation keys from the start, not as one more
     hardcoded literal to extract later)
  2. `TreeView` + `TreeRow`
  3. `EditorPane` + `MarkdownToolbar` (incl. `preview-zoom/PLAN.md`'s new zoom-button labels/tooltips and
     `pane-mode-toolbar-semantics/PLAN.md`'s relabeled toggle buttons)
  4. `SettingsDialog` (incl. this plan's own new language row, §4)
  5. `components/dialogs/*`
  6. `Toast`
- Each of the six other v1.2.0 UI-facing plans in this parent folder (`math-rendering`, `pane-mode-
  toolbar-semantics`, `tab-double-click-jump`, `preview-zoom`, `quick-open-discoverability`) should write
  its own new user-facing strings as ordinary Japanese literals in its own PLAN.md/implementation — this
  plan's extraction pass folds them into `locales/ja.ts`/`en.ts` when it reaches that component, not
  before. No other plan needs to call `useT()` itself as a prerequisite; if this plan's extraction pass
  has not yet reached a given component when another v1.2.0 feature ships first, that feature's strings
  simply remain hardcoded Japanese until this plan's sweep reaches them — not a blocking dependency in
  either direction, sequencing is `developer-lite`'s call at implementation time.

## 7. Error-Message Translation

`src/shared/errorMessages.ts`'s `MESSAGES: Record<ErrorCode, string>` (12 entries as of inspection —
`NAME_DUPLICATE`, `NAME_INVALID`, `NOT_FOUND`, `IO_LOCKED`, `TRASH_FAILED`, `DECRYPT_FAILED`, `CONFLICT`,
`META_CORRUPT`, `LONG_PATH`, `TAB_LOCKED`, `ANOMALOUS_FILE_STATE`, `MEMO_EDIT_LOCKED`, `UNKNOWN`) becomes
per-language. Two equally valid shapes (`developer-lite` discretion, `[detailed-spec v1.2.0 §1.4]`):

- **(a)** Fold each code into `locales/ja.ts`/`en.ts` under an `errors.<CODE>` key namespace (e.g.
  `errors.NAME_DUPLICATE`), looked up via the same `useT()` hook at every existing
  `errorMessageFor(code)` call site — `errorMessageFor` becomes `errorMessageFor(code, t)` or is replaced
  outright by `t(\`errors.${code}\`)` inline.
- **(b)** Keep `errorMessageFor(code: ErrorCode, lang: Language): string` as its own small function
  reading from a per-language `MESSAGES` map (`{ ja: MESSAGES_JA, en: MESSAGES_EN }`), mirroring the
  current shape most closely — call sites pass the current `lang` (from `useT`'s context or a sibling
  `useLang()` accessor) instead of `t`.

Either satisfies `[spec v1.2.0 §1.2]`'s "add the code→message mapping to each language file." **No
backend/Rust change**: `link_resolve`/every other Tauri command's `Result<T>` wire payload sends
`code`/`message`/`path?` where `message` is already English-oriented internal-diagnostic text
(`errorMessages.ts`'s own file-header comment), never displayed to the user as-is — confirmed by
`[detailed-spec v1.2.0 §1.4]`/`[detailed-spec v1.2.0 §10.3]`, no new `ErrorCode` and no wire-shape change.

## 8. Testing / Verification

- Toggle `AppSettings.language` between `ja`/`en` in Settings; confirm every extracted component
  re-renders in the new language immediately, with no reload/remount and no stale-language flash.
- Fresh install / pre-upgrade settings file (no `language` key): confirm the app boots in Japanese with
  zero visible change from pre-v1.2.0 behavior.
- A hand-edited `AppSettings.json` with an unrecognized `language` value (e.g. `"fr"`): confirm silent
  fallback to `ja`, no crash, no error dialog.
- `tsc`/build check: confirm `en.ts` fails to compile if a key is deleted from it (verifies the
  `Record<TranslationKey, string>` type-safety net actually catches a missing translation).
- Spot-check that every error toast/dialog message (triggered via a real error condition, e.g. a
  duplicate-name save) renders in the current UI language, not hardcoded Japanese.
