// Rust とやりとりする型（Rust の側と同じ形・camelCase）。

/** 回転・反転（左右反転してから時計回りに rotation 度）。 */
export type Orientation = { rotation: 0 | 90 | 180 | 270; mirror: boolean };

/** トリミング範囲（回転・反転した後の原寸画像の座標、px）。 */
export type CropRect = { x: number; y: number; width: number; height: number };

/** テイスト（残りは #9 で足す）。 */
export type FilterType = "none" | "hdr";

/** Rust の pipeline::EditSettings。 */
export type EditSettings = {
  orientation: Orientation;
  crop: CropRect | null;
  width: number | null;
  height: number | null;
  keepAspect: boolean;
  filter: FilterType;
  vignette: number;
  aging: number;
  temperature: number;
  saturation: number;
  brightness: number;
  contrast: number;
  exposure: number;
  sharpen: number;
  blur: number;
  denoise: number;
  dioramaBlur: number;
  dioramaPosition: number;
  dioramaWidth: number;
  dioramaVivid: number;
  text: { text: string; size: number };
};

/** 既定の設定（Rust の EditSettings::default と同じ）。 */
export function defaultSettings(): EditSettings {
  return {
    orientation: { rotation: 0, mirror: false },
    crop: null,
    width: null,
    height: null,
    keepAspect: true,
    filter: "none",
    vignette: 0,
    aging: 0,
    temperature: 6500,
    saturation: 0,
    brightness: 0,
    contrast: 0,
    exposure: 0,
    sharpen: 0,
    blur: 0,
    denoise: 0,
    dioramaBlur: 0,
    dioramaPosition: 50,
    dioramaWidth: 20,
    dioramaVivid: 30,
    text: { text: "", size: 5 },
  };
}

export type ExifEntry = { group: string; tag: string; label: string; value: string };

export type ExifInfo = {
  entries: ExifEntry[];
  makerNote: string | null;
  gps: { latitude: number; longitude: number } | null;
};

/** Rust の OpenInfo（読み込みの結果）。 */
export type OpenInfo = {
  name: string;
  format: string | null;
  width: number;
  height: number;
  previewWidth: number;
  previewHeight: number;
  hasAlpha: boolean;
  frameCount: number;
  decodeMs: number;
  resizeMs: number;
  exif: ExifInfo;
};
