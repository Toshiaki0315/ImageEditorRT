// ImageEditorRT の画面。画像を開き、設定を Rust に渡してプレビューを描く。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { message, open } from "@tauri-apps/plugin-dialog";
import { bench } from "./bench";
import { showExif } from "./exif";
import { Panel } from "./panel";
import { Preview } from "./preview";
import { Tabs } from "./tabs";
import { defaultSettings, type EditSettings, type OpenInfo } from "./types";

const NO_IMAGE_MESSAGE = "画像が読み込まれていません";
const MULTI_FRAME_NOTE = "複数フレームの画像のため、先頭フレームのみ扱います";
const LOAD_ERROR_TITLE = "画像を読み込めません";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const stage = $<HTMLElement>("stage");
const placeholder = $<HTMLElement>("placeholder");
const status = $<HTMLElement>("status");

const settings: EditSettings = defaultSettings();
let notes: string[] = [];
let loaded: OpenInfo | null = null;
let opening = false;
let extensions: string[] = [];
let formatsText = "";

const preview = new Preview(stage, $<HTMLCanvasElement>("canvas"), (error) => showError("プレビューを更新できません", error));
const tabs = new Tabs(document.querySelector(".side")!);
const panel = new Panel($("page-adjust"), $("page-diorama"), settings, settingsChanged);

/** 設定を変えたとき: プレビューとステータスバー（出力の大きさ）を更新する。 */
function settingsChanged() {
  if (!loaded) return;
  preview.request(settings);
  void updateStatus();
}

function extensionOf(path: string): string {
  const name = path.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

const isSupported = (path: string) => extensions.includes(extensionOf(path));

async function showError(title: string, error: unknown, withFormats = false) {
  let text = String(error);
  if (withFormats) text += `\n\n対応形式: ${formatsText}`;
  await message(text, { title, kind: "warning" });
}

/** ステータスバー: ファイル名・原寸・出力の大きさ（と、読み込みのときのお知らせ）。 */
async function updateStatus() {
  if (!loaded) {
    status.textContent = notes.length ? notes.join("／") : NO_IMAGE_MESSAGE;
    return;
  }
  let output = "出力 —";
  try {
    const [width, height] = await invoke<[number, number]>("output_size", { settings });
    output = `出力 ${width}×${height} px`;
  } catch {
    // 大きさの指定が範囲外のときは「—」
  }
  let text = `${loaded.name} ｜ 原寸 ${loaded.width}×${loaded.height} px ｜ ${output}`;
  if (notes.length) text += `（${notes.join("／")}）`;
  status.textContent = text;
}

/** 画像を開く。読めなければダイアログで知らせ、それまでの画像はそのまま残す。 */
async function openPath(path: string, openNotes: string[] = []) {
  if (opening) return;
  opening = true;
  try {
    const info = await invoke<OpenInfo>("open_path", { path });
    showLoaded(info, openNotes);
  } catch (error) {
    await showError(LOAD_ERROR_TITLE, error, true);
  } finally {
    opening = false;
  }
}

/** ドロップ・Finder などから届いたファイル。複数なら先頭の 1 枚だけを開く（旧版 FR-UI-03）。 */
function openPaths(paths: string[]) {
  if (paths.length === 0) return;
  const openNotes = paths.length > 1 ? [`${paths.length} 件中、先頭の 1 枚のみ読み込みました`] : [];
  void openPath(paths[0], openNotes);
}

async function openDialog() {
  const path = await open({
    title: "画像を開く",
    multiple: false,
    directory: false,
    filters: [{ name: "画像ファイル", extensions }],
  });
  if (typeof path === "string") await openPath(path);
}

function showLoaded(info: OpenInfo, openNotes: string[]) {
  loaded = info;
  Object.assign(settings, defaultSettings());
  panel.show();
  placeholder.hidden = true;
  preview.show(info.previewWidth, info.previewHeight, info.hasAlpha);
  showExif($("page-exif"), info.exif);
  tabs.setEnabled("exif", info.exif.entries.length > 0);
  notes = info.frameCount > 1 ? [...openNotes, MULTI_FRAME_NOTE] : openNotes;
  void updateStatus();
  preview.request(settings);
}

/** ドロップ: 対応形式のときだけハイライトし、受け付ける（旧版 FR-UI-02）。 */
function setupDrop() {
  let accepted = false;
  void getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter") {
      accepted = payload.paths.length > 0 && isSupported(payload.paths[0]);
    } else if (payload.type === "drop") {
      if (accepted) openPaths(payload.paths);
      accepted = false;
    } else if (payload.type === "leave") {
      accepted = false;
    }
    stage.classList.toggle("dragging", accepted && (payload.type === "enter" || payload.type === "over"));
  });
}

async function setup() {
  [extensions, formatsText] = await invoke<[string[], string]>("supported_formats");
  tabs.setEnabled("exif", false);
  setupDrop();
  await listen<string>("menu", (event) => {
    if (event.payload === "open") void openDialog();
  });
  await listen<string[]>("open-paths", (event) => openPaths(event.payload));

  // IMAGEEDITORRT_BENCH を付けて起動したときは、計測して結果を出力して終わる
  if (await invoke<boolean>("bench_mode")) {
    try {
      showLoaded(await invoke<OpenInfo>("open_sample"), []);
      const result = await bench(preview, `${loaded!.previewWidth}×${loaded!.previewHeight}`);
      await invoke("report", { text: result });
    } catch (error) {
      await invoke("report", { text: `計測に失敗しました: ${error}` });
    }
    return;
  }
  // コマンドライン引数・Finder から、画面の準備ができる前に届いたファイル
  openPaths(await invoke<string[]>("take_pending_paths"));
}

window.addEventListener("DOMContentLoaded", () => void setup());
