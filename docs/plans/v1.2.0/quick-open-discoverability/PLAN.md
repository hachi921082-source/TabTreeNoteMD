# PLAN: Quick Open Discoverability Hint (v1.2.0)

> Status: planning complete, not yet implemented. Small, tooltip-only follow-up to `docs/plans/v1.1.0/
> quick-open/PLAN.md` (Quick Open itself — trigger, matching, ordering, Tab Lock exclusion — is entirely
> unmodified by this plan). Touches `src/components/TabBar/TabBar.tsx`'s existing 🔍 cross-tab
> content-search button only. Extracted UI copy folds into `docs/plans/v1.2.0/i18n/PLAN.md`'s translation
> maps.

Sources: `docs/spec.v1.2.0.md` §7; `docs/detailed-spec.v1.2.0.md` §7, §11.

---

## 1. Scope Recap

Quick Open (`Ctrl+O` default, `docs/plans/v1.1.0/quick-open/PLAN.md`) has no visible button anywhere in
the UI — a keyboard-only overlay, discoverable today only via Settings → Shortcuts or release notes. The
adjacent 🔍 cross-tab content-search toolbar button sits exactly where a user would naturally look for
"find a memo," but its tooltip says nothing about Quick Open. This plan extends that button's tooltip to
also name Quick Open and its current shortcut. Tooltip-text change only — no new button, no schema
change, no shortcut change.

## 2. Exact Touch Point

`src/components/TabBar/TabBar.tsx`, line ~389:

```diff
- <button type="button" className="ttn-toolbar-btn" title="検索" aria-label="検索"
+ <button type="button" className="ttn-toolbar-btn"
+   title={`検索 ／ クイックオープン (${resolveShortcuts(settings?.shortcuts)["search.quickOpen"]}) でメモ名検索`}
+   aria-label="検索"
    onClick={crossTabSearch.open}>
```

- Exact `title` copy per `[spec v1.2.0 §7]`'s own suggested phrasing: "検索 ／ クイックオープン (Ctrl+O)
  でメモ名検索" (or the currently-bound combo string, not hardcoded `"Ctrl+O"`).
- `aria-label` may reasonably stay the shorter `"検索"`, or match `title` verbatim — the spec does not
  distinguish the two (unlike `pane-mode-toolbar-semantics/PLAN.md`'s explicit `aria-label` instruction),
  so this is a minor `developer-lite` implementation call.
- **No new import, no new prop**: `TabBar.tsx` already imports both `comboFromEvent` and `resolveShortcuts`
  from `../../shared/shortcuts` and already calls `resolveShortcuts(settings?.shortcuts)` twice elsewhere
  in the same component; `settings` (the component's actual prop name — not `appSettings`, the
  differently-named prop `MarkdownToolbar.tsx` uses instead) is already available.
- **No other change**: `crossTabSearch.open` (the button's `onClick`) is untouched — this button still
  only opens in-memo/cross-tab **search**, not Quick Open itself. The tooltip merely now also tells the
  user Quick Open exists and its shortcut; it does not make this button trigger Quick Open.

## 3. Testing / Verification

- Hover the 🔍 button: tooltip reads "検索 ／ クイックオープン (Ctrl+O) でメモ名検索" (or whatever combo
  is currently bound to `search.quickOpen`).
- Rebind `search.quickOpen` in Settings → Shortcuts to a different combo: hover the 🔍 button again,
  confirm the tooltip text updates to the new combo, no stale "Ctrl+O" text left behind.
- Click the 🔍 button: still opens the cross-tab content-search dialog, unchanged behavior — confirms no
  regression to the button's actual function.
- Quick Open itself (`Ctrl+O` trigger, matching, ordering, Tab Lock exclusion) remains fully unmodified —
  spot-check one Quick Open interaction to confirm no accidental regression from this tooltip-only change.
