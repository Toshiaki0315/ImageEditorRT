// ImageEditorRT の画面。画像を開き、設定を Rust に渡してプレビューを描く。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { message, open, save } from "@tauri-apps/plugin-dialog";
import { bench, benchSave } from "./bench";
import { CropController } from "./crop";
import { OutputSize } from "./output";
import { TextDialog } from "./textDialog";
import { showExif } from "./exif";
import { Panel } from "./panel";
import { Preview } from "./preview";
import { SaveOptionsPanel } from "./saveOptions";
import { Tabs } from "./tabs";
import {
  defaultSettings,
  type AspectRatio,
  type DioramaGuide,
  type EditSettings,
  type FilterType,
  type FrameKind,
  type OpenInfo,
  type ShapeType,
  type TextFont,
  type TextPosition,
} from "./types";

const NO_IMAGE_MESSAGE = "画像が読み込まれていません";
const MULTI_FRAME_NOTE = "複数フレームの画像のため、先頭フレームのみ扱います";
const LOAD_ERROR_TITLE = "画像を読み込めません";
const SAVE_ERROR_TITLE = "保存できません";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const stage = $<HTMLElement>("stage");
const placeholder = $<HTMLElement>("placeholder");
const status = $<HTMLElement>("status");
const saveButton = $<HTMLButtonElement>("save");
/** 「加工」タブの「文字…」のボタン（設定パネルを作った後に取る） */
let textButton: HTMLButtonElement;
const canvas = $<HTMLCanvasElement>("canvas");
const guide = document.getElementById("guide") as unknown as SVGSVGElement;
const SVG = "http://www.w3.org/2000/svg";

const settings: EditSettings = defaultSettings();
let notes: string[] = [];
let loaded: OpenInfo | null = null;
let opening = false;
let saving = false;
let extensions: string[] = [];
let savableExtensions: string[] = [];
let formatsText = "";

const preview = new Preview(stage, canvas, (error) => showError("プレビューを更新できません", error));
const tabs = new Tabs(document.querySelector(".side")!);
let panel: Panel;
let crop: CropController;
let textDialog: TextDialog;
const output = new OutputSize(settings, settingsChanged);
const saveOptions = new SaveOptionsPanel(
  $<HTMLInputElement>("jpeg-quality"),
  $<HTMLOutputElement>("jpeg-quality-value"),
  $<HTMLInputElement>("keep-exif"),
  $<HTMLInputElement>("keep-gps"),
);

/** 設定を変えたとき: プレビューとステータスバー（出力の大きさ）を更新する。 */
function settingsChanged() {
  if (!loaded) return;
  // 出力の幅・高さは範囲・フレームなどで変わるので、先に合わせてからプレビューを描く
  void output.refresh().then(() => {
    preview.request(settings);
    void updateStatus();
    void updateGuide();
  });
}

/** 「ジオラマ」タブを開いている間、プレビューにピントの帯のガイドを重ねる（ぼかしが 0 でも出す）。 */
async function updateGuide() {
  if (!loaded || tabs.selected() !== "diorama") {
    guide.toggleAttribute("hidden", true);
    return;
  }
  const result = await invoke<DioramaGuide>("diorama_guide", { settings, trimmed: preview.trimmed });
  const { width, height } = canvas;
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

/** ステータスバー: ファイル名・原寸・出力の大きさ（と、読み込みのときのお知らせ・extra）。 */
async function updateStatus(extra?: string) {
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
  const all = extra ? [...notes, extra] : notes;
  if (all.length) text += `（${all.join("／")}）`;
  status.textContent = text;
}

/** 画像を開く。読めなければダイアログで知らせ、それまでの画像はそのまま残す。 */
async function openPath(path: string, openNotes: string[] = []) {
  if (opening || saving) return;
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
  if (saving) return;
  const path = await open({
    title: "画像を開く",
    multiple: false,
    directory: false,
    filters: [{ name: "画像ファイル", extensions }],
  });
  if (typeof path === "string") await openPath(path);
}

type SaveFailure = { kind: "sameFile" | "extension" | "other"; message: string };

/** 保存: ダイアログで保存先を選び、原寸で処理して書き出す（処理は Rust の別のスレッド）。 */
async function saveDialog() {
  if (!loaded || saving) return;
  const defaultPath = (await invoke<string | null>("default_save_path")) ?? `${loaded.name}_edited.png`;
  // 元の画像と同じファイルが選ばれたら、知らせてダイアログを開き直す（旧版 FR-IO-11）
  for (;;) {
    const path = await save({
      title: "保存",
      defaultPath,
      filters: [{ name: "画像ファイル", extensions: savableExtensions }],
    });
    if (!path) return;
    const result = await saveTo(path);
    if (result !== "sameFile") return;
  }
}

/** 保存する。元の画像と同じファイルなら "sameFile" を返す。 */
async function saveTo(path: string): Promise<"done" | "sameFile" | "failed"> {
  const name = path.split("/").pop() ?? path;
  setSaving(true);
  void updateStatus(`保存中… ${name}`);
  try {
    await invoke<string>("save_image", { path, settings, options: saveOptions.value() });
    void updateStatus(`保存しました: ${name}`);
    return "done";
  } catch (error) {
    void updateStatus();
    const failure = error as SaveFailure;
    if (failure?.kind === "sameFile") {
      await showError(SAVE_ERROR_TITLE, failure.message);
      return "sameFile";
    }
    if (failure?.kind === "extension") {
      await showError(SAVE_ERROR_TITLE, failure.message, true);
    } else {
      await showError(SAVE_ERROR_TITLE, `${name}\n(${failure?.message ?? error})`);
    }
    return "failed";
  } finally {
    setSaving(false);
  }
}

/** 保存中は保存・開く・ドロップを受け付けない（画面は固まらない）。 */
function setSaving(value: boolean) {
  saving = value;
  saveButton.disabled = saving || !loaded;
}

function showLoaded(info: OpenInfo, openNotes: string[]) {
  loaded = info;
  saveButton.disabled = false;
  Object.assign(settings, defaultSettings());
  panel.show();
  placeholder.hidden = true;
  preview.trimmed = false;
  preview.show(info.previewWidth, info.previewHeight, info.hasAlpha);
  crop.reset([info.width, info.height]);
  output.reset(true);
  textDialog.show();
  textButton.disabled = false;
  showExif($("page-exif"), info.exif);
  tabs.setEnabled("exif", info.exif.entries.length > 0);
  notes = info.frameCount > 1 ? [...openNotes, MULTI_FRAME_NOTE] : openNotes;
  settingsChanged();
}

/** ドロップ: 対応形式のときだけハイライトし、受け付ける（旧版 FR-UI-02）。 */
function setupDrop() {
  let accepted = false;
  void getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter") {
      accepted = payload.paths.length > 0 && isSupported(payload.paths[0]);
    } else if (payload.type === "drop") {
      if (accepted && !saving) openPaths(payload.paths);
      accepted = false;
    } else if (payload.type === "leave") {
      accepted = false;
    }
    stage.classList.toggle("dragging", accepted && (payload.type === "enter" || payload.type === "over"));
  });
}

async function setup() {
  [extensions, savableExtensions, formatsText] = await invoke<[string[], string[], string]>("supported_formats");
  const filters = await invoke<[FilterType, string][]>("filter_types");
  panel = new Panel($("page-adjust"), $("page-diorama"), filters, settings, settingsChanged);
  const ratios = await invoke<[AspectRatio, string][]>("aspect_ratios");
  const [frames, shapes] = await invoke<[[FrameKind, string][], [ShapeType, string][]]>("frame_shape_types");
  crop = new CropController(
    settings,
    canvas,
    ratios,
    frames,
    shapes,
    settingsChanged,
    (trimmed) => {
      preview.trimmed = trimmed;
      settingsChanged();
    },
    () => output.rotate(),
  );
  const [fonts, positions] = await invoke<[[TextFont, string][], [TextPosition, string][]]>("text_options");
  textDialog = new TextDialog(settings, fonts, positions, settingsChanged);
  textButton = $<HTMLButtonElement>("text-button");
  textButton.addEventListener("click", () => {
    if (loaded) textDialog.open();
  });
  preview.onResize = () => {
    crop.draw();
    void updateGuide();
  };
  saveButton.addEventListener("click", () => void saveDialog());
  tabs.setEnabled("exif", false);
  tabs.onSelect = () => void updateGuide();
  setupDrop();
  await listen<string>("menu", (event) => {
    if (event.payload === "open") void openDialog();
    if (event.payload === "save") void saveDialog();
    if (event.payload === "text" && loaded) textDialog.open();
  });
  await listen<string[]>("open-paths", (event) => {
    if (!saving) openPaths(event.payload);
  });

  // IMAGEEDITORRT_BENCH を付けて起動したときは、計測して結果を出力して終わる
  if (await invoke<boolean>("bench_mode")) {
    try {
      showLoaded(await invoke<OpenInfo>("open_sample"), []);
      const result = await bench(preview, `${loaded!.previewWidth}×${loaded!.previewHeight}`);
      await invoke("report", { text: `${result}\n${await benchSave(preview)}` });
    } catch (error) {
      await invoke("report", { text: `計測に失敗しました: ${error}` });
    }
    return;
  }
  // コマンドライン引数・Finder から、画面の準備ができる前に届いたファイル
  openPaths(await invoke<string[]>("take_pending_paths"));
}

window.addEventListener("DOMContentLoaded", () => void setup());
