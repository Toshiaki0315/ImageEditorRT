// 「注釈」タブ（旧版にはない）: 矢印・四角・楕円・手書きの線・吹き出しをプレビューの上のドラッグで描く。
// 「選ぶ」で注釈を選んで動かす・色や太さ・文字を変える・消す。座標は回転・反転した後の原寸の画像の座標で持ち、
// 描くのは Rust（core/annotate.rs）。

import { svgElement } from "./overlay";
import { type Point, screenScale } from "./regions";
import type { Annotation, AnnotationKind, EditSettings } from "./types";

/** 道具（注釈の種類か、選んで動かす）。 */
type Tool = AnnotationKind | "select";

/** 太さの段階 1〜20 を、原寸の短辺に対する px にする割合。 */
const WIDTH_PER_LEVEL = 1 / 500;
const WIDTH_DEFAULT = 4;
/** 手書きの線は、前の点からこれ以上離れたら点を足す（画面の px） */
const PEN_STEP = 3;
/** これより小さい注釈（クリックしただけなど）は足さない（画面の px） */
const MIN_SIZE = 4;
const BUBBLE_TEXT_DEFAULT = "テキスト";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const toHex = ([r, g, b]: [number, number, number]) => `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
const fromHex = (hex: string): [number, number, number] =>
  [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as [number, number, number];

/** 注釈を囲む四角（原寸の座標。太さの分を足す）。 */
export function annotationBounds(note: Annotation): [number, number, number, number] {
  const xs = note.points.map((p) => p[0]);
  const ys = note.points.map((p) => p[1]);
  const pad = note.width * 2;
  return [Math.min(...xs) - pad, Math.min(...ys) - pad, Math.max(...xs) + pad, Math.max(...ys) + pad];
}

/** point（原寸の座標）に当たる注釈の番号（後に描いたものほど上）。なければ null。 */
export function hitAnnotation(notes: Annotation[], [x, y]: Point): number | null {
  for (let i = notes.length - 1; i >= 0; i--) {
    if (notes[i].points.length === 0) continue;
    const [left, top, right, bottom] = annotationBounds(notes[i]);
    if (x >= left && x <= right && y >= top && y <= bottom) return i;
  }
  return null;
}

type Drag = { mode: "draw" } | { mode: "move"; anchor: Point; start: [number, number][] };

export class AnnotatePanel {
  private readonly overlay = document.getElementById("annotate-overlay") as unknown as SVGSVGElement;
  private readonly toolButtons = [...document.querySelectorAll<HTMLButtonElement>("[data-annotate-tool]")];
  private readonly color = $<HTMLInputElement>("annotate-color");
  private readonly width = $<HTMLInputElement>("annotate-width");
  private readonly widthValue = $<HTMLOutputElement>("annotate-width-value");
  private readonly text = $<HTMLInputElement>("annotate-text");
  private readonly deleteButton = $<HTMLButtonElement>("annotate-delete");
  private readonly clearButton = $<HTMLButtonElement>("annotate-clear");
  private tool: Tool = "arrow";
  /** 次に描く注釈の色・太さの段階 */
  private nextColor: [number, number, number] = [230, 40, 40];
  private nextLevel = WIDTH_DEFAULT;
  private selected: number | null = null;
  private drag: Drag | null = null;
  private active = false;
  private readonly settings: EditSettings;
  private readonly canvas: HTMLCanvasElement;
  private readonly size: () => Point | null;
  private readonly onChange: () => void;

  /**
   * @param size 回転・反転した後の原寸の大きさ（画像がなければ null）
   * @param onChange 注釈を変えたとき（履歴に積み、プレビューを描き直す）
   */
  constructor(settings: EditSettings, canvas: HTMLCanvasElement, size: () => Point | null, onChange: () => void) {
    this.settings = settings;
    this.canvas = canvas;
    this.size = size;
    this.onChange = onChange;
    for (const button of this.toolButtons) {
      button.addEventListener("click", () => {
        this.tool = button.dataset.annotateTool as Tool;
        if (this.tool !== "select") this.selected = null;
        this.show();
      });
    }
    this.color.addEventListener("input", () => {
      this.nextColor = fromHex(this.color.value);
      this.updateSelected((note) => (note.color = this.nextColor));
    });
    this.width.addEventListener("input", () => {
      this.nextLevel = Number(this.width.value);
      this.updateSelected((note) => (note.width = this.widthPx()));
    });
    this.text.addEventListener("input", () => {
      this.updateSelected((note) => {
        if (note.kind === "bubble") note.text = this.text.value;
      });
    });
    this.deleteButton.addEventListener("click", () => this.remove());
    this.clearButton.addEventListener("click", () => {
      this.settings.annotations = [];
      this.selected = null;
      this.changed();
    });
    this.overlay.addEventListener("pointerdown", (e) => this.pointerDown(e));
    this.overlay.addEventListener("pointermove", (e) => this.pointerMove(e));
    this.overlay.addEventListener("pointerup", (e) => this.pointerUp(e));
    this.show();
  }

  /** ドラッグしている間は true（ドラッグ全体を 1 回の操作として履歴に積む）。 */
  isDragging(): boolean {
    return this.drag !== null;
  }

  /** 描けるか（「注釈」タブを開いていて、切り抜いた表示でない）。 */
  setActive(active: boolean) {
    this.active = active;
    this.draw();
  }

  /** 画像を開いた・閉じたとき。 */
  reset() {
    this.selected = null;
    this.drag = null;
    this.show();
  }

  /** Esc キー: 選んでいる注釈を消す（元に戻せる）。選んでいなければ false。 */
  cancelSelected(): boolean {
    this.drag = null;
    if (this.selected === null) return false;
    this.remove();
    return true;
  }

  /** 設定の値をタブに反映する。 */
  show() {
    if (this.selected !== null && this.selected >= this.settings.annotations.length) this.selected = null;
    const loaded = this.size() !== null;
    const note = this.current();
    for (const button of this.toolButtons) {
      button.setAttribute("aria-pressed", String(button.dataset.annotateTool === this.tool));
      button.disabled = !loaded;
    }
    this.color.value = toHex(note?.color ?? this.nextColor);
    const level = note ? Math.max(1, Math.round(note.width / this.unit())) : this.nextLevel;
    this.width.value = String(level);
    this.widthValue.textContent = String(level);
    this.color.disabled = this.width.disabled = !loaded;
    this.text.disabled = !loaded || (note !== null && note.kind !== "bubble");
    if (note?.kind === "bubble" && document.activeElement !== this.text) this.text.value = note.text;
    this.deleteButton.disabled = note === null;
    this.clearButton.disabled = this.settings.annotations.length === 0;
    this.draw();
  }

  /** 選んでいる注釈の枠を描く。 */
  draw() {
    const size = this.size();
    const shown = this.active && size !== null && !this.canvas.hidden;
    this.overlay.toggleAttribute("hidden", !shown);
    this.overlay.classList.toggle("drawing", this.tool !== "select");
    if (!shown || !size) return;
    const [width, height] = [this.canvas.clientWidth, this.canvas.clientHeight];
    this.overlay.setAttribute("viewBox", `0 0 ${width} ${height}`);
    this.overlay.replaceChildren();
    const note = this.current();
    if (!note || note.points.length === 0) return;
    const [sx, sy] = screenScale([width, height], size);
    const [left, top, right, bottom] = annotationBounds(note);
    const rect = { x: left * sx, y: top * sy, width: (right - left) * sx, height: (bottom - top) * sy };
    for (const kind of ["edge-shadow", "edge selected"]) this.overlay.append(svgElement("rect", kind, rect));
  }

  private current(): Annotation | null {
    return this.selected === null ? null : (this.settings.annotations[this.selected] ?? null);
  }

  /** 原寸の短辺の 1/500（太さの段階 1 つ分の px）。 */
  private unit(): number {
    const size = this.size();
    return size ? Math.max(1, Math.min(size[0], size[1]) * WIDTH_PER_LEVEL) : 1;
  }

  private widthPx(): number {
    return Math.round(this.nextLevel * this.unit() * 10) / 10;
  }

  private updateSelected(change: (note: Annotation) => void) {
    const note = this.current();
    if (!note) {
      this.show();
      return;
    }
    change(note);
    this.changed();
  }

  private remove() {
    if (this.selected === null) return;
    this.settings.annotations.splice(this.selected, 1);
    this.selected = null;
    this.changed();
  }

  private changed() {
    this.show();
    this.onChange();
  }

  private toImage(event: PointerEvent): Point {
    const box = this.overlay.getBoundingClientRect();
    const [width, height] = this.size()!;
    return [((event.clientX - box.left) * width) / box.width, ((event.clientY - box.top) * height) / box.height];
  }

  /** 画面の 1px が原寸の何 px か。 */
  private screenToImage(): number {
    const size = this.size()!;
    return size[0] / Math.max(1, this.canvas.clientWidth);
  }

  private pointerDown(event: PointerEvent) {
    if (event.button !== 0 || !this.size()) return;
    const point = this.toImage(event);
    if (this.tool === "select") {
      const hit = hitAnnotation(this.settings.annotations, point);
      this.selected = hit;
      if (hit !== null) {
        const start = this.settings.annotations[hit].points.map((p) => [p[0], p[1]] as [number, number]);
        this.drag = { mode: "move", anchor: point, start };
      }
      this.show();
    } else {
      const kind = this.tool;
      const note: Annotation = {
        kind,
        points: kind === "pen" ? [point] : [point, point],
        color: this.nextColor,
        width: this.widthPx(),
        text: kind === "bubble" ? this.text.value.trim() || BUBBLE_TEXT_DEFAULT : "",
      };
      this.settings.annotations.push(note);
      this.selected = this.settings.annotations.length - 1;
      this.drag = { mode: "draw" };
      this.changed();
    }
    this.overlay.setPointerCapture(event.pointerId);
  }

  private pointerMove(event: PointerEvent) {
    const note = this.current();
    if (!this.drag || !note) return;
    const point = this.toImage(event);
    if (this.drag.mode === "move") {
      const [dx, dy] = [point[0] - this.drag.anchor[0], point[1] - this.drag.anchor[1]];
      note.points = this.drag.start.map(([x, y]) => [x + dx, y + dy]);
    } else if (note.kind === "pen") {
      const last = note.points[note.points.length - 1];
      if (Math.hypot(point[0] - last[0], point[1] - last[1]) < PEN_STEP * this.screenToImage()) return;
      note.points.push(point);
    } else {
      note.points = [note.points[0], point];
    }
    this.changed();
  }

  private pointerUp(event: PointerEvent) {
    if (!this.drag || event.button !== 0) return;
    const drawing = this.drag.mode === "draw";
    this.drag = null;
    const note = this.current();
    if (drawing && note) {
      // クリックしただけ（小さすぎる注釈）は足さない
      const [left, top, right, bottom] = annotationBounds({ ...note, width: 0 });
      const min = MIN_SIZE * this.screenToImage();
      const tooSmall = note.kind === "pen" ? note.points.length < 2 : right - left < min && bottom - top < min;
      if (tooSmall) {
        this.settings.annotations.splice(this.selected!, 1);
        this.selected = null;
      }
    }
    this.changed();
  }
}
