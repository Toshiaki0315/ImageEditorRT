// Rust から画素を受け取るときのバイト列の読み方（JSON にしないので受け渡しが速い）。どれもリトルエンディアン。

import { type Histogram, readHistogram } from "./histogram";

/** プレビュー（render_preview）: 幅・高さ・処理の時間 (µs) の u32 が 3 つ、RGBA の画素、ヒストグラム。 */
export type PreviewImage = { width: number; height: number; renderMs: number; pixels: Uint8ClampedArray<ArrayBuffer>; histogram: Histogram };

/** 原寸の画像（render_actual_size）: 幅・高さの u32 が 2 つ、RGBA の画素。 */
export type RawImage = { width: number; height: number; pixels: Uint8ClampedArray<ArrayBuffer> };

export function readPreview(buffer: ArrayBuffer): PreviewImage {
  const header = new DataView(buffer, 0, 12);
  const width = header.getUint32(0, true);
  const height = header.getUint32(4, true);
  const renderMs = header.getUint32(8, true) / 1000;
  const size = width * height * 4;
  return {
    width,
    height,
    renderMs,
    pixels: new Uint8ClampedArray(buffer, 12, size),
    histogram: readHistogram(buffer, 12 + size),
  };
}

export function readRawImage(buffer: ArrayBuffer): RawImage {
  const header = new DataView(buffer, 0, 8);
  const width = header.getUint32(0, true);
  const height = header.getUint32(4, true);
  return { width, height, pixels: new Uint8ClampedArray(buffer, 8, width * height * 4) };
}
