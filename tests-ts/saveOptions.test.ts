// 保存の設定の読み込み（壊れた値・古い形）と、Rust に渡す形。

import assert from "node:assert/strict";
import { test } from "node:test";
import { parseStored, toSaveOptions } from "../src/saveOptions.ts";

const DEFAULT = { quality: 90, keepExif: true, keepGps: false, limit: false, limitMb: 1 };

test("覚えておいた設定がなければ・壊れていれば既定値", () => {
  assert.deepEqual(parseStored(null), DEFAULT);
  assert.deepEqual(parseStored("text"), DEFAULT);
  assert.deepEqual(parseStored({ quality: 0, keepExif: "yes", limitMb: 500 }), DEFAULT);
  assert.deepEqual(parseStored({ quality: 70.5 }), DEFAULT);
});

test("古い形（大きさの上限がない）も読む", () => {
  assert.deepEqual(parseStored({ quality: 75, keepExif: false, keepGps: true }), {
    ...DEFAULT,
    quality: 75,
    keepExif: false,
    keepGps: true,
  });
});

test("大きさの上限は KB にして渡す（外していれば null）", () => {
  const stored = parseStored({ limit: true, limitMb: 1.5 });
  assert.equal(toSaveOptions(stored).maxKb, 1536);
  assert.equal(toSaveOptions({ ...stored, limit: false }).maxKb, null);
  assert.equal(toSaveOptions({ ...stored, limitMb: 0.0001 }).maxKb, 1);
  assert.equal("limit" in toSaveOptions(stored), false);
});
