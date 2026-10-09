// 環境設定の読み書きは、保存先が使えなくても例外を出さない。

import assert from "node:assert/strict";
import { test } from "node:test";
import { readStored, writeStored } from "../src/storage.ts";

const globals = globalThis as { localStorage?: unknown };

test("読み書きできる保存先なら、そのまま読み書きする", () => {
  const data = new Map<string, string>();
  globals.localStorage = { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => data.set(k, v) };
  writeStored("a", "1");
  assert.equal(readStored("a"), "1");
  assert.equal(readStored("b"), null);
  delete globals.localStorage;
});

test("保存先がない・使えなければ、読むと null、書いても何も起きない", () => {
  delete globals.localStorage;
  assert.equal(readStored("a"), null);
  assert.doesNotThrow(() => writeStored("a", "1"));
  globals.localStorage = {
    getItem: () => {
      throw new Error("denied");
    },
    setItem: () => {
      throw new Error("denied");
    },
  };
  assert.equal(readStored("a"), null);
  assert.doesNotThrow(() => writeStored("a", "1"));
  delete globals.localStorage;
});
