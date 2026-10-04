// アプリ自身が開いたファイル選択ダイアログ(ツールバーの画像挿入)が表示中かどうかのゲート
// (GitHub issue #274, `[spec v1.2.0 §11]`)。`state/useTabLockAutoRelock.ts` の OS フォーカス喪失
// トリガーだけがこれを参照し、ダイアログ表示中のフォーカス喪失では再ロックしない。
// 無操作タイマーなど他の再ロック条件には一切影響しない。
//
// `modalActivityGate.ts` と同じくモジュールレベルのカウンタ。加えて、例外が終わった瞬間に
// ウィンドウが非フォーカスなら抑止した再ロックを適用できるよう、終了通知の購読口を持つ。
// imagePaste.ts を再ロックフックの内部に結合させないための仕組み。
let count = 0;
const exitListeners = new Set<() => void>();

/** `input.click()` の直前に呼ぶ。 */
export function enterFilePicker(): void {
  count += 1;
}

/** ダイアログ終了時(change / cancel / フォーカス復帰のいずれか)に 1 回だけ呼ぶ。`enterFilePicker` と 1:1。 */
export function exitFilePicker(): void {
  count = Math.max(0, count - 1);
  if (count === 0) for (const listener of exitListeners) listener();
}

/** 例外が終わった(カウンタが 0 になった)ときに呼ばれるリスナーを登録。戻り値で解除。 */
export function onFilePickerExit(listener: () => void): () => void {
  exitListeners.add(listener);
  return () => {
    exitListeners.delete(listener);
  };
}

/** アプリ自身のファイル選択ダイアログが表示中なら `true`。 */
export function isFilePickerActive(): boolean {
  return count > 0;
}
