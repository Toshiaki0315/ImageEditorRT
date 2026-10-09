// 左右に分けて比べる表示の計算。

import assert from "node:assert/strict";
import { test } from "node:test";
import { fitInside, SPLIT_DEFAULT, splitClip, splitFraction } from "../src/split.ts";

test("境目の位置は画像の幅に対する比率で、はみ出したら端", () => {
  assert.equal(splitFraction(150, 100, 200), 0.25);
  assert.equal(splitFraction(300, 100, 200), 1);
  assert.equal(splitFraction(50, 100, 200), 0);
  assert.equal(splitFraction(400, 100, 200), 1);
  // 幅がない（表示する前）なら真ん中
  assert.equal(splitFraction(10, 0, 0), SPLIT_DEFAULT);
});

test("加工前は境目より左だけ見せる", () => {
  assert.equal(splitClip(0.5), "inset(0 50% 0 0)");
  assert.equal(splitClip(0.25), "inset(0 75% 0 0)");
  assert.equal(splitClip(1), "inset(0 0% 0 0)");
  assert.equal(splitClip(-1), "inset(0 100% 0 0)");
});

test("加工前の画像は縦横比を保って加工後の中央に置く", () => {
  // 同じ大きさならそのまま
  assert.deepEqual(fitInside(400, 300, 400, 300), { x: 0, y: 0, width: 400, height: 300 });
  // フレームで加工後が縦に長い（ポラロイドなど）→ 幅に合わせて上下中央
  assert.deepEqual(fitInside(400, 400, 440, 540), { x: 0, y: 50, width: 440, height: 440 });
  // 縮小版の大きさが違うだけ
  assert.deepEqual(fitInside(800, 600, 400, 300), { x: 0, y: 0, width: 400, height: 300 });
  assert.deepEqual(fitInside(0, 10, 100, 100), { x: 0, y: 0, width: 0, height: 0 });
});
