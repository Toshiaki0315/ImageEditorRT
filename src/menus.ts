// メニューの項目の ID（Rust の menu.rs・presets.rs と同じ）と、使える・使えないの切り替え。

import { invoke } from "@tauri-apps/api/core";
import { parts, state } from "./app";
import { isZoomed } from "./view";

/** "menu" のイベントで届く項目の ID。 */
export const MENU = {
  quit: "quit",
  open: "open",
  save: "save",
  batch: "batch",
  saveSizes: "save-sizes",
  revealSaved: "reveal-saved",
  nextPhoto: "next-photo",
  previousPhoto: "previous-photo",
  share: "share",
  print: "print",
  collage: "collage",
  undo: "undo",
  redo: "redo",
  paste: "paste",
  text: "text",
  copyImage: "copy-image",
  copyLook: "copy-look",
  pasteLook: "paste-look",
  keepVersion: "keep-version",
  versions: "versions",
  historyList: "history-list",
  actualSize: "actual_size",
  fit: "fit",
  histogram: "histogram",
  split: "split",
  help: "help",
  presetSave: "preset-save",
  presetExport: "preset-export",
  presetImport: "preset-import",
  /** 後ろに一覧の中の番号が付く */
  presetApply: "preset-apply:",
  presetDelete: "preset-delete:",
} as const;

/** 前に送った状態（同じなら送らない）。 */
const menuEnabled = new Map<string, boolean>();

function setMenuEnabled(id: string, enabled: boolean) {
  if (menuEnabled.get(id) === enabled) return;
  menuEnabled.set(id, enabled);
  void invoke("set_menu_enabled", { id, enabled });
}

/** メニューの「元に戻す」「やり直す」「加工をコピー／ペースト」「100% で表示」「画面に合わせる」「左右に分けて比べる」を、今の状態に合わせる。 */
export function updateMenus() {
  const editable = state.loaded !== null && !state.saving;
  setMenuEnabled(MENU.undo, editable && (parts.recorder?.canUndo() ?? false));
  setMenuEnabled(MENU.redo, editable && (parts.recorder?.canRedo() ?? false));
  setMenuEnabled(MENU.copyLook, state.loaded !== null);
  setMenuEnabled(MENU.copyImage, state.loaded !== null && !state.saving);
  setMenuEnabled(MENU.share, state.loaded !== null && !state.saving);
  setMenuEnabled(MENU.print, state.loaded !== null && !state.saving);
  setMenuEnabled(MENU.nextPhoto, state.loaded !== null && !state.saving);
  setMenuEnabled(MENU.previousPhoto, state.loaded !== null && !state.saving);
  setMenuEnabled(MENU.pasteLook, editable && state.copiedLook !== null);
  setMenuEnabled(MENU.keepVersion, editable);
  setMenuEnabled(MENU.versions, state.loaded !== null);
  setMenuEnabled(MENU.historyList, state.loaded !== null);
  setMenuEnabled(MENU.actualSize, state.loaded !== null && !isZoomed());
  setMenuEnabled(MENU.fit, isZoomed());
  setMenuEnabled(MENU.revealSaved, state.savedPaths.length > 0);
  setMenuEnabled(MENU.split, state.loaded !== null && !isZoomed());
}

/** チェックの付く項目の状態を変える。 */
export function setMenuChecked(id: string, checked: boolean) {
  void invoke("set_menu_checked", { id, checked });
}
