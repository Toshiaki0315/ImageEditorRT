// 左右に分けて比べる表示の計算（境目の位置・加工前の画像の置き方）。画面の部品に依存しない。

/** 境目の位置の既定値（表示の幅に対する比率）。 */
export const SPLIT_DEFAULT = 0.5;

/** 境目の位置（0〜1）。x は指の位置、left・width は表示している画像の左端と幅（どれも同じ座標）。 */
export function splitFraction(x: number, left: number, width: number): number {
  if (!(width > 0)) return SPLIT_DEFAULT;
  return Math.min(Math.max((x - left) / width, 0), 1);
}

/** 加工前の画像を見せる範囲（右側を切り落とす clip-path）。fraction は境目の位置。 */
export function splitClip(fraction: number): string {
  const hidden = (1 - Math.min(Math.max(fraction, 0), 1)) * 100;
  return `inset(0 ${Number(hidden.toFixed(3))}% 0 0)`;
}

/**
 * 大きさ width×height の加工前の画像を、加工後の画像（targetWidth×targetHeight）の中に縦横比を保って
 * いちばん大きく、中央に置く位置と大きさ（画素に丸める。フレームなどで加工後の大きさ・比が違っても重なるように）。
 */
export function fitInside(
  width: number,
  height: number,
  targetWidth: number,
  targetHeight: number,
): { x: number; y: number; width: number; height: number } {
  if (!(width > 0 && height > 0)) return { x: 0, y: 0, width: 0, height: 0 };
  const scale = Math.min(targetWidth / width, targetHeight / height);
  const w = Math.round(width * scale);
  const h = Math.round(height * scale);
  return { x: Math.round((targetWidth - w) / 2), y: Math.round((targetHeight - h) / 2), width: w, height: h };
}
