// 切り抜きの形の輪郭のパス。

import assert from "node:assert/strict";
import { test } from "node:test";
import { shapeOutline } from "../src/cropShape.ts";

const area = { x: 10, y: 20, width: 200, height: 100 };

test("円は短辺を直径とする中央の円", () => {
  assert.equal(shapeOutline("circle", 0, area), "M60 70a50 50 0 1 0 100 0a50 50 0 1 0 -100 0Z");
});

test("角丸は短辺に対する % の半径（50% まで）", () => {
  assert.equal(
    shapeOutline("rounded", 10, area),
    "M20 20H200A10 10 0 0 1 210 30V110A10 10 0 0 1 200 120H20A10 10 0 0 1 10 110V30A10 10 0 0 1 20 20Z",
  );
  assert.equal(shapeOutline("rounded", 80, area), shapeOutline("rounded", 50, area));
});

test("矩形・半径 0 の角丸は輪郭なし", () => {
  assert.equal(shapeOutline("rectangle", 30, area), null);
  assert.equal(shapeOutline("rounded", 0, area), null);
});
