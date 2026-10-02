// 設定のスライダー（「加工」「ジオラマ」タブ）。範囲・刻み・値の表示は旧版と同じ（FR-UI-21〜28・53・60）。

import type { EditSettings, FilterType } from "./types";

/** スライダーで変える数値の項目。 */
type NumberKey = { [K in keyof EditSettings]: EditSettings[K] extends number ? K : never }[keyof EditSettings];

type Slider = {
  key: NumberKey;
  label: string;
  min: number;
  max: number;
  /** 既定値（ダブルクリックでこの値に戻す） */
  initial: number;
  step?: number;
  /** 値の表示（省略時はそのまま） */
  text?: (value: number) => string;
};

/** 露出を「+1.3 EV」「-0.5 EV」「0.0 EV」のように表示する。 */
export const evText = (ev: number) => (ev ? `${ev > 0 ? "+" : ""}${ev.toFixed(1)} EV` : "0.0 EV");
/** 0 以外は符号付きで表示する（例: +30, -50）。 */
export const signedText = (value: number) => (value > 0 ? `+${value}` : String(value));
/** 色温度を「6500 K」のように表示する。 */
export const kelvinText = (kelvin: number) => `${kelvin} K`;

/** 「加工をリセット」で戻す、色の調整のスライダー（旧版と同じ順）。 */
const COLOR: Slider[] = [
  { key: "exposure", label: "露出", min: -5, max: 5, step: 0.1, initial: 0, text: evText },
  { key: "brightness", label: "明るさ", min: -100, max: 100, initial: 0, text: signedText },
  { key: "contrast", label: "コントラスト", min: -100, max: 100, initial: 0, text: signedText },
  { key: "temperature", label: "色温度", min: 2000, max: 10000, step: 100, initial: 6500, text: kelvinText },
  { key: "saturation", label: "彩度", min: -100, max: 100, initial: 0, text: signedText },
  { key: "vignette", label: "周辺減光", min: 0, max: 100, initial: 0 },
  { key: "aging", label: "経年劣化", min: 0, max: 100, initial: 0 },
];

/** 「加工をリセット」で戻す、ディテールのスライダー。 */
const DETAIL: Slider[] = [
  { key: "sharpen", label: "シャープ", min: 0, max: 100, initial: 0 },
  { key: "blur", label: "ぼかし", min: 0, max: 100, initial: 0 },
  { key: "denoise", label: "ノイズ除去", min: 0, max: 100, initial: 0 },
];

/** 「ジオラマ」タブのスライダー（#11 で旧版に合わせる）。 */
const DIORAMA: Slider[] = [
  { key: "dioramaBlur", label: "ぼかし", min: 0, max: 100, initial: 0 },
  { key: "dioramaPosition", label: "ピントの位置", min: 0, max: 100, initial: 50 },
  { key: "dioramaWidth", label: "ピントの幅", min: 0, max: 100, initial: 20 },
  { key: "dioramaVivid", label: "鮮やかさ", min: 0, max: 100, initial: 30 },
];

/** スライダーを作り、値を変えるたびに onChange を呼ぶ。 */
export class Panel {
  private readonly rows = new Map<NumberKey, { slider: Slider; input: HTMLInputElement; output: HTMLOutputElement }>();
  private readonly filter = document.createElement("select");

  constructor(
    adjustPage: HTMLElement,
    dioramaPage: HTMLElement,
    filters: [FilterType, string][],
    private readonly settings: EditSettings,
    private readonly onChange: () => void,
  ) {
    // テイスト（色だけを変える。旧版 FR-UI-20）
    const heading = document.createElement("h2");
    heading.textContent = "テイスト";
    this.filter.className = "filter";
    for (const [value, label] of filters) this.filter.add(new Option(label, value));
    this.filter.addEventListener("change", () => {
      this.settings.filter = this.filter.value as FilterType;
      this.onChange();
    });
    adjustPage.append(heading, this.filter);
    this.build(adjustPage, "加工", COLOR);
    this.build(adjustPage, "ディテール", DETAIL);
    const reset = document.createElement("button");
    reset.type = "button";
    reset.className = "reset-adjustments";
    reset.textContent = "加工をリセット";
    reset.title = "テイスト・色の調整・ディテールを既定値に戻します（切り抜き・サイズ・ジオラマはそのまま）";
    reset.addEventListener("click", () => this.resetAdjustments());
    adjustPage.append(reset);
    this.build(dioramaPage, "ジオラマ", DIORAMA);
    this.show();
  }

  private build(page: HTMLElement, title: string, sliders: Slider[]) {
    const heading = document.createElement("h2");
    heading.textContent = title;
    page.append(heading);
    for (const slider of sliders) {
      const row = document.createElement("label");
      row.className = "row";
      const input = document.createElement("input");
      input.type = "range";
      input.min = String(slider.min);
      input.max = String(slider.max);
      input.step = String(slider.step ?? 1);
      input.title = "ダブルクリックで既定値に戻す";
      const output = document.createElement("output");
      input.addEventListener("input", () => this.set(slider, Number(input.value)));
      // ダブルクリックでその項目を既定値に戻す（旧版 FR-UI-53）
      input.addEventListener("dblclick", () => this.set(slider, slider.initial));
      this.rows.set(slider.key, { slider, input, output });
      row.append(slider.label, input, output);
      page.append(row);
    }
  }

  private set(slider: Slider, value: number) {
    // 0.1 刻みの露出は、小数の誤差を残さない
    const step = slider.step ?? 1;
    const rounded = step < 1 ? Math.round(value / step) * step : value;
    const fixed = Number(rounded.toFixed(step < 1 ? 1 : 0));
    if (this.settings[slider.key] === fixed) {
      this.showRow(slider.key);
      return;
    }
    this.settings[slider.key] = fixed;
    this.showRow(slider.key);
    this.onChange();
  }

  /** テイスト・色の調整・ディテールを既定値に戻す（旧版 FR-UI-54。確認は出さない）。 */
  resetAdjustments() {
    this.settings.filter = "none";
    for (const slider of [...COLOR, ...DETAIL]) this.settings[slider.key] = slider.initial;
    this.show();
    this.onChange();
  }

  /** settings の値をスライダーに反映する。 */
  show() {
    this.filter.value = this.settings.filter;
    for (const key of this.rows.keys()) this.showRow(key);
  }

  private showRow(key: NumberKey) {
    const row = this.rows.get(key)!;
    const value = this.settings[key];
    row.input.value = String(value);
    row.output.textContent = row.slider.text ? row.slider.text(value) : String(value);
  }
}
