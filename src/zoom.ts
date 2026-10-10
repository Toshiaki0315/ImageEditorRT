// 100% 表示: 原寸で処理した保存結果を、画像 1px = 画面の 1 画素（Retina では実ピクセル）で見せる（旧版 FR-UI-48）。
// ドラッグとスクロールで見る場所を動かし、画像が表示より小さい向きは中央にそろえる。

// Node のテスト（tests-ts/）からも読むので、拡張子まで書く
import { context2d, imageData } from "./colorSpace.ts";

/** マウスのホイールを行単位で送ってきたときの 1 行の移動量 (px)。1 段（3 行）で 30px（旧版と同じ） */
const LINE_SCROLL_PX = 10;

/** 大きさ（論理ピクセル）。 */
export type Size = { width: number; height: number };

/**
 * 画像の左上の位置を、見える範囲に収める: 画像が表示より小さい向きは中央にそろえ、大きい向きは
 * はみ出し過ぎない（端が表示の内側に入らない）ようにする。
 */
export function clampOffset(x: number, y: number, image: Size, area: Size): { x: number; y: number } {
  const clamp = (value: number, length: number, room: number) =>
    length <= room ? (room - length) / 2 : Math.min(0, Math.max(room - length, value));
  return { x: clamp(x, image.width, area.width), y: clamp(y, image.height, area.height) };
}

/** 画像の点 center（画素）を表示の中央に置くときの、画像の左上の位置（ratio は画面の 1 論理ピクセルあたりの画素）。 */
export function centeredOffset(center: [number, number], area: Size, ratio: number): { x: number; y: number } {
  return { x: area.width / 2 - center[0] / ratio, y: area.height / 2 - center[1] / ratio };
}

export class ZoomView {
  private readonly context: CanvasRenderingContext2D;
  /** 画像の左上（表示エリアの座標、論理ピクセル） */
  private offset = { x: 0, y: 0 };
  /** 表示している画像の大きさ（画素）。表示していなければ null */
  private size: [number, number] | null = null;
  private drag: { pointer: number; x: number; y: number; from: { x: number; y: number } } | null = null;
  /** 見る場所が変わったとき（左上の表示を合わせる） */
  onMove: () => void = () => {};
  /** ダブルクリックしたとき（画面に合わせた表示に戻す） */
  onDoubleClick: () => void = () => {};

  private readonly stage: HTMLElement;
  private readonly container: HTMLElement;
  private readonly holder: HTMLElement;
  private readonly canvas: HTMLCanvasElement;

  constructor(stage: HTMLElement, container: HTMLElement, holder: HTMLElement, canvas: HTMLCanvasElement) {
    this.stage = stage;
    this.container = container;
    this.holder = holder;
    this.canvas = canvas;
    this.context = context2d(canvas);
    container.addEventListener("pointerdown", (event) => {
      if (event.button !== 0 || !this.size) return;
      container.setPointerCapture(event.pointerId);
      this.drag = { pointer: event.pointerId, x: event.clientX, y: event.clientY, from: { ...this.offset } };
      container.classList.add("dragging");
    });
    container.addEventListener("pointermove", (event) => {
      if (!this.drag || event.pointerId !== this.drag.pointer) return;
      this.moveTo(this.drag.from.x + event.clientX - this.drag.x, this.drag.from.y + event.clientY - this.drag.y);
    });
    const end = () => {
      this.drag = null;
      container.classList.remove("dragging");
    };
    container.addEventListener("pointerup", end);
    container.addEventListener("pointercancel", end);
    container.addEventListener(
      "wheel",
      (event) => {
        if (!this.size) return;
        event.preventDefault();
        const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? LINE_SCROLL_PX : 1;
        this.moveTo(this.offset.x - event.deltaX * unit, this.offset.y - event.deltaY * unit);
      },
      { passive: false },
    );
    container.addEventListener("dblclick", () => this.onDoubleClick());
    new ResizeObserver(() => {
      if (this.size) this.moveTo(this.offset.x, this.offset.y);
    }).observe(stage);
  }

  /** 画像を表示しているか。 */
  get active(): boolean {
    return this.size !== null;
  }

  /**
   * 画像を表示する。center（画像の座標）を渡すとその点を表示の中央にする。渡さなければ、
   * 同じ大きさの画像を表示中なら見ている場所を保ち、そうでなければ画像の中央を見せる。
   */
  show(width: number, height: number, pixels: Uint8ClampedArray<ArrayBuffer>, center: [number, number] | null) {
    const previous = this.size;
    this.size = [width, height];
    this.canvas.width = width;
    this.canvas.height = height;
    const ratio = window.devicePixelRatio || 1;
    this.canvas.style.width = `${width / ratio}px`;
    this.canvas.style.height = `${height / ratio}px`;
    this.context.putImageData(imageData(pixels, width, height), 0, 0);
    this.container.hidden = false;
    if (center || !previous || previous[0] !== width || previous[1] !== height) {
      const start = centeredOffset(center ?? [width / 2, height / 2], this.area(), ratio);
      this.moveTo(start.x, start.y);
    } else {
      this.moveTo(this.offset.x, this.offset.y);
    }
  }

  /** 表示をやめる。 */
  hide() {
    this.size = null;
    this.drag = null;
    this.container.hidden = true;
    this.canvas.width = 0;
    this.canvas.height = 0;
  }

  /** 表示している画像の範囲（表示エリアの座標、論理ピクセル。はみ出すこともある）。 */
  rect(): { x: number; y: number; width: number; height: number } | null {
    if (!this.size) return null;
    const ratio = window.devicePixelRatio || 1;
    return { ...this.offset, width: this.size[0] / ratio, height: this.size[1] / ratio };
  }

  /** 表示エリアの大きさ。 */
  private area(): Size {
    return { width: this.stage.clientWidth, height: this.stage.clientHeight };
  }

  /** 画像が表示より小さい向きは中央にそろえ、大きい向きははみ出し過ぎないよう止める。 */
  private moveTo(x: number, y: number) {
    const rect = this.rect();
    if (!rect) return;
    this.offset = clampOffset(x, y, rect, this.area());
    this.holder.style.transform = `translate(${this.offset.x}px, ${this.offset.y}px)`;
    this.onMove();
  }
}
