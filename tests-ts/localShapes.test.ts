// 部分補正の範囲の当たり判定と線の位置。

import assert from "node:assert/strict";
import { test } from "node:test";
import { bandLines, distanceToSegment, insideEllipse, moveShape, rectBetween, shapeLabel } from "../src/localShapes.ts";

test("楕円の中か", () => {
  const rect = { x: 0, y: 0, width: 100, height: 50 };
  assert.ok(insideEllipse(rect, [50, 25]));
  assert.ok(insideEllipse(rect, [99, 25]));
  assert.ok(!insideEllipse(rect, [5, 5]), "四隅は外");
  assert.ok(!insideEllipse({ x: 0, y: 0, width: 0, height: 10 }, [0, 5]));
});

test("線分までの距離", () => {
  assert.equal(distanceToSegment([0, 0], [10, 0], [5, 3]), 3);
  assert.equal(distanceToSegment([0, 0], [10, 0], [13, 4]), 5, "端の先は端からの距離");
  assert.equal(distanceToSegment([2, 2], [2, 2], [5, 6]), 5);
});

test("帯の線は向きと直角に、枠を横切る", () => {
  const lines = bandLines([50, 0], [50, 40], 100, 100);
  assert.equal(lines.length, 2);
  for (const [[x1, y1], [x2, y2]] of lines) {
    assert.equal(y1, y2, "縦の帯なら横の線");
    assert.ok(Math.min(x1, x2) < 0 && Math.max(x1, x2) > 100, "枠いっぱい");
  }
  assert.deepEqual([lines[0][0][1], lines[1][0][1]], [0, 40]);
  assert.deepEqual(bandLines([5, 5], [5, 5], 10, 10), []);
});

test("動かす・2 点の範囲・名前", () => {
  assert.deepEqual(moveShape({ kind: "ellipse", rect: { x: 1, y: 2, width: 3, height: 4 } }, 10.4, -1.6), {
    kind: "ellipse",
    rect: { x: 11, y: 0, width: 3, height: 4 },
  });
  assert.deepEqual(moveShape({ kind: "band", from: [0, 0], to: [0, 10] }, 5, 5), { kind: "band", from: [5, 5], to: [5, 15] });
  assert.deepEqual(rectBetween([30, 5], [10, 25]), { x: 10, y: 5, width: 20, height: 20 });
  assert.equal(shapeLabel({ kind: "band", from: [0, 0], to: [1, 1] }, 1), "2. 帯");
});
