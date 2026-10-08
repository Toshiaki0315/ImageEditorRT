// 「切り抜き」タブの回転・反転とトリミング、プレビュー上のドラッグでの範囲の選択（旧版 FR-UI-30〜36・55）。
// 同じタブの「水平の補正」「背景」は photoControls.ts。
// 範囲の計算は Rust（core/crop.rs）に任せ、ここではマウスの操作・数値の欄・線の描画だけを行う。
// 座標はどれも、回転・反転した後の原寸画像の座標（px）。

import { invoke } from "@tauri-apps/api/core";
import type { AspectRatio, CropRect, EditSettings, FrameKind, OrientOp, Orientation, Region, ShapeType } from "./types";

type DragMode = "new" | "move" | "resize";
type Drag = { mode: DragMode; anchor: [number, number]; start: CropRect | null };
type Oriented = { orientation: Orientation; crop: CropRect | null; regions: Region[]; size: [number, number] };
/** アンドゥ／リドゥで範囲と一緒に戻す、比の選択と「縦向き」（戻した範囲と比の固定が食い違わないように） */
export type AspectState = { ratio: AspectRatio; portrait: boolean };

const SVG = "http://www.w3.org/2000/svg";
/** ハンドルの大きさと、当たり判定の半径（画面の px） */
const HANDLE_SIZE = 8;
const HANDLE_HIT = 10;
const TRIM_TEXT = "トリミング実行";
/** フレーム・円を選んでいるときに比のプルダウンに出す項目 */
const FOLLOW = "follow";
const FOLLOW_TEXT = "フレーム・円に合わせる";
const CORNER_RADIUS_DEFAULT = 10;
const EDIT_RANGE_TEXT = "範囲を編集";
/** 縦向きを選べる比（自由と 1:1 には向きがない） */
const HAS_ORIENTATION: AspectRatio[] = ["ratio5x4", "ratio4x3", "ratio3x2", "ratio16x9"];

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
  private readonly frame = $<HTMLSelectElement>("frame");
  private readonly shape = $<HTMLSelectElement>("shape");
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

  /** 範囲をドラッグしている間は true（ドラッグ全体を 1 回の操作として履歴に積む）。 */
  isDragging(): boolean {
    return this.drag !== null;
  }

  isTrimmed(): boolean {
    return this.trim.getAttribute("aria-pressed") === "true";
  }

  /** プレビューの表示の大きさが変わったとき・描き直したとき、範囲・形の線を描き直す。 */
  draw() {
    const active = this.size !== null && !this.isTrimmed() && !this.canvas.hidden;
    this.overlay.toggleAttribute("hidden", !active);
    if (!active || !this.size) return;
    const [width, height] = [this.canvas.clientWidth, this.canvas.clientHeight];
    this.overlay.setAttribute("viewBox", `0 0 ${width} ${height}`);
    this.overlay.replaceChildren();
    const crop = this.settings.crop;
    const outline = this.shapeOutline();
    if (!crop && !outline) return;
    const r = crop ? this.toScreen(crop) : null;
    // 範囲の外と形の外側を暗くする（形があれば形、なければ範囲の内側だけを明るく残す。旧版 FR-UI-58）
    const hole = outline ?? (r ? `M${r.x} ${r.y}h${r.width}v${r.height}h${-r.width}Z` : "");
    const mask = document.createElementNS(SVG, "path");
    mask.setAttribute("class", "mask");
    mask.setAttribute("d", `M0 0H${width}V${height}H0Z ${hole}`);
    this.overlay.append(mask);
    // 枠と形の輪郭（明るい写真でも暗い写真でも見えるよう、白い線の外側に黒い線）
    for (const kind of ["edge-shadow", "edge"]) {
      if (outline) {
        const path = document.createElementNS(SVG, "path");
        path.setAttribute("class", kind);
        path.setAttribute("d", outline);
        this.overlay.append(path);
      }
      if (r) {
        const rect = document.createElementNS(SVG, "rect");
        rect.setAttribute("class", kind);
        for (const [key, value] of Object.entries(r)) rect.setAttribute(key, String(value));
        this.overlay.append(rect);
      }
    }
    if (!r) return;
    for (const [cx, cy] of corners(r)) {
      const handle = document.createElementNS(SVG, "rect");
      handle.setAttribute("class", "handle");
      handle.setAttribute("x", String(cx - HANDLE_SIZE / 2));
      handle.setAttribute("y", String(cy - HANDLE_SIZE / 2));
      handle.setAttribute("width", String(HANDLE_SIZE));
      handle.setAttribute("height", String(HANDLE_SIZE));
      this.overlay.append(handle);
    }
  }

  /** 形（角丸・円）の輪郭の SVG のパス（画面の座標）。矩形・半径 0 の角丸なら null。 */
  private shapeOutline(): string | null {
    if (!this.size) return null;
    const area = this.toScreen(this.shapeArea ?? { x: 0, y: 0, width: this.size[0], height: this.size[1] });
    const short = Math.min(area.width, area.height);
    if (this.settings.shape === "circle") {
      // 中央の、短辺を直径とする正円
      const [cx, cy, radius] = [area.x + area.width / 2, area.y + area.height / 2, short / 2];
      return `M${cx - radius} ${cy}a${radius} ${radius} 0 1 0 ${2 * radius} 0a${radius} ${radius} 0 1 0 ${-2 * radius} 0Z`;
    }
    if (this.settings.shape === "rounded" && this.settings.cornerRadius > 0) {
      const r = (short * Math.min(this.settings.cornerRadius, 50)) / 100;
      const { x, y, width, height } = area;
      return (
        `M${x + r} ${y}H${x + width - r}A${r} ${r} 0 0 1 ${x + width} ${y + r}V${y + height - r}` +
        `A${r} ${r} 0 0 1 ${x + width - r} ${y + height}H${x + r}A${r} ${r} 0 0 1 ${x} ${y + height - r}` +
        `V${y + r}A${r} ${r} 0 0 1 ${x + r} ${y}Z`
      );
    }
    return null;
  }

  /** 形をかける範囲を Rust に聞いて覚え、線を描き直す。 */
  private async refreshShapeArea() {
    if (!this.size) return;
    this.shapeArea = await invoke<CropRect | null>("effective_crop", { settings: this.settings, size: this.size });
    this.draw();
  }

  private setCorner(value: number) {
    this.settings.cornerRadius = value;
    this.updateControls();
    this.draw();
    this.onChange();
  }

  // --- 範囲・比・向き ----------------------------------------------------------

  private aspectChoice() {
    return {
      ratio: this.chosenRatio,
      portrait: this.portrait.checked,
      frame: this.settings.frame,
      shape: this.settings.shape,
    };
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
      size: this.size,
    });
    this.settings.orientation = result.orientation;
    this.settings.regions = result.regions;
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
    const [width, height] = this.size!;
    const sx = this.canvas.clientWidth / width;
    const sy = this.canvas.clientHeight / height;
    return { x: crop.x * sx, y: crop.y * sy, width: crop.width * sx, height: crop.height * sy };
  }

  /** 画面の点が四隅のハンドルの上なら、角の番号（左上から時計回りに 0〜3）。 */
  private hitCorner(event: PointerEvent): number | null {
    if (!this.settings.crop) return null;
    const box = this.overlay.getBoundingClientRect();
    const [px, py] = [event.clientX - box.left, event.clientY - box.top];
    const index = corners(this.toScreen(this.settings.crop)).findIndex(
      ([cx, cy]) => Math.abs(px - cx) <= HANDLE_HIT && Math.abs(py - cy) <= HANDLE_HIT,
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

/** 左上・右上・右下・左下の順の角。 */
function corners(r: CropRect): [number, number][] {
  return [
    [r.x, r.y],
    [r.x + r.width, r.y],
    [r.x + r.width, r.y + r.height],
    [r.x, r.y + r.height],
  ];
}
