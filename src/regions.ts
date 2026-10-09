// 投稿加工の範囲の計算（画面の部品に依存しない。座標はどれも回転・反転した後の原寸の画像の座標）。

import type { CropRect } from "./types";

export type Point = [number, number];

/** 2 点を対角とする範囲（画像の中に収める）。 */
export function rectFromPoints(a: Point, b: Point, size: Point): CropRect {
  const clampX = (v: number) => Math.min(Math.max(Math.round(v), 0), size[0]);
  const clampY = (v: number) => Math.min(Math.max(Math.round(v), 0), size[1]);
  const [x1, x2] = [clampX(a[0]), clampX(b[0])].sort((p, q) => p - q);
  const [y1, y2] = [clampY(a[1]), clampY(b[1])].sort((p, q) => p - q);
  return { x: x1, y: y1, width: x2 - x1, height: y2 - y1 };
}

/** 範囲を (dx, dy) だけ動かす（画像からはみ出さないよう止める）。 */
export function moveRect(rect: CropRect, dx: number, dy: number, size: Point): CropRect {
  const x = Math.min(Math.max(Math.round(rect.x + dx), 0), Math.max(size[0] - rect.width, 0));
  const y = Math.min(Math.max(Math.round(rect.y + dy), 0), Math.max(size[1] - rect.height, 0));
  return { ...rect, x, y };
}

/** 原寸 1px が画面の何 px か（横・縦）。shown は表示の大きさ、size は原寸の大きさ。 */
export function screenScale(shown: Point, size: Point): Point {
  return [shown[0] / size[0], shown[1] / size[1]];
}

/** 範囲を横・縦それぞれの倍率で拡大・縮小する（原寸の座標を画面の座標にするなど）。 */
export function scaleRect(rect: CropRect, [sx, sy]: Point): CropRect {
  return { x: rect.x * sx, y: rect.y * sy, width: rect.width * sx, height: rect.height * sy };
}

/** 2 点が横・縦とも hit 以内か（ハンドルをつかんだか）。 */
export function nearPoint([ax, ay]: Point, [bx, by]: Point, hit: number): boolean {
  return Math.abs(ax - bx) <= hit && Math.abs(ay - by) <= hit;
}

/** 左上・右上・右下・左下の順の角。 */
export function rectCorners(r: CropRect): Point[] {
  return [
    [r.x, r.y],
    [r.x + r.width, r.y],
    [r.x + r.width, r.y + r.height],
    [r.x, r.y + r.height],
  ];
}

/** 点を含む範囲のうち、上に重なっている（後に足した）ものの番号。なければ null。 */
export function hitRegion(rects: CropRect[], point: Point): number | null {
  for (let i = rects.length - 1; i >= 0; i--) {
    const r = rects[i];
    if (point[0] >= r.x && point[0] <= r.x + r.width && point[1] >= r.y && point[1] <= r.y + r.height) return i;
  }
  return null;
}
