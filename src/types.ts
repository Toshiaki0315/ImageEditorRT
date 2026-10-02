// Rust とやりとりする型（Rust の側と同じ形・camelCase）。

/** Rust の preview::Settings。 */
export type Settings = {
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

export const DEFAULT_SETTINGS: Settings = {
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
