// 設定の変更の知らせと、元に戻す／やり直す（旧版 FR-UI-43）・未保存の変更の確認。

import { ask } from "@tauri-apps/plugin-dialog";
import { output, parts, preview, type Snapshot, state } from "./app";
import { HistoryRecorder, sameValue } from "./history";
import { updateMenus } from "./menus";
import { updateStatus } from "./status";
import { defaultSettings } from "./types";
import { scheduleZoomRender, updateGuide } from "./view";

export const DISCARD_TITLE = "未保存の変更";
export const DISCARD_QUESTION = "保存していない変更があります。破棄してよろしいですか？";

/** スライダーをドラッグしている間（履歴に積むのを離すまで待つ） */
let sliderDragging = false;

/** 画面で設定を変えたとき: 履歴に積む（落ち着いてから）と、プレビューなどの更新。 */
export function userChanged() {
  parts.recorder?.changed();
  settingsChanged();
}

/** 設定が変わったとき: プレビューとステータスバー（出力の大きさ）を更新する。 */
export function settingsChanged() {
  if (!state.loaded) return;
  // 出力の幅・高さは範囲・フレームなどで変わるので、先に合わせてからプレビューを描く
  void output.refresh().then(() => {
    preview.request(state.settings);
    void updateStatus();
    void updateGuide();
  });
  // 100% 表示中は、変更が落ち着いてから原寸で処理し直す
  scheduleZoomRender();
}

/** 今の設定パネルの状態。出力の幅・高さは出力の欄の状態から決まるので、設定の側には持たない。 */
function snapshot(): Snapshot {
  return {
    settings: { ...structuredClone(state.settings), width: null, height: null },
    aspect: parts.crop.aspectState(),
    size: output.snapshot(),
  };
}

/** 履歴の状態を設定パネルと設定に戻す（比の固定で範囲を直したりしない）。 */
function restore(snapshot: Snapshot) {
  const { loaded } = state;
  if (!loaded) return;
  Object.assign(state.settings, structuredClone(snapshot.settings));
  parts.crop.restore(snapshot.aspect, [loaded.width, loaded.height]);
  output.restore(snapshot.size);
  parts.panel.show();
  parts.textDialog.show();
  settingsChanged();
}

/** 履歴を作る。スライダーをドラッグしている間は積まない（離したら 1 回の操作として積む）。 */
export function setupHistory() {
  parts.recorder = new HistoryRecorder<Snapshot>({
    snapshot,
    restore,
    isAdjusting: () => sliderDragging || parts.crop.isDragging(),
    onUpdate: updateMenus,
  });
  document.addEventListener("pointerdown", (event) => {
    if (event.target instanceof HTMLInputElement && event.target.type === "range") sliderDragging = true;
  });
  for (const type of ["pointerup", "pointercancel"]) {
    document.addEventListener(type, () => (sliderDragging = false));
  }
}

/** 入力欄で文字を編集中か（そのときの ⌘Z・⇧⌘Z・⌘V は入力欄の文字に効かせる）。 */
export function isEditingText(): boolean {
  const active = document.activeElement;
  if (active instanceof HTMLTextAreaElement) return true;
  return active instanceof HTMLInputElement && ["text", "number", "search"].includes(active.type);
}

export function undo() {
  if (isEditingText()) {
    document.execCommand("undo");
    return;
  }
  if (state.loaded && !state.saving) parts.recorder.undo();
}

export function redo() {
  if (isEditingText()) {
    document.execCommand("redo");
    return;
  }
  if (state.loaded && !state.saving) parts.recorder.redo();
}

/** 初期状態から設定を変えていて、その設定でまだ保存していなければ true。 */
export function hasUnsavedChanges(): boolean {
  const { loaded, settings, savedSettings } = state;
  return loaded !== null && !sameValue(settings, defaultSettings()) && !sameValue(settings, savedSettings);
}

/** 未保存の変更があれば、破棄してよいかを確かめる（okLabel は確かめた後にすること）。 */
export async function confirmDiscard(okLabel = "破棄"): Promise<boolean> {
  if (!hasUnsavedChanges()) return true;
  return ask(DISCARD_QUESTION, { title: DISCARD_TITLE, kind: "warning", okLabel, cancelLabel: "キャンセル" });
}
