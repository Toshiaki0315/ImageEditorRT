// 「投稿加工」タブ（旧版にはない）: 範囲のぼかし・モザイク。プレビューの上のドラッグで範囲を足し、選んで動かす・
// 大きさを変える・強さと隠し方を変える。範囲は回転・反転した後の原寸の画像の座標で持つ（トリミング範囲と同じ）。
// 位置情報の削除のチェックは saveOptions.ts が受け持つ。

import { hitRegion, moveRect, type Point, rectCorners, rectFromPoints } from "./regions";
import type { CropRect, EditSettings, Region, RegionKind } from "./types";

const SVG = "http://www.w3.org/2000/svg";
const HANDLE_SIZE = 8;
const HANDLE_HIT = 10;
const STRENGTH_DEFAULT = 50;
/** これより小さい範囲（クリックしただけなど）は足さない（原寸の px） */
const MIN_SIDE = 4;

type Drag =
  | { mode: "new"; anchor: Point }
  | { mode: "move"; anchor: Point; start: CropRect }
  | { mode: "resize"; anchor: Point };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class PrivacyPanel {
  private readonly overlay = document.getElementById("privacy-overlay") as unknown as SVGSVGElement;
  private readonly strength = $<HTMLInputElement>("region-strength");
  private readonly strengthValue = $<HTMLOutputElement>("region-strength-value");
  private readonly deleteButton = $<HTMLButtonElement>("region-delete");
  private readonly clearButton = $<HTMLButtonElement>("region-clear");
  private readonly toolButtons = [...document.querySelectorAll<HTMLButtonElement>("[data-region-tool]")];
  private tool: RegionKind = "blur";
  /** 新しく足す範囲の強さ（範囲を選んでいないときにスライダーで変える） */
  private nextStrength = STRENGTH_DEFAULT;
  private selected: number | null = null;
  private drag: Drag | null = null;
  private active = false;
  private readonly settings: EditSettings;
  private readonly canvas: HTMLCanvasElement;
  private readonly size: () => Point | null;
  private readonly onChange: () => void;

  /**
   * @param size 回転・反転した後の原寸の大きさ（画像がなければ null）
   * @param onChange 範囲を変えたとき（履歴に積み、プレビューを描き直す）
   */
  constructor(settings: EditSettings, canvas: HTMLCanvasElement, size: () => Point | null, onChange: () => void) {
    this.settings = settings;
    this.canvas = canvas;
    this.size = size;
    this.onChange = onChange;
    for (const button of this.toolButtons) {
      button.addEventListener("click", () => this.setTool(button.dataset.regionTool as RegionKind));
    }
    this.strength.addEventListener("input", () => this.setStrength(Number(this.strength.value)));
    this.strength.addEventListener("dblclick", () => this.setStrength(STRENGTH_DEFAULT));
    this.deleteButton.addEventListener("click", () => this.remove());
    this.clearButton.addEventListener("click", () => {
      this.settings.regions = [];
      this.selected = null;
      this.changed();
    });
    this.overlay.addEventListener("pointerdown", (e) => this.pointerDown(e));
    this.overlay.addEventListener("pointermove", (e) => this.pointerMove(e));
    this.overlay.addEventListener("pointerup", (e) => this.pointerUp(e));
    this.show();
  }

  /** 範囲をドラッグしている間は true（ドラッグ全体を 1 回の操作として履歴に積む）。 */
  isDragging(): boolean {
    return this.drag !== null;
  }

  /**
   * 範囲を描けるか（「投稿加工」タブを開いていて、切り抜いた表示でない）を変える。
   * 切り抜いた表示では座標が違うので、範囲は描かない。
   */
  setActive(active: boolean) {
    this.active = active;
    this.draw();
  }

  /** 設定の値をタブに反映する（画像を開いた・元に戻したなど。選んでいる範囲がなくなっていれば選ばない）。 */
  show() {
    if (this.selected !== null && this.selected >= this.settings.regions.length) this.selected = null;
    const region = this.current();
    const loaded = this.size() !== null;
    for (const button of this.toolButtons) {
      button.setAttribute("aria-pressed", String(button.dataset.regionTool === (region?.kind ?? this.tool)));
      button.disabled = !loaded;
    }
    const strength = region?.strength ?? this.nextStrength;
    this.strength.value = String(strength);
    this.strengthValue.textContent = String(strength);
    this.strength.disabled = !loaded;
    this.deleteButton.disabled = region === null;
    this.clearButton.disabled = this.settings.regions.length === 0;
    this.draw();
  }

  /** 画像を開いた・閉じたとき（選んでいる範囲をなくす）。 */
  reset() {
    this.selected = null;
    this.drag = null;
    this.show();
  }

  /** 範囲の枠を描き直す（プレビューの大きさが変わったときなど）。 */
  draw() {
    const size = this.size();
    const shown = this.active && size !== null && !this.canvas.hidden;
    this.overlay.toggleAttribute("hidden", !shown);
    if (!shown || !size) return;
    const [width, height] = [this.canvas.clientWidth, this.canvas.clientHeight];
    this.overlay.setAttribute("viewBox", `0 0 ${width} ${height}`);
    this.overlay.replaceChildren();
    this.settings.regions.forEach((region, index) => {
      const r = this.toScreen(region.rect);
      const selected = index === this.selected;
      for (const kind of ["edge-shadow", selected ? "edge selected" : "edge"]) {
        const rect = document.createElementNS(SVG, "rect");
        rect.setAttribute("class", kind);
        for (const [key, value] of Object.entries(r)) rect.setAttribute(key, String(value));
        this.overlay.append(rect);
      }
      if (!selected) return;
      for (const [cx, cy] of rectCorners(r)) {
        const handle = document.createElementNS(SVG, "rect");
        handle.setAttribute("class", "handle");
        handle.setAttribute("x", String(cx - HANDLE_SIZE / 2));
        handle.setAttribute("y", String(cy - HANDLE_SIZE / 2));
        handle.setAttribute("width", String(HANDLE_SIZE));
        handle.setAttribute("height", String(HANDLE_SIZE));
        this.overlay.append(handle);
      }
    });
  }

  private current(): Region | null {
    return this.selected === null ? null : (this.settings.regions[this.selected] ?? null);
  }

  /** 隠し方を選ぶ。範囲を選んでいれば、その範囲の隠し方も変える。 */
  private setTool(kind: RegionKind) {
    this.tool = kind;
    const region = this.current();
    if (region && region.kind !== kind) {
      region.kind = kind;
      this.changed();
      return;
    }
    this.show();
  }

  /** 強さ（選んでいる範囲、なければ次に足す範囲）。 */
  private setStrength(value: number) {
    const region = this.current();
    if (!region) {
      this.nextStrength = value;
      this.show();
      return;
    }
    if (region.strength === value) return;
    region.strength = value;
    this.changed();
  }

  private remove() {
    if (this.selected === null) return;
    this.settings.regions.splice(this.selected, 1);
    this.selected = null;
    this.changed();
  }

  private changed() {
    this.show();
    this.onChange();
  }

  // --- プレビュー上のドラッグ ----------------------------------------------------

  private toImage(event: PointerEvent): Point {
    const box = this.overlay.getBoundingClientRect();
    const [width, height] = this.size()!;
    return [((event.clientX - box.left) * width) / box.width, ((event.clientY - box.top) * height) / box.height];
  }

  private toScreen(rect: CropRect): CropRect {
    const [width, height] = this.size()!;
    const sx = this.canvas.clientWidth / width;
    const sy = this.canvas.clientHeight / height;
    return { x: rect.x * sx, y: rect.y * sy, width: rect.width * sx, height: rect.height * sy };
  }

  /** 選んでいる範囲の四隅のハンドルの上なら、その角の番号。 */
  private hitCorner(event: PointerEvent): number | null {
    const region = this.current();
    if (!region) return null;
    const box = this.overlay.getBoundingClientRect();
    const [px, py] = [event.clientX - box.left, event.clientY - box.top];
    const index = rectCorners(this.toScreen(region.rect)).findIndex(
      ([cx, cy]) => Math.abs(px - cx) <= HANDLE_HIT && Math.abs(py - cy) <= HANDLE_HIT,
    );
    return index >= 0 ? index : null;
  }

  private pointerDown(event: PointerEvent) {
    if (event.button !== 0 || !this.size()) return;
    const point = this.toImage(event);
    const corner = this.hitCorner(event);
    const region = this.current();
    if (corner !== null && region) {
      // 押した角の対角を止めて、大きさを変える
      this.drag = { mode: "resize", anchor: rectCorners(region.rect)[(corner + 2) % 4] };
    } else {
      const hit = hitRegion(
        this.settings.regions.map((r) => r.rect),
        point,
      );
      if (hit !== null) {
        this.selected = hit;
        this.drag = { mode: "move", anchor: point, start: { ...this.settings.regions[hit].rect } };
      } else {
        this.selected = null;
        this.drag = { mode: "new", anchor: point };
      }
      this.show();
    }
    this.overlay.setPointerCapture(event.pointerId);
  }

  private pointerMove(event: PointerEvent) {
    if (!this.drag) {
      this.updateCursor(event);
      return;
    }
    this.dragTo(event);
  }

  private pointerUp(event: PointerEvent) {
    if (!this.drag || event.button !== 0) return;
    this.dragTo(event);
    const region = this.current();
    // クリックしただけ（小さすぎる範囲）は足さない
    if (this.drag.mode === "new" && region && (region.rect.width < MIN_SIDE || region.rect.height < MIN_SIDE)) {
      this.settings.regions.splice(this.selected!, 1);
      this.selected = null;
      this.drag = null;
      this.changed();
      return;
    }
    this.drag = null;
    this.show();
  }

  private dragTo(event: PointerEvent) {
    const drag = this.drag;
    const size = this.size();
    if (!drag || !size) return;
    const point = this.toImage(event);
    if (drag.mode === "new") {
      const rect = rectFromPoints(drag.anchor, point, size);
      if (this.selected === null) {
        if (rect.width === 0 && rect.height === 0) return;
        this.settings.regions.push({ kind: this.tool, rect, strength: this.nextStrength });
        this.selected = this.settings.regions.length - 1;
      } else {
        this.settings.regions[this.selected].rect = rect;
      }
    } else if (drag.mode === "move") {
      const dx = point[0] - drag.anchor[0];
      const dy = point[1] - drag.anchor[1];
      this.settings.regions[this.selected!].rect = moveRect(drag.start, dx, dy, size);
    } else {
      this.settings.regions[this.selected!].rect = rectFromPoints(drag.anchor, point, size);
    }
    this.changed();
  }

  private updateCursor(event: PointerEvent) {
    const corner = this.hitCorner(event);
    const inside = hitRegion(
      this.settings.regions.map((r) => r.rect),
      this.toImage(event),
    );
    this.overlay.style.cursor =
      corner === 0 || corner === 2
        ? "nwse-resize"
        : corner === 1 || corner === 3
          ? "nesw-resize"
          : inside !== null
            ? "move"
            : "crosshair";
  }
}
