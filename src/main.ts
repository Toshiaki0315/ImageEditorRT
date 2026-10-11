// ImageEditorRT の画面の入り口: 部品を作り、メニュー・イベントを機能につなぐ。
// 機能は editing（変更と履歴）・view（比較・100% 表示）・files（開く・保存・貼り付け・終了）・
// presetsUi（プリセット）、共有の状態と部品は app にある。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { $, dom, histogramView, hooks, output, parts, preview, saveOptions, state, tabs, zoomView } from "./app";
import { BatchDialog } from "./batchDialog";
import { CollageDialog } from "./collageDialog";
import { bench, benchSave, verdict } from "./bench";
import { clippingShown, setClippingShown } from "./clipping";
import { ColorPanel } from "./colorPanel";
import { setupAssist } from "./assist";
import { CropController } from "./crop";
import { applySnapshot, isEditingText, redo, settingsChanged, setupHistory, snapshot, undo, userChanged } from "./editing";
import { isEscapeKey } from "./keys";
import {
  makeCollage,
  openDialog,
  openPaths,
  paste,
  requestQuit,
  resetImage,
  saveDialog,
  revealSaved,
  copyImage,
  openNeighbor,
  printImage,
  resumeEdits,
  saveSizesDialog,
  shareImage,
  setupDrop,
  showLoaded,
  startBatch,
} from "./files";
import { MENU, setMenuChecked } from "./menus";
import { Panel } from "./panel";
import { PhotoControls } from "./photoControls";
import { LocalPanel } from "./localPanel";
import { LutControls } from "./lutControls";
import { MaskedPanel } from "./maskedPanel";
import { MonoPanel } from "./monoPanel";
import { Versions } from "./versions";
import { HistoryPanel } from "./historyPanel";
import { ColorMatchControls } from "./colorMatchControls";
import { PrivacyPanel } from "./privacy";
import { copyLook, loadPresets, onPresetMenu, pasteLook, showPresetMenu } from "./presetsUi";
import { catchUnexpectedErrors, notify, reportUnexpected, showError } from "./status";
import { TasteGallery } from "./tasteGallery";
import { TextDialog } from "./textDialog";
import { TextDrag } from "./textDrag";
import type { AspectRatio, FilterType, FrameKind, OpenInfo, ShapeType, TextFont, TextPosition } from "./types";
import {
  fitToWindow,
  isZoomed,
  onPreviewDoubleClick,
  placeBadge,
  setupCompare,
  setupSplit,
  drawStraightenGrid,
  showActualSize,
  showStraightenGrid,
  toggleSplit,
  updateGuide,
} from "./view";

catchUnexpectedErrors();
hooks.userChanged = userChanged;
hooks.previewError = (error) => void showError("プレビューを更新できません", error);
preview.onHistogram = (histogram) => histogramView.set(state.loaded ? histogram : null);
zoomView.onMove = placeBadge;
zoomView.onDoubleClick = fitToWindow;

/** メニューの項目が選ばれたときにすること（プリセットの項目は onPresetMenu）。 */
const menuActions: Record<string, () => void> = {
  [MENU.quit]: () => void requestQuit(),
  [MENU.open]: () => void openDialog(),
  [MENU.save]: () => void saveDialog(),
  [MENU.saveSizes]: () => void saveSizesDialog(),
  [MENU.revealSaved]: () => void revealSaved(),
  [MENU.copyImage]: () => void copyImage(),
  [MENU.share]: () => void shareImage(),
  [MENU.print]: () => void printImage(),
  [MENU.nextPhoto]: () => void openNeighbor(1),
  [MENU.previousPhoto]: () => void openNeighbor(-1),
  [MENU.batch]: () => void startBatch(),
  [MENU.collage]: () => void makeCollage(),
  [MENU.paste]: () => void paste(),
  [MENU.undo]: undo,
  [MENU.redo]: redo,
  [MENU.text]: () => state.loaded && parts.textDialog.open(),
  [MENU.copyLook]: copyLook,
  [MENU.pasteLook]: () => void pasteLook(),
  [MENU.keepVersion]: () => state.loaded && !state.saving && parts.versions.keep(),
  [MENU.versions]: () => state.loaded && parts.versions.open(),
  [MENU.historyList]: () => state.loaded && parts.historyPanel.open(),
  [MENU.actualSize]: () => showActualSize(),
  [MENU.fit]: fitToWindow,
  [MENU.split]: toggleSplit,
  [MENU.help]: () => {
    const help = $<HTMLDialogElement>("help-dialog");
    if (!help.open) help.showModal();
  },
  [MENU.clipping]: () => {
    setClippingShown(!clippingShown());
    setMenuChecked(MENU.clipping, clippingShown());
    if (state.loaded) preview.request(state.settings);
  },
  [MENU.histogram]: () => {
    histogramView.setShown(!histogramView.shown);
    setMenuChecked(MENU.histogram, histogramView.shown);
  },
};

/** 設定パネルの部品を作る（選択肢は Rust から読む）。 */
async function createParts() {
  const { settings } = state;
  const filters = await invoke<[FilterType, string][]>("filter_types");
  parts.panel = new Panel($("page-adjust"), $("page-diorama"), filters, settings, userChanged);
  parts.colorPanel = new ColorPanel($("color-extra"), settings, userChanged);
  parts.versions = new Versions(snapshot, applySnapshot, notify);
  parts.historyPanel = new HistoryPanel(() => parts.recorder);
  parts.monoPanel = new MonoPanel($("mono-extra"), settings, () => state.loaded !== null, userChanged);
  parts.lutControls = new LutControls($("lut-extra"), settings, () => state.loaded !== null, userChanged);
  parts.lutControls.onError = (error) => void showError("LUT を読めません", error);
  parts.colorMatchControls = new ColorMatchControls($("color-match-extra"), settings, () => state.loaded !== null, userChanged);
  parts.colorMatchControls.extensions = () => state.extensions;
  parts.colorMatchControls.onError = (error) => void showError("参考の写真を読めません", error);
  const ratios = await invoke<[AspectRatio, string][]>("aspect_ratios");
  const [frames, shapes] = await invoke<[[FrameKind, string][], [ShapeType, string][]]>("frame_shape_types");
  parts.crop = new CropController(
    settings,
    dom.canvas,
    ratios,
    frames,
    shapes,
    userChanged,
    (trimmed) => {
      preview.trimmed = trimmed;
      updateOverlays();
      settingsChanged();
    },
    () => output.rotate(),
  );
  parts.photoControls = new PhotoControls(settings, () => state.loaded !== null, userChanged);
  parts.photoControls.onStraightenAdjust = showStraightenGrid;
  parts.photoControls.chooseImage = async () => {
    const path = await open({
      title: "背景の画像を選ぶ",
      multiple: false,
      filters: [{ name: "画像ファイル", extensions: state.extensions }],
    });
    return typeof path === "string" ? path : null;
  };
  const stamps = await invoke<string[]>("stamp_list");
  parts.privacy = new PrivacyPanel(settings, dom.canvas, orientedSize, stamps, userChanged);
  parts.localPanel = new LocalPanel($("local-extra"), settings, dom.canvas, orientedSize, userChanged);
  parts.maskedPanel = new MaskedPanel($("masked-extra"), settings, () => state.loaded !== null, userChanged);
  const [fonts, positions] = await invoke<[[TextFont, string][], [TextPosition, string][]]>("text_options");
  parts.textDialog = new TextDialog(settings, fonts, positions, userChanged);
  parts.textDrag = new TextDrag(settings, dom.canvas, orientedSize, () => parts.crop.photoArea(), userChanged);
  parts.crop.onAreaChange = () => parts.textDrag.draw();
  parts.textDialog.logoExtensions = state.extensions;
  parts.textButton = $<HTMLButtonElement>("text-button");
  parts.tasteGallery = new TasteGallery(filters, settings, (filter) => parts.panel.selectFilter(filter));
  parts.tasteButton = $<HTMLButtonElement>("taste-button");
  parts.autoButton = $<HTMLButtonElement>("auto-adjust");
  parts.batchDialog = new BatchDialog(state.extensions);
  parts.batchDialog.fileName = () => saveOptions.fileName();
  parts.collageDialog = new CollageDialog(state.extensions);
  setupHistory();
  setupAssist();
}

/** 回転・反転した後の原寸の大きさ（画像がなければ null）。 */
function orientedSize(): [number, number] | null {
  const { loaded, settings } = state;
  if (!loaded) return null;
  return settings.orientation.rotation % 180 === 0 ? [loaded.width, loaded.height] : [loaded.height, loaded.width];
}

/** 投稿加工の範囲は「投稿加工」タブを開いていて、切り抜いた表示でないときだけ描く・選べる。切り抜きのガイドは「切り抜き」タブの間だけ描く。 */
function updateOverlays() {
  parts.privacy.setActive(tabs.selected() === "privacy" && !preview.trimmed);
  parts.crop.setGuideActive(tabs.selected() === "crop");
  parts.localPanel.setActive(tabs.selected() === "adjust" && !preview.trimmed);
  parts.textDrag.setActive(!preview.trimmed);
}

/** ボタン・プレビューの操作をつなぐ。 */
function connectControls() {
  dom.resetButton.addEventListener("click", () => void resetImage());
  dom.saveButton.addEventListener("click", () => void saveDialog());
  dom.revealButton.addEventListener("click", () => void revealSaved());
  dom.resumeButton.addEventListener("click", () => void resumeEdits());
  const presetButton = $<HTMLButtonElement>("preset-button");
  presetButton.addEventListener("click", () =>
    showPresetMenu(presetButton).catch((error) => showError("プリセットのメニューを出せません", error)),
  );
  parts.textButton.addEventListener("click", () => {
    if (state.loaded) parts.textDialog.open();
  });
  parts.tasteButton.addEventListener("click", () => {
    if (!state.loaded) return;
    parts.tasteGallery.open(parts.tasteButton).catch((error) => {
      parts.tasteGallery.close();
      void showError("テイストの一覧を作れません", error);
    });
  });
  preview.onResize = () => {
    parts.crop.draw();
    parts.privacy.draw();
    parts.localPanel.draw();
    parts.textDrag.draw();
    if (!dom.straightenGrid.hasAttribute("hidden")) drawStraightenGrid();
    void updateGuide();
    placeBadge();
  };
  dom.canvas.addEventListener("dblclick", (event) => void onPreviewDoubleClick(event));
  setupCompare();
  setupSplit();
  setupEscape();
  tabs.setEnabled("exif", false);
  tabs.onSelect = () => {
    updateOverlays();
    void updateGuide();
  };
  updateOverlays();
  setupDrop();
}

/**
 * Esc キーで範囲の指定を解除する: 投稿加工・部分補正の選んでいる範囲は取り消し、部分補正の「円を足す」などはやめる（そのタブで先に）。それも
 * なければトリミング範囲をクリアする（範囲はどのタブでもプレビューに出ていて、ドラッグで選べるので、どのタブでも）。
 * 文字の入力中・ダイアログを開いているとき・100% 表示の間は、何もしない。
 */
function setupEscape() {
  window.addEventListener("keydown", (event) => {
    if (!isEscapeKey(event) || event.defaultPrevented || !state.loaded || isZoomed()) return;
    if (isEditingText() || document.querySelector("dialog[open]")) return;
    const tab = tabs.selected();
    const handled =
      (tab === "privacy" && parts.privacy.cancelSelected()) ||
      (tab === "adjust" && parts.localPanel.cancel()) ||
      parts.crop.cancelRange();
    if (handled) event.preventDefault();
  });
}

/** Rust からのイベント（メニュー・終了の求め・想定外のエラー・Finder から開くファイル）をつなぐ。 */
async function listenEvents() {
  await listen<string>("menu", (event) => {
    menuActions[event.payload]?.();
    onPresetMenu(event.payload);
  });
  // メニューのチェックを環境設定に残した表示・非表示に合わせる
  setMenuChecked(MENU.histogram, histogramView.shown);
  setMenuChecked(MENU.clipping, clippingShown());
  await listen<{ message: string; logPath: string }>("unexpected-error", (event) =>
    reportUnexpected(event.payload.message, event.payload.logPath),
  );
  // Dock の「終了」など、メニュー以外からの終了の求めも同じく確かめる
  await listen("quit-requested", () => requestQuit());
  // ウィンドウを閉じる（赤いボタン・⌘W）とアプリが終わるので、同じく確かめる
  await getCurrentWindow().onCloseRequested((event) => {
    event.preventDefault();
    void requestQuit();
  });
  await listen<string[]>("open-paths", (event) => {
    if (!state.saving) openPaths(event.payload);
  });
}

/** 計測モード（IMAGEEDITORRT_BENCH）: 計測して結果を出力し、終わる。 */
async function runBench() {
  try {
    // NFR-01: 12MP の JPEG を開いてから、プレビューを描き終えるまで
    const jpeg = await invoke<string>("bench_jpeg_path");
    const start = performance.now();
    showLoaded(await invoke<OpenInfo>("open_path", { path: jpeg }), []);
    await preview.render(state.settings);
    const loadMs = performance.now() - start;
    const load = `読み込み→プレビュー表示（12MP の JPEG）: ${loadMs.toFixed(1)}ms`;
    showLoaded(await invoke<OpenInfo>("open_sample"), []);
    const loaded = state.loaded!;
    const result = await bench(preview, `${loaded.previewWidth}×${loaded.previewHeight}`);
    const text = [load, result.text, await benchSave(preview), verdict(loadMs, result.heavyMax)].join("\n");
    await invoke("report", { text });
  } catch (error) {
    await invoke("report", { text: `計測に失敗しました: ${error}` });
  }
}

async function setup() {
  [state.extensions, state.savableExtensions, state.formatsText] =
    await invoke<[string[], string[], string]>("supported_formats");
  await createParts();
  await loadPresets();
  connectControls();
  await listenEvents();
  if (await invoke<boolean>("bench_mode")) {
    await runBench();
    return;
  }
  // コマンドライン引数・Finder から、画面の準備ができる前に届いたファイル
  openPaths(await invoke<string[]>("take_pending_paths"));
}

window.addEventListener("DOMContentLoaded", () => void setup());
