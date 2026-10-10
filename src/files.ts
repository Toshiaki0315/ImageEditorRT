// 画像を開く・保存する・貼り付ける・リセット・終了・まとめて処理（ファイルと画像の出し入れ）。

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { message, open, save } from "@tauri-apps/plugin-dialog";
import {
  dom,
  exifView,
  histogramView,
  isSupported,
  output,
  parts,
  preview,
  saveOptions,
  state,
  tabs,
} from "./app";
import { confirmDiscard, isEditingText, settingsChanged } from "./editing";
import { updateMenus } from "./menus";
import { type ClipboardContents, pasteAction, pasteStamp } from "./paste";
import { SIZE_PRESETS } from "./output";
import { presetNames } from "./presetsUi";
import { parseSizes, savedSummary } from "./sizes";
import { notify, showError, updateStatus } from "./status";
import { defaultSettings, type EditSettings, type OpenInfo } from "./types";
import { fitToWindow, setComparing, stopSplit, updateGuide } from "./view";
import { readStored, writeStored } from "./storage";

const MULTI_FRAME_NOTE = "複数フレームの画像のため、先頭フレームのみ扱います";
const LOAD_ERROR_TITLE = "画像を読み込めません";
const SAVE_ERROR_TITLE = "保存できません";
const NOTHING_TO_PASTE_MESSAGE = "クリップボードに画像がありません";
/** 画像を開いていないときの、まとめて処理の長辺の初期値 */
const DEFAULT_BATCH_LONG_SIDE = 2048;

// --- 開く ---------------------------------------------------------------------------

/** 読み込んだ画像を表示し、設定・履歴を初期状態にする（加工後・画面に合わせた表示に戻す）。 */
export function showLoaded(info: OpenInfo, openNotes: string[]) {
  setComparing(false);
  fitToWindow();
  state.loaded = info;
  state.savedSettings = null;
  dom.beforeButton.disabled = dom.resetButton.disabled = dom.saveButton.disabled = false;
  Object.assign(state.settings, defaultSettings());
  state.facesPrepared = false;
  parts.panel.show();
  dom.placeholder.hidden = true;
  preview.trimmed = false;
  preview.show(info.previewWidth, info.previewHeight, info.hasAlpha);
  parts.crop.reset([info.width, info.height]);
  parts.privacy.reset();
  dom.revealButton.hidden = true;
  parts.localPanel.reset();
  output.reset(true);
  parts.textDialog.show();
  parts.textButton.disabled = parts.tasteButton.disabled = parts.autoButton.disabled = false;
  exifView.show(info.exif);
  tabs.setEnabled("exif", !info.exif.empty);
  state.notes = info.frameCount > 1 ? [...openNotes, MULTI_FRAME_NOTE] : openNotes;
  settingsChanged();
  // 読み込んだ状態を履歴の始まりにする
  parts.recorder.reset();
  updateMenus();
}

/** 画像を開く。読めなければダイアログで知らせ、それまでの画像はそのまま残す。 */
async function openPath(path: string, openNotes: string[] = []) {
  if (state.opening || state.saving) return;
  state.opening = true;
  try {
    // 未保存の変更があれば確かめ、キャンセルされたら開かない
    if (!(await confirmDiscard())) return;
    showLoaded(await invoke<OpenInfo>("open_path", { path }), openNotes);
  } catch (error) {
    await showError(LOAD_ERROR_TITLE, error, true);
  } finally {
    state.opening = false;
  }
}

/** フォルダの中の前後の写真（Rust の batch::Neighbor）。number は何枚目か（1 から）。 */
type Neighbor = { path: string; number: number; total: number };

/** 同じフォルダの、次（step = 1）・前（step = -1）の写真を開く（未保存の変更があれば確かめる）。 */
export async function openNeighbor(step: 1 | -1) {
  if (!state.loaded || state.opening || state.saving) return;
  let neighbor: Neighbor | null;
  try {
    neighbor = await invoke<Neighbor | null>("neighbor_image", { step });
  } catch (error) {
    await showError(LOAD_ERROR_TITLE, error);
    return;
  }
  if (!neighbor) {
    notify(step > 0 ? "このフォルダに、これより後の写真はありません" : "このフォルダに、これより前の写真はありません");
    return;
  }
  await openPath(neighbor.path);
  if (state.loaded?.name === neighbor.path.split("/").pop()) {
    notify(`フォルダの ${neighbor.number} / ${neighbor.total} 枚目`);
  }
}

/** ドロップ・Finder などから届いたファイル。複数なら先頭の 1 枚だけを開く（旧版 FR-UI-03）。 */
export function openPaths(paths: string[]) {
  if (paths.length === 0) return;
  const openNotes = paths.length > 1 ? [`${paths.length} 件中、先頭の 1 枚のみ読み込みました`] : [];
  void openPath(paths[0], openNotes);
}

export async function openDialog() {
  if (state.saving) return;
  const path = await open({
    title: "画像を開く",
    multiple: false,
    directory: false,
    filters: [{ name: "画像ファイル", extensions: state.extensions }],
  });
  if (typeof path === "string") await openPath(path);
}

/** ドロップ: 対応形式のときだけハイライトし、受け付ける（旧版 FR-UI-02）。 */
export function setupDrop() {
  let accepted = false;
  void getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter") {
      accepted = payload.paths.length > 0 && isSupported(payload.paths[0]);
    } else if (payload.type === "drop") {
      if (accepted && !state.saving) openPaths(payload.paths);
      accepted = false;
    } else if (payload.type === "leave") {
      accepted = false;
    }
    dom.stage.classList.toggle("dragging", accepted && (payload.type === "enter" || payload.type === "over"));
  });
}

// --- 貼り付け（旧版 FR-UI-64） ------------------------------------------------------

/**
 * 編集 > ペースト（⌘V）: クリップボードの画像を開く。Finder でコピーしたファイルならそのファイルを開く。
 * 文字・数値の入力欄では、文字があれば入力欄に貼り付ける。
 */
export async function paste() {
  const editing = isEditingText();
  if (!editing && (state.saving || state.opening)) return;
  const contents = await invoke<ClipboardContents>("clipboard_contents");
  const action = pasteAction(contents, editing, isSupported);
  if (action.kind === "text") {
    const text = await invoke<string | null>("clipboard_text");
    // 入力欄の取り消し（⌘Z）でも戻せるよう、入力として入れる
    if (text) document.execCommand("insertText", false, text);
    return;
  }
  if (state.saving || state.opening) return;
  if (action.kind === "files") openPaths(action.paths);
  if (action.kind === "image") await openClipboardImage();
  if (action.kind === "nothing") notify(NOTHING_TO_PASTE_MESSAGE);
}

/** 「並べて 1 枚に」: 写真を選んで並べた画像を作り、元のファイルのない画像として開く（未保存の変更があれば確かめる）。 */
export async function makeCollage() {
  if (state.saving || state.opening) return;
  const request = await parts.collageDialog.run();
  if (!request) return;
  state.opening = true;
  try {
    if (!(await confirmDiscard())) return;
    notify("写真を並べています…");
    showLoaded(await invoke<OpenInfo>("make_collage", request), []);
  } catch (error) {
    await showError("写真を並べられません", error);
  } finally {
    state.opening = false;
  }
}

/** クリップボードの画像を、元のファイルのない画像として開く（未保存の変更があれば確かめる）。 */
async function openClipboardImage() {
  state.opening = true;
  try {
    if (!(await confirmDiscard())) return;
    showLoaded(await invoke<OpenInfo>("open_clipboard_image"), []);
  } catch (error) {
    await showError("画像を貼り付けられません", error);
  } finally {
    state.opening = false;
  }
}

// --- 保存 ---------------------------------------------------------------------------

type SaveFailure = { kind: "sameFile" | "extension" | "other"; message: string };

/** 保存: ダイアログで保存先を選び、原寸で処理して書き出す（処理は Rust の別のスレッド）。 */
export async function saveDialog() {
  const { loaded } = state;
  if (!loaded || state.saving) return;
  const stamp = pasteStamp(new Date());
  const defaultPath = (await invoke<string | null>("default_save_path", { stamp })) ?? `${loaded.name}_edited.png`;
  // 元の画像と同じファイルが選ばれたら、知らせてダイアログを開き直す（旧版 FR-IO-11）
  for (;;) {
    const path = await save({
      title: "保存",
      defaultPath,
      filters: [{ name: "画像ファイル", extensions: state.savableExtensions }],
    });
    if (!path) return;
    const result = await saveTo(path);
    if (result !== "sameFile") return;
  }
}

/** 複数の大きさで保存で選んだ大きさを残す環境設定の名前 */
const SIZES_STORAGE_KEY = "saveSizes";

/** 複数の大きさで保存: 大きさを選び、保存ダイアログで名前を決め、長辺ごとに続けて保存する。 */
export async function saveSizesDialog() {
  const { loaded } = state;
  if (!loaded || state.saving) return;
  const sizes = await chooseSizes();
  if (!sizes) return;
  if (sizes.length === 0) {
    notify("大きさが選ばれていないので、保存しません");
    return;
  }
  const stamp = pasteStamp(new Date());
  const defaultPath = (await invoke<string | null>("default_save_path", { stamp })) ?? `${loaded.name}_edited.jpg`;
  for (;;) {
    const path = await save({
      title: "複数の大きさで保存（名前の後ろに長辺を付けます）",
      defaultPath,
      filters: [{ name: "画像ファイル", extensions: state.savableExtensions }],
    });
    if (!path) return;
    if ((await saveSizesTo(path, sizes)) !== "sameFile") return;
  }
}

/** 大きさを選んでもらう（キャンセルなら null）。選んだ大きさは環境設定に残す。 */
function chooseSizes(): Promise<number[] | null> {
  const dialog = document.getElementById("sizes-dialog") as HTMLDialogElement;
  const list = document.getElementById("sizes-list") as HTMLElement;
  const chosen = parseSizes(
    readStored(SIZES_STORAGE_KEY),
    SIZE_PRESETS.map(([, px]) => px),
  );
  list.replaceChildren(
    ...SIZE_PRESETS.map(([label, px]) => {
      const row = document.createElement("label");
      row.className = "check";
      const box = document.createElement("input");
      box.type = "checkbox";
      box.value = String(px);
      box.checked = chosen.includes(px);
      row.append(box, ` ${label}`);
      return row;
    }),
  );
  dialog.returnValue = "";
  dialog.showModal();
  return new Promise((resolve) =>
    dialog.addEventListener(
      "close",
      () => {
        if (dialog.returnValue !== "ok") return resolve(null);
        const sizes = [...list.querySelectorAll<HTMLInputElement>("input:checked")].map((box) => Number(box.value));
        writeStored(SIZES_STORAGE_KEY, JSON.stringify(sizes));
        resolve(sizes);
      },
      { once: true },
    ),
  );
}

/** 複数の大きさで保存した 1 つ分（Rust の saving::SizedSaved）。 */
type SizedSaved = { path: string; longSide: number; saved: Saved };

/** 長辺ごとに続けて保存する。元の画像と同じファイルになるなら "sameFile" を返す。 */
function saveSizesTo(path: string, sizes: number[]): Promise<SaveResult> {
  return runSave(path, `保存中… ${sizes.length} つの大きさ`, async (settings) => {
    const results = await invoke<SizedSaved[]>("save_sizes", {
      path,
      settings,
      options: saveOptions.value(),
      longSides: sizes,
    });
    const fitted = results.filter((r) => r.saved.fitted).length;
    const note = fitted > 0 ? `（${fitted} つは大きさの上限に合わせて品質を下げた・縮めた）` : "";
    const paths = results.map((r) => r.path);
    return { text: `${savedSummary(paths)}${note}`, paths };
  });
}

/** Rust の save::Saved（保存した品質・大きさ・ファイルの大きさ、上限に合わせたか）。 */
type Saved = { quality: number; size: [number, number]; bytes: number; fitted: boolean };

/** 大きさの上限に合わせて品質を下げた・縮めたときの知らせ。 */
function fittedNote(saved: Saved): string {
  if (!saved.fitted) return "";
  const kb = Math.round(saved.bytes / 1024);
  return `（大きさに合わせて品質 ${saved.quality}・${saved.size[0]}×${saved.size[1]} px・${kb} KB）`;
}

/** 保存する。元の画像と同じファイルなら "sameFile" を返す。 */
function saveTo(path: string): Promise<SaveResult> {
  const name = path.split("/").pop() ?? path;
  return runSave(path, `保存中… ${name}`, async (settings) => {
    const result = await invoke<Saved>("save_image", { path, settings, options: saveOptions.value() });
    return { text: `保存しました: ${name}${fittedNote(result)}`, paths: [path] };
  });
}

type SaveResult = "done" | "sameFile" | "failed";

/**
 * 保存を走らせて結果を知らせる（保存中は保存・開くなどを止める）。run は今の設定の写しで保存し、ステータスバーに
 * 出す文と保存したファイルを返す。元の画像と同じファイルなら "sameFile"（呼んだ側が保存ダイアログを開き直す）。
 * 保存できたら「Finder で表示」を出す。
 */
async function runSave(
  path: string,
  busy: string,
  run: (settings: EditSettings) => Promise<{ text: string; paths: string[] }>,
): Promise<SaveResult> {
  const name = path.split("/").pop() ?? path;
  setSaving(true);
  void updateStatus(busy);
  try {
    const saved = structuredClone(state.settings);
    const done = await run(saved);
    state.savedSettings = saved;
    state.savedPaths = done.paths;
    dom.revealButton.hidden = false;
    void updateStatus(done.text);
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

/** 加工後の画像（原寸）をクリップボードにコピーする（保存と同じく、処理の間は保存・開くなどを止める）。 */
export async function copyImage() {
  if (!state.loaded || state.saving) return;
  setSaving(true);
  void updateStatus("コピー中…");
  try {
    const [width, height] = await invoke<[number, number]>("copy_image", { settings: structuredClone(state.settings) });
    notify(`加工後の画像をコピーしました（${width}×${height} px）。メッセージやメールに貼り付けられます`);
  } catch (error) {
    void updateStatus();
    await showError("コピーできません", error);
  } finally {
    setSaving(false);
  }
}

/** 加工後の画像（原寸。保存の設定に従う）を、macOS の共有の一覧（AirDrop・メッセージ・メールなど）に渡す。 */
export async function shareImage() {
  if (!state.loaded || state.saving) return;
  // 一覧はプレビューの上の真ん中に出す（ウィンドウの左上からの px）
  const rect = dom.canvas.getBoundingClientRect();
  const at = [rect.left + rect.width / 2, Math.max(rect.top, 0) + 8];
  setSaving(true);
  void updateStatus("共有の準備中…");
  try {
    await invoke("share_image", { settings: structuredClone(state.settings), options: saveOptions.value(), at });
    void updateStatus();
  } catch (error) {
    void updateStatus();
    await showError("共有できません", error);
  } finally {
    setSaving(false);
  }
}

/** 最後に保存したファイルを Finder で表示する（ファイルを選んだ状態でフォルダを開く）。 */
export async function revealSaved() {
  if (state.savedPaths.length === 0) return;
  try {
    await invoke("reveal_in_finder", { paths: state.savedPaths });
  } catch (error) {
    await showError("Finder で表示できません", error);
  }
}

/** 保存中・まとめて処理中は保存・開く・ドロップ・リセットを受け付けない（画面は固まらない）。 */
export function setSaving(value: boolean) {
  state.saving = value;
  dom.saveButton.disabled = dom.resetButton.disabled = value || !state.loaded;
  updateMenus();
}

// --- まとめて処理（旧版 FR-UI-45） --------------------------------------------------

/** まとめて処理のダイアログを開き、「開始」なら 1 枚ずつ処理して結果を知らせる（実行中は保存・開くを止める）。 */
export async function startBatch() {
  if (state.saving || state.opening) return;
  const { loaded, settings } = state;
  const result = await parts.batchDialog
    .run({
      presets: presetNames(),
      longSide: loaded ? output.longSide() : DEFAULT_BATCH_LONG_SIDE,
      resize: loaded !== null && (settings.width !== null || settings.height !== null),
      settings: structuredClone(settings),
      save: saveOptions.value(),
      onBusy: setSaving,
    })
    .catch(async (error) => {
      await showError("まとめて処理できません", error);
      return null;
    });
  if (!result) return;
  void updateStatus(`まとめて処理: ${result.saved} 枚を保存しました`);
  await message(result.message, { title: "まとめて処理", kind: "info" });
}

// --- リセット・終了（旧版 FR-UI-42） ------------------------------------------------

/** リセット: 画像と設定を未読込の状態に戻す（未保存の変更があれば確かめる）。 */
export async function resetImage() {
  if (!state.loaded || state.saving || state.opening || !(await confirmDiscard())) return;
  setComparing(false);
  fitToWindow();
  await invoke("close_image");
  state.loaded = null;
  state.savedSettings = null;
  state.notes = [];
  preview.clear();
  stopSplit();
  Object.assign(state.settings, defaultSettings());
  parts.panel.show();
  preview.trimmed = false;
  parts.crop.reset(null);
  parts.privacy.reset();
  dom.revealButton.hidden = true;
  parts.localPanel.reset();
  output.reset(false);
  parts.textDialog.close();
  parts.textDialog.show();
  parts.textButton.disabled = parts.tasteButton.disabled = parts.autoButton.disabled = true;
  parts.tasteGallery.close();
  exifView.show(null);
  tabs.setEnabled("exif", false);
  histogramView.set(null);
  dom.placeholder.hidden = false;
  dom.saveButton.disabled = dom.beforeButton.disabled = dom.resetButton.disabled = true;
  void updateGuide();
  parts.recorder.reset();
  updateMenus();
  void updateStatus();
}

/** 終了を確かめている間（⌘Q を続けて押しても、ダイアログは 1 つだけにする） */
let quitting = false;

/**
 * 終了する（⌘Q・ウィンドウを閉じる・Dock の「終了」）。未保存の変更があれば確かめ、キャンセルなら終了しない。
 * 保存中・まとめて処理中は、ファイルが途中で切れないよう終了しない。
 */
export async function requestQuit() {
  if (quitting) return;
  quitting = true;
  try {
    if (state.saving) {
      await message("保存（またはまとめて処理）の途中です。終わってから終了してください。", {
        title: "終了できません",
        kind: "warning",
      });
      return;
    }
    if (!(await confirmDiscard("終了"))) return;
    await invoke("quit_app");
  } finally {
    quitting = false;
  }
}
