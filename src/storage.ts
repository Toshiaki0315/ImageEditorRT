// 画面の環境設定（WebView の localStorage）の読み書き。使えない・読めないときも、画面は既定の値で動く。

/** key の値（なければ・読めなければ null）。 */
export function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** key に value を残す（残せなくても、画面の動きはそのまま）。 */
export function writeStored(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 保存先が使えなくても動く
  }
}
