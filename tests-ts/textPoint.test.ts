// 文字・ロゴの自由な位置の計算。

import assert from "node:assert/strict";
import { test } from "node:test";
import { fromFraction, nearest, toFraction } from "../src/textPoint.ts";

const area = { x: 100, y: 50, width: 400, height: 200 };

test("写真の範囲に対する割合と点の行き来（範囲の外は端）", () => {
  assert.deepEqual(toFraction([200, 150], area), [0.25, 0.5]);
  assert.deepEqual(fromFraction([0.25, 0.5], area), [200, 150]);
  assert.deepEqual(toFraction([0, 900], area), [0, 1]);
});

test("押した点にいちばん近いもの", () => {
  assert.equal(nearest([[0, 0], [100, 100]], [90, 80]), 1);
  assert.equal(nearest([[0, 0]], [500, 500]), 0);
  assert.equal(nearest([], [0, 0]), null);
});
