// 白飛び・黒つぶれの表示（src/clipping.ts）。

import assert from "node:assert/strict";
import { test } from "node:test";
import { markClipping } from "../src/clipping.ts";

test("真っ白を赤、真っ黒を青で塗り、ほかはそのまま", () => {
  const pixels = new Uint8ClampedArray([255, 255, 255, 255, 0, 0, 0, 255, 128, 64, 32, 255, 255, 255, 0, 255, 255, 255, 255, 0]);
  const { pixels: out, white, black } = markClipping(pixels);
  assert.deepEqual([...out.slice(0, 4)], [255, 0, 0, 255]);
  assert.deepEqual([...out.slice(4, 8)], [0, 80, 255, 255]);
  assert.deepEqual([...out.slice(8, 12)], [128, 64, 32, 255], "ふつうの色はそのまま");
  assert.deepEqual([...out.slice(12, 16)], [255, 255, 0, 255], "黄色（青が 0）は白飛びではない");
  assert.deepEqual([...out.slice(16, 20)], [255, 255, 255, 0], "透明な画素は塗らない");
  assert.equal(white, 1);
  assert.equal(black, 1);
  assert.deepEqual([...pixels.slice(0, 4)], [255, 255, 255, 255], "元の画素は変えない");
});
