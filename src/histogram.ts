// プレビューの右下に重ねるヒストグラム（R・G・B を半透明で、輝度を灰色で重ねる。旧版 FR-UI-46）。

const BINS = 256;
const GRAPH_WIDTH = BINS; // 横は 1 段階 1px
const GRAPH_HEIGHT = 90;
const PADDING = 6;
const BACKGROUND = "rgba(0, 0, 0, 0.59)";
/** R・G・B・輝度の塗りの色（重なると混ざるよう半透明）。 */
const CHANNEL_COLORS = [
  "rgba(255, 70, 70, 0.43)",
  "rgba(70, 220, 70, 0.43)",
  "rgba(80, 130, 255, 0.47)",
  "rgba(230, 230, 230, 0.47)",
];
/** 表示するかを残す環境設定のキー。 */
const STORAGE_KEY = "view.histogram";

/** R・G・B・輝度の順に 256 個ずつの画素数。 */
export type Histogram = Uint32Array[];

/** プレビューの応答の末尾（R・G・B・輝度の順に 256 個ずつの u32 リトルエンディアン）を読む。 */
export function readHistogram(buffer: ArrayBuffer, offset: number): Histogram {
  const view = new DataView(buffer, offset, 4 * BINS * 4);
  return [0, 1, 2, 3].map((c) => Uint32Array.from({ length: BINS }, (_, i) => view.getUint32((c * BINS + i) * 4, true)));
}

/**
 * 高さの基準にする値: すべてのチャンネルの、両端（0・255）を除いた最大の画素数（最小 1）。
 * 白飛び・黒つぶれで両端だけ多くてもほかがつぶれないよう、両端はグラフの上端で切る（Rust の Histogram::peak と同じ）。
 */
export function histogramPeak(histogram: Histogram): number {
  let peak = 1;
  for (const channel of histogram) {
    for (let i = 1; i < BINS - 1; i++) peak = Math.max(peak, channel[i]);
  }
  return peak;
}

export class HistogramView {
  private histogram: Histogram | null = null;
  /** 表示する設定か（メニュー「表示 > ヒストグラム」。環境設定に残す。既定は表示） */
  shown: boolean;

  constructor(private readonly canvas: HTMLCanvasElement) {
    let saved: string | null = null;
    try {
      saved = localStorage.getItem(STORAGE_KEY);
    } catch {
      // 環境設定が読めなければ既定の表示にする
    }
    this.shown = saved !== "false";
    canvas.style.width = `${GRAPH_WIDTH + PADDING * 2}px`;
    canvas.style.height = `${GRAPH_HEIGHT + PADDING * 2}px`;
  }

  /** 表示・非表示を切り替えて、環境設定に残す。 */
  setShown(shown: boolean) {
    this.shown = shown;
    try {
      localStorage.setItem(STORAGE_KEY, String(shown));
    } catch {
      // 残せなくても表示は切り替える
    }
    this.draw();
  }

  /** 描き直したプレビューのヒストグラム。null で消す（画像がないとき）。 */
  set(histogram: Histogram | null) {
    this.histogram = histogram;
    this.draw();
  }

  private draw() {
    const visible = this.shown && this.histogram !== null;
    this.canvas.hidden = !visible;
    if (!visible) return;
    const ratio = window.devicePixelRatio || 1;
    const width = GRAPH_WIDTH + PADDING * 2;
    const height = GRAPH_HEIGHT + PADDING * 2;
    this.canvas.width = Math.round(width * ratio);
    this.canvas.height = Math.round(height * ratio);
    const context = this.canvas.getContext("2d")!;
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    context.fillStyle = BACKGROUND;
    context.beginPath();
    context.roundRect(0, 0, width, height, 6);
    context.fill();

    const histogram = this.histogram!;
    const peak = histogramPeak(histogram);
    const bottom = PADDING + GRAPH_HEIGHT;
    const step = GRAPH_WIDTH / BINS;
    histogram.forEach((channel, c) => {
      context.beginPath();
      context.moveTo(PADDING, bottom);
      channel.forEach((count, i) => {
        const top = bottom - GRAPH_HEIGHT * Math.min(1, count / peak);
        const x = PADDING + i * step;
        context.lineTo(x, top);
        context.lineTo(x + step, top);
      });
      context.lineTo(PADDING + GRAPH_WIDTH, bottom);
      context.closePath();
      context.fillStyle = CHANNEL_COLORS[c];
      context.fill();
    });
  }
}
