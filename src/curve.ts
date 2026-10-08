// トーンカーブの計算（Rust の curve::curve_row と同じ。グラフを描くのに使う）。画面の部品に依存しない。

export type CurvePoint = [number, number];

/** 既定のトーンカーブ（まっすぐ = 変化なし）。 */
export const identityCurve = (): CurvePoint[] => [
  [0, 0],
  [255, 255],
];

/** 0〜255 の各値を曲線に通した値（単調な 3 次補間。Fritsch–Carlson）。 */
export function curveRow(points: CurvePoint[]): number[] {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  const n = xs.length;
  const slopes = xs.slice(0, -1).map((x, i) => (ys[i + 1] - ys[i]) / (xs[i + 1] - x));
  const m = xs.map((_, i) => {
    if (i === 0) return slopes[0];
    if (i === n - 1) return slopes[n - 2];
    return slopes[i - 1] * slopes[i] <= 0 ? 0 : (slopes[i - 1] + slopes[i]) / 2;
  });
  for (let i = 0; i < n - 1; i++) {
    if (slopes[i] === 0) {
      m[i] = 0;
      m[i + 1] = 0;
      continue;
    }
    const a = m[i] / slopes[i];
    const b = m[i + 1] / slopes[i];
    const length = Math.hypot(a, b);
    if (length > 3) {
      m[i] = (3 / length) * a * slopes[i];
      m[i + 1] = (3 / length) * b * slopes[i];
    }
  }
  return Array.from({ length: 256 }, (_, x) => {
    let i = xs.findIndex((_, k) => k < n - 1 && x <= xs[k + 1]);
    if (i < 0) i = n - 2;
    const h = xs[i + 1] - xs[i];
    const t = (x - xs[i]) / h;
    const [t2, t3] = [t * t, t * t * t];
    const y =
      (2 * t3 - 3 * t2 + 1) * ys[i] + (t3 - 2 * t2 + t) * h * m[i] + (-2 * t3 + 3 * t2) * ys[i + 1] + (t3 - t2) * h * m[i + 1];
    return Math.min(255, Math.max(0, Math.round(y)));
  });
}

/**
 * 点を足す（x の順に並べる。両端と同じ x・すでにある点と近すぎる x には足さない）。足した点の番号を返す
 * （足さなければ null）。
 */
export function addPoint(points: CurvePoint[], point: CurvePoint, minGap = 4): number | null {
  const x = Math.round(point[0]);
  if (x <= 0 || x >= 255 || points.some((p) => Math.abs(p[0] - x) < minGap)) return null;
  const index = points.findIndex((p) => p[0] > x);
  points.splice(index, 0, [x, clampByte(point[1])]);
  return index;
}

/** index の点を動かす（両端は x を変えない。ほかの点は両隣のあいだに収める）。 */
export function movePoint(points: CurvePoint[], index: number, point: CurvePoint) {
  const last = points.length - 1;
  const x = index === 0 ? 0 : index === last ? 255 : Math.min(points[index + 1][0] - 1, Math.max(points[index - 1][0] + 1, Math.round(point[0])));
  points[index] = [x, clampByte(point[1])];
}

const clampByte = (v: number) => Math.min(255, Math.max(0, Math.round(v)));
