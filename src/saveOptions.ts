// 「出力」タブの「保存の設定」（JPEG・HEIC の品質・EXIF・位置情報・ファイルの大きさの上限・透過の色・権利の情報）と、「投稿加工」タブの「位置情報を消して保存する」
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
  /** EXIF に書く権利の情報（空の項目は書かない） */
  rights: Rights;
};

/** Rust の save::ExifRights（著作権・作者・説明）。 */
export type Rights = { copyright: string; artist: string; description: string };

const RIGHTS_KEYS = ["copyright", "artist", "description"] as const;

/** 画面で持つ設定（上限を外しても、入れた MB は覚えておく）。fileName は保存の名前の書き方（Rust には保存の設定として渡さない）。 */
export type Stored = Omit<SaveOptions, "maxKb"> & { limit: boolean; limitMb: number; fileName: string };

/** 保存の名前の既定の書き方（Rust の naming::DEFAULT_TEMPLATE と同じ） */
export const DEFAULT_FILE_NAME = "{名前}_edited";

const DEFAULT_OPTIONS: Stored = {
  quality: 90,
  keepExif: true,
  keepGps: false,
  limit: false,
  limitMb: 1,
  fill: [255, 255, 255],
  rights: { copyright: "", artist: "", description: "" },
  fileName: DEFAULT_FILE_NAME,
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
  /** 著作権・作者・説明の欄 */
  rights: Record<keyof Rights, HTMLInputElement>;
  /** 保存の名前の書き方の欄 */
  fileName: HTMLInputElement;
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
  private readonly rights: Record<keyof Rights, HTMLInputElement>;
  private readonly fileNameInput: HTMLInputElement;

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
      rights: this.rights,
      fileName: this.fileNameInput,
    } = fields);
    this.fileNameInput.addEventListener("change", () =>
      this.update({ fileName: this.fileNameInput.value.trim() || DEFAULT_FILE_NAME }),
    );
    const { quality, keepExif, keepGps, removeGps, limit, limitMb } = fields;
    this.options = load();
    quality.addEventListener("input", () => this.update({ quality: Number(quality.value) }));
    quality.addEventListener("dblclick", () => this.update({ quality: DEFAULT_OPTIONS.quality }));
    keepExif.addEventListener("change", () => this.update({ keepExif: keepExif.checked }));
    keepGps.addEventListener("change", () => this.update({ keepGps: keepGps.checked }));
    removeGps.addEventListener("change", () => this.update({ keepGps: !removeGps.checked }));
    limit.addEventListener("change", () => this.update({ limit: limit.checked }));
    fields.fill.addEventListener("input", () => this.update({ fill: fromHex(fields.fill.value) }));
    for (const key of RIGHTS_KEYS) {
      const input = this.rights[key];
      input.addEventListener("change", () => this.update({ rights: { ...this.options.rights, [key]: input.value.trim() } }));
    }
    limitMb.addEventListener("change", () => {
      const mb = Number(limitMb.value);
      if (mb >= LIMIT_MIN_MB && mb <= LIMIT_MAX_MB) this.update({ limitMb: mb });
      else this.show();
    });
    this.show();
  }

  /** 保存の名前の書き方。 */
  fileName(): string {
    return this.options.fileName;
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
    for (const key of RIGHTS_KEYS) this.rights[key].value = this.options.rights[key];
    this.fileNameInput.value = this.options.fileName;
  }
}

/** 画面で持つ設定を、Rust に渡す保存の設定にする（上限は KB、外していれば null）。 */
export function toSaveOptions(stored: Stored): SaveOptions {
  const { limit, limitMb, fileName: _fileName, ...rest } = stored;
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
    rights: parseRights(s.rights),
    fileName: typeof s.fileName === "string" && s.fileName.trim() ? s.fileName : DEFAULT_FILE_NAME,
  };
}

/** 覚えておいた権利の情報（文字でない項目は空にする）。 */
function parseRights(value: unknown): Rights {
  const r = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  const text = (v: unknown) => (typeof v === "string" ? v : "");
  return { copyright: text(r.copyright), artist: text(r.artist), description: text(r.description) };
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
