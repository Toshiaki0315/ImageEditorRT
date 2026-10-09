// プレビュー: Rust に設定をかけさせた縮小版を canvas に描き、エリアに収まるよう縦横比を保って表示する。

import { invoke } from "@tauri-apps/api/core";
import { type Histogram, readPreview } from "./protocol";
import { fitInside } from "./split";
import type { EditSettings } from "./types";

/** 1 回の描き直しの内訳 (ms)。 */
export type Timing = {
  render: number; // Rust の処理
  transfer: number; // 受け渡し（invoke の往復から Rust の時間を引いたもの）
  draw: number; // canvas に描いて、画面の更新が来るまで
  total: number;
};

/** 描画の待ちの上限。ウィンドウが隠れていると requestAnimationFrame が呼ばれないため。 */
const FRAME_TIMEOUT_MS = 200;

const nextFrame = () =>
  new Promise<void>((resolve) => {
    const timer = setTimeout(resolve, FRAME_TIMEOUT_MS);
    requestAnimationFrame(() => {
      clearTimeout(timer);
      resolve();
    });
  });

export class Preview {
  private readonly context: CanvasRenderingContext2D;
  private busy = false;
  private pending: EditSettings | null = null;
  /** 切り抜いた範囲だけを表示する（「トリミング実行」） */
  trimmed = false;
  /** 加工前を表示する（向きと切り抜く範囲だけを残す） */
  comparing = false;
  /** 左右に分けて比べるとき、加工前を描く canvas（null なら描かない。加工前の表示中も描かない） */
  splitCanvas: HTMLCanvasElement | null = null;
  /** 画像を閉じるたびに増やす（閉じる前の依頼の結果は描かない） */
  private generation = 0;
  /** 表示の大きさが変わったとき（ガイド・範囲の線を描き直す） */
  onResize: () => void = () => {};
  /** 描き直したとき、保存される写真のヒストグラムを知らせる */
  onHistogram: (histogram: Histogram) => void = () => {};

  constructor(
    private readonly stage: HTMLElement,
    private readonly canvas: HTMLCanvasElement,
    private readonly onError: (error: unknown) => void,
  ) {
    this.context = canvas.getContext("2d")!;
    new ResizeObserver(() => this.fit()).observe(stage);
  }

  /** 画像を開いたとき。透過があれば市松模様の上に描く。 */
  show(width: number, height: number, transparent: boolean) {
    this.canvas.width = width;
    this.canvas.height = height;
    this.canvas.classList.toggle("transparent", transparent);
    this.canvas.hidden = false;
    this.fit();
  }

  /** エリアに収まるよう、縦横比を保って表示の大きさを決める（ウィンドウの大きさに追従する）。 */
  fit() {
    const { width, height } = this.canvas;
    if (this.canvas.hidden || width === 0 || height === 0) return;
    const padding = 16;
    const areaWidth = this.stage.clientWidth - padding * 2;
    const areaHeight = this.stage.clientHeight - padding * 2;
    const scale = Math.min(areaWidth / width, areaHeight / height);
    if (!(scale > 0)) return;
    this.canvas.style.width = `${Math.floor(width * scale)}px`;
    this.canvas.style.height = `${Math.floor(height * scale)}px`;
    this.onResize();
  }

  /** 設定を変えたとき。描いている途中なら、終わってから最新の設定で 1 回だけ描き直す。 */
  request(settings: EditSettings) {
    this.pending = structuredClone(settings);
    if (!this.busy) void this.drain();
  }

  /** 画像を閉じたとき（リセット）: 描きかけの結果は描かず、canvas を隠す。 */
  clear() {
    this.pending = null;
    this.generation += 1;
    this.canvas.hidden = true;
  }

  private async drain() {
    this.busy = true;
    const generation = this.generation;
    try {
      while (this.pending && generation === this.generation) {
        const settings = this.pending;
        this.pending = null;
        await this.render(settings, this.trimmed);
      }
    } catch (error) {
      if (generation === this.generation) this.onError(error);
    } finally {
      this.busy = false;
    }
  }

  /** 加工前（向き・切り抜く範囲だけ）を作らせ、加工後と同じ大きさの canvas に縦横比を保って中央に描く。 */
  private async renderBefore(target: HTMLCanvasElement, settings: EditSettings, trimmed: boolean) {
    const generation = this.generation;
    const buffer = await invoke<ArrayBuffer>("render_preview", { settings, trimmed, comparing: true });
    if (generation !== this.generation || this.splitCanvas !== target) return;
    const { width, height, pixels } = readPreview(buffer);
    const source = new OffscreenCanvas(width, height);
    source.getContext("2d")!.putImageData(new ImageData(pixels, width, height), 0, 0);
    target.width = this.canvas.width;
    target.height = this.canvas.height;
    const context = target.getContext("2d")!;
    // 加工後と比が違う（フレームなど）ときの余りは、表示エリアの背景の色で埋める
    context.fillStyle = getComputedStyle(this.stage).backgroundColor;
    context.fillRect(0, 0, target.width, target.height);
    const place = fitInside(width, height, target.width, target.height);
    context.clearRect(place.x, place.y, place.width, place.height);
    context.drawImage(source, place.x, place.y, place.width, place.height);
  }

  /** Rust にプレビューを作らせて描き、内訳を返す。 */
  async render(settings: EditSettings, trimmed = false): Promise<Timing> {
    const start = performance.now();
    const generation = this.generation;
    const buffer = await invoke<ArrayBuffer>("render_preview", { settings, trimmed, comparing: this.comparing });
    const received = performance.now();
    if (generation !== this.generation) return { render: 0, transfer: 0, draw: 0, total: 0 };
    const { width, height, renderMs: render, pixels, histogram } = readPreview(buffer);
    if (this.canvas.width !== width || this.canvas.height !== height) {
      this.canvas.width = width;
      this.canvas.height = height;
      this.fit();
    }
    this.context.putImageData(new ImageData(pixels, width, height), 0, 0);
    this.onHistogram(histogram);
    if (this.splitCanvas && !this.comparing) await this.renderBefore(this.splitCanvas, settings, trimmed);
    await nextFrame(); // 画面に出るところまで含める
    const end = performance.now();
    return { render, transfer: received - start - render, draw: end - received, total: end - start };
  }
}
