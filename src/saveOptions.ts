// 「出力」タブの「保存の設定」（JPEG・HEIC の品質・EXIF・位置情報・ファイルの大きさの上限）と、「投稿加工」タブの「位置情報を消して保存する」
// （位置情報を残すかの逆。どちらで変えても両方に反映する）。
// 保存の好みなので画像を開いても戻さず、アプリを終了しても残す（旧版 FR-UI-56）。

// Node のテスト（tests-ts/）からも読むので、拡張子まで書く
import { readStored, writeStored } from "./storage.ts";

/** Rust の save::SaveOptions。maxKb はファイルの大きさの上限（KB、JPEG・HEIC のとき。なければ null）。 */
export type SaveOptions = {
  quality: number;
  keepExif: boolean;
  keepGps: boolean;
  maxKb: number | null;
  /** JPEG・BMP で保存するとき透過を塗る色 */
  fill: [number, number, number];
};

/** 画面で持つ設定（上限を外しても、入れた MB は覚えておく）。 */
export type Stored = Omit<SaveOptions, "maxKb"> & { limit: boolean; limitMb: number };

const DEFAULT_OPTIONS: Stored = {
  quality: 90,
  keepExif: true,
  keepGps: false,
  limit: false,
  limitMb: 1,
  fill: [255, 255, 255],
};

/** 色を色の欄の値（#rrggbb）にする。 */
export const toHex = ([r, g, b]: [number, number, number]) =>
  `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;

/** 色の欄の値（#rrggbb）を色にする。 */
export const fromHex = (hex: string): [number, number, number] =>
  [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as [number, number, number];
const LIMIT_MIN_MB = 0.05;
const LIMIT_MAX_MB = 100;
const STORAGE_KEY = "saveOptions";

/** 画面の欄（「出力」タブの保存の設定と、「投稿加工」タブの位置情報を消す）。 */
export type SaveOptionsFields = {
  quality: HTMLInputElement;
  qualityValue: HTMLOutputElement;
  keepExif: HTMLInputElement;
  keepGps: HTMLInputElement;
  removeGps: HTMLInputElement;
  limit: HTMLInputElement;
  limitMb: HTMLInputElement;
  fill: HTMLInputElement;
};

export class SaveOptionsPanel {
  private options: Stored;
  private readonly quality: HTMLInputElement;
  private readonly qualityValue: HTMLOutputElement;
  private readonly keepExif: HTMLInputElement;
  private readonly keepGps: HTMLInputElement;
  private readonly removeGps: HTMLInputElement;
  private readonly limit: HTMLInputElement;
  private readonly limitMb: HTMLInputElement;
  private readonly fill: HTMLInputElement;

  constructor(fields: SaveOptionsFields) {
    ({
      quality: this.quality,
      qualityValue: this.qualityValue,
      keepExif: this.keepExif,
      keepGps: this.keepGps,
      removeGps: this.removeGps,
      limit: this.limit,
      limitMb: this.limitMb,
      fill: this.fill,
    } = fields);
    const { quality, keepExif, keepGps, removeGps, limit, limitMb } = fields;
    this.options = load();
    quality.addEventListener("input", () => this.update({ quality: Number(quality.value) }));
    quality.addEventListener("dblclick", () => this.update({ quality: DEFAULT_OPTIONS.quality }));
    keepExif.addEventListener("change", () => this.update({ keepExif: keepExif.checked }));
    keepGps.addEventListener("change", () => this.update({ keepGps: keepGps.checked }));
    removeGps.addEventListener("change", () => this.update({ keepGps: !removeGps.checked }));
    limit.addEventListener("change", () => this.update({ limit: limit.checked }));
    fields.fill.addEventListener("input", () => this.update({ fill: fromHex(fields.fill.value) }));
    limitMb.addEventListener("change", () => {
      const mb = Number(limitMb.value);
      if (mb >= LIMIT_MIN_MB && mb <= LIMIT_MAX_MB) this.update({ limitMb: mb });
      else this.show();
    });
    this.show();
  }

  /** 今の保存の設定。 */
  value(): SaveOptions {
    return toSaveOptions(this.options);
  }

  private update(change: Partial<Stored>) {
    this.options = { ...this.options, ...change };
    writeStored(STORAGE_KEY, JSON.stringify(this.options));
    this.show();
  }

  private show() {
    this.quality.value = String(this.options.quality);
    this.qualityValue.textContent = String(this.options.quality);
    this.keepExif.checked = this.options.keepExif;
    this.keepGps.checked = this.options.keepGps;
    // 位置情報は EXIF を残すときだけ選べる（残さなければ位置情報も消える）
    this.keepGps.disabled = !this.options.keepExif;
    this.removeGps.checked = !this.options.keepExif || !this.options.keepGps;
    this.removeGps.disabled = !this.options.keepExif;
    this.limit.checked = this.options.limit;
    this.limitMb.value = String(this.options.limitMb);
    this.limitMb.disabled = !this.options.limit;
    this.fill.value = toHex(this.options.fill);
  }
}

/** 画面で持つ設定を、Rust に渡す保存の設定にする（上限は KB、外していれば null）。 */
export function toSaveOptions(stored: Stored): SaveOptions {
  const { limit, limitMb, ...rest } = stored;
  return { ...rest, maxKb: limit ? Math.max(1, Math.round(limitMb * 1024)) : null };
}

/** 覚えておいた設定（JSON を読んだもの）を、正しい値だけ使い、ほかは既定値にする。 */
export function parseStored(saved: unknown): Stored {
  if (!saved || typeof saved !== "object") return { ...DEFAULT_OPTIONS };
  const s = saved as Record<string, unknown>;
  const quality = Number(s.quality);
  const limitMb = Number(s.limitMb);
  const bool = (value: unknown, fallback: boolean) => (typeof value === "boolean" ? value : fallback);
  return {
    quality: Number.isInteger(quality) && quality >= 1 && quality <= 100 ? quality : DEFAULT_OPTIONS.quality,
    keepExif: bool(s.keepExif, DEFAULT_OPTIONS.keepExif),
    keepGps: bool(s.keepGps, DEFAULT_OPTIONS.keepGps),
    limit: bool(s.limit, DEFAULT_OPTIONS.limit),
    limitMb: limitMb >= LIMIT_MIN_MB && limitMb <= LIMIT_MAX_MB ? limitMb : DEFAULT_OPTIONS.limitMb,
    fill: isColor(s.fill) ? s.fill : DEFAULT_OPTIONS.fill,
  };
}

/** 0〜255 の整数 3 つの色か。 */
function isColor(value: unknown): value is [number, number, number] {
  return Array.isArray(value) && value.length === 3 && value.every((v) => Number.isInteger(v) && v >= 0 && v <= 255);
}

function load(): Stored {
  try {
    return parseStored(JSON.parse(readStored(STORAGE_KEY) ?? "null"));
  } catch {
    // 壊れていれば既定値
    return { ...DEFAULT_OPTIONS };
  }
}
