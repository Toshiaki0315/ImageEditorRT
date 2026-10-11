// キーボードショートカットの書き方（src/shortcutKeys.ts）。

import assert from "node:assert/strict";
import { test } from "node:test";
import { acceleratorFromKey, displayAccelerator } from "../src/shortcutKeys.ts";

const key = (code: string, mods: Partial<Record<"metaKey" | "ctrlKey" | "altKey" | "shiftKey", boolean>> = {}) => ({
  code,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

test("押したキーを書き方にする", () => {
  assert.equal(acceleratorFromKey(key("KeyK", { metaKey: true, shiftKey: true })), "CmdOrCtrl+Shift+K");
  assert.equal(acceleratorFromKey(key("Digit1", { metaKey: true, altKey: true })), "CmdOrCtrl+Alt+1");
  assert.equal(acceleratorFromKey(key("BracketLeft", { metaKey: true })), "CmdOrCtrl+[");
  assert.equal(acceleratorFromKey(key("F5")), "F5");
  assert.equal(acceleratorFromKey(key("ShiftLeft", { shiftKey: true })), null, "修飾キーだけは null");
  assert.equal(acceleratorFromKey(key("IntlYen", { metaKey: true })), null);
});

test("メニューと同じ記号で表示する", () => {
  assert.equal(displayAccelerator("CmdOrCtrl+Shift+K"), "⇧⌘K");
  assert.equal(displayAccelerator("Alt+CmdOrCtrl+Z"), "⌥⌘Z");
  assert.equal(displayAccelerator("CmdOrCtrl+Shift+/"), "⇧⌘/");
  assert.equal(displayAccelerator("F5"), "F5");
  assert.equal(displayAccelerator(""), "なし");
});
