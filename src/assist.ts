// 自動の処理（旧版にはない）: 傾きの自動補正・おまかせ切り抜き・背景の被写体・顔／文字を見つけて隠す・肌のための顔・自動補正。
// どれも Rust（macOS の Vision・色の分布）に問い合わせ、エラーはダイアログ、結果はステータスバーで知らせる。
// 部品（photoControls・privacy・panel）には問い合わせ方だけを渡し、ここで画面の文をまとめて持つ。

import { invoke } from "@tauri-apps/api/core";
import { dom, hooks, parts, preview, state } from "./app";
import { evText, kelvinText, signedText } from "./panel";
import { degreesText } from "./photoControls";
import { notify, showError } from "./status";
import type { CropRect } from "./types";

/** 顔・文字の問い合わせの名前と、画面に出す名前。 */
const TARGETS = { faces: { command: "detect_faces", name: "顔" }, text: { command: "detect_text", name: "文字" } };
const COVER_NAMES = { blur: "ぼかし", mosaic: "モザイク", stamp: "スタンプ", heal: "修復" } as const;

/** 部品に問い合わせ方と知らせ方を渡す（部品を作った後に 1 回呼ぶ）。 */
export function setupAssist() {
  const photo = parts.photoControls;
  photo.findTilt = () =>
    invoke<number | null>("auto_straighten", { settings: state.settings }).catch((error) => {
      void showError("傾きを求められません", error);
      return undefined;
    });
  photo.onAutoStraighten = (degrees) =>
    notify(degrees === null ? "傾きが分かりませんでした（水平線や長い直線が見つかりません）" : `傾きを ${degreesText(degrees)} 直しました`);
  photo.prepareBackground = () => {
    notify("被写体を探しています…");
    return invoke<boolean>("prepare_background").catch((error) => {
      void showError("背景を消せません", error);
      return null;
    });
  };
  photo.onNoSubject = () => notify("被写体が見つからないので、背景はそのままにしました");
  parts.maskedPanel.prepareMask = photo.prepareBackground;
  parts.maskedPanel.onNoSubject = () => notify("被写体が見つからないので、被写体／背景の補正はかけません");

  parts.crop.findSubjectCrop = (aspect) => {
    notify("目立つ被写体を探しています…");
    return invoke<CropRect | null>("auto_crop", { settings: state.settings, aspect }).catch((error) => {
      void showError("おまかせで切り抜けません", error);
      return undefined;
    });
  };
  parts.crop.onAutoCrop = (found) =>
    notify(found ? "目立つ被写体に合わせて範囲を選びました（ドラッグで直せます）" : "目立つ被写体が見つからないので、範囲はそのままにしました");

  parts.privacy.find = (target) =>
    invoke<CropRect[]>(TARGETS[target].command, { settings: state.settings }).catch((error) => {
      void showError(`${TARGETS[target].name}を認識できません`, error);
      return null;
    });
  parts.privacy.onFound = (target, count, kind) => {
    const name = TARGETS[target].name;
    notify(count > 0 ? `${name}を ${count} か所見つけて、${COVER_NAMES[kind]}で隠しました` : `${name}が見つかりませんでした`);
  };

  parts.autoButton.addEventListener("click", () => void autoAdjust());
  parts.pickerButton.addEventListener("click", () => setPicking(!picking));
  // スポイトの間は、プレビューを押した所の色を拾う（範囲などの操作より先に受け取る）
  dom.stage.addEventListener(
    "pointerdown",
    (event) => {
      if (!picking || event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      void pickWhiteBalance(event.clientX, event.clientY);
    },
    { capture: true },
  );
  document.addEventListener("keydown", (event) => {
    if (picking && event.key === "Escape") setPicking(false);
  });
  hooks.prepareFaces = prepareFaces;
}

/** スポイトで色を拾う待ちか */
let picking = false;

/** スポイトの待ちを切り替える（待っている間はプレビューの上で十字のカーソル）。 */
export function setPicking(value: boolean) {
  picking = value && state.loaded !== null;
  parts.pickerButton?.setAttribute("aria-pressed", String(picking));
  dom.stage.classList.toggle("picking", picking);
  if (picking) notify("プレビューで、白・灰色のはずの所をクリックしてください（Esc でやめる）");
}

/** スポイト: 押した所の色が灰色になるよう、色温度・色かぶりを合わせる（1 回の操作として元に戻せる）。 */
async function pickWhiteBalance(clientX: number, clientY: number) {
  const sample = preview.sampleAt(clientX, clientY);
  setPicking(false);
  if (!sample) return;
  const { temperature, tint } = state.settings;
  try {
    const [kelvin, newTint] = await invoke<[number, number]>("pick_white_balance", { sample, temperature, tint });
    parts.panel.setAuto({ temperature: kelvin, tint: newTint });
    notify(`スポイト: 色温度 ${kelvinText(kelvin)}・色かぶり ${signedText(newTint)}`);
  } catch (error) {
    await showError("ホワイトバランスを合わせられません", error);
  }
}

/** 自動補正: 今の写真から露出・コントラスト・色温度を求めてスライダーに入れる（1 回の操作として元に戻せる）。 */
async function autoAdjust() {
  if (!state.loaded) return;
  parts.autoButton.disabled = true;
  try {
    const values = await invoke<{ exposure: number; contrast: number; temperature: number }>("auto_adjust", {
      settings: state.settings,
    });
    parts.panel.setAuto(values);
    notify(`自動補正: 露出 ${evText(values.exposure)}・コントラスト ${signedText(values.contrast)}・色温度 ${kelvinText(values.temperature)}`);
  } catch (error) {
    await showError("自動補正できません", error);
  } finally {
    parts.autoButton.disabled = state.loaded === null;
  }
}

/**
 * 肌をなめらかに・赤目の補正をするときは、先に顔を探しておく（画像ごとに 1 回）。探し終えたら描き直す（settingsChanged は
 * 設定が変わるたびに呼ぶ）。
 */
function prepareFaces(redraw: () => void) {
  if ((state.settings.skinSmooth === 0 && !state.settings.redEye) || state.facesPrepared) return;
  state.facesPrepared = true;
  void invoke<number>("prepare_faces")
    .then((count) => {
      if (count === 0) notify("顔が見つからないので、肌をなめらかに・赤目の補正はかかりません");
      redraw();
    })
    .catch((error) => {
      state.facesPrepared = false;
      void showError("顔を認識できません", error);
    });
}
