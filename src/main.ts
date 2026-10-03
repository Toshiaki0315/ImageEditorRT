// ImageEditorRT の画面。画像を開き、設定を Rust に渡してプレビューを描く。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ask, message, open, save } from "@tauri-apps/plugin-dialog";
import { bench, benchSave } from "./bench";
import { BatchDialog } from "./batchDialog";
import { type AspectState, CropController } from "./crop";
import { OutputSize, type SizeState } from "./output";
import { TextDialog } from "./textDialog";
import { ExifView } from "./exif";
import { HistogramView } from "./histogram";
import { HistoryRecorder, sameValue } from "./history";
import { Panel } from "./panel";
import { Preview } from "./preview";
import { SaveOptionsPanel } from "./saveOptions";
import { Tabs } from "./tabs";
import { ZoomView } from "./zoom";
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
const zoomGuide = document.getElementById("zoom-guide") as unknown as SVGSVGElement;
/** 全体表示のプレビュー（canvas・範囲の選択・ガイド）。100% 表示の間は隠す */
const frame = document.querySelector<HTMLElement>(".frame")!;
const badge = $<HTMLElement>("badge");
const beforeButton = $<HTMLButtonElement>("before");
const resetButton = $<HTMLButtonElement>("reset");
const DISCARD_TITLE = "未保存の変更";
const DISCARD_QUESTION = "保存していない変更があります。破棄してよろしいですか？";
/** 100% 表示で、設定の変更が落ち着いてから処理し直すまでの時間 (ms) */
const ZOOM_DELAY_MS = 300;
/** 画像の左上から「加工前」などの表示までの間隔 (px) */
const BADGE_MARGIN = 8;
const SVG = "http://www.w3.org/2000/svg";

const settings: EditSettings = defaultSettings();
let notes: string[] = [];
let loaded: OpenInfo | null = null;
let opening = false;
let saving = false;
let extensions: string[] = [];
let savableExtensions: string[] = [];
let formatsText = "";
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
/** 最後に保存したときの設定（未保存の変更の判定に使う） */
let savedSettings: EditSettings | null = null;
/** まとめて処理のダイアログ（対応形式の一覧を読んでから作る） */
let batchDialog: BatchDialog;
/** 画像を開いていないときの、まとめて処理の長辺の初期値 */
const DEFAULT_BATCH_LONG_SIDE = 2048;
/** プリセットの名前の一覧（「プリセット ▾」のメニューに出す） */
let presetNames: string[] = [];
/** スライダーをドラッグしている間（履歴に積むのを離すまで待つ） */
let sliderDragging = false;

/** アンドゥ／リドゥで戻す、設定パネルの状態（旧版の PanelState）。 */
type Snapshot = { settings: EditSettings; aspect: AspectState; size: SizeState };
let recorder: HistoryRecorder<Snapshot>;

const preview = new Preview(stage, canvas, (error) => showError("プレビューを更新できません", error));
const zoomView = new ZoomView(
  stage,
  $<HTMLElement>("zoom"),
  $<HTMLElement>("zoom-image"),
  $<HTMLCanvasElement>("zoom-canvas"),
);
zoomView.onMove = () => placeBadge();
zoomView.onDoubleClick = () => fitToWindow();
const histogramView = new HistogramView($<HTMLCanvasElement>("histogram"));
preview.onHistogram = (histogram) => histogramView.set(loaded ? histogram : null);
const tabs = new Tabs(document.querySelector(".side")!);
const exifView = new ExifView($("page-exif"));
let panel: Panel;
let crop: CropController;
let textDialog: TextDialog;
const output = new OutputSize(settings, userChanged);
const saveOptions = new SaveOptionsPanel(
  $<HTMLInputElement>("jpeg-quality"),
  $<HTMLOutputElement>("jpeg-quality-value"),
  $<HTMLInputElement>("keep-exif"),
  $<HTMLInputElement>("keep-gps"),
);

/** 画面で設定を変えたとき: 履歴に積む（落ち着いてから）と、プレビューなどの更新。 */
function userChanged() {
  recorder?.changed();
  settingsChanged();
}

/** 設定が変わったとき: プレビューとステータスバー（出力の大きさ）を更新する。 */
function settingsChanged() {
  if (!loaded) return;
  // 出力の幅・高さは範囲・フレームなどで変わるので、先に合わせてからプレビューを描く
  void output.refresh().then(() => {
    preview.request(settings);
    void updateStatus();
    void updateGuide();
  });
  if (zoomed) {
    // 100% 表示中は、変更が落ち着いてから原寸で処理し直す
    clearTimeout(zoomTimer);
    zoomTimer = setTimeout(() => void renderZoom(), ZOOM_DELAY_MS);
  }
}

// --- 元に戻す／やり直す・リセット（旧版 FR-UI-42・43） --------------------------------

/** 今の設定パネルの状態。出力の幅・高さは出力の欄の状態から決まるので、設定の側には持たない。 */
function snapshot(): Snapshot {
  return {
    settings: { ...structuredClone(settings), width: null, height: null },
    aspect: crop.aspectState(),
    size: output.snapshot(),
  };
}

/** 履歴の状態を設定パネルと設定に戻す（比の固定で範囲を直したりしない）。 */
function restore(state: Snapshot) {
  if (!loaded) return;
  Object.assign(settings, structuredClone(state.settings));
  crop.restore(state.aspect, [loaded.width, loaded.height]);
  output.restore(state.size);
  panel.show();
  textDialog.show();
  settingsChanged();
}

/** 入力欄で文字を編集中か（そのときの ⌘Z・⇧⌘Z は入力欄の文字に効かせる）。 */
function isEditingText(): boolean {
  const active = document.activeElement;
  if (active instanceof HTMLTextAreaElement) return true;
  return active instanceof HTMLInputElement && ["text", "number", "search"].includes(active.type);
}

function undo() {
  if (isEditingText()) {
    document.execCommand("undo");
    return;
  }
  if (loaded && !saving) recorder.undo();
}

function redo() {
  if (isEditingText()) {
    document.execCommand("redo");
    return;
  }
  if (loaded && !saving) recorder.redo();
}

/** 初期状態から設定を変えていて、その設定でまだ保存していなければ true。 */
function hasUnsavedChanges(): boolean {
  return loaded !== null && !sameValue(settings, defaultSettings()) && !sameValue(settings, savedSettings);
}

/** 未保存の変更があれば、破棄してよいかを確かめる。 */
async function confirmDiscard(): Promise<boolean> {
  if (!hasUnsavedChanges()) return true;
  return ask(DISCARD_QUESTION, { title: DISCARD_TITLE, kind: "warning", okLabel: "破棄", cancelLabel: "キャンセル" });
}

/** リセット: 画像と設定を未読込の状態に戻す（未保存の変更があれば確かめる）。 */
async function resetImage() {
  if (!loaded || saving || opening || !(await confirmDiscard())) return;
  setComparing(false);
  fitToWindow();
  await invoke("close_image");
  loaded = null;
  savedSettings = null;
  notes = [];
  preview.clear();
  Object.assign(settings, defaultSettings());
  panel.show();
  preview.trimmed = false;
  crop.reset(null);
  output.reset(false);
  textDialog.close();
  textDialog.show();
  textButton.disabled = true;
  exifView.show(null);
  tabs.setEnabled("exif", false);
  histogramView.set(null);
  placeholder.hidden = false;
  saveButton.disabled = beforeButton.disabled = resetButton.disabled = true;
  void updateGuide();
  recorder.reset();
  updateMenus();
  void updateStatus();
}

// --- プリセット（旧版 FR-UI-59） ----------------------------------------------------

/**
 * 「プリセット ▾」のメニューをボタンの下に出す（メニューは Rust で作る）。選んだ項目は "menu" のイベントで届く:
 * "preset-apply:<番号>"（当てはめる）・"preset-save"（保存…）・"preset-delete:<番号>"（削除）。
 */
async function showPresetMenu(button: HTMLElement) {
  const rect = button.getBoundingClientRect();
  await invoke("show_preset_menu", { x: rect.left, y: rect.bottom, loaded: loaded !== null });
}

/** "menu" のイベントのうち、プリセットのメニューの項目を処理する。 */
function onPresetMenu(id: string) {
  const index = (prefix: string) => Number(id.slice(prefix.length));
  if (id === "preset-save") void savePresetDialog();
  if (id.startsWith("preset-apply:")) {
    const name = presetNames[index("preset-apply:")];
    if (name !== undefined) void applyPreset(name);
  }
  if (id.startsWith("preset-delete:")) {
    const name = presetNames[index("preset-delete:")];
    if (name !== undefined) void deletePreset(name);
  }
}

/** プリセットの加工を当てはめる（サイズ・範囲・向きはそのまま）。1 回の操作として元に戻せる。 */
async function applyPreset(name: string) {
  if (!loaded || saving) return;
  try {
    Object.assign(settings, await invoke<EditSettings>("apply_preset", { name, settings }));
  } catch (error) {
    await showError("プリセットを当てはめられません", error);
    return;
  }
  panel.show();
  textDialog.show();
  // フレーム・円の比が変わったら、手で選んだときと同じく範囲をその比に直す
  await crop.refit();
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
  if (!loaded) return;
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
    presetNames = await invoke<string[]>("save_preset", { name, settings });
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
    presetNames = await invoke<string[]>("delete_preset", { name });
    void updateStatus(`プリセット「${name}」を削除しました`);
  } catch (error) {
    await showError("プリセットを保存できません", error);
  }
}

// --- まとめて処理（旧版 FR-UI-45） --------------------------------------------------

/** まとめて処理のダイアログを開き、「開始」なら 1 枚ずつ処理して結果を知らせる（実行中は保存・開くを止める）。 */
async function startBatch() {
  if (saving || opening) return;
  const result = await batchDialog.run({
    presets: presetNames,
    longSide: loaded ? output.longSide() : DEFAULT_BATCH_LONG_SIDE,
    resize: loaded !== null && (settings.width !== null || settings.height !== null),
    settings: structuredClone(settings),
    save: saveOptions.value(),
    onBusy: setSaving,
  }).catch(async (error) => {
    await showError("まとめて処理できません", error);
    return null;
  });
  if (!result) return;
  void updateStatus(`まとめて処理: ${result.saved} 枚を保存しました`);
  await message(result.message, { title: "まとめて処理", kind: "info" });
}

// --- 加工前との比較（旧版 FR-UI-44） ------------------------------------------------

/** 加工前の表示を切り替える（押している間だけ true）。設定は変えない。 */
function setComparing(value: boolean) {
  value = value && loaded !== null;
  if (value === comparing) return;
  comparing = value;
  preview.comparing = value;
  updateBadge();
  void updateGuide();
  preview.request(settings);
  if (zoomed) void renderZoom();
}

/** \ キー（JIS 配列の ¥ キーも）。どの入力欄にフォーカスがあっても効き、文字としては入らない。 */
function isCompareKey(event: KeyboardEvent): boolean {
  return event.key === "\\" || event.key === "¥" || event.code === "Backslash" || event.code === "IntlYen";
}

function setupCompare() {
  for (const type of ["keydown", "keyup"] as const) {
    window.addEventListener(
      type,
      (event) => {
        if (!loaded || !isCompareKey(event) || event.metaKey || event.ctrlKey) return;
        event.preventDefault();
        event.stopPropagation();
        if (!event.repeat) setComparing(type === "keydown");
      },
      true,
    );
  }
  // キーを押したまま別のウィンドウに切り替えると離したことが届かないので、ここで戻す
  window.addEventListener("blur", () => setComparing(false));
  beforeButton.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    beforeButton.setPointerCapture(event.pointerId);
    setComparing(true);
  });
  for (const type of ["pointerup", "pointercancel", "lostpointercapture"]) {
    beforeButton.addEventListener(type, () => setComparing(false));
  }
}

// --- 100% 表示（旧版 FR-UI-48） -----------------------------------------------------

/** 100% 表示にする。center は表示の中央にしたい点（保存結果の画像の座標。省略時は画像の中央）。 */
function showActualSize(center: [number, number] | null = null) {
  if (!loaded) return;
  zoomed = true;
  zoomCenter = center;
  void renderZoom();
  updateMenus();
}

/** 画面に合わせた表示に戻す。 */
function fitToWindow() {
  if (!zoomed) return;
  zoomed = false;
  zoomGeneration += 1; // 処理中の結果は使わない
  zoomPending = false;
  clearTimeout(zoomTimer);
  zoomCenter = null;
  zoomView.hide();
  frame.hidden = false;
  preview.fit();
  updateBadge();
  void updateGuide();
  updateMenus();
}

/** 今の設定（加工前の表示中なら加工前）で原寸の処理を始め、終わったら表示する。 */
async function renderZoom() {
  clearTimeout(zoomTimer);
  if (!zoomed || !loaded) return;
  const generation = ++zoomGeneration;
  zoomPending = true;
  updateBadge();
  try {
    const buffer = await invoke<ArrayBuffer>("render_actual_size", { settings, comparing });
    if (generation !== zoomGeneration) return; // 古い依頼の結果（設定がその後変わった）
    const header = new DataView(buffer, 0, 8);
    const width = header.getUint32(0, true);
    const height = header.getUint32(4, true);
    zoomView.show(width, height, new Uint8ClampedArray(buffer, 8, width * height * 4), zoomCenter);
    zoomCenter = null;
    frame.hidden = true;
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
async function onPreviewDoubleClick(event: MouseEvent) {
  if (!loaded || zoomed || !preview.trimmed) return;
  const rect = canvas.getBoundingClientRect();
  if (rect.width === 0 || rect.height === 0) return;
  const [width, height] = await invoke<[number, number]>("output_size", { settings });
  showActualSize([((event.clientX - rect.left) / rect.width) * width, ((event.clientY - rect.top) / rect.height) * height]);
}

/** メニューの項目を使える・使えないにする（前と同じなら送らない）。 */
const menuEnabled = new Map<string, boolean>();
function setMenuEnabled(id: string, enabled: boolean) {
  if (menuEnabled.get(id) === enabled) return;
  menuEnabled.set(id, enabled);
  void invoke("set_menu_enabled", { id, enabled });
}

/** メニューの「元に戻す」「やり直す」「100% で表示」「画面に合わせる」を、今の状態に合わせる。 */
function updateMenus() {
  const editable = loaded !== null && !saving;
  setMenuEnabled("undo", editable && (recorder?.canUndo() ?? false));
  setMenuEnabled("redo", editable && (recorder?.canRedo() ?? false));
  setMenuEnabled("actual_size", loaded !== null && !zoomed);
  setMenuEnabled("fit", zoomed);
}

/** 左上の表示（「加工前」「100%」「更新中…」）をまとめて出す。 */
function updateBadge() {
  const parts: string[] = [];
  if (comparing) parts.push("加工前");
  if (zoomed) {
    parts.push("100%");
    if (zoomPending) parts.push("更新中…");
  }
  badge.textContent = parts.join(" ・ ");
  badge.hidden = parts.length === 0;
  placeBadge();
}

/** 表示している画像の左上に置く（100% 表示で画像が左上にはみ出していても、見える位置に出す）。 */
function placeBadge() {
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

/**
 * 「ジオラマ」タブを開いている間、プレビューにピントの帯のガイドを重ねる（ぼかしが 0 でも出す）。
 * 加工前の表示中は出さない。100% 表示では保存結果の写真の部分に対する位置に出す。
 */
async function updateGuide() {
  const zoomCanvas = zoomView.active ? $<HTMLCanvasElement>("zoom-canvas") : null;
  const shown = loaded !== null && tabs.selected() === "diorama" && !comparing;
  guide.toggleAttribute("hidden", true);
  zoomGuide.toggleAttribute("hidden", true);
  if (!shown) return;
  const result = await invoke<DioramaGuide>("diorama_guide", {
    settings,
    trimmed: preview.trimmed,
    zoomed: zoomCanvas !== null,
  });
  drawGuide(zoomCanvas ? zoomGuide : guide, result, zoomCanvas ?? canvas);
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
  // 未保存の変更があれば確かめ、キャンセルされたら開かない
  if (!(await confirmDiscard())) {
    opening = false;
    return;
  }
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
    const saved = structuredClone(settings);
    await invoke<string>("save_image", { path, settings: saved, options: saveOptions.value() });
    savedSettings = saved;
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
  saveButton.disabled = resetButton.disabled = saving || !loaded;
  updateMenus();
}

function showLoaded(info: OpenInfo, openNotes: string[]) {
  // 画像を読み込んだら、加工後・画面に合わせた表示に戻す
  setComparing(false);
  fitToWindow();
  loaded = info;
  savedSettings = null;
  beforeButton.disabled = resetButton.disabled = false;
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
  exifView.show(info.exif);
  tabs.setEnabled("exif", !info.exif.empty);
  notes = info.frameCount > 1 ? [...openNotes, MULTI_FRAME_NOTE] : openNotes;
  settingsChanged();
  // 読み込んだ状態を履歴の始まりにする
  recorder.reset();
  updateMenus();
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
  panel = new Panel($("page-adjust"), $("page-diorama"), filters, settings, userChanged);
  const ratios = await invoke<[AspectRatio, string][]>("aspect_ratios");
  const [frames, shapes] = await invoke<[[FrameKind, string][], [ShapeType, string][]]>("frame_shape_types");
  crop = new CropController(
    settings,
    canvas,
    ratios,
    frames,
    shapes,
    userChanged,
    (trimmed) => {
      preview.trimmed = trimmed;
      settingsChanged();
    },
    () => output.rotate(),
  );
  const [fonts, positions] = await invoke<[[TextFont, string][], [TextPosition, string][]]>("text_options");
  textDialog = new TextDialog(settings, fonts, positions, userChanged);
  recorder = new HistoryRecorder<Snapshot>({
    snapshot,
    restore,
    isAdjusting: () => sliderDragging || crop.isDragging(),
    onUpdate: updateMenus,
  });
  // スライダーをドラッグしている間は履歴に積まない（離したら 1 回の操作として積む）
  document.addEventListener("pointerdown", (event) => {
    if (event.target instanceof HTMLInputElement && event.target.type === "range") sliderDragging = true;
  });
  for (const type of ["pointerup", "pointercancel"]) {
    document.addEventListener(type, () => (sliderDragging = false));
  }
  resetButton.addEventListener("click", () => void resetImage());
  batchDialog = new BatchDialog(extensions);
  const presetButton = $<HTMLButtonElement>("preset-button");
  presetButton.addEventListener("click", () =>
    showPresetMenu(presetButton).catch((error) => showError("プリセットのメニューを出せません", error)),
  );
  // プリセットを読む。ファイルが壊れていたら知らせ、プリセットなしで使えるようにする
  const presetResult = await invoke<{ names: string[]; error: string | null }>("load_presets");
  presetNames = presetResult.names;
  if (presetResult.error) void showError("プリセットを読み込めません", presetResult.error);
  textButton = $<HTMLButtonElement>("text-button");
  textButton.addEventListener("click", () => {
    if (loaded) textDialog.open();
  });
  preview.onResize = () => {
    crop.draw();
    void updateGuide();
    placeBadge();
  };
  canvas.addEventListener("dblclick", (event) => void onPreviewDoubleClick(event));
  setupCompare();
  saveButton.addEventListener("click", () => void saveDialog());
  tabs.setEnabled("exif", false);
  tabs.onSelect = () => void updateGuide();
  setupDrop();
  await listen<string>("menu", (event) => {
    if (event.payload === "open") void openDialog();
    if (event.payload === "batch") void startBatch();
    if (event.payload === "save") void saveDialog();
    if (event.payload === "text" && loaded) textDialog.open();
    onPresetMenu(event.payload);
    if (event.payload === "undo") undo();
    if (event.payload === "redo") redo();
    if (event.payload === "actual_size") showActualSize();
    if (event.payload === "fit") fitToWindow();
    if (event.payload === "histogram") {
      histogramView.setShown(!histogramView.shown);
      void invoke("set_menu_checked", { id: "histogram", checked: histogramView.shown });
    }
  });
  // メニューのチェックを環境設定に残した表示・非表示に合わせる
  void invoke("set_menu_checked", { id: "histogram", checked: histogramView.shown });
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
