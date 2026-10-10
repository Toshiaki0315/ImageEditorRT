// 切り抜きのガイド線（三分割・黄金比・対角線）の位置の計算。画面の部品に依存しない。

/** ガイドの種類。 */
export type GuideKind = "none" | "thirds" | "golden" | "diagonal";

export const GUIDE_KINDS: [GuideKind, string][] = [
  ["none", "なし"],
  ["thirds", "三分割"],
  ["golden", "黄金比"],
  ["diagonal", "対角線"],
];

/** 黄金比で分ける位置（短い方 : 長い方 = 1 : φ。0.382 と 0.618）。 */
const GOLDEN = 1 / (1 + (1 + Math.sqrt(5)) / 2);

type Rect = { x: number; y: number; width: number; height: number };
/** 線（x1, y1, x2, y2）。 */
export type Line = [number, number, number, number];

/** 範囲 rect の中に引くガイドの線。 */
export function guideLines(kind: GuideKind, rect: Rect): Line[] {
  const { x, y, width, height } = rect;
  if (!(width > 0 && height > 0)) return [];
  const split = (fractions: number[]): Line[] => [
    ...fractions.map((f): Line => [x + width * f, y, x + width * f, y + height]),
    ...fractions.map((f): Line => [x, y + height * f, x + width, y + height * f]),
  ];
  switch (kind) {
    case "thirds":
      return split([1 / 3, 2 / 3]);
    case "golden":
      return split([GOLDEN, 1 - GOLDEN]);
    case "diagonal":
      return [
        [x, y, x + width, y + height],
        [x + width, y, x, y + height],
      ];
    default:
      return [];
  }
}

/**
 * 水平の補正で使う格子の線（width×height の画像に、正方形のマス目。短辺を cells 等分した間隔で、中心を通る線から
 * 外へ並べる）。縦の線・横の線の順。
 */
export function straightenGrid(width: number, height: number, cells = 8): Line[] {
  if (!(width > 0 && height > 0 && cells > 0)) return [];
  const step = Math.min(width, height) / cells;
  const positions = (length: number) => {
    const center = length / 2;
    const count = Math.floor(center / step);
    const list: number[] = [];
    for (let k = -count; k <= count; k++) list.push(center + k * step);
    return list;
  };
  return [
    ...positions(width).map((x): Line => [x, 0, x, height]),
    ...positions(height).map((y): Line => [0, y, width, y]),
  ];
}

/** 環境設定に残した値を読む（知らない値ならなし）。 */
export function parseGuide(value: string | null): GuideKind {
  return GUIDE_KINDS.some(([kind]) => kind === value) ? (value as GuideKind) : "none";
}
