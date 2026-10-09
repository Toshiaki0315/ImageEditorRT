// 表示の切り替え: 加工前との比較（旧版 FR-UI-44）・100% 表示（FR-UI-48）・左上の表示・ジオラマのガイド。

import { invoke } from "@tauri-apps/api/core";
import { dom, preview, state, tabs, zoomView } from "./app";
import { isCompareKey } from "./keys";
import { MENU, setMenuChecked, updateMenus } from "./menus";
import { readRawImage } from "./protocol";
import { SPLIT_DEFAULT, splitClip, splitFraction } from "./split";
import { showError } from "./status";
import type { DioramaGuide } from "./types";

/** 100% 表示で、設定の変更が落ち着いてから処理し直すまでの時間 (ms) */
const ZOOM_DELAY_MS = 300;
/** 画像の左上から「加工前」などの表示までの間隔 (px) */
const BADGE_MARGIN = 8;
const SVG = "http://www.w3.org/2000/svg";

/** 加工前を表示中か（\ キーか「加工前」ボタンを押している間） */
let comparing = false;
/** 100% 表示中か（原寸の処理が終わるまでは前の表示のまま） */
let zoomed = false;
/** 100% 表示の原寸の処理中か */
let zoomPending = false;
/** 100% 表示の依頼の番号（古い依頼の結果は表示しない） */
let zoomGeneration = 0;
/** 100% 表示を始めるときに表示の中央にする点（保存結果の画像の座標） */
let zoomCenter: [number, number] | null = null;
let zoomTimer: ReturnType<typeof setTimeout> | undefined;

export const isZoomed = () => zoomed;

/** 左右に分けて比べているか */
let splitting = false;
/** 境目の位置（表示している画像の幅に対する比率） */
let splitAt = SPLIT_DEFAULT;

// --- 加工前との比較 -----------------------------------------------------------------

/** 加工前の表示を切り替える（押している間だけ true）。設定は変えない。 */
export function setComparing(value: boolean) {
  value = value && state.loaded !== null;
  if (value === comparing) return;
  comparing = value;
  preview.comparing = value;
  updateSplit();
  updateBadge();
  void updateGuide();
  preview.request(state.settings);
  if (zoomed) void renderZoom();
}

/** \ キー・「加工前」ボタン。キーはどの入力欄にフォーカスがあっても効き、文字としては入らない。 */
export function setupCompare() {
  for (const type of ["keydown", "keyup"] as const) {
    window.addEventListener(
      type,
      (event) => {
        if (!state.loaded || !isCompareKey(event) || event.metaKey || event.ctrlKey) return;
        event.preventDefault();
        event.stopPropagation();
        if (!event.repeat) setComparing(type === "keydown");
      },
      true,
    );
  }
  // キーを押したまま別のウィンドウに切り替えると離したことが届かないので、ここで戻す
  window.addEventListener("blur", () => setComparing(false));
  const button = dom.beforeButton;
  button.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    button.setPointerCapture(event.pointerId);
    setComparing(true);
  });
  for (const type of ["pointerup", "pointercancel", "lostpointercapture"]) {
    button.addEventListener(type, () => setComparing(false));
  }
}

// --- 左右に分けて比べる -------------------------------------------------------------

/** 左右に分けて比べる表示を切り替える（100% 表示の間・画像がないときは使わない）。 */
export function toggleSplit() {
  const next = !splitting && state.loaded !== null && !zoomed;
  setMenuChecked(MENU.split, next); // メニューは選ぶとチェックが変わるので、使えないときも合わせ直す
  if (next === splitting) return;
  splitting = next;
  preview.splitCanvas = splitting ? dom.splitCanvas : null;
  updateSplit();
  if (splitting) preview.request(state.settings);
}

/** 境目の位置・表示する・しないを今の状態に合わせる（加工前の表示中は全体が加工前なので出さない）。 */
export function updateSplit() {
  dom.split.hidden = !(splitting && state.loaded !== null && !comparing);
  dom.splitCanvas.style.clipPath = splitClip(splitAt);
  dom.splitLine.style.left = `${splitAt * 100}%`;
}

/** 画像を閉じたとき: 左右に分けて比べるのをやめる。 */
export function stopSplit() {
  if (splitting) toggleSplit();
}

/** 境目のドラッグ。 */
export function setupSplit() {
  const line = dom.splitLine;
  const move = (event: PointerEvent) => {
    const rect = dom.canvas.getBoundingClientRect();
    splitAt = splitFraction(event.clientX, rect.left, rect.width);
    updateSplit();
  };
  line.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    line.setPointerCapture(event.pointerId);
    move(event);
  });
  line.addEventListener("pointermove", (event) => {
    if (line.hasPointerCapture(event.pointerId)) move(event);
  });
  // ダブルクリックで真ん中に戻す
  line.addEventListener("dblclick", (event) => {
    event.stopPropagation();
    splitAt = SPLIT_DEFAULT;
    updateSplit();
  });
}

// --- 100% 表示 ----------------------------------------------------------------------

/** 100% 表示にする。center は表示の中央にしたい点（保存結果の画像の座標。省略時は画像の中央）。 */
export function showActualSize(center: [number, number] | null = null) {
  if (!state.loaded) return;
  stopSplit(); // 100% 表示の間は使わない
  zoomed = true;
  zoomCenter = center;
  void renderZoom();
  updateMenus();
}

/** 画面に合わせた表示に戻す。 */
export function fitToWindow() {
  if (!zoomed) return;
  zoomed = false;
  zoomGeneration += 1; // 処理中の結果は使わない
  zoomPending = false;
  clearTimeout(zoomTimer);
  zoomCenter = null;
  zoomView.hide();
  dom.frame.hidden = false;
  preview.fit();
  updateBadge();
  void updateGuide();
  updateMenus();
}

/** 設定が変わったとき: 100% 表示中なら、変更が落ち着いてから原寸で処理し直す。 */
export function scheduleZoomRender() {
  if (!zoomed) return;
  clearTimeout(zoomTimer);
  zoomTimer = setTimeout(() => void renderZoom(), ZOOM_DELAY_MS);
}

/** 今の設定（加工前の表示中なら加工前）で原寸の処理を始め、終わったら表示する。 */
async function renderZoom() {
  clearTimeout(zoomTimer);
  if (!zoomed || !state.loaded) return;
  const generation = ++zoomGeneration;
  zoomPending = true;
  updateBadge();
  try {
    const buffer = await invoke<ArrayBuffer>("render_actual_size", { settings: state.settings, comparing });
    if (generation !== zoomGeneration) return; // 古い依頼の結果（設定がその後変わった）
    const { width, height, pixels } = readRawImage(buffer);
    zoomView.show(width, height, pixels, zoomCenter);
    zoomCenter = null;
    dom.frame.hidden = true;
    zoomPending = false;
    updateBadge();
    void updateGuide();
  } catch (error) {
    if (generation !== zoomGeneration) return;
    fitToWindow();
    await showError("100% で表示できません", error);
  }
}

/** 「トリミング実行」の表示でダブルクリックした点を中央にして 100% 表示にする（通常の表示では範囲の解除に使うので切り替えない）。 */
export async function onPreviewDoubleClick(event: MouseEvent) {
  if (!state.loaded || zoomed || !preview.trimmed) return;
  const rect = dom.canvas.getBoundingClientRect();
  if (rect.width === 0 || rect.height === 0) return;
  const [width, height] = await invoke<[number, number]>("output_size", { settings: state.settings });
  showActualSize([((event.clientX - rect.left) / rect.width) * width, ((event.clientY - rect.top) / rect.height) * height]);
}

// --- 左上の表示 ---------------------------------------------------------------------

/** 左上の表示（「加工前」「100%」「更新中…」）をまとめて出す。 */
export function updateBadge() {
  const parts: string[] = [];
  if (comparing) parts.push("加工前");
  if (zoomed) {
    parts.push("100%");
    if (zoomPending) parts.push("更新中…");
  }
  dom.badge.textContent = parts.join(" ・ ");
  dom.badge.hidden = parts.length === 0;
  placeBadge();
}

/** 表示している画像の左上に置く（100% 表示で画像が左上にはみ出していても、見える位置に出す）。 */
export function placeBadge() {
  const { badge, canvas, stage } = dom;
  if (badge.hidden) return;
  let origin = { x: 0, y: 0 };
  const zoomRect = zoomView.rect();
  if (zoomRect) {
    origin = zoomRect;
  } else if (!canvas.hidden) {
    const stageRect = stage.getBoundingClientRect();
    const rect = canvas.getBoundingClientRect();
    origin = { x: rect.left - stageRect.left, y: rect.top - stageRect.top };
  }
  badge.style.left = `${Math.round(Math.max(origin.x, 0)) + BADGE_MARGIN}px`;
  badge.style.top = `${Math.round(Math.max(origin.y, 0)) + BADGE_MARGIN}px`;
}

// --- ジオラマのガイド ---------------------------------------------------------------

/**
 * 「ジオラマ」タブを開いている間、プレビューにピントの帯のガイドを重ねる（ぼかしが 0 でも出す）。
 * 加工前の表示中は出さない。100% 表示では保存結果の写真の部分に対する位置に出す。
 */
export async function updateGuide() {
  const zoomCanvas = zoomView.active ? dom.zoomCanvas : null;
  const shown = state.loaded !== null && tabs.selected() === "diorama" && !comparing;
  dom.guide.toggleAttribute("hidden", true);
  dom.zoomGuide.toggleAttribute("hidden", true);
  if (!shown) return;
  const result = await invoke<DioramaGuide>("diorama_guide", {
    settings: state.settings,
    trimmed: preview.trimmed,
    zoomed: zoomCanvas !== null,
  });
  drawGuide(zoomCanvas ? dom.zoomGuide : dom.guide, result, zoomCanvas ?? dom.canvas);
}

/** ガイドの線を svg に描く（大きさは target の画素の数に合わせる）。 */
function drawGuide(guide: SVGSVGElement, result: DioramaGuide, target: HTMLCanvasElement) {
  const { width, height } = target;
  guide.setAttribute("viewBox", `0 0 ${width} ${height}`);
  guide.replaceChildren();
  for (const [fraction, solid] of result.lines) {
    const position = fraction * (result.horizontal ? height : width);
    // 影を下に描いてから線を描く（実線: くっきり残す範囲の端、破線: ぼけきる位置）
    for (const kind of ["shadow", "line"]) {
      const line = document.createElementNS(SVG, "line");
      const [x1, y1, x2, y2] = result.horizontal ? [0, position, width, position] : [position, 0, position, height];
      line.setAttribute("x1", String(x1));
      line.setAttribute("y1", String(y1));
      line.setAttribute("x2", String(x2));
      line.setAttribute("y2", String(y2));
      line.setAttribute("class", solid ? kind : `${kind} dashed`);
      guide.append(line);
    }
  }
  guide.toggleAttribute("hidden", false);
}
