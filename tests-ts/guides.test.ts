// 切り抜きのガイド線の位置。

import assert from "node:assert/strict";
import { test } from "node:test";
import { guideLines, parseGuide, straightenGrid } from "../src/guides.ts";

const rect = { x: 10, y: 20, width: 300, height: 150 };
const round = (lines: number[][]) => lines.map((l) => l.map((v) => Math.round(v * 10) / 10));

test("三分割は縦横 2 本ずつ", () => {
  assert.deepEqual(round(guideLines("thirds", rect)), [
    [110, 20, 110, 170],
    [210, 20, 210, 170],
    [10, 70, 310, 70],
    [10, 120, 310, 120],
  ]);
});

test("黄金比は 0.382 と 0.618 の位置", () => {
  assert.deepEqual(round(guideLines("golden", rect)), [
    [124.6, 20, 124.6, 170],
    [195.4, 20, 195.4, 170],
    [10, 77.3, 310, 77.3],
    [10, 112.7, 310, 112.7],
  ]);
});

test("対角線は四隅を結ぶ。なし・大きさがない範囲は線なし", () => {
  assert.deepEqual(guideLines("diagonal", rect), [
    [10, 20, 310, 170],
    [310, 20, 10, 170],
  ]);
  assert.deepEqual(guideLines("none", rect), []);
  assert.deepEqual(guideLines("thirds", { x: 0, y: 0, width: 0, height: 10 }), []);
});

test("環境設定の値は知らなければなし", () => {
  assert.equal(parseGuide("golden"), "golden");
  assert.equal(parseGuide("spiral"), "none");
  assert.equal(parseGuide(null), "none");
});

test("水平の補正の格子は、中心を通る正方形のマス目", () => {
  const lines = straightenGrid(400, 200, 4);
  // 短辺 200 を 4 等分 → 間隔 50。縦は x = 200 を中心に 0〜400、横は y = 100 を中心に 0〜200
  const xs = lines.filter(([x1, , x2]) => x1 === x2).map(([x]) => x);
  const ys = lines.filter(([, y1, , y2]) => y1 === y2).map(([, y]) => y);
  assert.deepEqual(xs, [0, 50, 100, 150, 200, 250, 300, 350, 400]);
  assert.deepEqual(ys, [0, 50, 100, 150, 200]);
  assert.ok(xs.includes(200) && ys.includes(100), "中心を通る");
  assert.deepEqual(straightenGrid(0, 100), []);
});
