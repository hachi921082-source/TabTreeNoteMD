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

/** Human-readable labels for `shared/shortcuts.ts`'s action ids — UI-only concern, kept out of
 * that framework-agnostic module on purpose. */
const SHORTCUT_LABELS: Record<string, string> = {
  "tab.switchNext": "次のタブ",
  "tab.switchPrev": "前のタブ",
  "editor.save": "保存",
  "editor.findReplace": "検索/置換を開く",
  "tree.rename": "名前を変更",
  "tree.forceDelete": "強制削除",
  "search.quickOpen": "クイックオープン",
  "favorites.open": "お気に入り一覧を開く",
  "markdown.undo": "元に戻す",
  "markdown.redo": "やり直し",
  "markdown.color": "文字色",
  "markdown.fontSize": "文字サイズ",
  "markdown.heading": "見出し",
  "markdown.list": "リスト",
  "markdown.bold": "太字",
  "markdown.italic": "斜体",
  "markdown.strikethrough": "取り消し線",
  "markdown.quote": "引用",
  "markdown.link": "リンク挿入",
  "markdown.codeBlock": "コードブロック",
  "markdown.tocUpdate": "目次を更新",
  "markdown.table": "表を挿入",
  "markdown.image": "画像を挿入",
  "markdown.clearFormatting": "書式クリア",
  "markdown.hardBreak": "強制改行",
  "markdown.rearrangeToggle": "ツールバー並べ替え",
  "markdown.toggleLiveView": "ライブ/ソース表示切替",
  "markdown.paneModeCycle": "ペイン表示モードを切り替え",
};

export function SettingsDialog({ settings, patchSettings, applyFetched, onClose }: SettingsDialogProps) {
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
      title="設定"
      onClose={onClose}
      className="ttn-settings-dialog"
      footer={
        <button type="button" onClick={onClose}>
          閉じる
        </button>
      }
    >
      <span className="ttn-settings-version">{__APP_GIT_BRANCH__}</span>
      <div className="ttn-settings-tabs" role="tablist">
        <button type="button" className={`ttn-settings-tab${tab === "general" ? " ttn-settings-tab-active" : ""}`} onClick={() => setTab("general")}>
          一般
        </button>
        <button type="button" className={`ttn-settings-tab${tab === "shortcuts" ? " ttn-settings-tab-active" : ""}`} onClick={() => setTab("shortcuts")}>
          ショートカット
        </button>
        <button type="button" className={`ttn-settings-tab${tab === "editor" ? " ttn-settings-tab-active" : ""}`} onClick={() => setTab("editor")}>
          エディタ
        </button>
      </div>

      {tab === "general" && (
        <div className="ttn-settings-body">
          <section className="ttn-settings-section">
            <h3>外観</h3>
            <label className="ttn-settings-row">
              <span>テーマ</span>
              <select value={settings.theme} onChange={(e) => void patchSettings({ theme: e.target.value as AppSettings["theme"] })}>
                <option value="light">ライト</option>
                <option value="dark">ダーク</option>
              </select>
            </label>
            <label className="ttn-settings-row">
              <span>UI文字サイズ ({Math.round(settings.uiFontScale * 100)}%)</span>
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
              <span>アクセントカラー</span>
              <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <input
                  type="color"
                  value={settings.appearance.accentColor ?? "#4a90d9"}
                  onChange={(e) => void patchSettings({ appearance: { accentColor: e.target.value } })}
                />
                {settings.appearance.accentColor && (
                  <button type="button" className="ttn-settings-shortcut-reset" title="既定に戻す" onClick={() => void patchSettings({ appearance: { accentColor: null } })}>
                    ↺
                  </button>
                )}
              </span>
            </label>
            <label className="ttn-settings-row">
              <span>UIフォント</span>
              <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <input
                  type="text"
                  placeholder="Segoe UI, system-ui, sans-serif"
                  value={settings.appearance.uiFontFamily ?? ""}
                  onChange={(e) => void patchSettings({ appearance: { uiFontFamily: e.target.value || null } })}
                />
                {settings.appearance.uiFontFamily && (
                  <button type="button" className="ttn-settings-shortcut-reset" title="既定に戻す" onClick={() => void patchSettings({ appearance: { uiFontFamily: null } })}>
                    ↺
                  </button>
                )}
              </span>
            </label>
            <label className="ttn-settings-row">
              <span>ツリーの行間 ({settings.appearance.treeRowLineHeight.toFixed(1)})</span>
              <input
                type="range"
                min={1.0}
                max={2.0}
                step={0.1}
                value={settings.appearance.treeRowLineHeight}
                onChange={(e) => void patchSettings({ appearance: { treeRowLineHeight: Number(e.target.value) } })}
              />
            </label>
          </section>

          <section className="ttn-settings-section">
            <h3>リンク</h3>
            <label className="ttn-settings-row">
              <span>開くトリガー</span>
              <select
                value={settings.links.openTrigger}
                onChange={(e) => void patchSettings({ links: { openTrigger: e.target.value as AppSettings["links"]["openTrigger"] } })}
              >
                <option value="singleClick">シングルクリック</option>
                <option value="ctrlClick">Ctrl+クリック</option>
                <option value="doubleClick">ダブルクリック</option>
              </select>
            </label>
            <label className="ttn-settings-row ttn-settings-row-checkbox">
              <input
                type="checkbox"
                checked={settings.links.confirmBeforeOpen}
                onChange={(e) => void patchSettings({ links: { confirmBeforeOpen: e.target.checked } })}
              />
              <span>開く前に確認する</span>
            </label>
          </section>

          <section className="ttn-settings-section">
            <h3>スタートアップ</h3>
            <label className="ttn-settings-row ttn-settings-row-checkbox">
              <input
                type="checkbox"
                checked={settings.autostart.enabled}
                disabled={autostartBusy}
                onChange={(e) => void handleAutostartToggle(e.target.checked)}
              />
              <span>Windowsログイン時にトレイへ自動起動する</span>
            </label>
            {autostartError && <p className="ttn-settings-error">{autostartError}</p>}
          </section>
        </div>
      )}

      {tab === "shortcuts" && (
        <div className="ttn-settings-body">
          <section className="ttn-settings-section">
            <h3>ショートカット</h3>
            <div className="ttn-settings-shortcut-list">
              {REBINDABLE_ACTION_IDS.map((actionId) => {
                const isOverridden = settings.shortcuts[actionId] !== undefined;
                return (
                  <div className="ttn-settings-shortcut-row" key={actionId}>
                    <span className="ttn-settings-shortcut-label">{SHORTCUT_LABELS[actionId] ?? actionId}</span>
                    <button type="button" className="ttn-settings-shortcut-combo" onClick={() => startRecording(actionId)}>
                      {/* `favorites.open`-style unbound actions (`shared/shortcuts.ts`'s
                          `UNBOUND_ACTION_IDS`) resolve to `undefined` here unless user-bound. */}
                      {recordingActionId === actionId ? "キー入力待ち..." : (effectiveShortcuts[actionId] ?? "未設定")}
                    </button>
                    {isOverridden && (
                      <button type="button" className="ttn-settings-shortcut-reset" title="既定に戻す" onClick={() => resetShortcut(actionId)}>
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
            <h3>エディタの既定値</h3>
            <label className="ttn-settings-row">
              <span>折り返し</span>
              <select
                value={settings.editorDefaults.wrap}
                onChange={(e) => void patchSettings({ editorDefaults: { wrap: e.target.value as AppSettings["editorDefaults"]["wrap"] } })}
              >
                <option value="window">ウィンドウ幅で折り返す</option>
                <option value="chars">文字数で折り返す</option>
                <option value="none">折り返さない</option>
              </select>
            </label>
            <label className="ttn-settings-row">
              <span>タブ幅</span>
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
              <span>タブをスペースとして挿入する</span>
            </label>
            <label className="ttn-settings-row ttn-settings-row-checkbox">
              <input
                type="checkbox"
                checked={settings.editorDefaults.showLineNumbers}
                onChange={(e) => void patchSettings({ editorDefaults: { showLineNumbers: e.target.checked } })}
              />
              <span>行番号を表示する</span>
            </label>
            <label className="ttn-settings-row">
              <span>フォントサイズ</span>
              <input
                type="number"
                min={8}
                max={40}
                value={settings.editorDefaults.fontSize}
                onChange={(e) => void patchSettings({ editorDefaults: { fontSize: Number(e.target.value) } })}
              />
            </label>
            <label className="ttn-settings-row">
              <span>新規メモの既定のペイン表示</span>
              <select
                value={settings.editorDefaults.paneMode}
                onChange={(e) => void patchSettings({ editorDefaults: { paneMode: e.target.value as AppSettings["editorDefaults"]["paneMode"] } })}
              >
                <option value="edit">編集のみ</option>
                <option value="split">分割表示</option>
                <option value="preview">プレビューのみ</option>
              </select>
            </label>
          </section>

          <section className="ttn-settings-section">
            <h3>プレビュー</h3>
            <label className="ttn-settings-row">
              <span>プレビュー再描画の遅延 (ミリ秒)</span>
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
            <h3>自動保存</h3>
            <label className="ttn-settings-row">
              <span>自動保存の遅延 (ミリ秒)</span>
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
            <h3>改行コード</h3>
            <label className="ttn-settings-row">
              <span>保存時のポリシー</span>
              <select value={settings.eol.policy} onChange={(e) => void patchSettings({ eol: { policy: e.target.value as AppSettings["eol"]["policy"] } })}>
                <option value="preserve">読み込み時の形式を維持</option>
                <option value="crlf">CRLF</option>
                <option value="lf">LF</option>
                <option value="cr">CR</option>
              </select>
            </label>
            <label className="ttn-settings-row">
              <span>文字コード</span>
              <span>UTF-8（固定）</span>
            </label>
          </section>
        </div>
      )}
    </Dialog>
  );
}
