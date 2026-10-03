// プレビュー: Rust に設定をかけさせた縮小版を canvas に描き、エリアに収まるよう縦横比を保って表示する。

import { invoke } from "@tauri-apps/api/core";
import { type Histogram, readHistogram } from "./histogram";
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

  private async drain() {
    this.busy = true;
    try {
      while (this.pending) {
        const settings = this.pending;
        this.pending = null;
        await this.render(settings, this.trimmed);
      }
    } catch (error) {
      this.onError(error);
    } finally {
      this.busy = false;
    }
  }

  /** Rust にプレビューを作らせて描き、内訳を返す。 */
  async render(settings: EditSettings, trimmed = false): Promise<Timing> {
    const start = performance.now();
    const buffer = await invoke<ArrayBuffer>("render_preview", { settings, trimmed, comparing: this.comparing });
    const received = performance.now();
    const header = new DataView(buffer, 0, 12);
    const width = header.getUint32(0, true);
    const height = header.getUint32(4, true);
    const render = header.getUint32(8, true) / 1000;
    if (this.canvas.width !== width || this.canvas.height !== height) {
      this.canvas.width = width;
      this.canvas.height = height;
      this.fit();
    }
    const pixels = new Uint8ClampedArray(buffer, 12, width * height * 4);
    this.context.putImageData(new ImageData(pixels, width, height), 0, 0);
    this.onHistogram(readHistogram(buffer, 12 + width * height * 4));
    await nextFrame(); // 画面に出るところまで含める
    const end = performance.now();
    return { render, transfer: received - start - render, draw: end - received, total: end - start };
  }
}
