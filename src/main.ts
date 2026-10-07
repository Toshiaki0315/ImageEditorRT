// ImageEditorRT の画面の入り口: 部品を作り、メニュー・イベントを機能につなぐ。
// 機能は editing（変更と履歴）・view（比較・100% 表示）・files（開く・保存・貼り付け・終了）・
// presetsUi（プリセット）、共有の状態と部品は app にある。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { $, dom, histogramView, hooks, output, parts, preview, state, tabs, zoomView } from "./app";
import { BatchDialog } from "./batchDialog";
import { bench, benchSave, verdict } from "./bench";
import { CropController, degreesText } from "./crop";
import { redo, settingsChanged, setupHistory, undo, userChanged } from "./editing";
import {
  openDialog,
  openPaths,
  paste,
  requestQuit,
  resetImage,
  saveDialog,
  setupDrop,
  showLoaded,
  startBatch,
} from "./files";
import { MENU, setMenuChecked } from "./menus";
import { Panel } from "./panel";
import { PrivacyPanel } from "./privacy";
import { loadPresets, onPresetMenu, showPresetMenu } from "./presetsUi";
import { catchUnexpectedErrors, notify, reportUnexpected, showError } from "./status";
import { TasteGallery } from "./tasteGallery";
import { TextDialog } from "./textDialog";
import type { AspectRatio, CropRect, FilterType, FrameKind, OpenInfo, ShapeType, TextFont, TextPosition } from "./types";
import { fitToWindow, onPreviewDoubleClick, placeBadge, setupCompare, showActualSize, updateGuide } from "./view";

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
  [MENU.batch]: () => void startBatch(),
  [MENU.paste]: () => void paste(),
  [MENU.undo]: undo,
  [MENU.redo]: redo,
  [MENU.text]: () => state.loaded && parts.textDialog.open(),
  [MENU.actualSize]: () => showActualSize(),
  [MENU.fit]: fitToWindow,
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
      updatePrivacyActive();
      settingsChanged();
    },
    () => output.rotate(),
  );
  parts.crop.findTilt = () =>
    invoke<number | null>("auto_straighten", { settings: state.settings }).catch((error) => {
      void showError("傾きを求められません", error);
      return undefined;
    });
  parts.crop.onAutoStraighten = (degrees) =>
    notify(degrees === null ? "傾きが分かりませんでした（水平線や長い直線が見つかりません）" : `傾きを ${degreesText(degrees)} 直しました`);
  const stamps = await invoke<string[]>("stamp_list");
  parts.privacy = new PrivacyPanel(settings, dom.canvas, orientedSize, stamps, userChanged);
  parts.privacy.detectFaces = () =>
    invoke<CropRect[]>("detect_faces", { settings: state.settings }).catch((error) => {
      void showError("顔を認識できません", error);
      return null;
    });
  const coverNames = { blur: "ぼかし", mosaic: "モザイク", stamp: "スタンプ" } as const;
  parts.privacy.onFaces = (count, kind) =>
    notify(count > 0 ? `顔を ${count} 個見つけて、${coverNames[kind]}で隠しました` : "顔が見つかりませんでした");
  const [fonts, positions] = await invoke<[[TextFont, string][], [TextPosition, string][]]>("text_options");
  parts.textDialog = new TextDialog(settings, fonts, positions, userChanged);
  parts.textButton = $<HTMLButtonElement>("text-button");
  parts.tasteGallery = new TasteGallery(filters, settings, (filter) => parts.panel.selectFilter(filter));
  parts.tasteButton = $<HTMLButtonElement>("taste-button");
  parts.batchDialog = new BatchDialog(state.extensions);
  setupHistory();
}

/** 回転・反転した後の原寸の大きさ（画像がなければ null）。 */
function orientedSize(): [number, number] | null {
  const { loaded, settings } = state;
  if (!loaded) return null;
  return settings.orientation.rotation % 180 === 0 ? [loaded.width, loaded.height] : [loaded.height, loaded.width];
}

/** 投稿加工の範囲は「投稿加工」タブを開いていて、切り抜いた表示でないときだけ描く・選べる。 */
function updatePrivacyActive() {
  parts.privacy.setActive(tabs.selected() === "privacy" && !preview.trimmed);
}

/** ボタン・プレビューの操作をつなぐ。 */
function connectControls() {
  dom.resetButton.addEventListener("click", () => void resetImage());
  dom.saveButton.addEventListener("click", () => void saveDialog());
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
    void updateGuide();
    placeBadge();
  };
  dom.canvas.addEventListener("dblclick", (event) => void onPreviewDoubleClick(event));
  setupCompare();
  tabs.setEnabled("exif", false);
  tabs.onSelect = () => {
    updatePrivacyActive();
    void updateGuide();
  };
  updatePrivacyActive();
  setupDrop();
}

/** Rust からのイベント（メニュー・終了の求め・想定外のエラー・Finder から開くファイル）をつなぐ。 */
async function listenEvents() {
  await listen<string>("menu", (event) => {
    menuActions[event.payload]?.();
    onPresetMenu(event.payload);
  });
  // メニューのチェックを環境設定に残した表示・非表示に合わせる
  setMenuChecked(MENU.histogram, histogramView.shown);
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
