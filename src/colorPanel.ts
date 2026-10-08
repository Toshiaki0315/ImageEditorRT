// 「加工」タブのトーンカーブと色ごとの調整（旧版にはない）。曲線の計算は curve.ts（Rust と同じ）。

import { addPoint, type CurvePoint, curveRow, identityCurve, movePoint } from "./curve";
import { type EditSettings, type HslAdjust, neutralHsl } from "./types";

/** 色ごとの調整の色（Rust の curve::HSL_CENTERS と同じ順）。 */
const BANDS: [string, string][] = [
  ["赤", "#e5484d"],
  ["オレンジ", "#f76b15"],
  ["黄", "#ffc53d"],
  ["緑", "#30a46c"],
  ["水色", "#05a2c2"],
  ["青", "#3e63dd"],
  ["紫", "#8e4ec6"],
  ["マゼンタ", "#d6409f"],
];

type HslKey = keyof HslAdjust;
const HSL_SLIDERS: { key: HslKey; label: string; max: number }[] = [
  { key: "hue", label: "色相", max: 30 },
  { key: "saturation", label: "彩度", max: 100 },
  { key: "lightness", label: "明るさ", max: 100 },
];

/** グラフの大きさ（CSS の px）と、点の当たり判定の半径。 */
const SIZE = 220;
const HIT = 9;

const signed = (v: number) => (v > 0 ? `+${v}` : String(v));

export class ColorPanel {
  private readonly canvas = document.createElement("canvas");
  private readonly bandButtons: HTMLButtonElement[] = [];
  private readonly sliders = new Map<HslKey, { input: HTMLInputElement; output: HTMLOutputElement }>();
  private band = 0;
  private dragging: number | null = null;
  private readonly settings: EditSettings;
  private readonly onChange: () => void;

  constructor(container: HTMLElement, settings: EditSettings, onChange: () => void) {
    this.settings = settings;
    this.onChange = onChange;
    // トーンカーブ
    const curveHeading = document.createElement("h2");
    curveHeading.textContent = "トーンカーブ";
    this.canvas.className = "tone-curve";
    this.canvas.title = "点をドラッグして明るさの曲線を作ります（クリックで点を足す・ダブルクリックで消す）";
    this.canvas.addEventListener("pointerdown", (e) => this.pointerDown(e));
    this.canvas.addEventListener("pointermove", (e) => this.pointerMove(e));
    this.canvas.addEventListener("pointerup", () => this.pointerUp());
    this.canvas.addEventListener("dblclick", (e) => this.removeAt(e));
    const curveReset = this.button("曲線を元に戻す", () => {
      this.settings.toneCurve = identityCurve();
      this.changed();
    });
    // 色ごとの調整
    const hslHeading = document.createElement("h2");
    hslHeading.textContent = "色ごとの調整";
    const bands = document.createElement("div");
    bands.className = "hsl-bands";
    bands.setAttribute("role", "group");
    bands.setAttribute("aria-label", "調整する色");
    BANDS.forEach(([name, color], index) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "hsl-band";
      button.textContent = name;
      button.style.setProperty("--band", color);
      button.addEventListener("click", () => {
        this.band = index;
        this.show();
      });
      this.bandButtons.push(button);
      bands.append(button);
    });
    const rows = HSL_SLIDERS.map(({ key, label, max }) => {
      const row = document.createElement("label");
      row.className = "row";
      const input = document.createElement("input");
      input.type = "range";
      input.min = String(-max);
      input.max = String(max);
      input.step = "1";
      input.title = "ダブルクリックで 0 に戻す";
      const output = document.createElement("output");
      input.addEventListener("input", () => this.setHsl(key, Number(input.value)));
      input.addEventListener("dblclick", () => this.setHsl(key, 0));
      this.sliders.set(key, { input, output });
      row.append(label, input, output);
      return row;
    });
    const hslReset = this.button("色ごとの調整をリセット", () => {
      this.settings.hsl = neutralHsl();
      this.changed();
    });
    container.append(curveHeading, this.canvas, curveReset, hslHeading, bands, ...rows, hslReset);
    this.show();
  }

  /** 曲線の点をドラッグしている間は true（ドラッグ全体を 1 回の操作として履歴に積む）。 */
  isDragging(): boolean {
    return this.dragging !== null;
  }

  /** 設定の値をグラフとスライダーに反映する。 */
  show() {
    this.bandButtons.forEach((button, index) => button.setAttribute("aria-pressed", String(index === this.band)));
    const values = this.settings.hsl[this.band];
    for (const { key } of HSL_SLIDERS) {
      const { input, output } = this.sliders.get(key)!;
      input.value = String(values[key]);
      output.textContent = signed(values[key]);
    }
    this.draw();
  }

  private button(text: string, onClick: () => void): HTMLDivElement {
    const row = document.createElement("div");
    row.className = "buttons";
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = text;
    button.addEventListener("click", onClick);
    row.append(button);
    return row;
  }

  private setHsl(key: HslKey, value: number) {
    if (this.settings.hsl[this.band][key] === value) return;
    this.settings.hsl = this.settings.hsl.map((b, i) => (i === this.band ? { ...b, [key]: value } : b));
    this.changed();
  }

  private changed() {
    this.show();
    this.onChange();
  }

  // --- トーンカーブのグラフ ------------------------------------------------------

  private draw() {
    const ratio = window.devicePixelRatio || 1;
    this.canvas.style.width = this.canvas.style.height = `${SIZE}px`;
    this.canvas.width = this.canvas.height = Math.round(SIZE * ratio);
    const ctx = this.canvas.getContext("2d")!;
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, SIZE, SIZE);
    const style = getComputedStyle(this.canvas);
    // 4 等分の目盛りと、まっすぐ（変化なし）の線
    ctx.strokeStyle = style.getPropertyValue("--line") || "#ccc";
    ctx.lineWidth = 1;
    for (let i = 1; i < 4; i++) {
      const v = (SIZE * i) / 4 + 0.5;
      ctx.beginPath();
      ctx.moveTo(v, 0);
      ctx.lineTo(v, SIZE);
      ctx.moveTo(0, v);
      ctx.lineTo(SIZE, v);
      ctx.stroke();
    }
    ctx.beginPath();
    ctx.moveTo(0, SIZE);
    ctx.lineTo(SIZE, 0);
    ctx.stroke();
    // 曲線と点
    const row = curveRow(this.settings.toneCurve);
    ctx.strokeStyle = style.getPropertyValue("--accent") || "#0a64d8";
    ctx.lineWidth = 2;
    ctx.beginPath();
    row.forEach((y, x) => {
      const [sx, sy] = this.toScreen([x, y]);
      if (x === 0) ctx.moveTo(sx, sy);
      else ctx.lineTo(sx, sy);
    });
    ctx.stroke();
    ctx.fillStyle = ctx.strokeStyle;
    for (const point of this.settings.toneCurve) {
      const [sx, sy] = this.toScreen(point);
      ctx.beginPath();
      ctx.arc(sx, sy, 4, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  private toScreen([x, y]: CurvePoint): [number, number] {
    return [(x / 255) * SIZE, SIZE - (y / 255) * SIZE];
  }

  private toCurve(event: PointerEvent | MouseEvent): CurvePoint {
    const box = this.canvas.getBoundingClientRect();
    return [((event.clientX - box.left) / box.width) * 255, (1 - (event.clientY - box.top) / box.height) * 255];
  }

  /** 画面の点の近くにある曲線の点の番号。 */
  private hit(event: PointerEvent | MouseEvent): number | null {
    const box = this.canvas.getBoundingClientRect();
    const [px, py] = [event.clientX - box.left, event.clientY - box.top];
    const index = this.settings.toneCurve.findIndex((p) => {
      const [sx, sy] = this.toScreen(p);
      return Math.hypot(px - sx, py - sy) <= HIT;
    });
    return index >= 0 ? index : null;
  }

  private pointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    let index = this.hit(event);
    if (index === null) {
      // 点のないところを押したら、そこに点を足してそのままドラッグする
      const points = this.settings.toneCurve.map((p) => [...p] as CurvePoint);
      index = addPoint(points, this.toCurve(event));
      if (index === null) return;
      this.settings.toneCurve = points;
      this.changed();
    }
    this.dragging = index;
    this.canvas.setPointerCapture(event.pointerId);
  }

  private pointerMove(event: PointerEvent) {
    if (this.dragging === null) {
      this.canvas.style.cursor = this.hit(event) === null ? "crosshair" : "grab";
      return;
    }
    const points = this.settings.toneCurve.map((p) => [...p] as CurvePoint);
    movePoint(points, this.dragging, this.toCurve(event));
    this.settings.toneCurve = points;
    this.changed();
  }

  private pointerUp() {
    if (this.dragging === null) return;
    this.dragging = null;
    this.onChange();
  }

  /** ダブルクリックした点を消す（両端は消さない）。 */
  private removeAt(event: MouseEvent) {
    const index = this.hit(event);
    const last = this.settings.toneCurve.length - 1;
    if (index === null || index === 0 || index === last) return;
    this.settings.toneCurve = this.settings.toneCurve.filter((_, i) => i !== index);
    this.changed();
  }
}
