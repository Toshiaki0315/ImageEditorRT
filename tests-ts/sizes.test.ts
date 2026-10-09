// 複数の大きさで保存の、選択の覚え方と知らせ方。

import assert from "node:assert/strict";
import { test } from "node:test";
import { parseSizes, savedSummary } from "../src/sizes.ts";

const choices = [1080, 1280, 1600, 1920, 3840];

test("残した選択は、よく使う大きさにあるものだけ順に読む", () => {
  assert.deepEqual(parseSizes("[3840, 1080, 999]", choices), [1080, 3840]);
  assert.deepEqual(parseSizes("[]", choices), []);
  // なし・壊れていれば既定（Instagram と X）
  assert.deepEqual(parseSizes(null, choices), [1080, 1600]);
  assert.deepEqual(parseSizes("{", choices), [1080, 1600]);
  assert.deepEqual(parseSizes('"x"', choices), [1080, 1600]);
});

test("保存した名前を並べ、多ければまとめる", () => {
  assert.equal(savedSummary(["/a/p_1080.jpg", "/a/p_1600.jpg"]), "2 つの大きさで保存しました: p_1080.jpg・p_1600.jpg");
  assert.equal(
    savedSummary(["/a/p_1080.jpg", "/a/p_1280.jpg", "/a/p_1600.jpg", "/a/p_1920.jpg"]),
    "4 つの大きさで保存しました: p_1080.jpg・p_1280.jpg ほか 2 件",
  );
});
