// 文字・ロゴの「自由（ドラッグで動かす）」: 文字・透かしのダイアログを開いている間、プレビューの上に文字・ロゴの
// 中心の印を出し、ドラッグで動かす（旧版にはない）。位置は写真の範囲（実際に切り抜く範囲）に対する割合で持つ。

import { svgElement } from "./overlay";
import { scaleRect, screenScale } from "./regions";
import { fromFraction, nearest, toFraction } from "./textPoint";
import { type CropRect, type EditSettings, FREE_POINT_DEFAULT } from "./types";

type Point = [number, number];
type Target = "text" | "logo";

/** 印の円の半径（画面の px） */
const MARK_RADIUS = 9;

export class TextDrag {
  private readonly overlay = document.getElementById("text-overlay") as unknown as SVGSVGElement;
  private readonly dialog = document.getElementById("text-dialog") as HTMLDialogElement;
  private dragging: Target | null = null;
  /** プレビューの上で範囲を描けるか（切り抜いた表示・100% 表示でない） */
  private active = false;
  private readonly settings: EditSettings;
  private readonly canvas: HTMLCanvasElement;
  private readonly size: () => Point | null;
  private readonly area: () => CropRect | null;
  private readonly onChange: () => void;

  /**
   * @param size 回転・反転した後の原寸の大きさ（画像がなければ null）
   * @param area 文字・ロゴを描く写真の範囲（原寸の座標）
   * @param onChange 位置を変えたとき（履歴に積み、プレビューを描き直す）
   */
  constructor(
    settings: EditSettings,
    canvas: HTMLCanvasElement,
    size: () => Point | null,
    area: () => CropRect | null,
    onChange: () => void,
  ) {
    this.settings = settings;
    this.canvas = canvas;
    this.size = size;
    this.area = area;
    this.onChange = onChange;
    this.overlay.addEventListener("pointerdown", (e) => this.pointerDown(e));
    this.overlay.addEventListener("pointermove", (e) => this.pointerMove(e));
    this.overlay.addEventListener("pointerup", () => (this.dragging = null));
    this.overlay.addEventListener("pointercancel", () => (this.dragging = null));
    // ダイアログを開いた・閉じたときに出し直す
    new MutationObserver(() => this.draw()).observe(this.dialog, { attributes: true, attributeFilter: ["open"] });
  }

  /** ドラッグしている間は true（ドラッグ全体を 1 回の操作として履歴に積む）。 */
  isDragging(): boolean {
    return this.dragging !== null;
  }

  /** 範囲を描けるか（切り抜いた表示・100% 表示でない）を変える。 */
  setActive(active: boolean) {
    this.active = active;
    this.draw();
  }

  /** 動かせるもの（位置が「自由」で、文字・ロゴがあるもの）。 */
  private targets(): Target[] {
    const { text, logo } = this.settings;
    const list: Target[] = [];
    if (text.position === "free" && text.text.trim()) list.push("text");
    if (logo.position === "free" && logo.path.trim()) list.push("logo");
    return list;
  }

  private point(target: Target): Point {
    return this.settings[target].point ?? FREE_POINT_DEFAULT;
  }

  /** 写真の範囲（画面の座標）。 */
  private screenArea(): CropRect | null {
    const size = this.size();
    const area = this.area();
    if (!size || !area) return null;
    return scaleRect(area, screenScale([this.canvas.clientWidth, this.canvas.clientHeight], size));
  }

  /** 印を描き直す（ダイアログを開いていて、動かせるものがあるときだけ出し、操作を受ける）。 */
  draw() {
    const area = this.screenArea();
    const targets = this.targets();
    const shown = this.active && this.dialog.open && !this.canvas.hidden && area !== null && targets.length > 0;
    this.overlay.toggleAttribute("hidden", !shown);
    if (!shown || !area) return;
    this.overlay.setAttribute("viewBox", `0 0 ${this.canvas.clientWidth} ${this.canvas.clientHeight}`);
    this.overlay.replaceChildren();
    for (const target of targets) {
      const [cx, cy] = fromFraction(this.point(target), area);
      for (const kind of ["edge-shadow", "edge selected"]) {
        this.overlay.append(svgElement("circle", kind, { cx, cy, r: MARK_RADIUS }));
        this.overlay.append(svgElement("line", kind, { x1: cx - MARK_RADIUS, y1: cy, x2: cx + MARK_RADIUS, y2: cy }));
        this.overlay.append(svgElement("line", kind, { x1: cx, y1: cy - MARK_RADIUS, x2: cx, y2: cy + MARK_RADIUS }));
      }
    }
  }

  private screenPoint(event: PointerEvent): Point {
    const box = this.overlay.getBoundingClientRect();
    return [event.clientX - box.left, event.clientY - box.top];
  }

  private pointerDown(event: PointerEvent) {
    const area = this.screenArea();
    const targets = this.targets();
    if (event.button !== 0 || !area || targets.length === 0) return;
    // 押した点にいちばん近いもの（文字かロゴ）を、押した点へ動かす
    const point = this.screenPoint(event);
    const index = nearest(
      targets.map((t) => fromFraction(this.point(t), area)),
      point,
    );
    if (index === null) return;
    this.dragging = targets[index];
    this.overlay.setPointerCapture(event.pointerId);
    this.moveTo(point, area);
  }

  private pointerMove(event: PointerEvent) {
    const area = this.screenArea();
    if (this.dragging && area) this.moveTo(this.screenPoint(event), area);
  }

  private moveTo(point: Point, area: CropRect) {
    const target = this.dragging;
    if (!target) return;
    const [fx, fy] = toFraction(point, area);
    // 割合は 0.001 刻み（小さな誤差で履歴を積まない）
    const next: Point = [Math.round(fx * 1000) / 1000, Math.round(fy * 1000) / 1000];
    const now = this.point(target);
    if (now[0] === next[0] && now[1] === next[1]) return;
    if (target === "text") this.settings.text = { ...this.settings.text, point: next };
    else this.settings.logo = { ...this.settings.logo, point: next };
    this.draw();
    this.onChange();
  }
}
