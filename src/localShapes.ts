// 部分補正の範囲（円・帯）の当たり判定と、プレビューに描く線の位置。画面の部品に依存しない。
// 座標はどれも同じ座標系（画面の px か、原寸の px）で渡す。

import type { CropRect, LocalShape } from "./types";

export type Point = [number, number];

/** 点が楕円（rect に内接）の中か。 */
export function insideEllipse(rect: CropRect, [x, y]: Point): boolean {
  const rx = rect.width / 2;
  const ry = rect.height / 2;
  if (rx <= 0 || ry <= 0) return false;
  const dx = (x - rect.x - rx) / rx;
  const dy = (y - rect.y - ry) / ry;
  return dx * dx + dy * dy <= 1;
}

/** 点から線分 a–b までの距離。 */
export function distanceToSegment([ax, ay]: Point, [bx, by]: Point, [px, py]: Point): number {
  const [vx, vy] = [bx - ax, by - ay];
  const length = vx * vx + vy * vy;
  const t = length === 0 ? 0 : Math.min(Math.max(((px - ax) * vx + (py - ay) * vy) / length, 0), 1);
  return Math.hypot(px - (ax + vx * t), py - (ay + vy * t));
}

/**
 * 帯の始まり・終わりを示す線（帯の向きと直角に、width×height の枠いっぱいに引く）。
 * 始まりの線（ここまで全部効く）と終わりの線（ここで効かなくなる）を返す。向きがなければ空。
 */
export function bandLines(from: Point, to: Point, width: number, height: number): [Point, Point][] {
  const [vx, vy] = [to[0] - from[0], to[1] - from[1]];
  const length = Math.hypot(vx, vy);
  if (length === 0) return [];
  // 直角の向き。枠の対角線より長く伸ばせば、どこにあっても枠を横切る
  const [nx, ny] = [-vy / length, vx / length];
  const reach = Math.hypot(width, height);
  return [from, to].map(([x, y]): [Point, Point] => [
    [x - nx * reach, y - ny * reach],
    [x + nx * reach, y + ny * reach],
  ]);
}

/** 範囲の形を、ずらした形にする（ドラッグで動かす）。 */
export function moveShape(shape: LocalShape, dx: number, dy: number): LocalShape {
  const r = Math.round;
  if (shape.kind === "ellipse") {
    const { x, y, width, height } = shape.rect;
    return { kind: "ellipse", rect: { x: r(x + dx), y: r(y + dy), width, height } };
  }
  return {
    kind: "band",
    from: [r(shape.from[0] + dx), r(shape.from[1] + dy)],
    to: [r(shape.to[0] + dx), r(shape.to[1] + dy)],
  };
}

/** 2 点を対角とする範囲（ドラッグで円の大きさを決める）。 */
export function rectBetween([ax, ay]: Point, [bx, by]: Point): CropRect {
  const x = Math.round(Math.min(ax, bx));
  const y = Math.round(Math.min(ay, by));
  return { x, y, width: Math.round(Math.max(ax, bx)) - x, height: Math.round(Math.max(ay, by)) - y };
}

/** 一覧に出す名前（1 から数える）。 */
export function shapeLabel(shape: LocalShape, index: number): string {
  return `${index + 1}. ${shape.kind === "ellipse" ? "円" : "帯"}`;
}
