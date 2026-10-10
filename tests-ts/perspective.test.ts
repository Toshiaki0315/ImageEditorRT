// 遠近の補正の向きを、回転・反転に合わせて直す。

import assert from "node:assert/strict";
import { test } from "node:test";
import { orientPerspective } from "../src/perspective.ts";

test("上が細い台形（縦 +30）は、回すと細い辺の場所に合わせて縦横が入れ替わる", () => {
  // 右に回すと上の辺は右 → 右が細い（横 -30）
  assert.deepEqual(orientPerspective("rotate_right", 30, 0), [0, -30]);
  // 左に回すと上の辺は左 → 左が細い（横 +30）
  assert.deepEqual(orientPerspective("rotate_left", 30, 0), [0, 30]);
  // 左が細い（横 +20）を右に回すと上が細い（縦 +20）
  assert.deepEqual(orientPerspective("rotate_right", 0, 20), [20, 0]);
  // 4 回まわすと元に戻る
  let state: [number, number] = [30, -10];
  for (let i = 0; i < 4; i++) state = orientPerspective("rotate_right", ...state);
  assert.deepEqual(state, [30, -10]);
});

test("反転は、その向きの符号だけ入れ替える", () => {
  assert.deepEqual(orientPerspective("flip_horizontal", 30, 20), [30, -20]);
  assert.deepEqual(orientPerspective("flip_vertical", 30, 20), [-30, 20]);
  assert.deepEqual(orientPerspective("flip_vertical", 0, 0), [0, 0]);
});
