// 履歴の一覧に出す「何を変えたか」（前の状態との違いから作る。旧版にはない）。
// 画面の部品には依存しない（tests-ts/historyLabels.test.ts で確かめる）。

import { sameValue } from "./history.ts";

/** 設定の項目の名前（EditSettings のキー → 画面の言葉）。ここにない項目は「加工」とする。 */
const NAMES: Record<string, string> = {
  orientation: "回転・反転",
  straighten: "水平の補正",
  perspectiveVertical: "遠近の補正",
  perspectiveHorizontal: "遠近の補正",
  crop: "切り抜き",
  width: "出力の大きさ",
  height: "出力の大きさ",
  keepAspect: "縦横比を保つ",
  filter: "テイスト",
  filterStrength: "テイストの強さ",
  vignette: "周辺減光",
  aging: "経年劣化",
  temperature: "色温度",
  saturation: "彩度",
  brightness: "明るさ",
  contrast: "コントラスト",
  highlights: "ハイライト",
  shadows: "シャドウ",
  dehaze: "かすみの除去",
  exposure: "露出",
  sharpen: "シャープ",
  blur: "ぼかし",
  denoise: "ノイズ除去",
  dioramaBlur: "ジオラマ",
  dioramaDirection: "ジオラマ",
  dioramaPosition: "ジオラマ",
  dioramaWidth: "ジオラマ",
  dioramaVivid: "ジオラマ",
  text: "文字・透かし",
  frame: "フレーム",
  shape: "形",
  cornerRadius: "角丸",
  regions: "投稿加工の範囲",
  background: "背景",
  backgroundBlur: "背景",
  backgroundColor: "背景",
  backgroundImage: "背景",
  subjectAdjust: "被写体の補正",
  backgroundAdjust: "背景の補正",
  toneCurve: "トーンカーブ",
  hsl: "色ごとの調整",
  mono: "白黒",
  logo: "ロゴ",
  lut: "LUT",
  colorMatch: "色を合わせる",
  skinSmooth: "肌をなめらかに",
  redEye: "赤目の補正",
  localAdjustments: "部分補正",
};

/** 一覧に出す名前の数（それより多ければ「ほか」を付ける） */
const MAX_NAMES = 3;

/** 履歴の 1 つ分の状態（app の Snapshot と同じ形のうち、使うところ）。 */
export type LabeledState = { settings: Record<string, unknown>; aspect: unknown; size: unknown };

/** previous から next への変更の説明（「露出・コントラスト」など）。previous がなければ「開いたとき」。 */
export function describeChange(previous: LabeledState | null, next: LabeledState): string {
  if (!previous) return "開いたとき";
  const names: string[] = [];
  const add = (name: string) => {
    if (!names.includes(name)) names.push(name);
  };
  const keys = new Set([...Object.keys(previous.settings), ...Object.keys(next.settings)]);
  for (const key of keys) {
    if (!sameValue(previous.settings[key], next.settings[key])) add(NAMES[key] ?? "加工");
  }
  if (!sameValue(previous.aspect, next.aspect)) add("切り抜きの比");
  if (!sameValue(previous.size, next.size)) add("出力の大きさ");
  if (names.length === 0) return "変更";
  const shown = names.slice(0, MAX_NAMES).join("・");
  return names.length > MAX_NAMES ? `${shown} ほか` : shown;
}

/** 履歴のすべての状態の説明（古い順）。 */
export function describeHistory(states: LabeledState[]): string[] {
  return states.map((state, i) => describeChange(i === 0 ? null : states[i - 1], state));
}
