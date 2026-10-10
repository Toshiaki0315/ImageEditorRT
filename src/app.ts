// 画面全体で共有する状態・要素・部品。機能ごとのファイル（editing・view・files・presetsUi など）から使う。
// ここでは機能のファイルを読み込まない（部品のコールバックだけ、呼ばれたときに機能を呼ぶ）。

import type { BatchDialog } from "./batchDialog";
import type { CollageDialog } from "./collageDialog";
import type { ColorPanel } from "./colorPanel";
import type { CropController } from "./crop";
import { ExifView } from "./exif";
import { HistogramView } from "./histogram";
import type { HistoryRecorder } from "./history";
import { OutputSize, type SizeState } from "./output";
import type { Panel } from "./panel";
import type { PhotoControls } from "./photoControls";
import { Preview } from "./preview";
import type { LocalPanel } from "./localPanel";
import type { LutControls } from "./lutControls";
import type { TextDrag } from "./textDrag";
import type { PrivacyPanel } from "./privacy";
import { SaveOptionsPanel } from "./saveOptions";
import { Tabs } from "./tabs";
import type { TasteGallery } from "./tasteGallery";
import type { TextDialog } from "./textDialog";
import { defaultSettings, type EditSettings, type OpenInfo } from "./types";
import type { AspectState } from "./crop";
import { ZoomView } from "./zoom";

export const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

/** 画面の要素。 */
export const dom = {
  stage: $<HTMLElement>("stage"),
  placeholder: $<HTMLElement>("placeholder"),
  status: $<HTMLElement>("status"),
  saveButton: $<HTMLButtonElement>("save"),
  beforeButton: $<HTMLButtonElement>("before"),
  resetButton: $<HTMLButtonElement>("reset"),
  canvas: $<HTMLCanvasElement>("canvas"),
  guide: document.getElementById("guide") as unknown as SVGSVGElement,
  zoomGuide: document.getElementById("zoom-guide") as unknown as SVGSVGElement,
  straightenGrid: document.getElementById("straighten-grid") as unknown as SVGSVGElement,
  zoomCanvas: $<HTMLCanvasElement>("zoom-canvas"),
  /** 全体表示のプレビュー（canvas・範囲の選択・ガイド）。100% 表示の間は隠す */
  frame: document.querySelector<HTMLElement>(".frame")!,
  badge: $<HTMLElement>("badge"),
  revealButton: $<HTMLButtonElement>("reveal-saved"),
  resumeButton: $<HTMLButtonElement>("resume-edits"),
  /** 左右に分けて比べる表示（加工前の canvas・境目） */
  split: $<HTMLElement>("split"),
  splitCanvas: $<HTMLCanvasElement>("split-canvas"),
  splitLine: $<HTMLElement>("split-line"),
};

/** 開いている画像と、操作の状態。 */
export const state = {
  /** 編集設定（部品と共有するので、置き換えずに中身を書き換える） */
  settings: defaultSettings() as EditSettings,
  loaded: null as OpenInfo | null,
  /** 読み込みのときのお知らせ（ステータスバーに出す） */
  notes: [] as string[],
  opening: false,
  /** 保存中・まとめて処理中（保存・開く・リセット・終了などを止める） */
  saving: false,
  /** 開いている画像の顔を探したか（肌をなめらかに。画像を開くたびに false に戻す） */
  facesPrepared: false,
  /** 「加工をコピー」でコピーした写真の設定（加工の項目だけを使う。アプリを終了するまで覚えておく） */
  copiedLook: null as EditSettings | null,
  /** 最後に保存したときの設定（未保存の変更の判定に使う） */
  savedSettings: null as EditSettings | null,
  /** 最後に保存したファイル（複数の大きさで保存なら全部。「Finder で表示」に使う） */
  savedPaths: [] as string[],
  /** 読み込める拡張子・保存できる拡張子・対応形式の説明（起動時に Rust から読む） */
  extensions: [] as string[],
  savableExtensions: [] as string[],
  formatsText: "",
};

/** アンドゥ／リドゥで戻す、設定パネルの状態（旧版の PanelState）。 */
export type Snapshot = { settings: EditSettings; aspect: AspectState; size: SizeState };

/** 画面の部品のうち、起動時に Rust から選択肢を読んでから作るもの（setup で入れる）。 */
export const parts = {} as {
  panel: Panel;
  colorPanel: ColorPanel;
  crop: CropController;
  photoControls: PhotoControls;
  textDialog: TextDialog;
  /** 「加工」タブの「文字…」のボタン（設定パネルを作った後に取る） */
  textButton: HTMLButtonElement;
  /** テイストの一覧と、それを開く「一覧…」のボタン */
  tasteGallery: TasteGallery;
  tasteButton: HTMLButtonElement;
  /** 「加工」タブの「自動補正」のボタン */
  autoButton: HTMLButtonElement;
  recorder: HistoryRecorder<Snapshot>;
  privacy: PrivacyPanel;
  localPanel: LocalPanel;
  lutControls: LutControls;
  textDrag: TextDrag;
  batchDialog: BatchDialog;
  collageDialog: CollageDialog;
};

/** 部品の知らせを受ける先（機能のファイルが setup で入れる。ここから機能のファイルを読み込まないため）。 */
export const hooks = {
  /** 画面で設定を変えたとき（履歴に積み、プレビューを描き直す） */
  userChanged: () => {},
  /** プレビューを描き直せなかったとき */
  previewError: (_error: unknown) => {},
  /** 設定が変わったとき、肌をなめらかにするための顔を必要なら探す（assist.ts。探し終えたら redraw） */
  prepareFaces: (_redraw: () => void) => {},
};

export const preview = new Preview(dom.stage, dom.canvas, (error) => hooks.previewError(error));
export const zoomView = new ZoomView(dom.stage, $<HTMLElement>("zoom"), $<HTMLElement>("zoom-image"), dom.zoomCanvas);
export const histogramView = new HistogramView($<HTMLCanvasElement>("histogram"));
export const tabs = new Tabs(document.querySelector(".side")!);
export const exifView = new ExifView($("page-exif"));
export const output = new OutputSize(state.settings, () => hooks.userChanged());
export const saveOptions = new SaveOptionsPanel({
  quality: $<HTMLInputElement>("jpeg-quality"),
  qualityValue: $<HTMLOutputElement>("jpeg-quality-value"),
  keepExif: $<HTMLInputElement>("keep-exif"),
  keepGps: $<HTMLInputElement>("keep-gps"),
  removeGps: $<HTMLInputElement>("remove-gps"),
  limit: $<HTMLInputElement>("limit-size"),
  limitMb: $<HTMLInputElement>("limit-mb"),
  fill: $<HTMLInputElement>("fill-color"),
  rights: {
    copyright: $<HTMLInputElement>("rights-copyright"),
    artist: $<HTMLInputElement>("rights-artist"),
    description: $<HTMLInputElement>("rights-description"),
  },
});

function extensionOf(path: string): string {
  const name = path.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** 読み込める拡張子のファイルか。 */
export const isSupported = (path: string) => state.extensions.includes(extensionOf(path));
