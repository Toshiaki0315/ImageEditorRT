// 編集設定の既定値（src/types.ts の defaultSettings()）が、Rust の EditSettings::default() と同じか。
// 両方が fixtures/default-settings.json と同じかを、それぞれのテストで確かめる（Rust の側は
// crates/core/tests/default_settings.rs）。既定値を変えたときは、両方とこのファイルをそろえる。

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { defaultSettings, defaultText } from "../src/types.ts";

const shared = JSON.parse(readFileSync(new URL("./fixtures/default-settings.json", import.meta.url), "utf8"));

test("編集設定の既定値が Rust と同じ", () => {
  assert.deepEqual(defaultSettings(), shared);
});

test("文字・透かしの既定値が Rust と同じ", () => {
  assert.deepEqual(defaultText(), shared.text);
});

test("既定値は呼ぶたびに新しいもの（書き換えても次の既定値に影響しない）", () => {
  const a = defaultSettings();
  a.text.color[0] = 0;
  a.orientation.rotation = 90;
  assert.deepEqual(defaultSettings(), shared);
});
