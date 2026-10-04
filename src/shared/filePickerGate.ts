// アプリ自身が開いたファイル選択ダイアログ(ツールバーの画像挿入)が表示中かどうかのゲート
// (GitHub issue #274, `[spec v1.2.0 §11]`)。`state/useTabLockAutoRelock.ts` の OS フォーカス喪失
// トリガーだけがこれを参照し、ダイアログ表示中のフォーカス喪失では再ロックしない。
// 無操作タイマーなど他の再ロック条件には一切影響しない。
//
// `modalActivityGate.ts` と同じくモジュールレベルのカウンタ。フォーカス喪失イベント受信時に
// 同期的に読むだけなので購読の仕組みは不要。
let count = 0;

/** `input.click()` の直前に呼ぶ。 */
export function enterFilePicker(): void {
  count += 1;
}

/** ダイアログ終了時(change / cancel / フォーカス復帰のいずれか)に 1 回だけ呼ぶ。`enterFilePicker` と 1:1。 */
export function exitFilePicker(): void {
  count = Math.max(0, count - 1);
}

/** アプリ自身のファイル選択ダイアログが表示中なら `true`。 */
export function isFilePickerActive(): boolean {
  return count > 0;
}
