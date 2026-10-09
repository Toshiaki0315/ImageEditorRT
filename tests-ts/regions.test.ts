// 投稿加工の範囲の計算。

import assert from "node:assert/strict";
import { test } from "node:test";
import { hitRegion, moveRect, nearPoint, rectCorners, rectFromPoints, scaleRect, screenScale } from "../src/regions.ts";

test("2 点から範囲を作る（逆向きのドラッグ・画像の外は端に収める）", () => {
  assert.deepEqual(rectFromPoints([50, 40], [10, 20], [100, 80]), { x: 10, y: 20, width: 40, height: 20 });
  assert.deepEqual(rectFromPoints([-5, 70], [120, 90], [100, 80]), { x: 0, y: 70, width: 100, height: 10 });
});

test("範囲を動かすときは画像からはみ出さない", () => {
  const rect = { x: 10, y: 10, width: 30, height: 20 };
  assert.deepEqual(moveRect(rect, 5, -3, [100, 80]), { x: 15, y: 7, width: 30, height: 20 });
  assert.deepEqual(moveRect(rect, 500, 500, [100, 80]), { x: 70, y: 60, width: 30, height: 20 });
  assert.deepEqual(moveRect(rect, -500, -500, [100, 80]), { x: 0, y: 0, width: 30, height: 20 });
});

test("押した点の範囲は、上に重なっているほうを選ぶ", () => {
  const rects = [
    { x: 0, y: 0, width: 50, height: 50 },
    { x: 30, y: 30, width: 50, height: 50 },
  ];
  assert.equal(hitRegion(rects, [40, 40]), 1);
  assert.equal(hitRegion(rects, [10, 10]), 0);
  assert.equal(hitRegion(rects, [90, 10]), null);
  assert.deepEqual(rectCorners(rects[0]), [
    [0, 0],
    [50, 0],
    [50, 50],
    [0, 50],
  ]);
});

test("画面と原寸の倍率・範囲の拡大・ハンドルの当たり", () => {
  const scale = screenScale([400, 300], [4000, 3000]);
  assert.deepEqual(scale, [0.1, 0.1]);
  assert.deepEqual(scaleRect({ x: 100, y: 200, width: 1000, height: 500 }, scale), { x: 10, y: 20, width: 100, height: 50 });
  assert.ok(nearPoint([10, 10], [18, 2], 10));
  assert.ok(!nearPoint([10, 10], [21, 10], 10));
});
