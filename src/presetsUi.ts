// プリセット（名前付きの加工の組み合わせ。旧版 FR-UI-59）: 「プリセット ▾」のメニュー・保存・当てはめ・削除と、
// 保存せずに使い回す「加工をコピー／ペースト」（旧版にはない）。

import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { $, parts, state } from "./app";
import { userChanged } from "./editing";
import { MENU, updateMenus } from "./menus";
import { notify, showError, updateStatus } from "./status";
import type { EditSettings } from "./types";

/** プリセットの名前の一覧（「プリセット ▾」のメニューに出す。並びは Rust の一覧と同じ） */
let names: string[] = [];

export const presetNames = () => names;

/** 起動時: プリセットを読む。ファイルが壊れていたら知らせ、プリセットなしで使えるようにする。 */
export async function loadPresets() {
  const result = await invoke<{ names: string[]; error: string | null }>("load_presets");
  names = result.names;
  if (result.error) void showError("プリセットを読み込めません", result.error);
}

/**
 * 「プリセット ▾」のメニューをボタンの下に出す（メニューは Rust で作る）。選んだ項目は "menu" のイベントで
 * onPresetMenu に届く。
 */
export async function showPresetMenu(button: HTMLElement) {
  const rect = button.getBoundingClientRect();
  await invoke("show_preset_menu", { x: rect.left, y: rect.bottom, loaded: state.loaded !== null });
}

/** "menu" のイベントのうち、プリセットのメニューの項目を処理する。 */
export function onPresetMenu(id: string) {
  const named = (prefix: string) => (id.startsWith(prefix) ? names[Number(id.slice(prefix.length))] : undefined);
  if (id === MENU.presetSave) void savePresetDialog();
  const apply = named(MENU.presetApply);
  if (apply !== undefined) void applyPreset(apply);
  const remove = named(MENU.presetDelete);
  if (remove !== undefined) void deletePreset(remove);
}

/** プリセットの加工を当てはめる（サイズ・範囲・向きはそのまま）。1 回の操作として元に戻せる。 */
function applyPreset(name: string) {
  return applyLook("プリセットを当てはめられません", () =>
    invoke<EditSettings>("apply_preset", { name, settings: state.settings }),
  );
}

/** 「加工をコピー」: 今の写真の設定を覚える（ペーストでは加工の項目だけを使う）。 */
export function copyLook() {
  if (!state.loaded) return;
  state.copiedLook = structuredClone(state.settings);
  updateMenus();
  notify("加工をコピーしました（「編集 > 加工をペースト」で別の写真に当てはめます）");
}

/** 「加工をペースト」: コピーした加工を、プリセットと同じく今の写真に当てはめる。 */
export function pasteLook() {
  const look = state.copiedLook;
  if (!look) return;
  return applyLook("加工をペーストできません", () =>
    invoke<EditSettings>("apply_look", { look, settings: state.settings }),
  );
}

/** 加工を当てはめる（fetch が当てはめた後の設定を返す）。1 回の操作として元に戻せる。 */
async function applyLook(failure: string, fetch: () => Promise<EditSettings>) {
  if (!state.loaded || state.saving) return;
  try {
    Object.assign(state.settings, await fetch());
  } catch (error) {
    await showError(failure, error);
    return;
  }
  parts.panel.show();
  parts.textDialog.show();
  // フレーム・円の比が変わったら、手で選んだときと同じく範囲をその比に直す
  await parts.crop.refit();
  userChanged();
}

/** 名前を入力してもらう（キャンセルなら null）。 */
function askPresetName(initial: string): Promise<string | null> {
  const dialog = $<HTMLDialogElement>("preset-dialog");
  const input = $<HTMLInputElement>("preset-name");
  input.value = initial;
  dialog.returnValue = "";
  dialog.showModal();
  input.select();
  return new Promise((resolve) =>
    dialog.addEventListener("close", () => resolve(dialog.returnValue === "ok" ? input.value : null), {
      once: true,
    }),
  );
}

/** 今の加工を、名前を付けてプリセットとして保存する。同じ名前なら上書きを確かめる。 */
async function savePresetDialog() {
  if (!state.loaded) return;
  const text = await askPresetName(await invoke<string>("default_preset_name"));
  if (text === null) return;
  const check = await invoke<{ name: string; exists: boolean }>("check_preset_name", { name: text });
  const name = check.name;
  if (!name) return;
  if (
    check.exists &&
    !(await ask(`プリセット「${name}」はすでにあります。上書きしますか？`, {
      title: "プリセットを保存",
      kind: "warning",
      okLabel: "上書き",
      cancelLabel: "キャンセル",
    }))
  ) {
    return;
  }
  try {
    names = await invoke<string[]>("save_preset", { name, settings: state.settings });
    void updateStatus(`プリセット「${name}」を保存しました`);
  } catch (error) {
    await showError("プリセットを保存できません", error);
  }
}

/** プリセットを確かめてから削除する。 */
async function deletePreset(name: string) {
  const ok = await ask(`プリセット「${name}」を削除しますか？`, {
    title: "プリセットを削除",
    kind: "warning",
    okLabel: "削除",
    cancelLabel: "キャンセル",
  });
  if (!ok) return;
  try {
    names = await invoke<string[]>("delete_preset", { name });
    void updateStatus(`プリセット「${name}」を削除しました`);
  } catch (error) {
    await showError("プリセットを保存できません", error);
  }
}
