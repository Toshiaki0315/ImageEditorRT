// ImageEditorRT の画面（試作）。設定を Rust に渡してプレビューを描き、かかった時間を表示する。

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";

type Format = "jpeg" | "rgba";

/** Rust の preview::Settings と同じ形（camelCase）。 */
type Settings = {
  exposure: number;
  brightness: number;
  contrast: number;
  temperature: number;
  saturation: number;
  denoise: number;
  blur: number;
  sharpen: number;
  dioramaBlur: number;
  dioramaPosition: number;
  dioramaWidth: number;
  dioramaVivid: number;
  hdr: boolean;
  vignette: number;
  aging: number;
  text: string;
  textSize: number;
};

type ExifEntry = { group: string; tag: string; label: string; value: string };

type ExifInfo = {
  entries: ExifEntry[];
  makerNote: string | null;
  gps: { latitude: number; longitude: number } | null;
};

type OpenInfo = {
  width: number;
  height: number;
  previewWidth: number;
  previewHeight: number;
  decodeMs: number;
  resizeMs: number;
  exifMs: number;
  exif: ExifInfo;
};

/** 1 回のプレビュー更新の内訳 (ms)。 */
type Timing = {
  render: number; // Rust の処理
  encode: number; // Rust での JPEG への変換
  transfer: number; // 受け渡し（invoke の往復から Rust の時間を引いたもの）
  draw: number; // canvas に描く
  total: number; // invoke を呼んでから描き終わるまで
  bytes: number;
};

type Slider = {
  key: keyof Settings;
  label: string;
  min: number;
  max: number;
  step?: number;
};

const DEFAULTS: Settings = {
  exposure: 0,
  brightness: 0,
  contrast: 0,
  temperature: 6500,
  saturation: 0,
  denoise: 0,
  blur: 0,
  sharpen: 0,
  dioramaBlur: 0,
  dioramaPosition: 50,
  dioramaWidth: 20,
  dioramaVivid: 30,
  hdr: false,
  vignette: 0,
  aging: 0,
  text: "",
  textSize: 5,
};

/** Python 版のベンチマークの「重い設定」（Rust の Settings::heavy と同じ）。 */
const HEAVY: Settings = {
  ...DEFAULTS,
  exposure: 0.5,
  brightness: 10,
  contrast: 20,
  temperature: 5000,
  saturation: 20,
  denoise: 50,
  blur: 10,
  sharpen: 50,
  dioramaBlur: 80,
  hdr: true,
  vignette: 50,
  aging: 30,
  text: "© 2026 写真",
};

const GROUPS: [string, Slider[]][] = [
  [
    "色・明るさ",
    [
      { key: "exposure", label: "露出 (EV)", min: -3, max: 3, step: 0.1 },
      { key: "brightness", label: "明るさ", min: -100, max: 100 },
      { key: "contrast", label: "コントラスト", min: -100, max: 100 },
      { key: "temperature", label: "色温度 (K)", min: 2000, max: 12000, step: 100 },
      { key: "saturation", label: "彩度", min: -100, max: 100 },
    ],
  ],
  [
    "ディテール",
    [
      { key: "denoise", label: "ノイズ除去", min: 0, max: 100 },
      { key: "blur", label: "ぼかし", min: 0, max: 100 },
      { key: "sharpen", label: "シャープ", min: 0, max: 100 },
    ],
  ],
  [
    "ジオラマ",
    [
      { key: "dioramaBlur", label: "ぼかし", min: 0, max: 100 },
      { key: "dioramaPosition", label: "帯の位置", min: 0, max: 100 },
      { key: "dioramaWidth", label: "帯の幅", min: 0, max: 100 },
      { key: "dioramaVivid", label: "鮮やかさ", min: 0, max: 100 },
    ],
  ],
  [
    "効果",
    [
      { key: "vignette", label: "周辺減光", min: 0, max: 100 },
      { key: "aging", label: "経年劣化", min: 0, max: 100 },
      { key: "textSize", label: "文字の大きさ", min: 1, max: 20, step: 0.5 },
    ],
  ],
];

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const canvas = $<HTMLCanvasElement>("canvas");
const context = canvas.getContext("2d")!;
const panel = $<HTMLElement>("panel");
const exifPanel = $<HTMLElement>("exif");
const stage = $<HTMLElement>("stage");
const hint = $<HTMLElement>("hint");
const status = $<HTMLElement>("status");
const timingLabel = $<HTMLElement>("timing");
const formatSelect = $<HTMLSelectElement>("format");

let settings: Settings = { ...DEFAULTS };
let loaded = false;
let busy = false;
let pending = false;
const inputs = new Map<keyof Settings, HTMLInputElement>();

/** 描画の待ちの上限。ウィンドウが隠れていると requestAnimationFrame が呼ばれないため。 */
const FRAME_TIMEOUT_MS = 200;
let frameTimeouts = 0;
const nextFrame = () =>
  new Promise<void>((resolve) => {
    const timer = setTimeout(() => {
      frameTimeouts++;
      resolve();
    }, FRAME_TIMEOUT_MS);
    requestAnimationFrame(() => {
      clearTimeout(timer);
      resolve();
    });
  });
const fmt = (ms: number) => `${ms.toFixed(1)}ms`;
const median = (values: number[]) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];

/** 設定のスライダーなどを作る。 */
function buildPanel() {
  for (const [title, sliders] of GROUPS) {
    const heading = document.createElement("h2");
    heading.textContent = title;
    panel.append(heading);
    for (const slider of sliders) {
      const row = document.createElement("label");
      row.className = "row";
      const input = document.createElement("input");
      input.type = "range";
      input.min = String(slider.min);
      input.max = String(slider.max);
      input.step = String(slider.step ?? 1);
      const output = document.createElement("output");
      input.addEventListener("input", () => {
        (settings[slider.key] as number) = Number(input.value);
        output.textContent = input.value;
        requestRender();
      });
      inputs.set(slider.key, input);
      row.append(slider.label, input, output);
      panel.append(row);
    }
    if (title === "効果") {
      const hdr = document.createElement("label");
      hdr.className = "row";
      const box = document.createElement("input");
      box.type = "checkbox";
      box.addEventListener("change", () => {
        settings.hdr = box.checked;
        requestRender();
      });
      inputs.set("hdr", box);
      hdr.append("HDR 風", box);
      const text = document.createElement("label");
      text.className = "row";
      const field = document.createElement("input");
      field.type = "text";
      field.placeholder = "例: © 2026 写真";
      field.addEventListener("input", () => {
        settings.text = field.value;
        requestRender();
      });
      inputs.set("text", field);
      text.append("文字", field);
      panel.append(hdr, text);
    }
  }
  showSettings();
}

/** settings の値をスライダーなどに反映する。 */
function showSettings() {
  for (const [key, input] of inputs) {
    const value = settings[key];
    if (typeof value === "boolean") input.checked = value;
    else input.value = String(value);
    const output = input.nextElementSibling;
    if (output instanceof HTMLOutputElement) output.textContent = input.value;
  }
}

/** Rust にプレビューを作らせて canvas に描き、内訳を返す。 */
async function renderOnce(current: Settings, format: Format): Promise<Timing> {
  const start = performance.now();
  const buffer = await invoke<ArrayBuffer>("render_preview", { settings: current, format });
  const received = performance.now();
  const header = new DataView(buffer, 0, 16);
  const width = header.getUint32(0, true);
  const height = header.getUint32(4, true);
  const render = header.getUint32(8, true) / 1000;
  const encode = header.getUint32(12, true) / 1000;
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
  }
  if (format === "rgba") {
    const pixels = new Uint8ClampedArray(buffer, 16, width * height * 4);
    context.putImageData(new ImageData(pixels, width, height), 0, 0);
  } else {
    const bitmap = await createImageBitmap(new Blob([new Uint8Array(buffer, 16)], { type: "image/jpeg" }));
    context.drawImage(bitmap, 0, 0);
    bitmap.close();
  }
  await nextFrame(); // 画面に出るところまで含める
  const end = performance.now();
  return {
    render,
    encode,
    transfer: received - start - render - encode,
    draw: end - received,
    total: end - start,
    bytes: buffer.byteLength,
  };
}

/** スライダーを動かすたびに呼ぶ。処理中なら終わってから最新の設定で 1 回だけ描き直す。 */
async function requestRender() {
  if (!loaded) return;
  if (busy) {
    pending = true;
    return;
  }
  busy = true;
  try {
    do {
      pending = false;
      const t = await renderOnce({ ...settings }, formatSelect.value as Format);
      timingLabel.textContent =
        `合計 ${fmt(t.total)}（処理 ${fmt(t.render)}・変換 ${fmt(t.encode)}・` +
        `受け渡し ${fmt(t.transfer)}・描画 ${fmt(t.draw)}・${(t.bytes / 1024).toFixed(0)}KB）`;
    } while (pending);
  } catch (error) {
    status.textContent = String(error);
  } finally {
    busy = false;
  }
}

/** 「EXIF」タブに、グループごとの一覧を出す。 */
function showExif(exif: ExifInfo) {
  exifPanel.replaceChildren();
  if (exif.entries.length === 0) {
    const empty = document.createElement("p");
    empty.className = "empty";
    empty.textContent = "EXIF はありません";
    exifPanel.append(empty);
    return;
  }
  if (exif.makerNote) {
    const note = document.createElement("p");
    note.textContent = `MakerNote の形式: ${exif.makerNote}`;
    exifPanel.append(note);
  }
  let group = "";
  let table: HTMLTableElement | null = null;
  for (const entry of exif.entries) {
    if (entry.group !== group || !table) {
      group = entry.group;
      const heading = document.createElement("h2");
      heading.textContent = group;
      table = document.createElement("table");
      exifPanel.append(heading, table);
    }
    const row = table.insertRow();
    const name = document.createElement("th");
    name.textContent = entry.label;
    const value = row.insertCell();
    value.textContent = entry.value;
    row.prepend(name);
  }
}

function showLoaded(info: OpenInfo, name: string) {
  showExif(info.exif);
  loaded = true;
  canvas.hidden = false;
  hint.hidden = true;
  status.textContent =
    `${name}  ${info.width}×${info.height}（プレビュー ${info.previewWidth}×${info.previewHeight}）` +
    `  読み込み ${fmt(info.decodeMs)}・縮小 ${fmt(info.resizeMs)}・EXIF ${fmt(info.exifMs)}`;
  requestRender();
}

async function open(command: string, args: Record<string, unknown> | Uint8Array, name: string) {
  status.textContent = `${name} を読み込み中…`;
  try {
    showLoaded(await invoke<OpenInfo>(command, args), name);
  } catch (error) {
    status.textContent = String(error);
  }
}

/** JPEG・RGBA それぞれで、重い設定と軽い設定のプレビュー更新を繰り返して中央値を出す。 */
async function bench(): Promise<string> {
  const log = (text: string) => invoke("log", { text });
  await log("計測を始めます");
  if (!loaded) await open("open_sample", {}, "計測用の画像");
  await log(`読み込み: ${status.textContent}`);
  const light: Settings = { ...DEFAULTS, exposure: 0.7, saturation: 20 };
  await renderOnce({ ...settings }, "jpeg"); // canvas の大きさを決める
  const lines = [`プレビュー ${canvas.width}×${canvas.height}・各 20 回の中央値`];
  for (const format of ["jpeg", "rgba"] as Format[]) {
    for (const [name, base] of [
      ["重い設定", HEAVY],
      ["軽い設定", light],
    ] as [string, Settings][]) {
      const runs: Timing[] = [];
      for (let i = 0; i < 23; i++) {
        // 毎回少しだけ設定を変える（同じ結果の使い回しをさせない）
        const t = await renderOnce({ ...base, exposure: base.exposure + (i % 2) * 0.1 }, format);
        if (i >= 3) runs.push(t); // 最初の 3 回は慣らし
      }
      const m = (key: keyof Timing) => median(runs.map((r) => r[key]));
      lines.push(
        `${format.toUpperCase().padEnd(4)} ${name}: 合計 ${fmt(m("total"))}（最大 ${fmt(Math.max(...runs.map((r) => r.total)))}）` +
          ` = 処理 ${fmt(m("render"))} + 変換 ${fmt(m("encode"))} + 受け渡し ${fmt(m("transfer"))} + 描画 ${fmt(m("draw"))}` +
          `（${(m("bytes") / 1024).toFixed(0)}KB）`,
      );
    }
  }
  if (frameTimeouts > 0) lines.push(`注意: 画面の更新を ${frameTimeouts} 回待てませんでした（ウィンドウが隠れていた）`);
  return lines.join("\n");
}

function setup() {
  buildPanel();
  for (const tab of document.querySelectorAll<HTMLButtonElement>(".tab")) {
    tab.addEventListener("click", () => {
      for (const other of document.querySelectorAll<HTMLButtonElement>(".tab")) {
        other.classList.toggle("active", other === tab);
        $<HTMLElement>(other.dataset.tab!).hidden = other !== tab;
      }
    });
  }
  $<HTMLInputElement>("file").addEventListener("change", async (event) => {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (file) await open("open_bytes", new Uint8Array(await file.arrayBuffer()), file.name);
  });
  $<HTMLButtonElement>("sample").addEventListener("click", () => open("open_sample", {}, "計測用の画像"));
  $<HTMLButtonElement>("reset").addEventListener("click", () => {
    settings = { ...DEFAULTS };
    showSettings();
    requestRender();
  });
  formatSelect.addEventListener("change", () => requestRender());
  $<HTMLButtonElement>("bench").addEventListener("click", async () => {
    status.textContent = "計測中…";
    const result = await bench();
    status.textContent = result;
    await requestRender();
  });

  getCurrentWebview().onDragDropEvent((event) => {
    const { type } = event.payload;
    stage.classList.toggle("dragging", type === "enter" || type === "over");
    if (event.payload.type === "drop" && event.payload.paths.length > 0) {
      const path = event.payload.paths[0];
      open("open_path", { path }, path.split("/").pop() ?? path);
    }
  });

  // IMAGEEDITORRT_BENCH を付けて起動したときは、計測して結果を出力して終わる
  invoke<boolean>("bench_mode").then(async (on) => {
    if (!on) return;
    try {
      const result = await bench();
      await invoke("report", { text: result });
    } catch (error) {
      await invoke("report", { text: `計測に失敗しました: ${error}` });
    }
  });
}

window.addEventListener("error", (event) => invoke("log", { text: `エラー: ${event.message}` }));
window.addEventListener("unhandledrejection", (event) => invoke("log", { text: `エラー: ${event.reason}` }));
window.addEventListener("DOMContentLoaded", setup);
