// トーンカーブの計算（Rust の curve::curve_row と同じ値になるか）。

import assert from "node:assert/strict";
import { test } from "node:test";
import { addPoint, type CurvePoint, curveRow, identityCurve, movePoint } from "../src/curve.ts";

const at = (points: CurvePoint[]) => {
  const row = curveRow(points);
  return [0, 32, 64, 100, 128, 140, 192, 230, 255].map((x) => row[x]);
};

test("Rust と同じ曲線になる", () => {
  // Rust の curve_row で求めた値
  assert.deepEqual(
    at([
      [0, 0],
      [64, 100],
      [192, 230],
      [255, 255],
    ]),
    [0, 52, 100, 144, 174, 186, 230, 247, 255],
  );
  assert.deepEqual(
    at([
      [0, 30],
      [128, 90],
      [255, 220],
    ]),
    [30, 43, 56, 72, 90, 100, 151, 193, 220],
  );
  assert.deepEqual(
    at([
      [0, 0],
      [100, 120],
      [150, 120],
      [255, 255],
    ]),
    [0, 47, 94, 120, 120, 120, 155, 217, 255],
  );
  assert.deepEqual(curveRow(identityCurve()), Array.from({ length: 256 }, (_, i) => i));
});

test("点を足す・動かす", () => {
  const points = identityCurve();
  assert.equal(addPoint(points, [100.4, 140]), 1);
  assert.deepEqual(points[1], [100, 140]);
  assert.equal(addPoint(points, [102, 10]), null); // 近すぎる
  assert.equal(addPoint(points, [0, 10]), null); // 端
  movePoint(points, 1, [300, -5]); // 右隣（255）より左・0〜255 に収める
  assert.deepEqual(points[1], [254, 0]);
  movePoint(points, 0, [50, 40]); // 端は x を変えない
  assert.deepEqual(points[0], [0, 40]);
});
