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
  /** 遠近の補正 -100〜100（縦: 正で上が細くなった台形を直す、横: 正で左が細くなった台形を直す） */
  perspectiveVertical: number;
  perspectiveHorizontal: number;
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
  /** ハイライト・シャドウ -100〜100（明部・暗部だけを明るく・暗くする） */
  highlights: number;
  shadows: number;
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
  /** 投稿加工で隠す範囲（回転・反転した後の原寸の座標） */
  regions: Region[];
  /** 背景を消す（そのまま・透明・白） */
  background: BackgroundMode;
  /** 背景のぼかしの強さ 1〜100（背景を「ぼかす」とき） */
  backgroundBlur: number;
  /** トーンカーブの点（[x, y]。x は 0〜255 で増えていく順、両端は x = 0・255） */
  toneCurve: [number, number][];
  /** 色ごとの調整（赤・オレンジ・黄・緑・水色・青・紫・マゼンタの 8 色） */
  hsl: HslAdjust[];
  /** ロゴの透かし（path が空ならなし） */
  logo: LogoSettings;
  /** LUT（.cube。path が空ならなし。テイストの後にかける） */
  lut: LutSettings;
  /** 肌をなめらかに 0〜100（見つけた顔のまわりだけ） */
  skinSmooth: number;
  /** 赤目の補正（見つけた顔の目のあたりだけ） */
  redEye: boolean;
  /** 部分補正（円・帯の範囲の中だけの調整。回転・反転した後の原寸の座標） */
  localAdjustments: LocalAdjust[];
};

/** 部分補正の範囲の形（Rust の local::LocalShape）。 */
export type LocalShape = { kind: "ellipse"; rect: CropRect } | { kind: "band"; from: [number, number]; to: [number, number] };

/** 部分補正の 1 つ分（Rust の local::LocalAdjust）。feather は楕円の境目のぼかし幅（%）。 */
export type LocalAdjust = {
  shape: LocalShape;
  exposure: number;
  contrast: number;
  temperature: number;
  saturation: number;
  feather: number;
};

/** LUT（Rust の lut::LutSettings）。strength は %。 */
export type LutSettings = { path: string; strength: number };

/** ロゴの透かし（Rust の logo::LogoSettings）。size は写真の短辺に対する %、opacity は %。 */
export type LogoSettings = {
  path: string;
  position: TextPosition;
  size: number;
  opacity: number;
  /** 位置が「自由」のときの中心（写真の幅・高さに対する割合。既定の位置ならないこともある） */
  point?: [number, number];
};

/** 自由な位置の既定（Rust の text::FREE_POINT_DEFAULT）。 */
export const FREE_POINT_DEFAULT: [number, number] = [0.5, 0.85];

/** 色ごとの調整の 1 色分（Rust の curve::HslAdjust）。色相 -30〜30、彩度・明るさ -100〜100。 */
export type HslAdjust = { hue: number; saturation: number; lightness: number };

/** 既定の色ごとの調整（すべて 0）。 */
export const neutralHsl = (): HslAdjust[] => Array.from({ length: 8 }, () => ({ hue: 0, saturation: 0, lightness: 0 }));

/** 背景の扱い（Rust の background::Background）。 */
export type BackgroundMode = "keep" | "transparent" | "white" | "blur";

/** 投稿加工の隠し方（Rust の privacy::RegionKind）。 */
export type RegionKind = "blur" | "mosaic" | "stamp";

/** 投稿加工で隠す範囲 1 つ（Rust の privacy::Region）。強さは 1〜100（スタンプでは使わない）、stamp はスタンプの絵文字。 */
export type Region = { kind: RegionKind; rect: CropRect; strength: number; stamp: string };

/** フレーム（Rust の frames::FrameType）。 */
export type FrameKind = "none" | "polaroid" | "instax_mini";

/** 写真の形（Rust の shapes::ShapeType）。 */
export type ShapeType = "rectangle" | "rounded" | "circle";

/** 既定の設定（Rust の EditSettings::default と同じ）。 */
export function defaultSettings(): EditSettings {
  return {
    orientation: { rotation: 0, mirror: false },
    straighten: 0,
    perspectiveVertical: 0,
    perspectiveHorizontal: 0,
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
    highlights: 0,
    shadows: 0,
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
    regions: [],
    background: "keep",
    backgroundBlur: 50,
    toneCurve: [
      [0, 0],
      [255, 255],
    ],
    hsl: neutralHsl(),
    logo: { path: "", position: "bottom_right", size: 15, opacity: 80 },
    lut: { path: "", strength: 100 },
    skinSmooth: 0,
    redEye: false,
    localAdjustments: [],
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
export type AspectRatio = "free" | "square" | "ratio5x4" | "ratio4x3" | "ratio3x2" | "ratio16x9";

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
  | "frame_margin"
  | "tiled"
  | "free";

/** 文字・透かしの設定（Rust の text::TextSettings）。size は写真の短辺に対する %、opacity は %。 */
export type TextSettings = {
  text: string;
  font: TextFont;
  size: number;
  color: [number, number, number];
  opacity: number;
  position: TextPosition;
  /** 飾り（なしのときは Rust が書かないので、ないこともある） */
  effect?: TextEffect;
  /** 位置が「自由」のときの中心（写真の幅・高さに対する割合。既定の位置なら Rust が書かないので、ないこともある） */
  point?: [number, number];
};

/** 文字の飾り（Rust の text::TextEffect）。 */
export type TextEffect = "none" | "outline" | "shadow";

/** 文字・透かしの既定値。 */
export function defaultText(): TextSettings {
  return { text: "", font: "GOTHIC", size: 5, color: [255, 255, 255], opacity: 80, position: "bottom_right" };
}
