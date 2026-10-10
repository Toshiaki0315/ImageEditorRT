// 文字・ロゴの「自由」な位置の計算（画面の部品に依存しない）。座標はどれも同じ座標系で渡す。

import type { CropRect } from "./types";

type Point = [number, number];

/** 点を、写真の範囲 area に対する割合（0〜1 に収める）にする。 */
export function toFraction([x, y]: Point, area: CropRect): Point {
  const clamp = (v: number) => Math.min(Math.max(v, 0), 1);
  return [clamp((x - area.x) / area.width), clamp((y - area.y) / area.height)];
}

/** 割合を、写真の範囲 area の中の点にする。 */
export function fromFraction([fx, fy]: Point, area: CropRect): Point {
  return [area.x + fx * area.width, area.y + fy * area.height];
}

/** 押した点にいちばん近いものの番号（なければ null）。 */
export function nearest(points: Point[], [x, y]: Point): number | null {
  let best: number | null = null;
  let distance = Infinity;
  points.forEach(([px, py], index) => {
    const d = Math.hypot(px - x, py - y);
    if (d < distance) {
      distance = d;
      best = index;
    }
  });
  return best;
}
