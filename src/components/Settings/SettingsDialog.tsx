// Settings screen shell (`docs/plans/window-operation/PLAN.md` §7): General/Shortcuts/Editor
// three-tab modal. General houses Appearance (`appearance-theme/PLAN.md` §2/§3 in full — theme,
// uiFontScale, and the "basic customization of colors/fonts/line spacing" layer: accent color, UI
// font family, tree row line-height, all under `AppSettings.appearance`), Links (`link/PLAN.md`'s
// two schema fields), and Autostart (§4). Shortcuts (§3) has its own tab since its 20+ rows made
// General too long to scroll (GitHub issue #6). Editor holds defaults/autosave/EOL (referenced by
// `editor-save/PLAN.md`/`markdown-editing/PLAN.md`).
// Opened from `TabBar.tsx`'s ⚙ button; every field reads from the already-loaded `AppSettings` and
// writes immediately on change via `settings_set` (no separate "Apply" step — `spec.md` doesn't
// mandate one, and every value here is a cheap settings-file write).
import { useState } from "react";
import type { AppSettings, AppSettingsPatch } from "../../shared/models";
import { REBINDABLE_ACTION_IDS, comboFromEvent, resolveShortcuts } from "../../shared/shortcuts";
import { autostartSetEnabled } from "../../ipc/autostart";
import { Dialog } from "../dialogs/Dialog";
import { resolveDefaultLanguage, useT } from "../../i18n";
import type { TranslationKey } from "../../i18n";
import "./Settings.css";

export interface SettingsDialogProps {
  settings: AppSettings;
  patchSettings: (patch: AppSettingsPatch) => Promise<void>;
  /** Reflects an `AppSettings` obtained from a command other than `settings_set` (currently just
   * `autostart_set_enabled`) into the shared settings cache — see `state/useSettings.ts`. */
  applyFetched: (settings: AppSettings) => void;
  onClose: () => void;
}

type SettingsTab = "general" | "shortcuts" | "editor";

/** Human-readable label KEYS for `shared/shortcuts.ts`'s action ids — UI-only concern, kept out of
 * that framework-agnostic module on purpose. `TranslationKey` values (not literal labels) so this
 * module-level table stays translatable via `useT()` at render time, looked up under the
 * `settings.shortcutLabel.<actionId>` namespace (`docs/plans/v1.2.0/i18n/PLAN.md` §6). */
const SHORTCUT_LABEL_KEYS: Record<string, TranslationKey> = {
  "tab.switchNext": "settings.shortcutLabel.tab.switchNext",
  "tab.switchPrev": "settings.shortcutLabel.tab.switchPrev",
  "editor.save": "settings.shortcutLabel.editor.save",
  "editor.findReplace": "settings.shortcutLabel.editor.findReplace",
  "tree.rename": "settings.shortcutLabel.tree.rename",
  "tree.forceDelete": "settings.shortcutLabel.tree.forceDelete",
  "search.quickOpen": "settings.shortcutLabel.search.quickOpen",
  "favorites.open": "settings.shortcutLabel.favorites.open",
  "markdown.undo": "settings.shortcutLabel.markdown.undo",
  "markdown.redo": "settings.shortcutLabel.markdown.redo",
  "markdown.color": "settings.shortcutLabel.markdown.color",
  "markdown.fontSize": "settings.shortcutLabel.markdown.fontSize",
  "markdown.heading": "settings.shortcutLabel.markdown.heading",
  "markdown.list": "settings.shortcutLabel.markdown.list",
  "markdown.bold": "settings.shortcutLabel.markdown.bold",
  "markdown.italic": "settings.shortcutLabel.markdown.italic",
  "markdown.strikethrough": "settings.shortcutLabel.markdown.strikethrough",
  "markdown.quote": "settings.shortcutLabel.markdown.quote",
  "markdown.link": "settings.shortcutLabel.markdown.link",
  "markdown.codeBlock": "settings.shortcutLabel.markdown.codeBlock",
  "markdown.tocUpdate": "settings.shortcutLabel.markdown.tocUpdate",
  "markdown.table": "settings.shortcutLabel.markdown.table",
  "markdown.image": "settings.shortcutLabel.markdown.image",
  "markdown.clearFormatting": "settings.shortcutLabel.markdown.clearFormatting",
  "markdown.hardBreak": "settings.shortcutLabel.markdown.hardBreak",
  "markdown.rearrangeToggle": "settings.shortcutLabel.markdown.rearrangeToggle",
  "markdown.toggleLiveView": "settings.shortcutLabel.markdown.toggleLiveView",
  "markdown.paneModeCycle": "settings.shortcutLabel.markdown.paneModeCycle",
};

export function SettingsDialog({ settings, patchSettings, applyFetched, onClose }: SettingsDialogProps) {
  const t = useT();
  const [tab, setTab] = useState<SettingsTab>("general");
  const [recordingActionId, setRecordingActionId] = useState<string | null>(null);
  const [autostartBusy, setAutostartBusy] = useState(false);
  const [autostartError, setAutostartError] = useState<string | null>(null);

  const effectiveShortcuts = resolveShortcuts(settings.shortcuts);

  const startRecording = (actionId: string) => {
    setRecordingActionId(actionId);
    const handler = (e: KeyboardEvent) => {
      // Esc cancels recording without rebinding (consistent with Esc-closes-dialogs convention,
      // `[spec §9]`) rather than being captured as the new combo itself. Stop it here so it
      // doesn't keep propagating up to `Dialog.tsx`'s own Esc listener and close the whole
      // Settings dialog too — only the recording should be cancelled.
      e.preventDefault();
      if (e.key === "Escape") {
        e.stopPropagation();
      } else if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
        // A bare modifier keydown (e.g. `Ctrl` pressed on its own, on the way to `Ctrl+I`) is not a
        // complete combo — keep the listener alive and keep waiting for the actual non-modifier key
        // instead of committing prematurely (GitHub issue #111).
        return;
      } else {
        const combo = comboFromEvent(e);
        void patchSettings({ shortcuts: { ...settings.shortcuts, [actionId]: combo } });
      }
      window.removeEventListener("keydown", handler, true);
      setRecordingActionId(null);
    };
    window.addEventListener("keydown", handler, true);
  };

  const resetShortcut = (actionId: string) => {
    const next = { ...settings.shortcuts };
    delete next[actionId];
    void patchSettings({ shortcuts: next });
  };

  const handleAutostartToggle = async (enabled: boolean) => {
    setAutostartBusy(true);
    setAutostartError(null);
    const result = await autostartSetEnabled(enabled);
    setAutostartBusy(false);
    if (result.ok) applyFetched(result.value);
    else setAutostartError(result.error.message);
  };

  return (
    <Dialog
      title={t("settings.title")}
      onClose={onClose}
      className="ttn-settings-dialog"
      footer={
        <button type="button" onClick={onClose}>
          {t("common.close")}
        </button>
      }
    >
      <span className="ttn-settings-version">{__APP_GIT_BRANCH__}</span>
      <div className="ttn-settings-tabs" role="tablist">
        <button type="button" className={`ttn-settings-tab${tab === "general" ? " ttn-settings-tab-active" : ""}`} onClick={() => setTab("general")}>
          {t("settings.tabGeneral")}
        </button>
        <button type="button" className={`ttn-settings-tab${tab === "shortcuts" ? " ttn-settings-tab-active" : ""}`} onClick={() => setTab("shortcuts")}>
          {t("settings.tabShortcuts")}
        </button>
        <button type="button" className={`ttn-settings-tab${tab === "editor" ? " ttn-settings-tab-active" : ""}`} onClick={() => setTab("editor")}>
          {t("settings.tabEditor")}
        </button>
      </div>

      {tab === "general" && (
        <div className="ttn-settings-body">
          <section className="ttn-settings-section">
            <h3>{t("settings.appearanceSection")}</h3>
            <label className="ttn-settings-row">
              <span>{t("settings.themeLabel")}</span>
              <select value={settings.theme} onChange={(e) => void patchSettings({ theme: e.target.value as AppSettings["theme"] })}>
                <option value="light">{t("settings.themeLight")}</option>
                <option value="dark">{t("settings.themeDark")}</option>
              </select>
            </label>
            <label className="ttn-settings-row">
              <span>{t("settings.uiFontScaleTemplate").replace("{pct}", String(Math.round(settings.uiFontScale * 100)))}</span>
              <input
                type="range"
                min={80}
                max={150}
                step={5}
                value={Math.round(settings.uiFontScale * 100)}
                onChange={(e) => void patchSettings({ uiFontScale: Number(e.target.value) / 100 })}
              />
            </label>
            {/* "Basic customization of colors/fonts/line spacing" layered on top of the active
                theme (`docs/plans/appearance-theme/PLAN.md` §3): accent color, UI font family, tree
                row line-height. `null` accentColor/uiFontFamily means "use the theme's default" —
                the color/text inputs below always show a concrete value (falling back to the
                built-in default) since native `<input type="color">`/`text` can't represent
                "unset" themselves; the ↺ buttons are how the user actually clears the override. */}
            <label className="ttn-settings-row">
              <span>{t("settings.accentColorLabel")}</span>
              <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <input
                  type="color"
                  value={settings.appearance.accentColor ?? "#4a90d9"}
                  onChange={(e) => void patchSettings({ appearance: { accentColor: e.target.value } })}
                />
                {settings.appearance.accentColor && (
                  <button type="button" className="ttn-settings-shortcut-reset" title={t("settings.resetToDefault")} onClick={() => void patchSettings({ appearance: { accentColor: null } })}>
                    ↺
                  </button>
                )}
              </span>
            </label>
            <label className="ttn-settings-row">
              <span>{t("settings.uiFontFamilyLabel")}</span>
              <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <input
                  type="text"
                  placeholder="Segoe UI, system-ui, sans-serif"
                  value={settings.appearance.uiFontFamily ?? ""}
                  onChange={(e) => void patchSettings({ appearance: { uiFontFamily: e.target.value || null } })}
                />
                {settings.appearance.uiFontFamily && (
                  <button type="button" className="ttn-settings-shortcut-reset" title={t("settings.resetToDefault")} onClick={() => void patchSettings({ appearance: { uiFontFamily: null } })}>
                    ↺
                  </button>
                )}
              </span>
            </label>
            <label className="ttn-settings-row">
              <span>{t("settings.treeRowLineHeightTemplate").replace("{value}", settings.appearance.treeRowLineHeight.toFixed(1))}</span>
              <input
                type="range"
                min={1.0}
                max={2.0}
                step={0.1}
                value={settings.appearance.treeRowLineHeight}
                onChange={(e) => void patchSettings({ appearance: { treeRowLineHeight: Number(e.target.value) } })}
              />
            </label>
            {/* Language switcher (`docs/plans/v1.2.0/i18n/PLAN.md` §4) — shows the EFFECTIVE
                language (resolved default, never a blank option) when `settings.language` is
                `null`. Option text is each language's own native name, deliberately NOT run
                through `t()` — a language picker conventionally names each option in that
                option's own language regardless of the currently-active UI language. */}
            <label className="ttn-settings-row">
              <span>{t("settings.language")}</span>
              <select
                value={settings.language ?? resolveDefaultLanguage()}
                onChange={(e) => void patchSettings({ language: e.target.value })}
              >
                <option value="ja">日本語</option>
                <option value="en">English</option>
              </select>
            </label>
          </section>

          <section className="ttn-settings-section">
            <h3>{t("settings.linksSection")}</h3>
            <label className="ttn-settings-row">
              <span>{t("settings.linkOpenTriggerLabel")}</span>
              <select
                value={settings.links.openTrigger}
                onChange={(e) => void patchSettings({ links: { openTrigger: e.target.value as AppSettings["links"]["openTrigger"] } })}
              >
                <option value="singleClick">{t("settings.linkTriggerSingleClick")}</option>
                <option value="ctrlClick">{t("settings.linkTriggerCtrlClick")}</option>
                <option value="doubleClick">{t("settings.linkTriggerDoubleClick")}</option>
              </select>
            </label>
            <label className="ttn-settings-row ttn-settings-row-checkbox">
              <input
                type="checkbox"
                checked={settings.links.confirmBeforeOpen}
                onChange={(e) => void patchSettings({ links: { confirmBeforeOpen: e.target.checked } })}
              />
              <span>{t("settings.linkConfirmBeforeOpen")}</span>
            </label>
          </section>

          <section className="ttn-settings-section">
            <h3>{t("settings.autostartSection")}</h3>
            <label className="ttn-settings-row ttn-settings-row-checkbox">
              <input
                type="checkbox"
                checked={settings.autostart.enabled}
                disabled={autostartBusy}
                onChange={(e) => void handleAutostartToggle(e.target.checked)}
              />
              <span>{t("settings.autostartEnabled")}</span>
            </label>
            {autostartError && <p className="ttn-settings-error">{autostartError}</p>}
          </section>

          <section className="ttn-settings-section">
            <h3>{t("settings.tabLockSection")}</h3>
            <label className="ttn-settings-row">
              <span>{t("settings.autoRelockLabel")}</span>
              <select
                value={settings.tabLockAutoRelockMinutes ?? "disabled"}
                onChange={(e) => {
                  const raw = e.target.value;
                  void patchSettings({ tabLockAutoRelockMinutes: raw === "disabled" ? null : Number(raw) });
                }}
              >
                <option value={5}>{t("settings.minutesTemplate").replace("{n}", "5")}</option>
                <option value={15}>{t("settings.minutesTemplate").replace("{n}", "15")}</option>
                <option value={30}>{t("settings.minutesTemplate").replace("{n}", "30")}</option>
                <option value={60}>{t("settings.minutesTemplate").replace("{n}", "60")}</option>
                <option value="disabled">{t("settings.disabled")}</option>
              </select>
            </label>
          </section>
        </div>
      )}

      {tab === "shortcuts" && (
        <div className="ttn-settings-body">
          <section className="ttn-settings-section">
            <h3>{t("settings.tabShortcuts")}</h3>
            <div className="ttn-settings-shortcut-list">
              {REBINDABLE_ACTION_IDS.map((actionId) => {
                const isOverridden = settings.shortcuts[actionId] !== undefined;
                const labelKey = SHORTCUT_LABEL_KEYS[actionId];
                return (
                  <div className="ttn-settings-shortcut-row" key={actionId}>
                    <span className="ttn-settings-shortcut-label">{labelKey ? t(labelKey) : actionId}</span>
                    <button type="button" className="ttn-settings-shortcut-combo" onClick={() => startRecording(actionId)}>
                      {/* `favorites.open`-style unbound actions (`shared/shortcuts.ts`'s
                          `UNBOUND_ACTION_IDS`) resolve to `undefined` here unless user-bound. */}
                      {recordingActionId === actionId ? t("settings.recordingKeyWait") : (effectiveShortcuts[actionId] ?? t("settings.unbound"))}
                    </button>
                    {isOverridden && (
                      <button type="button" className="ttn-settings-shortcut-reset" title={t("settings.resetToDefault")} onClick={() => resetShortcut(actionId)}>
                        ↺
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
          </section>
        </div>
      )}

      {tab === "editor" && (
        <div className="ttn-settings-body">
          <section className="ttn-settings-section">
            <h3>{t("settings.editorDefaultsSection")}</h3>
            <label className="ttn-settings-row">
              <span>{t("settings.wrapLabel")}</span>
              <select
                value={settings.editorDefaults.wrap}
                onChange={(e) => void patchSettings({ editorDefaults: { wrap: e.target.value as AppSettings["editorDefaults"]["wrap"] } })}
              >
                <option value="window">{t("settings.wrapWindow")}</option>
                <option value="chars">{t("settings.wrapChars")}</option>
                <option value="none">{t("settings.wrapNone")}</option>
              </select>
            </label>
            <label className="ttn-settings-row">
              <span>{t("settings.tabWidthLabel")}</span>
              <input
                type="number"
                min={1}
                max={16}
                value={settings.editorDefaults.tabWidth}
                onChange={(e) => void patchSettings({ editorDefaults: { tabWidth: Number(e.target.value) } })}
              />
            </label>
            <label className="ttn-settings-row ttn-settings-row-checkbox">
              <input
                type="checkbox"
                checked={settings.editorDefaults.softTab}
                onChange={(e) => void patchSettings({ editorDefaults: { softTab: e.target.checked } })}
              />
              <span>{t("settings.softTabLabel")}</span>
            </label>
            <label className="ttn-settings-row ttn-settings-row-checkbox">
              <input
                type="checkbox"
                checked={settings.editorDefaults.showLineNumbers}
                onChange={(e) => void patchSettings({ editorDefaults: { showLineNumbers: e.target.checked } })}
              />
              <span>{t("settings.showLineNumbersLabel")}</span>
            </label>
            <label className="ttn-settings-row">
              <span>{t("settings.fontSizeLabel")}</span>
              <input
                type="number"
                min={8}
                max={40}
                value={settings.editorDefaults.fontSize}
                onChange={(e) => void patchSettings({ editorDefaults: { fontSize: Number(e.target.value) } })}
              />
            </label>
            <label className="ttn-settings-row">
              <span>{t("settings.defaultPaneModeLabel")}</span>
              <select
                value={settings.editorDefaults.paneMode}
                onChange={(e) => void patchSettings({ editorDefaults: { paneMode: e.target.value as AppSettings["editorDefaults"]["paneMode"] } })}
              >
                <option value="edit">{t("paneMode.edit")}</option>
                <option value="split">{t("paneMode.split")}</option>
                <option value="preview">{t("paneMode.preview")}</option>
              </select>
            </label>
          </section>

          <section className="ttn-settings-section">
            <h3>{t("settings.previewSection")}</h3>
            <label className="ttn-settings-row">
              <span>{t("settings.previewDebounceLabel")}</span>
              <input
                type="number"
                min={50}
                max={2000}
                step={50}
                value={settings.preview.debounceMs}
                onChange={(e) => void patchSettings({ preview: { debounceMs: Number(e.target.value) } })}
              />
            </label>
          </section>

          <section className="ttn-settings-section">
            <h3>{t("settings.autosaveSection")}</h3>
            <label className="ttn-settings-row">
              <span>{t("settings.autosaveDebounceLabel")}</span>
              <input
                type="number"
                min={200}
                max={10000}
                step={100}
                value={settings.autosave.plainDebounceMs}
                onChange={(e) => void patchSettings({ autosave: { plainDebounceMs: Number(e.target.value) } })}
              />
            </label>
          </section>

          <section className="ttn-settings-section">
            <h3>{t("settings.eolSection")}</h3>
            <label className="ttn-settings-row">
              <span>{t("settings.eolPolicyLabel")}</span>
              <select value={settings.eol.policy} onChange={(e) => void patchSettings({ eol: { policy: e.target.value as AppSettings["eol"]["policy"] } })}>
                <option value="preserve">{t("settings.eolPreserve")}</option>
                <option value="crlf">CRLF</option>
                <option value="lf">LF</option>
                <option value="cr">CR</option>
              </select>
            </label>
            <label className="ttn-settings-row">
              <span>{t("settings.encodingLabel")}</span>
              <span>{t("settings.encodingFixedUtf8")}</span>
            </label>
          </section>
        </div>
      )}
    </Dialog>
  );
}
