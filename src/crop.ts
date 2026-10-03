// 「切り抜き」タブの回転・反転とトリミング、プレビュー上のドラッグでの範囲の選択（旧版 FR-UI-30〜36・55）。
// 範囲の計算は Rust（core/crop.rs）に任せ、ここではマウスの操作・数値の欄・線の描画だけを行う。
// 座標はどれも、回転・反転した後の原寸画像の座標（px）。

import { invoke } from "@tauri-apps/api/core";
import type { AspectRatio, CropRect, EditSettings, OrientOp, Orientation } from "./types";

type DragMode = "new" | "move" | "resize";
type Drag = { mode: DragMode; anchor: [number, number]; start: CropRect | null };
type Oriented = { orientation: Orientation; crop: CropRect | null; size: [number, number] };

const SVG = "http://www.w3.org/2000/svg";
/** ハンドルの大きさと、当たり判定の半径（画面の px） */
const HANDLE_SIZE = 8;
const HANDLE_HIT = 10;
const TRIM_TEXT = "トリミング実行";
const EDIT_RANGE_TEXT = "範囲を編集";
/** 縦向きを選べる比（自由と 1:1 には向きがない） */
const HAS_ORIENTATION: AspectRatio[] = ["ratio4x3", "ratio3x2", "ratio16x9"];

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
    private readonly onChange: () => void,
    private readonly onTrimChange: (trimmed: boolean) => void,
  ) {
    for (const [value, label] of ratios) this.aspect.add(new Option(label, value));
    this.aspect.addEventListener("change", () => void this.aspectChanged());
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

  /** 画像を開いたとき（size は原寸の大きさ）。向き・範囲・比・表示を初期状態に戻す。 */
  reset(size: [number, number] | null) {
    this.size = size;
    this.aspect.value = "free";
    this.portrait.checked = false;
    this.setTrimmed(false, false);
    this.updateControls();
    this.draw();
  }

  isTrimmed(): boolean {
    return this.trim.getAttribute("aria-pressed") === "true";
  }

  /** プレビューの表示の大きさが変わったとき・描き直したとき、範囲の線を描き直す。 */
  draw() {
    const active = this.size !== null && !this.isTrimmed() && !this.canvas.hidden;
    this.overlay.toggleAttribute("hidden", !active);
    if (!active || !this.size) return;
    const [width, height] = [this.canvas.clientWidth, this.canvas.clientHeight];
    this.overlay.setAttribute("viewBox", `0 0 ${width} ${height}`);
    this.overlay.replaceChildren();
    const crop = this.settings.crop;
    if (!crop) return;
    const r = this.toScreen(crop);
    // 範囲の外を暗くする（外側の四角から範囲をくり抜く）
    const mask = document.createElementNS(SVG, "path");
    mask.setAttribute("class", "mask");
    mask.setAttribute("d", `M0 0H${width}V${height}H0Z M${r.x} ${r.y}h${r.width}v${r.height}h${-r.width}Z`);
    this.overlay.append(mask);
    // 枠（明るい写真でも暗い写真でも見えるよう、白い線の外側に黒い線）
    for (const kind of ["edge-shadow", "edge"]) {
      const rect = document.createElementNS(SVG, "rect");
      rect.setAttribute("class", kind);
      for (const [key, value] of Object.entries(r)) rect.setAttribute(key, String(value));
      this.overlay.append(rect);
    }
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

  // --- 範囲・比・向き ----------------------------------------------------------

  private aspectChoice() {
    return { ratio: this.aspect.value as AspectRatio, portrait: this.portrait.checked };
  }

  /**
   * 範囲を設定し、数値の欄・線・プレビューを更新する。
   * keepSpins なら、範囲がない（幅・高さが 0 など入力の途中）ときに数値の欄を書き換えない。
   */
  private setCrop(crop: CropRect | null, notify = true, keepSpins = false) {
    this.settings.crop = crop;
    this.updateControls(keepSpins && !crop);
    this.draw();
    if (notify) this.onChange();
  }

  /** 比・縦向きを変えたとき: 範囲があれば、その中央を新しい比に直す。 */
  private async aspectChanged() {
    this.updateControls();
    if (!this.size) return;
    const crop = await invoke<CropRect | null>("crop_fit", {
      rect: this.settings.crop,
      aspect: this.aspectChoice(),
      size: this.size,
    });
    if (this.settings.crop) this.setCrop(crop);
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
      size: this.size,
    });
    this.settings.orientation = result.orientation;
    this.size = result.size;
    const swaps = op === "rotate_left" || op === "rotate_right";
    if (swaps && !this.portrait.disabled) this.portrait.checked = !this.portrait.checked;
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
    this.aspect.disabled = !loaded;
    this.portrait.disabled = !loaded || !HAS_ORIENTATION.includes(this.aspect.value as AspectRatio);
    this.clear.disabled = !loaded || !crop;
    for (const button of document.querySelectorAll<HTMLButtonElement>("[data-orient]")) button.disabled = !loaded;
    // 範囲がなくなったら全体の表示に戻す（フレーム・形は #13 で足す）
    if (!crop && this.isTrimmed()) this.setTrimmed(false);
    this.trim.disabled = !loaded || !crop;
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
