// 履歴の一覧の説明（src/historyLabels.ts）。

import assert from "node:assert/strict";
import { test } from "node:test";
import { describeChange, describeHistory } from "../src/historyLabels.ts";
import { History } from "../src/history.ts";

const state = (settings: Record<string, unknown>, aspect: unknown = "free", size: unknown = 1) => ({ settings, aspect, size });

test("変えた項目の名前を並べる", () => {
  const base = state({ exposure: 0, contrast: 0, filter: "none" });
  assert.equal(describeChange(null, base), "開いたとき");
  assert.equal(describeChange(base, state({ exposure: 1, contrast: 0, filter: "none" })), "露出");
  assert.equal(describeChange(base, state({ exposure: 1, contrast: 5, filter: "none" })), "露出・コントラスト");
  assert.equal(describeChange(base, state({ exposure: 0, contrast: 0, filter: "none" }, "square")), "切り抜きの比");
  assert.equal(describeChange(base, state({ exposure: 0, contrast: 0, filter: "none", unknownKey: 1 })), "加工");
  // 多ければ「ほか」
  const many = state({ exposure: 1, contrast: 1, filter: "sepia", dehaze: 3 });
  assert.equal(describeChange(state({ exposure: 0, contrast: 0, filter: "none", dehaze: 0 }), many), "露出・コントラスト・テイスト ほか");
  // ジオラマの項目はまとめて 1 つ
  assert.equal(describeChange(state({ dioramaBlur: 0, dioramaWidth: 1 }), state({ dioramaBlur: 5, dioramaWidth: 2 })), "ジオラマ");
});

test("履歴の一覧と、選んだ時点へ移る", () => {
  const h = new History(0);
  for (const v of [1, 2, 3]) h.push(v);
  h.undo();
  assert.deepEqual(h.entries(), { states: [0, 1, 2, 3], current: 2 });
  assert.equal(h.goto(0), 0);
  assert.deepEqual(h.entries(), { states: [0, 1, 2, 3], current: 0 });
  assert.equal(h.goto(3), 3);
  assert.equal(h.goto(9), 3, "範囲の外なら今の状態");
  const labels = describeHistory([state({ exposure: 0 }), state({ exposure: 1 })]);
  assert.deepEqual(labels, ["開いたとき", "露出"]);
});
