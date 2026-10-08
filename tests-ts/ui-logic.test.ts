// 画面の、部品に依存しない計算（100% 表示の位置・受け取るバイト列・ヒストグラム・キー・出力の欄）。

import assert from "node:assert/strict";
import { test } from "node:test";
import { histogramPeak } from "../src/histogram.ts";
import { isCompareKey } from "../src/keys.ts";
import { longSideState, SIZE_PRESETS, sizeSnapshot } from "../src/output.ts";
import { readImages, readPreview, readRawImage } from "../src/protocol.ts";
import { centeredOffset, clampOffset } from "../src/zoom.ts";

test("100% 表示: 表示より小さい向きは中央にそろえる", () => {
  assert.deepEqual(clampOffset(-500, 30, { width: 200, height: 100 }, { width: 800, height: 600 }), { x: 300, y: 250 });
});

test("100% 表示: 大きい向きは、端が表示の内側に入らないよう止める", () => {
  const image = { width: 2000, height: 1500 };
  const area = { width: 800, height: 600 };
  assert.deepEqual(clampOffset(100, 50, image, area), { x: 0, y: 0 }); // 右・下に動かし過ぎ
  assert.deepEqual(clampOffset(-5000, -5000, image, area), { x: -1200, y: -900 }); // 左・上に動かし過ぎ
  assert.deepEqual(clampOffset(-300, -200, image, area), { x: -300, y: -200 }); // 範囲の中はそのまま
});

test("100% 表示: 画像の点を表示の中央に置く（Retina では 2 画素が 1 論理ピクセル）", () => {
  assert.deepEqual(centeredOffset([1000, 600], { width: 800, height: 600 }, 2), { x: -100, y: 0 });
  assert.deepEqual(centeredOffset([100, 100], { width: 800, height: 600 }, 1), { x: 300, y: 200 });
});

/** u32 リトルエンディアンの並びを作る。 */
function u32s(values: number[]): Uint8Array {
  const bytes = new Uint8Array(values.length * 4);
  const view = new DataView(bytes.buffer);
  values.forEach((v, i) => view.setUint32(i * 4, v, true));
  return bytes;
}

test("プレビューのバイト列: 幅・高さ・時間・画素・ヒストグラム", () => {
  const pixels = Uint8Array.from({ length: 2 * 1 * 4 }, (_, i) => i + 1);
  const histogram = Array.from({ length: 4 * 256 }, (_, i) => (i % 256 === 7 ? i : 0));
  const buffer = new Uint8Array([...u32s([2, 1, 1500]), ...pixels, ...u32s(histogram)]).buffer;
  const preview = readPreview(buffer);
  assert.deepEqual([preview.width, preview.height, preview.renderMs], [2, 1, 1.5]);
  assert.deepEqual([...preview.pixels], [...pixels]);
  assert.equal(preview.histogram.length, 4);
  assert.deepEqual(preview.histogram.map((c) => c[7]), [7, 256 + 7, 512 + 7, 768 + 7]);
  assert.equal(preview.histogram[0][8], 0);
});

test("原寸の画像のバイト列: 幅・高さ・画素", () => {
  const buffer = new Uint8Array([...u32s([1, 2]), 9, 8, 7, 6, 5, 4, 3, 2]).buffer;
  const image = readRawImage(buffer);
  assert.deepEqual([image.width, image.height, [...image.pixels]], [1, 2, [9, 8, 7, 6, 5, 4, 3, 2]]);
});

test("テイストの一覧の見本のバイト列: 数・見本ごとの幅・高さ・画素", () => {
  const buffer = new Uint8Array([...u32s([2, 1, 1]), 1, 2, 3, 4, ...u32s([2, 1]), 5, 6, 7, 8, 9, 10, 11, 12]).buffer;
  const images = readImages(buffer);
  assert.deepEqual(
    images.map((i) => [i.width, i.height, [...i.pixels]]),
    [
      [1, 1, [1, 2, 3, 4]],
      [2, 1, [5, 6, 7, 8, 9, 10, 11, 12]],
    ],
  );
});

test("ヒストグラムの高さの基準は、両端（0・255）を除いた最大（最小 1）", () => {
  const channel = (values: Record<number, number>) => Uint32Array.from({ length: 256 }, (_, i) => values[i] ?? 0);
  assert.equal(histogramPeak([channel({ 0: 900, 255: 800, 10: 5 }), channel({ 20: 7 }), channel({}), channel({})]), 7);
  assert.equal(histogramPeak([channel({ 0: 9 }), channel({}), channel({}), channel({})]), 1);
});

test("加工前の表示のキー: \\ と JIS 配列の ¥", () => {
  assert.equal(isCompareKey({ key: "\\", code: "Backslash" }), true);
  assert.equal(isCompareKey({ key: "¥", code: "IntlYen" }), true);
  assert.equal(isCompareKey({ key: "|", code: "Backslash" }), true); // ⇧ を押していても同じ位置のキー
  assert.equal(isCompareKey({ key: "a", code: "KeyA" }), false);
});

test("出力の欄の履歴の状態: 手で変えていなければ幅・高さを持たない", () => {
  const untouched = { width: 3000, height: 2000, edited: false, last: "width" as const, keepAspect: true };
  assert.deepEqual(sizeSnapshot(untouched), { ...untouched, width: 1, height: 1 });
  // 読み込み直後に欄が埋まっても、同じ状態とみなす
  assert.deepEqual(sizeSnapshot({ ...untouched, width: 4000, height: 3000 }), sizeSnapshot(untouched));
  const edited = { ...untouched, edited: true, width: 800, height: 533 };
  assert.deepEqual(sizeSnapshot(edited), edited);
});

test("よく使う大きさ: 横長なら幅、縦長なら高さを長辺にし、比を保つ", () => {
  const landscape = { width: 4000, height: 3000, edited: false, last: "height" as const, keepAspect: false };
  assert.deepEqual(longSideState(landscape, 1080), { width: 1080, height: 3000, edited: true, last: "width", keepAspect: true });
  const portrait = { ...landscape, width: 2000, height: 3000 };
  assert.deepEqual(longSideState(portrait, 1600), { width: 2000, height: 1600, edited: true, last: "height", keepAspect: true });
  assert.ok(SIZE_PRESETS.every(([, px]) => Number.isInteger(px) && px > 0));
});
