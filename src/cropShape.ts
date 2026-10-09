// 切り抜きの形（角丸・円）の輪郭の SVG のパス。画面の部品に依存しない。

import type { CropRect, ShapeType } from "./types";

/** 形をかける範囲 area（画面の座標）の、形の輪郭のパス。矩形・半径 0 の角丸なら null。cornerRadius は短辺に対する %（50 まで）。 */
export function shapeOutline(shape: ShapeType, cornerRadius: number, area: CropRect): string | null {
  const short = Math.min(area.width, area.height);
  if (shape === "circle") {
    // 中央の、短辺を直径とする正円
    const [cx, cy, radius] = [area.x + area.width / 2, area.y + area.height / 2, short / 2];
    return `M${cx - radius} ${cy}a${radius} ${radius} 0 1 0 ${2 * radius} 0a${radius} ${radius} 0 1 0 ${-2 * radius} 0Z`;
  }
  if (shape === "rounded" && cornerRadius > 0) {
    const r = (short * Math.min(cornerRadius, 50)) / 100;
    const { x, y, width, height } = area;
    return (
      `M${x + r} ${y}H${x + width - r}A${r} ${r} 0 0 1 ${x + width} ${y + r}V${y + height - r}` +
      `A${r} ${r} 0 0 1 ${x + width - r} ${y + height}H${x + r}A${r} ${r} 0 0 1 ${x} ${y + height - r}` +
      `V${y + r}A${r} ${r} 0 0 1 ${x + r} ${y}Z`
    );
  }
  return null;
}
