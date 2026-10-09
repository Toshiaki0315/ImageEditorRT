// 「加工」タブの部分補正（旧版にはない）: 円（楕円）・帯の範囲をプレビューの上に置き、範囲ごとに露出・コントラスト・
// 色温度・彩度を変える。範囲は回転・反転した後の原寸の画像の座標で持つ（トリミング範囲・投稿加工の範囲と同じ）。
// 範囲の当たり判定・線の位置は localShapes.ts。

import { bandLines, distanceToSegment, insideEllipse, moveShape, type Point, rectBetween, shapeLabel } from "./localShapes";
import type { EditSettings, LocalAdjust, LocalShape } from "./types";

const SVG = "http://www.w3.org/2000/svg";
const HANDLE_SIZE = 8;
/** ハンドル・帯の線をつかめる距離（画面の px） */
const HIT = 10;
/** これより小さい範囲（クリックしただけなど）は足さない（画面の px） */
const MIN_DRAG = 6;

type Tool = "ellipse" | "band";
type NumberKey = "exposure" | "contrast" | "temperature" | "saturation" | "feather";
type Drag =
  | { mode: "new"; anchor: Point }
  | { mode: "move"; anchor: Point; start: LocalShape }
  | { mode: "corner"; anchor: Point }
  | { mode: "end"; end: "from" | "to" };

/** スライダー（値の範囲・刻み・既定値・表示）。 */
const SLIDERS: { key: NumberKey; label: string; min: number; max: number; step: number; initial: number; text: (v: number) => string }[] = [
  { key: "exposure", label: "露出", min: -3, max: 3, step: 0.1, initial: 0, text: (v) => `${v > 0 ? "+" : ""}${v.toFixed(1)} EV` },
  { key: "contrast", label: "コントラスト", min: -100, max: 100, step: 1, initial: 0, text: signed },
  { key: "temperature", label: "色温度", min: 2000, max: 10000, step: 100, initial: 6500, text: (v) => `${v} K` },
  { key: "saturation", label: "彩度", min: -100, max: 100, step: 1, initial: 0, text: signed },
  { key: "feather", label: "境目のぼかし", min: 0, max: 100, step: 1, initial: 50, text: (v) => `${v}%` },
];

function signed(value: number): string {
  return value > 0 ? `+${value}` : String(value);
}

/** 新しく置く範囲の調整（すぐ効き目が見えるよう、円は少し明るく、帯は少し暗く）。 */
function newAdjust(shape: LocalShape): LocalAdjust {
  return {
    shape,
    exposure: shape.kind === "ellipse" ? 0.5 : -0.5,
    contrast: 0,
    temperature: 6500,
    saturation: 0,
    feather: 50,
  };
}

export class LocalPanel {
  private readonly overlay = document.getElementById("local-overlay") as unknown as SVGSVGElement;
  private readonly toolButtons: Record<Tool, HTMLButtonElement>;
  private readonly list = document.createElement("select");
  private readonly deleteButton = document.createElement("button");
  private readonly clearButton = document.createElement("button");
  private readonly rows = new Map<NumberKey, { input: HTMLInputElement; output: HTMLOutputElement }>();
  private tool: Tool | null = null;
  private selected: number | null = null;
  private drag: Drag | null = null;
  private active = false;
  private readonly settings: EditSettings;
  private readonly canvas: HTMLCanvasElement;
  private readonly size: () => Point | null;
  private readonly onChange: () => void;

  /**
   * @param container 欄を作る場所（「加工」タブの中）
   * @param size 回転・反転した後の原寸の大きさ（画像がなければ null）
   * @param onChange 範囲・調整を変えたとき（履歴に積み、プレビューを描き直す）
   */
  constructor(
    container: HTMLElement,
    settings: EditSettings,
    canvas: HTMLCanvasElement,
    size: () => Point | null,
    onChange: () => void,
  ) {
    this.settings = settings;
    this.canvas = canvas;
    this.size = size;
    this.onChange = onChange;
    const heading = document.createElement("h2");
    heading.textContent = "部分補正";
    const tools = document.createElement("div");
    tools.className = "buttons";
    const button = (text: string, title: string) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = text;
      b.title = title;
      return b;
    };
    this.toolButtons = {
      ellipse: button("円を足す", "プレビューの上をドラッグして、円（楕円）の範囲を置きます"),
      band: button("帯を足す", "プレビューの上を、効かせたい側から弱めたい向きへドラッグして、帯の範囲を置きます（空を暗くするなど）"),
    };
    for (const [tool, b] of Object.entries(this.toolButtons)) {
      b.setAttribute("aria-pressed", "false");
      b.addEventListener("click", () => this.setTool(this.tool === tool ? null : (tool as Tool)));
      tools.append(b);
    }
    const listRow = document.createElement("label");
    listRow.className = "row";
    this.list.addEventListener("change", () => {
      this.selected = this.list.value === "" ? null : Number(this.list.value);
      this.tool = null;
      this.show();
    });
    listRow.append("範囲", this.list);
    container.append(heading, tools, listRow);
    for (const slider of SLIDERS) {
      const row = document.createElement("label");
      row.className = "row";
      const input = document.createElement("input");
      input.type = "range";
      input.min = String(slider.min);
      input.max = String(slider.max);
      input.step = String(slider.step);
      input.title = "ダブルクリックで既定値に戻す";
      const output = document.createElement("output");
      input.addEventListener("input", () => this.setValue(slider.key, Number(input.value)));
      input.addEventListener("dblclick", () => this.setValue(slider.key, slider.initial));
      row.append(slider.label, input, output);
      this.rows.set(slider.key, { input, output });
      container.append(row);
    }
    const actions = document.createElement("div");
    actions.className = "buttons";
    this.deleteButton.type = this.clearButton.type = "button";
    this.deleteButton.textContent = "範囲を削除";
    this.clearButton.textContent = "すべて削除";
    this.deleteButton.addEventListener("click", () => this.remove());
    this.clearButton.addEventListener("click", () => {
      this.settings.localAdjustments = [];
      this.selected = null;
      this.changed();
    });
    actions.append(this.deleteButton, this.clearButton);
    const note = document.createElement("p");
    note.className = "note";
    note.textContent =
      "「円を足す」「帯を足す」を押してプレビューの上をドラッグすると、その範囲だけを補正します。範囲を選ぶと、ドラッグで動かす・ハンドルで大きさや向きを変えられます。帯は始まりの線まで全部効き、終わりの線（点線）に向かって弱まります。";
    container.append(actions, note);
    this.overlay.addEventListener("pointerdown", (e) => this.pointerDown(e));
    this.overlay.addEventListener("pointermove", (e) => this.pointerMove(e));
    this.overlay.addEventListener("pointerup", (e) => this.pointerUp(e));
    this.show();
  }

  /** 範囲をドラッグしている間は true（ドラッグ全体を 1 回の操作として履歴に積む）。 */
  isDragging(): boolean {
    return this.drag !== null;
  }

  /** 範囲を描けるか（「加工」タブを開いていて、切り抜いた表示でない）を変える。 */
  setActive(active: boolean) {
    this.active = active;
    if (!active) this.tool = null;
    this.show();
  }

  /** 画像を開いた・閉じたとき（選んでいる範囲・道具をなくす）。 */
  reset() {
    this.selected = null;
    this.tool = null;
    this.drag = null;
    this.show();
  }

  /** 設定の値を欄に反映する（元に戻したときなど。選んでいる範囲がなくなっていれば選ばない）。 */
  show() {
    const adjusts = this.settings.localAdjustments;
    if (this.selected !== null && this.selected >= adjusts.length) this.selected = null;
    const loaded = this.size() !== null;
    for (const [tool, b] of Object.entries(this.toolButtons)) {
      b.setAttribute("aria-pressed", String(this.tool === tool));
      b.disabled = !loaded;
    }
    this.list.replaceChildren(
      new Option(adjusts.length === 0 ? "（まだありません）" : "（選ばない）", ""),
      ...adjusts.map((a, index) => new Option(shapeLabel(a.shape, index), String(index))),
    );
    this.list.value = this.selected === null ? "" : String(this.selected);
    this.list.disabled = adjusts.length === 0;
    const current = this.current();
    for (const slider of SLIDERS) {
      const row = this.rows.get(slider.key)!;
      const value = current ? current[slider.key] : slider.initial;
      row.input.value = String(value);
      row.output.textContent = slider.text(value);
      row.input.disabled = !current || (slider.key === "feather" && current.shape.kind !== "ellipse");
    }
    this.deleteButton.disabled = current === null;
    this.clearButton.disabled = adjusts.length === 0;
    this.draw();
  }

  /** 範囲の線を描き直す（プレビューの大きさが変わったときなど）。道具も選んだ範囲もなければ描かず、操作も受けない。 */
  draw() {
    const size = this.size();
    const shown = this.active && size !== null && !this.canvas.hidden && (this.tool !== null || this.selected !== null);
    this.overlay.toggleAttribute("hidden", !shown);
    if (!shown || !size) return;
    const [width, height] = [this.canvas.clientWidth, this.canvas.clientHeight];
    this.overlay.setAttribute("viewBox", `0 0 ${width} ${height}`);
    this.overlay.style.cursor = this.tool ? "crosshair" : "default";
    this.overlay.replaceChildren();
    this.settings.localAdjustments.forEach((adjust, index) => {
      const selected = index === this.selected;
      const shape = this.toScreenShape(adjust.shape);
      if (shape.kind === "ellipse") {
        const { x, y, width: w, height: h } = shape.rect;
        for (const kind of ["edge-shadow", selected ? "edge selected" : "edge"]) {
          const ellipse = this.element("ellipse", kind, { cx: x + w / 2, cy: y + h / 2, rx: w / 2, ry: h / 2 });
          this.overlay.append(ellipse);
        }
        if (selected) for (const [cx, cy] of corners(shape.rect)) this.handle(cx, cy);
      } else {
        const [start, end] = bandLines(shape.from, shape.to, width, height);
        if (!start || !end) return;
        for (const [line, dashed] of [
          [start, false],
          [end, true],
        ] as const) {
          for (const kind of ["edge-shadow", selected ? "edge selected" : "edge"]) {
            const [[x1, y1], [x2, y2]] = line;
            this.overlay.append(this.element("line", dashed ? `${kind} dashed` : kind, { x1, y1, x2, y2 }));
          }
        }
        if (selected) {
          const [[x1, y1], [x2, y2]] = [shape.from, shape.to];
          this.overlay.append(this.element("line", "edge", { x1, y1, x2, y2 }));
          this.handle(x1, y1);
          this.handle(x2, y2);
        }
      }
    });
  }

  private element(name: string, className: string, attributes: Record<string, number>): SVGElement {
    const element = document.createElementNS(SVG, name);
    element.setAttribute("class", className);
    for (const [key, value] of Object.entries(attributes)) element.setAttribute(key, String(value));
    return element;
  }

  private handle(cx: number, cy: number) {
    const s = HANDLE_SIZE;
    this.overlay.append(this.element("rect", "handle", { x: cx - s / 2, y: cy - s / 2, width: s, height: s }));
  }

  private current(): LocalAdjust | null {
    return this.selected === null ? null : (this.settings.localAdjustments[this.selected] ?? null);
  }

  private setTool(tool: Tool | null) {
    this.tool = tool;
    if (tool) this.selected = null;
    this.show();
  }

  private setValue(key: NumberKey, value: number) {
    const current = this.current();
    if (!current) return;
    const fixed = key === "exposure" ? Math.round(value * 10) / 10 : value;
    if (current[key] === fixed) return;
    current[key] = fixed;
    this.changed();
  }

  private remove() {
    if (this.selected === null) return;
    this.settings.localAdjustments.splice(this.selected, 1);
    this.selected = null;
    this.changed();
  }

  private changed() {
    this.show();
    this.onChange();
  }

  // --- 座標の換算 -------------------------------------------------------------------

  /** 原寸 1px が画面の何 px か（横・縦）。 */
  private scale(): Point {
    const [width, height] = this.size()!;
    return [this.canvas.clientWidth / width, this.canvas.clientHeight / height];
  }

  private toScreenShape(shape: LocalShape): LocalShape {
    const [sx, sy] = this.scale();
    if (shape.kind === "ellipse") {
      const { x, y, width, height } = shape.rect;
      return { kind: "ellipse", rect: { x: x * sx, y: y * sy, width: width * sx, height: height * sy } };
    }
    return { kind: "band", from: [shape.from[0] * sx, shape.from[1] * sy], to: [shape.to[0] * sx, shape.to[1] * sy] };
  }

  /** 押した点（画面の px）。 */
  private screenPoint(event: PointerEvent): Point {
    const box = this.overlay.getBoundingClientRect();
    return [event.clientX - box.left, event.clientY - box.top];
  }

  /** 画面の px を原寸の px にする。 */
  private toImage([x, y]: Point): Point {
    const [sx, sy] = this.scale();
    return [x / sx, y / sy];
  }

  // --- プレビュー上のドラッグ ----------------------------------------------------

  /** 選んでいる範囲のハンドルの上なら、そのドラッグ。 */
  private hitHandle(point: Point): Drag | null {
    const current = this.current();
    if (!current) return null;
    const near = ([x, y]: Point) => Math.abs(point[0] - x) <= HIT && Math.abs(point[1] - y) <= HIT;
    const shape = this.toScreenShape(current.shape);
    if (shape.kind === "ellipse") {
      const original = current.shape.kind === "ellipse" ? current.shape.rect : null;
      const index = corners(shape.rect).findIndex(near);
      if (index < 0 || !original) return null;
      return { mode: "corner", anchor: corners(original)[(index + 2) % 4] };
    }
    if (near(shape.from)) return { mode: "end", end: "from" };
    if (near(shape.to)) return { mode: "end", end: "to" };
    return null;
  }

  /** 押した点の範囲（上に重なっているほう＝後に足したほうを選ぶ）。 */
  private hitShape(point: Point): number | null {
    const adjusts = this.settings.localAdjustments;
    for (let index = adjusts.length - 1; index >= 0; index -= 1) {
      const shape = this.toScreenShape(adjusts[index].shape);
      const hit =
        shape.kind === "ellipse" ? insideEllipse(shape.rect, point) : distanceToSegment(shape.from, shape.to, point) <= HIT;
      if (hit) return index;
    }
    return null;
  }

  private pointerDown(event: PointerEvent) {
    if (event.button !== 0 || !this.size()) return;
    const point = this.screenPoint(event);
    const handle = this.hitHandle(point);
    if (handle) {
      this.drag = handle;
    } else if (this.tool) {
      this.drag = { mode: "new", anchor: point };
    } else {
      const hit = this.hitShape(point);
      this.selected = hit;
      this.drag = hit === null ? null : { mode: "move", anchor: point, start: this.settings.localAdjustments[hit].shape };
      this.show();
      if (!this.drag) return;
    }
    this.overlay.setPointerCapture(event.pointerId);
  }

  private pointerMove(event: PointerEvent) {
    if (this.drag) this.dragTo(this.screenPoint(event));
  }

  private pointerUp(event: PointerEvent) {
    if (!this.drag || event.button !== 0) return;
    const drag = this.drag;
    const point = this.screenPoint(event);
    this.dragTo(point);
    this.drag = null;
    if (drag.mode === "new") {
      // クリックしただけ（小さすぎる範囲）は足さない
      const moved = Math.hypot(point[0] - drag.anchor[0], point[1] - drag.anchor[1]);
      if (moved < MIN_DRAG && this.selected !== null) {
        this.settings.localAdjustments.splice(this.selected, 1);
        this.selected = null;
        this.changed();
        return;
      }
      this.tool = null;
    }
    this.show();
  }

  private dragTo(point: Point) {
    const drag = this.drag;
    if (!drag) return;
    const image = this.toImage(point);
    const adjusts = this.settings.localAdjustments;
    if (drag.mode === "new") {
      const anchor = this.toImage(drag.anchor);
      const shape: LocalShape =
        this.tool === "band"
          ? { kind: "band", from: [Math.round(anchor[0]), Math.round(anchor[1])], to: [Math.round(image[0]), Math.round(image[1])] }
          : { kind: "ellipse", rect: rectBetween(anchor, image) };
      if (this.selected === null) {
        adjusts.push(newAdjust(shape));
        this.selected = adjusts.length - 1;
      } else {
        adjusts[this.selected].shape = shape;
      }
    } else if (drag.mode === "move") {
      const [ax, ay] = this.toImage(drag.anchor);
      adjusts[this.selected!].shape = moveShape(drag.start, image[0] - ax, image[1] - ay);
    } else if (drag.mode === "corner") {
      adjusts[this.selected!].shape = { kind: "ellipse", rect: rectBetween(drag.anchor, image) };
    } else {
      const shape = adjusts[this.selected!].shape;
      if (shape.kind === "band") shape[drag.end] = [Math.round(image[0]), Math.round(image[1])];
    }
    this.changed();
  }
}

/** 範囲の四隅（左上から時計回り）。 */
function corners({ x, y, width, height }: { x: number; y: number; width: number; height: number }): Point[] {
  return [
    [x, y],
    [x + width, y],
    [x + width, y + height],
    [x, y + height],
  ];
}
