// ⌘V で何をするか（src/paste.ts）。本物のクリップボードは使わない。

import assert from "node:assert/strict";
import { test } from "node:test";
import { pasteAction, pasteStamp } from "../src/paste.ts";

const supported = (path: string) => /\.(png|jpe?g|heic)$/i.test(path);

test("入力欄の外: ファイル → 画像 → なし", () => {
  assert.deepEqual(pasteAction({ files: ["/a/x.png", "/a/y.png"], hasImage: true, hasText: true }, false, supported), {
    kind: "files",
    paths: ["/a/x.png", "/a/y.png"],
  });
  // 対応形式でないファイルも開こうとする（開くときにエラーで知らせる）
  assert.deepEqual(pasteAction({ files: ["/a/x.txt"], hasImage: false, hasText: true }, false, supported), {
    kind: "files",
    paths: ["/a/x.txt"],
  });
  assert.deepEqual(pasteAction({ files: [], hasImage: true, hasText: true }, false, supported), { kind: "image" });
  assert.deepEqual(pasteAction({ files: [], hasImage: false, hasText: true }, false, supported), { kind: "nothing" });
  assert.deepEqual(pasteAction({ files: [], hasImage: false, hasText: false }, false, supported), { kind: "nothing" });
});

test("入力欄: 文字があれば文字を優先し、画像だけ・対応形式のファイルなら開く", () => {
  assert.deepEqual(pasteAction({ files: [], hasImage: false, hasText: true }, true, supported), { kind: "text" });
  assert.deepEqual(pasteAction({ files: [], hasImage: true, hasText: true }, true, supported), { kind: "text" });
  assert.deepEqual(pasteAction({ files: [], hasImage: true, hasText: false }, true, supported), { kind: "image" });
  // Finder でコピーした画像のファイル（ファイル名の文字も入っている）は開く
  assert.deepEqual(pasteAction({ files: ["/a/x.txt", "/a/p.HEIC"], hasImage: true, hasText: true }, true, supported), {
    kind: "files",
    paths: ["/a/p.HEIC"],
  });
  // 対応形式でないファイルだけなら文字（ファイル名）を貼り付ける
  assert.deepEqual(pasteAction({ files: ["/a/x.txt"], hasImage: true, hasText: true }, true, supported), {
    kind: "text",
  });
});

test("保存の名前の日時", () => {
  assert.equal(pasteStamp(new Date(2026, 9, 2, 6, 45, 0)), "20261002-064500");
  assert.equal(pasteStamp(new Date(2026, 0, 9, 23, 5, 7)), "20260109-230507");
});
