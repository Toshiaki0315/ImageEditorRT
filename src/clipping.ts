// 白飛び・黒つぶれの表示（旧版にはない）: プレビューで、真っ白になった部分を赤、真っ黒になった部分を青で塗って示す。
// 表示だけで、保存する画像は変えない。表示するかは環境設定に残す。
// Node のテスト（tests-ts/）からも読むので、拡張子まで書く

import { readStored, writeStored } from "./storage.ts";

const STORAGE_KEY = "clippingShown";
/** この値以上（すべての色）を白飛び、この値以下（すべての色）を黒つぶれとみなす */
export const WHITE_LEVEL = 254;
export const BLACK_LEVEL = 1;
const WHITE_MARK = [255, 0, 0];
const BLACK_MARK = [0, 80, 255];

let shown = readStored(STORAGE_KEY) === "true";

/** 白飛び・黒つぶれを表示しているか。 */
export const clippingShown = () => shown;

/** 表示を切り替えて、環境設定に残す。 */
export function setClippingShown(value: boolean) {
  shown = value;
  writeStored(STORAGE_KEY, String(value));
}

/** RGBA の画素のうち、白飛び・黒つぶれの所を塗った写し（透明な画素はそのまま）。塗った画素の数も返す。 */
export function markClipping(pixels: Uint8ClampedArray<ArrayBuffer>): { pixels: Uint8ClampedArray<ArrayBuffer>; white: number; black: number } {
  const out = new Uint8ClampedArray(pixels);
  let white = 0;
  let black = 0;
  for (let i = 0; i < out.length; i += 4) {
    if (out[i + 3] === 0) continue;
    const [r, g, b] = [out[i], out[i + 1], out[i + 2]];
    const mark = r >= WHITE_LEVEL && g >= WHITE_LEVEL && b >= WHITE_LEVEL ? WHITE_MARK : r <= BLACK_LEVEL && g <= BLACK_LEVEL && b <= BLACK_LEVEL ? BLACK_MARK : null;
    if (!mark) continue;
    if (mark === WHITE_MARK) white++;
    else black++;
    out[i] = mark[0];
    out[i + 1] = mark[1];
    out[i + 2] = mark[2];
  }
  return { pixels: out, white, black };
}
