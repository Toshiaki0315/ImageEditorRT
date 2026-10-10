// プレビューの色空間（旧版にはない）。Display P3 の写真は、画素を Display P3 として描く（鮮やかな色を落とさない）。
// canvas はどれも Display P3 で作り、画素（ImageData）には開いている画像の色空間を付ける（描くときに変換される）。

/** Rust の color_space::ColorSpace。 */
export type ColorSpaceName = "srgb" | "display_p3";

/** 開いている画像の画素の色空間 */
let current: PredefinedColorSpace = "srgb";

/** 画像を開いたとき: 画素の色空間を覚える。 */
export function setImageColorSpace(space: ColorSpaceName | undefined) {
  current = space === "display_p3" ? "display-p3" : "srgb";
}

/** 開いている画像の画素（RGBA）を ImageData にする（色空間を付ける）。 */
export function imageData(pixels: Uint8ClampedArray<ArrayBuffer>, width: number, height: number): ImageData {
  return new ImageData(pixels, width, height, { colorSpace: current });
}

/** Display P3 で描く canvas の 2D の描き先（sRGB の画素も正しく描ける）。 */
export function context2d(canvas: HTMLCanvasElement | OffscreenCanvas): CanvasRenderingContext2D {
  return canvas.getContext("2d", { colorSpace: "display-p3" }) as CanvasRenderingContext2D;
}
