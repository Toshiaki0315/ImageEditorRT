// 「切り抜き」タブのプレビューに重ねる線（範囲の外の暗がり・範囲と形の輪郭・四隅のハンドル・構図のガイド）を描く。
// 範囲の計算・ドラッグは crop.ts。

import { type GuideKind, guideLines } from "./guides";
import { handleElement, svgElement } from "./overlay";
import { rectCorners } from "./regions";
import type { CropRect } from "./types";

/** 描くもの（どれも画面の座標）。 */
export type CropOverlayView = {
  width: number;
  height: number;
  /** トリミング範囲（なければ null） */
  crop: CropRect | null;
  /** 形の輪郭のパス（矩形なら null） */
  outline: string | null;
  /** 構図のガイド（出さないなら null） */
  guide: GuideKind | null;
};

/** svg に範囲・形・ガイドの線を描き直す。 */
export function drawCropOverlay(svg: SVGSVGElement, { width, height, crop, outline, guide }: CropOverlayView) {
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
  svg.replaceChildren();
  // ガイドは範囲（なければ画像全体）の中に、ほかの線より下に描く（影の上に白い線）
  if (guide && guide !== "none") {
    for (const className of ["guide-shadow", "guide-line"]) {
      for (const [x1, y1, x2, y2] of guideLines(guide, crop ?? { x: 0, y: 0, width, height })) {
        svg.append(svgElement("line", className, { x1, y1, x2, y2 }));
      }
    }
  }
  if (!crop && !outline) return;
  // 範囲の外と形の外側を暗くする（形があれば形、なければ範囲の内側だけを明るく残す。旧版 FR-UI-58）
  const hole = outline ?? (crop ? `M${crop.x} ${crop.y}h${crop.width}v${crop.height}h${-crop.width}Z` : "");
  svg.append(svgElement("path", "mask", { d: `M0 0H${width}V${height}H0Z ${hole}` }));
  // 枠と形の輪郭（明るい写真でも暗い写真でも見えるよう、白い線の外側に黒い線）
  for (const kind of ["edge-shadow", "edge"]) {
    if (outline) svg.append(svgElement("path", kind, { d: outline }));
    if (crop) svg.append(svgElement("rect", kind, crop));
  }
  if (crop) for (const [cx, cy] of rectCorners(crop)) svg.append(handleElement(cx, cy));
}
