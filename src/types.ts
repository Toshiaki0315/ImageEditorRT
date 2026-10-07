// Rust とやりとりする型（Rust の側と同じ形・camelCase）。

/** 回転・反転（左右反転してから時計回りに rotation 度）。 */
export type Orientation = { rotation: 0 | 90 | 180 | 270; mirror: boolean };

/** トリミング範囲（回転・反転した後の原寸画像の座標、px）。 */
export type CropRect = { x: number; y: number; width: number; height: number };

/** テイスト（Rust の filters::FilterType。旧版と同じ名前）。 */
export type FilterType =
  | "none"
  | "sepia"
  | "monotone"
  | "high_tone"
  | "polaroid"
  | "positive_film"
  | "retro_camera"
  | "high_key"
  | "low_key"
  | "dramatic"
  | "modern"
  | "natural"
  | "cinematic"
  | "noir"
  | "bleach_bypass"
  | "pastel"
  | "cross_process"
  | "cyanotype"
  | "summer"
  | "autumn"
  | "soft_focus"
  | "hdr"
  | "infrared";

/** Rust の pipeline::EditSettings。 */
export type EditSettings = {
  orientation: Orientation;
  /** 水平の補正 -45〜45（度、正は時計回り）。回転・反転の後に回し、余白が出ないよう拡大する */
  straighten: number;
  crop: CropRect | null;
  width: number | null;
  height: number | null;
  keepAspect: boolean;
  filter: FilterType;
  /** テイストの強さ 0〜200（%）。100 = テイストのまま、100 を超えると変化を強める */
  filterStrength: number;
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
  dioramaDirection: "horizontal" | "vertical";
  dioramaPosition: number;
  dioramaWidth: number;
  dioramaVivid: number;
  text: TextSettings;
  frame: FrameKind;
  shape: ShapeType;
  cornerRadius: number;
};

/** フレーム（Rust の frames::FrameType）。 */
export type FrameKind = "none" | "polaroid" | "instax_mini";

/** 写真の形（Rust の shapes::ShapeType）。 */
export type ShapeType = "rectangle" | "rounded" | "circle";

/** 既定の設定（Rust の EditSettings::default と同じ）。 */
export function defaultSettings(): EditSettings {
  return {
    orientation: { rotation: 0, mirror: false },
    straighten: 0,
    crop: null,
    width: null,
    height: null,
    keepAspect: true,
    filter: "none",
    filterStrength: 100,
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
    dioramaDirection: "horizontal",
    dioramaPosition: 50,
    dioramaWidth: 20,
    dioramaVivid: 30,
    text: defaultText(),
    frame: "none",
    shape: "rectangle",
    cornerRadius: 10,
  };
}

export type ExifEntry = { group: string; tag: string; label: string; value: string };

export type ExifInfo = {
  entries: ExifEntry[];
  makerNote: string | null;
  gps: { latitude: number; longitude: number } | null;
  /** 表示する情報がない（画像の構造を表すタグしかない場合も含む） */
  empty: boolean;
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

/** Rust の pipeline::DioramaGuide（線の位置は表示している画像に対する割合、と実線かどうか）。 */
export type DioramaGuide = { horizontal: boolean; lines: [number, boolean][] };

/** トリミングの比（Rust の transform::AspectRatio）。 */
export type AspectRatio = "free" | "square" | "ratio4x3" | "ratio3x2" | "ratio16x9";

/** 回転・反転の操作（Rust の transform::OrientOp）。 */
export type OrientOp = "rotate_left" | "rotate_right" | "flip_horizontal" | "flip_vertical";

/** 文字・透かしのフォント（Rust の text::TextFont。旧版のプリセットと同じ名前）。 */
export type TextFont = "GOTHIC" | "GOTHIC_BOLD" | "MINCHO" | "MARU_GOTHIC" | "HELVETICA" | "TIMES";

/** 文字を置く場所（Rust の text::TextPosition）。 */
export type TextPosition =
  | "top_left"
  | "top"
  | "top_right"
  | "left"
  | "center"
  | "right"
  | "bottom_left"
  | "bottom"
  | "bottom_right"
  | "frame_margin";

/** 文字・透かしの設定（Rust の text::TextSettings）。size は写真の短辺に対する %、opacity は %。 */
export type TextSettings = {
  text: string;
  font: TextFont;
  size: number;
  color: [number, number, number];
  opacity: number;
  position: TextPosition;
};

/** 文字・透かしの既定値。 */
export function defaultText(): TextSettings {
  return { text: "", font: "GOTHIC", size: 5, color: [255, 255, 255], opacity: 80, position: "bottom_right" };
}
