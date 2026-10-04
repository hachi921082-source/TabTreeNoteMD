// アプリ自身が開いたファイル選択ダイアログ(ツールバーの画像挿入)が表示中かどうかのゲート
// (GitHub issue #274, `[spec v1.2.0 §11]`)。`state/useTabLockAutoRelock.ts` の OS フォーカス喪失
// トリガーだけがこれを参照し、ダイアログ表示中のフォーカス喪失では再ロックしない。
// 無操作タイマーなど他の再ロック条件には一切影響しない。
//
// `modalActivityGate.ts` と同じくモジュールレベルのカウンタ。加えて、例外が終わった瞬間に
// ウィンドウが非フォーカスなら抑止した再ロックを適用できるよう、終了通知の購読口を持つ。
// imagePaste.ts を再ロックフックの内部に結合させないための仕組み。
//
// 例外は 1 回の表示につき最長 2 分 (`FILE_PICKER_MAX_MS`)。ダイアログを開いたまま離席した場合に
// 例外が残り続けないよう、上限到達で終了扱いにする(終了通知経由で、非フォーカスなら再ロック)。
let count = 0;
const exitListeners = new Set<() => void>();

/** 例外の時間上限(2 分)。 */
const FILE_PICKER_MAX_MS = 2 * 60 * 1000;

/** `input.click()` の直前に呼ぶ。戻り値はその表示専用の終了関数(冪等)。ダイアログ終了時
 * (change / cancel / フォーカス復帰)に呼ぶ。上限時間到達時は自動で呼ばれ、その後の呼び出しは
 * 何もしない — 新しい別の表示のカウントには影響しない。 */
export function enterFilePicker(): () => void {
  count += 1;
  let done = false;
  const exit = () => {
    if (done) return;
    done = true;
    window.clearTimeout(timer);
    count = Math.max(0, count - 1);
    if (count === 0) for (const listener of exitListeners) listener();
  };
  const timer = window.setTimeout(exit, FILE_PICKER_MAX_MS);
  return exit;
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
