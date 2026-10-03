// 履歴（src/history.ts）の動き。`npm test` で動かす（Node の型を外す読み込みを使う）。

import assert from "node:assert/strict";
import { test } from "node:test";
import { History, HistoryRecorder, type Timers } from "../src/history.ts";

test("push・undo・redo", () => {
  const h = new History(0);
  assert.equal(h.canUndo(), false);
  assert.equal(h.push(1), true);
  assert.equal(h.push(1), false); // 同じ状態は積まない
  h.push(2);
  assert.equal(h.undo(), 1);
  assert.equal(h.undo(), 0);
  assert.equal(h.undo(), 0); // それ以上は戻らない
  assert.equal(h.canRedo(), true);
  assert.equal(h.redo(), 1);
  // 新しく変えると、やり直せる先は消える
  h.push(5);
  assert.equal(h.canRedo(), false);
  assert.equal(h.redo(), 5);
  assert.equal(h.undo(), 1);
});

test("同じ値かは中身で比べる（キーの並びは問わない）", () => {
  const h = new History<Record<string, unknown>>({ a: 1, b: [1, 2] });
  assert.equal(h.push({ a: 1, b: [1, 2] }), false);
  assert.equal(h.push({ b: [1, 2], a: 1 }), false);
  assert.equal(h.push({ a: 1, b: [1, 3] }), true);
});

test("最大の件数を超えたら古いものから消す", () => {
  const h = new History(0, 100);
  for (let i = 1; i <= 150; i++) h.push(i);
  let count = 0;
  while (h.canUndo()) {
    h.undo();
    count++;
  }
  assert.equal(count, 100);
  assert.equal(h.current(), 50);
  assert.throws(() => new History(0, 0), RangeError);
});

test("reset で履歴を消す", () => {
  const h = new History(0);
  h.push(1);
  h.undo();
  h.reset(9);
  assert.deepEqual([h.current(), h.canUndo(), h.canRedo()], [9, false, false]);
});

/** 手で進める時計 */
function fakeTimers() {
  let now = 0;
  const queue = new Map<number, { at: number; callback: () => void }>();
  let id = 0;
  const timers: Timers = {
    set: (callback, ms) => {
      queue.set(++id, { at: now + ms, callback });
      return id;
    },
    clear: (handle) => queue.delete(handle as number),
  };
  const advance = (ms: number) => {
    now += ms;
    for (const [key, item] of [...queue].sort((a, b) => a[1].at - b[1].at)) {
      if (item.at <= now && queue.has(key)) {
        queue.delete(key);
        item.callback();
      }
    }
  };
  return { timers, advance };
}

function setup() {
  const state = { value: 0, adjusting: false, updates: 0 };
  const { timers, advance } = fakeTimers();
  const recorder = new HistoryRecorder<number>({
    snapshot: () => state.value,
    restore: (v) => {
      state.value = v;
      recorder.changed(); // 戻している間の変更の知らせは積まない
    },
    isAdjusting: () => state.adjusting,
    onUpdate: () => state.updates++,
    timers,
  });
  const change = (v: number) => {
    state.value = v;
    recorder.changed();
  };
  return { state, recorder, change, advance };
}

test("続けた変更は 0.5 秒落ち着いてから 1 回にまとめる", () => {
  const { state, recorder, change, advance } = setup();
  change(1);
  advance(300);
  change(2);
  advance(300);
  change(3);
  assert.equal(recorder.pending, true);
  advance(499);
  assert.equal(recorder.pending, true);
  advance(1);
  assert.equal(recorder.pending, false);
  assert.equal(recorder.undo(), true);
  assert.equal(state.value, 0); // 1〜3 はまとめて 1 回で戻る
  assert.equal(recorder.undo(), false);
});

test("積む前の変更もすぐに戻せる・戻すとやり直せる", () => {
  const { state, recorder, change, advance } = setup();
  change(1);
  advance(600);
  change(2);
  assert.equal(recorder.canUndo(), true);
  assert.equal(recorder.canRedo(), false);
  recorder.undo();
  assert.equal(state.value, 1);
  assert.equal(recorder.pending, false); // 戻した状態は積まない
  recorder.redo();
  assert.equal(state.value, 2);
  recorder.undo();
  recorder.undo();
  assert.equal(state.value, 0);
  // 新しく変えると、やり直せる先は消える
  change(7);
  assert.equal(recorder.canRedo(), false);
  advance(500);
  assert.equal(recorder.redo(), false);
  assert.equal(state.value, 7);
});

test("ドラッグしている間は積まず、離してから 1 回にする", () => {
  const { state, recorder, change, advance } = setup();
  state.adjusting = true;
  change(1);
  advance(600);
  change(2);
  advance(2000);
  assert.equal(recorder.pending, true); // ドラッグ中は待つ
  state.adjusting = false;
  advance(500);
  assert.equal(recorder.pending, false);
  recorder.undo();
  assert.equal(state.value, 0); // ドラッグ全体が 1 回で戻る
});

test("ドラッグ中でも元に戻すならその場で積む", () => {
  const { state, recorder, change } = setup();
  state.adjusting = true;
  change(4);
  recorder.undo();
  assert.equal(state.value, 0);
  recorder.redo();
  assert.equal(state.value, 4);
});

test("reset で積んでいない変更も含めて消す", () => {
  const { recorder, change, advance } = setup();
  change(1);
  advance(500);
  change(2);
  recorder.reset();
  assert.deepEqual([recorder.pending, recorder.canUndo(), recorder.canRedo()], [false, false, false]);
  advance(1000);
  assert.equal(recorder.canUndo(), false);
});
