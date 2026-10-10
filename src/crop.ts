// 「切り抜き」タブの回転・反転とトリミング、プレビュー上のドラッグでの範囲の選択（旧版 FR-UI-30〜36・55）。
// 同じタブの「水平の補正」「背景」は photoControls.ts。
// 範囲の計算は Rust（core/crop.rs）に任せ、ここではマウスの操作・数値の欄・線の描画だけを行う。
// 座標はどれも、回転・反転した後の原寸画像の座標（px）。

import { invoke } from "@tauri-apps/api/core";
import { drawCropOverlay } from "./cropOverlay";
import { shapeOutline } from "./cropShape";
import { GUIDE_KINDS, type GuideKind, parseGuide } from "./guides";
import { HANDLE_HIT } from "./overlay";
import { nearPoint, rectCorners, scaleRect, screenScale } from "./regions";
import type {
  AspectRatio,
  CropRect,
  EditSettings,
  FrameKind,
  LocalAdjust,
  OrientOp,
  Orientation,
  Region,
  ShapeType,
} from "./types";
import { readStored, writeStored } from "./storage";

type DragMode = "new" | "move" | "resize";
type Drag = { mode: DragMode; anchor: [number, number]; start: CropRect | null };
type Oriented = {
  orientation: Orientation;
  crop: CropRect | null;
  regions: Region[];
  localAdjustments: LocalAdjust[];
  size: [number, number];
};
/** 範囲に保たせる縦横比の指定（Rust の crop::AspectChoice）。 */
type AspectChoice = { ratio: AspectRatio; portrait: boolean; frame: FrameKind; shape: ShapeType };
/** アンドゥ／リドゥで範囲と一緒に戻す、比の選択と「縦向き」（戻した範囲と比の固定が食い違わないように） */
export type AspectState = { ratio: AspectRatio; portrait: boolean };

const TRIM_TEXT = "トリミング実行";
/** フレーム・円を選んでいるときに比のプルダウンに出す項目 */
const FOLLOW = "follow";
const FOLLOW_TEXT = "フレーム・円に合わせる";
const CORNER_RADIUS_DEFAULT = 10;
const EDIT_RANGE_TEXT = "範囲を編集";
/** 縦向きを選べる比（自由と 1:1 には向きがない） */
const HAS_ORIENTATION: AspectRatio[] = ["ratio5x4", "ratio4x3", "ratio3x2", "ratio16x9"];
/** 選んだガイドを残す環境設定の名前 */
const GUIDE_STORAGE_KEY = "crop.guide";

/** 比の名前を縦の形にする（「16:9」→「9:16」）。 */
export const portraitName = (name: string) => name.split(":").reverse().join(":");

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class CropController {
  /** 回転・反転した後の原寸画像の大きさ（未読込なら null） */
  private size: [number, number] | null = null;
  private drag: Drag | null = null;
  /** ドラッグの結果が前後しないよう、問い合わせに番号を振る */
  private sequence = 0;
  private readonly overlay = document.getElementById("overlay") as unknown as SVGSVGElement;
  private readonly aspect = $<HTMLSelectElement>("aspect");
  private readonly portrait = $<HTMLInputElement>("portrait");
  private readonly trim = $<HTMLButtonElement>("trim");
  private readonly clear = $<HTMLButtonElement>("clear-crop");
  private readonly autoCrop = $<HTMLButtonElement>("auto-crop");
  /** おまかせ切り抜きの範囲を問い合わせる（見つからなければ null、失敗なら undefined。assist.ts が入れる） */
  /** 写真の範囲（photoArea）が変わったとき（文字・ロゴのドラッグの印を描き直す） */
  onAreaChange: () => void = () => {};
  findSubjectCrop: (aspect: AspectChoice) => Promise<CropRect | null | undefined> = async () => undefined;
  /** おまかせ切り抜きの結果を知らせる（範囲にしたか） */
  onAutoCrop: (found: boolean) => void = () => {};
  private readonly frame = $<HTMLSelectElement>("frame");
  private readonly shape = $<HTMLSelectElement>("shape");
  private readonly guide = $<HTMLSelectElement>("crop-guide");
  /** ガイドを描くか（「切り抜き」タブを開いている間だけ） */
  private guideActive = false;
  private readonly corner = $<HTMLInputElement>("corner-radius");
  private readonly cornerValue = $<HTMLOutputElement>("corner-radius-value");
  /** 向きのある比の、横向きの名前（「縦向き」で名前を縦の形にするため） */
  private readonly ratioNames = new Map<string, string>();
  /** フレーム・円で比を固定したときに覚えておく、比のプルダウンの選択 */
  private chosenRatio: AspectRatio = "free";
  /** 形をかける範囲（実際に切り抜く範囲。なければ画像全体） */
  private shapeArea: CropRect | null = null;
  private readonly spins = {
    x: $<HTMLInputElement>("crop-x"),
    y: $<HTMLInputElement>("crop-y"),
    width: $<HTMLInputElement>("crop-width"),
    height: $<HTMLInputElement>("crop-height"),
  };

  /**
   * @param onChange 範囲・向きが変わったとき（プレビューを描き直す）
   * @param onTrimChange 「トリミング実行」の表示を切り替えたとき
   */
  constructor(
    private readonly settings: EditSettings,
    private readonly canvas: HTMLCanvasElement,
    ratios: [AspectRatio, string][],
    frames: [FrameKind, string][],
    shapes: [ShapeType, string][],
    private readonly onChange: () => void,
    private readonly onTrimChange: (trimmed: boolean) => void,
    private readonly onRotate: () => Promise<void> = async () => {},
  ) {
    for (const [value, label] of ratios) {
      this.aspect.add(new Option(label, value));
      if (HAS_ORIENTATION.includes(value)) this.ratioNames.set(value, label);
    }
    const follow = new Option(FOLLOW_TEXT, FOLLOW);
    follow.disabled = true;
    this.aspect.add(follow);
    for (const [value, label] of GUIDE_KINDS) this.guide.add(new Option(label, value));
    // 環境設定に残したガイド（読めなければなし）
    this.guide.value = parseGuide(readStored(GUIDE_STORAGE_KEY));
    this.guide.addEventListener("change", () => {
      writeStored(GUIDE_STORAGE_KEY, this.guide.value);
      this.draw();
    });
    for (const [value, label] of frames) this.frame.add(new Option(label, value));
    for (const [value, label] of shapes) this.shape.add(new Option(label, value));
    this.aspect.addEventListener("change", () => {
      this.chosenRatio = this.aspect.value as AspectRatio;
      void this.aspectChanged();
    });
    this.frame.addEventListener("change", () => {
      this.settings.frame = this.frame.value as FrameKind;
      void this.aspectChanged();
    });
    this.shape.addEventListener("change", () => {
      this.settings.shape = this.shape.value as ShapeType;
      void this.aspectChanged();
    });
    this.corner.addEventListener("input", () => this.setCorner(Number(this.corner.value)));
    // ダブルクリックで既定値に戻す（旧版 FR-UI-53。角丸以外の形では操作できない）
    this.corner.addEventListener("dblclick", () => this.setCorner(CORNER_RADIUS_DEFAULT));
    this.portrait.addEventListener("change", () => void this.aspectChanged());
    for (const button of document.querySelectorAll<HTMLButtonElement>("[data-orient]")) {
      button.addEventListener("click", () => void this.orient(button.dataset.orient as OrientOp));
    }
    // 旧版の数値の欄と同じく、入力のたびに反映する（矢印・1 文字ずつ）
    for (const [field, input] of Object.entries(this.spins)) {
      input.addEventListener("input", () => void this.spinEdited(field as keyof CropRect));
    }
    this.clear.addEventListener("click", () => this.setCrop(null));
    this.autoCrop.addEventListener("click", () => void this.cropAutomatically());
    this.trim.addEventListener("click", () => this.setTrimmed(!this.isTrimmed()));
    this.overlay.addEventListener("pointerdown", (e) => this.pointerDown(e));
    this.overlay.addEventListener("pointermove", (e) => void this.pointerMove(e));
    this.overlay.addEventListener("pointerup", (e) => void this.pointerUp(e));
    this.reset(null);
  }

  /** 画像を開いたとき（size は原寸の大きさ）。向き・範囲・比・フレーム・形・表示を初期状態に戻す。 */
  reset(size: [number, number] | null) {
    this.size = size;
    this.chosenRatio = "free";
    this.aspect.value = "free";
    this.shapeArea = null;
    this.portrait.checked = false;
    this.setTrimmed(false, false);
    this.updateControls();
    this.draw();
  }

  /** 比の選択と「縦向き」（履歴に積む）。 */
  aspectState(): AspectState {
    return { ratio: this.chosenRatio, portrait: this.portrait.checked };
  }

  /**
   * 履歴の状態に戻したとき（settings はもう戻してある）。比の固定で範囲を直したりせず、取ったときの値をそのまま使う。
   * originalSize は回転・反転する前の原寸。
   */
  restore(state: AspectState, originalSize: [number, number]) {
    this.chosenRatio = state.ratio;
    this.portrait.checked = state.portrait;
    const [width, height] = originalSize;
    this.size = this.settings.orientation.rotation % 180 === 0 ? [width, height] : [height, width];
    this.shapeArea = null;
    this.updateControls();
    this.draw();
    void this.refreshShapeArea();
  }

  /** プリセットでフレーム・形が変わったとき: 手で選んだときと同じく、範囲をその比に直す。 */
  refit(): Promise<void> {
    return this.aspectChanged();
  }

  /** 文字・ロゴを描く写真の範囲（実際に切り抜く範囲。なければ画像全体。回転・反転した後の原寸の座標）。画像がなければ null。 */
  photoArea(): CropRect | null {
    if (!this.size) return null;
    return this.shapeArea ?? { x: 0, y: 0, width: this.size[0], height: this.size[1] };
  }

  /** 範囲をドラッグしている間は true（ドラッグ全体を 1 回の操作として履歴に積む）。 */
  isDragging(): boolean {
    return this.drag !== null;
  }

  /**
   * Esc キー: トリミング範囲をクリアする（「範囲をクリア」と同じ。ドラッグの途中ならドラッグもやめる）。
   * 範囲が見えていない（画像がない・切り抜いた表示）か、範囲がなければ何もせず false。
   */
  cancelRange(): boolean {
    if (!this.size || this.isTrimmed() || !this.settings.crop) return false;
    this.drag = null;
    this.sequence += 1; // 問い合わせ中のドラッグの結果は使わない
    this.setCrop(null);
    return true;
  }

  isTrimmed(): boolean {
    return this.trim.getAttribute("aria-pressed") === "true";
  }

  /** ガイドを描くかを切り替える（「切り抜き」タブを開いたとき・閉じたとき）。 */
  setGuideActive(active: boolean) {
    if (active === this.guideActive) return;
    this.guideActive = active;
    this.draw();
  }

  /** プレビューの表示の大きさが変わったとき・描き直したとき、範囲・形の線を描き直す。 */
  draw() {
    const active = this.size !== null && !this.isTrimmed() && !this.canvas.hidden;
    this.overlay.toggleAttribute("hidden", !active);
    if (!active || !this.size) return;
    const area = this.toScreen(this.shapeArea ?? { x: 0, y: 0, width: this.size[0], height: this.size[1] });
    drawCropOverlay(this.overlay, {
      width: this.canvas.clientWidth,
      height: this.canvas.clientHeight,
      crop: this.settings.crop ? this.toScreen(this.settings.crop) : null,
      outline: shapeOutline(this.settings.shape, this.settings.cornerRadius, area),
      guide: this.guideActive ? (this.guide.value as GuideKind) : null,
    });
  }

  /** 形をかける範囲を Rust に聞いて覚え、線を描き直す。 */
  private async refreshShapeArea() {
    if (!this.size) return;
    this.shapeArea = await invoke<CropRect | null>("effective_crop", { settings: this.settings, size: this.size });
    this.draw();
    this.onAreaChange();
  }

  private setCorner(value: number) {
    this.settings.cornerRadius = value;
    this.updateControls();
    this.draw();
    this.onChange();
  }

  // --- 範囲・比・向き ----------------------------------------------------------

  private aspectChoice(): AspectChoice {
    return {
      ratio: this.chosenRatio,
      portrait: this.portrait.checked,
      frame: this.settings.frame,
      shape: this.settings.shape,
    };
  }

  /** おまかせ切り抜き: 目立つ被写体が中央寄りに入る範囲にする（見つからなければ範囲はそのまま）。 */
  private async cropAutomatically() {
    if (!this.size) return;
    this.autoCrop.disabled = true;
    try {
      const crop = await this.findSubjectCrop(this.aspectChoice());
      if (crop === undefined) return;
      if (crop) this.setCrop(crop);
      this.onAutoCrop(crop !== null);
    } finally {
      this.autoCrop.disabled = this.size === null;
    }
  }

  /** フレーム・円を選んでいれば、比をそれに固定する（比のプルダウンは選べない）。 */
  private locked(): boolean {
    return this.settings.frame !== "none" || this.settings.shape === "circle";
  }

  /**
   * 範囲を設定し、数値の欄・線・プレビューを更新する。
   * keepSpins なら、範囲がない（幅・高さが 0 など入力の途中）ときに数値の欄を書き換えない。
   */
  private setCrop(crop: CropRect | null, notify = true, keepSpins = false) {
    this.settings.crop = crop;
    this.updateControls(keepSpins && !crop);
    this.draw();
    void this.refreshShapeArea();
    if (notify) this.onChange();
  }

  /** 比・縦向き・フレーム・形を変えたとき: 範囲があれば、その中央を新しい比に直す。 */
  private async aspectChanged() {
    this.updateControls();
    if (!this.size) return;
    const crop = await invoke<CropRect | null>("crop_fit", {
      rect: this.settings.crop,
      aspect: this.aspectChoice(),
      size: this.size,
    });
    this.setCrop(this.settings.crop ? crop : null);
  }

  private async spinEdited(field: keyof CropRect) {
    if (!this.size) return;
    const value = (input: HTMLInputElement) => Math.max(0, Math.round(Number(input.value) || 0));
    const values: CropRect = {
      x: value(this.spins.x),
      y: value(this.spins.y),
      width: value(this.spins.width),
      height: value(this.spins.height),
    };
    const crop = await invoke<CropRect | null>("crop_spin", {
      field,
      values,
      previous: this.settings.crop,
      aspect: this.aspectChoice(),
      size: this.size,
    });
    this.setCrop(crop, true, true);
  }

  /** 表示中の向きに対して 90° 回転・反転する。範囲も一緒に回し、90° なら比の「縦向き」も入れ替える。 */
  private async orient(op: OrientOp) {
    if (!this.size) return;
    const result = await invoke<Oriented>("crop_orient", {
      orientation: this.settings.orientation,
      op,
      crop: this.settings.crop,
      regions: this.settings.regions,
      localAdjustments: this.settings.localAdjustments,
      size: this.size,
    });
    this.settings.orientation = result.orientation;
    this.settings.regions = result.regions;
    this.settings.localAdjustments = result.localAdjustments;
    // 反転すると傾きの向きも逆になる（90° 回転では同じ角度のまま）
    if ((op === "flip_horizontal" || op === "flip_vertical") && this.settings.straighten !== 0) {
      this.settings.straighten = -this.settings.straighten;
    }
    this.size = result.size;
    const swaps = op === "rotate_left" || op === "rotate_right";
    if (swaps && !this.portrait.disabled) this.portrait.checked = !this.portrait.checked;
    // 90° 回したときは、手で変えた出力の幅・高さも入れ替える
    if (swaps) await this.onRotate();
    this.shapeArea = null;
    this.setCrop(result.crop);
  }

  /** 「トリミング実行」: 切り抜いた範囲だけの表示と、元の画角全体＋範囲の表示を切り替える。 */
  private setTrimmed(trimmed: boolean, notify = true) {
    this.trim.setAttribute("aria-pressed", String(trimmed));
    this.trim.textContent = trimmed ? EDIT_RANGE_TEXT : TRIM_TEXT;
    this.draw();
    if (notify) this.onTrimChange(trimmed);
  }

  /** 数値の欄・ボタンの状態を、今の範囲・比に合わせる。keepSpins なら欄の値はそのまま。 */
  private updateControls(keepSpins = false) {
    const loaded = this.size !== null;
    const crop = this.settings.crop;
    const values = crop ?? { x: 0, y: 0, width: 0, height: 0 };
    for (const [field, input] of Object.entries(this.spins)) {
      input.disabled = !loaded;
      if (!keepSpins) input.value = loaded ? String(values[field as keyof CropRect]) : "";
    }
    if (this.size) {
      this.spins.x.max = String(Math.max(0, this.size[0] - 1));
      this.spins.y.max = String(Math.max(0, this.size[1] - 1));
      this.spins.width.max = String(this.size[0]);
      this.spins.height.max = String(this.size[1]);
    }
    // フレーム・円を選んでいるときは比をそれに固定し、プルダウンは「フレーム・円に合わせる」にする
    const locked = this.locked();
    this.aspect.value = locked ? FOLLOW : this.chosenRatio;
    this.aspect.disabled = !loaded || locked;
    this.portrait.disabled = !loaded || locked || !HAS_ORIENTATION.includes(this.chosenRatio);
    this.showRatioNames();
    this.frame.value = this.settings.frame;
    this.shape.value = this.settings.shape;
    this.frame.disabled = this.shape.disabled = !loaded;
    this.corner.value = String(this.settings.cornerRadius);
    this.cornerValue.textContent = `${this.settings.cornerRadius}%`;
    this.corner.disabled = !loaded || this.settings.shape !== "rounded";
    this.clear.disabled = !loaded || !crop;
    this.autoCrop.disabled = !loaded;
    for (const button of document.querySelectorAll<HTMLButtonElement>("[data-orient]")) button.disabled = !loaded;
    // 範囲・フレーム・形（矩形以外）のどれもなければ「トリミング実行」は押せず、全体の表示に戻す
    const canTrim = loaded && (crop !== null || this.settings.frame !== "none" || this.settings.shape !== "rectangle");
    if (!canTrim && this.isTrimmed()) this.setTrimmed(false);
    this.trim.disabled = !canTrim;
  }

  /** 「縦向き」にチェックがあれば、向きのある比の名前を縦の形（4:5・9:16 など）で出す。 */
  private showRatioNames() {
    for (const option of this.aspect.options) {
      const name = this.ratioNames.get(option.value);
      if (name) option.text = this.portrait.checked ? portraitName(name) : name;
    }
  }

  // --- プレビュー上のドラッグ ----------------------------------------------------

  /** 画面の座標を原寸画像の座標にする（画像の外は端に収める）。 */
  private toImage(event: PointerEvent): [number, number] {
    const box = this.overlay.getBoundingClientRect();
    const [width, height] = this.size!;
    const x = Math.round(((event.clientX - box.left) * width) / box.width);
    const y = Math.round(((event.clientY - box.top) * height) / box.height);
    return [Math.min(Math.max(x, 0), width), Math.min(Math.max(y, 0), height)];
  }

  /** 原寸画像の座標の範囲を、画面（重ねている SVG）の座標にする。 */
  private toScreen(crop: CropRect): CropRect {
    return scaleRect(crop, screenScale([this.canvas.clientWidth, this.canvas.clientHeight], this.size!));
  }

  /** 画面の点が四隅のハンドルの上なら、角の番号（左上から時計回りに 0〜3）。 */
  private hitCorner(event: PointerEvent): number | null {
    if (!this.settings.crop) return null;
    const box = this.overlay.getBoundingClientRect();
    const [px, py] = [event.clientX - box.left, event.clientY - box.top];
    const index = rectCorners(this.toScreen(this.settings.crop)).findIndex((corner) =>
      nearPoint(corner, [px, py], HANDLE_HIT),
    );
    return index >= 0 ? index : null;
  }

  private inside(event: PointerEvent): boolean {
    const crop = this.settings.crop;
    if (!crop) return false;
    const [x, y] = this.toImage(event);
    return x >= crop.x && x <= crop.x + crop.width && y >= crop.y && y <= crop.y + crop.height;
  }

  private pointerDown(event: PointerEvent) {
    if (event.button !== 0 || !this.size) return;
    const point = this.toImage(event);
    const crop = this.settings.crop;
    const corner = this.hitCorner(event);
    if (corner !== null && crop) {
      const anchors: [number, number][] = [
        [crop.x + crop.width, crop.y + crop.height],
        [crop.x, crop.y + crop.height],
        [crop.x, crop.y],
        [crop.x + crop.width, crop.y],
      ];
      this.drag = { mode: "resize", anchor: anchors[corner], start: crop };
    } else if (crop && this.inside(event)) {
      this.drag = { mode: "move", anchor: point, start: crop };
    } else {
      this.drag = { mode: "new", anchor: point, start: null };
      this.setCrop(null);
    }
    this.overlay.setPointerCapture(event.pointerId);
  }

  private async pointerMove(event: PointerEvent) {
    if (!this.drag) {
      this.updateCursor(event);
      return;
    }
    await this.dragTo(event);
  }

  private async pointerUp(event: PointerEvent) {
    if (!this.drag || event.button !== 0) return;
    await this.dragTo(event); // 離した位置まで反映する
    this.drag = null;
    const crop = this.settings.crop;
    // 幅・高さが 0（クリックしただけ）ならトリミングなし
    if (crop && (crop.width <= 0 || crop.height <= 0)) this.setCrop(null);
  }

  private async dragTo(event: PointerEvent) {
    const drag = this.drag;
    if (!drag || !this.size) return;
    const sequence = ++this.sequence;
    const crop = await invoke<CropRect | null>("crop_drag", {
      mode: drag.mode,
      anchor: drag.anchor,
      point: this.toImage(event),
      start: drag.start,
      aspect: this.aspectChoice(),
      size: this.size,
    });
    if (sequence === this.sequence) this.setCrop(crop);
  }

  private updateCursor(event: PointerEvent) {
    const corner = this.hitCorner(event);
    this.overlay.style.cursor =
      corner === 0 || corner === 2
        ? "nwse-resize"
        : corner === 1 || corner === 3
          ? "nesw-resize"
          : this.inside(event)
            ? "move"
            : "crosshair";
  }
}
