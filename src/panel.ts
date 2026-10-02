// 設定のスライダー（「加工」「ジオラマ」タブ）。値の範囲・刻みは後の Issue（#8・#10・#11）で旧版に合わせる。

import type { EditSettings } from "./types";

/** スライダーで変える数値の項目。 */
type NumberKey = { [K in keyof EditSettings]: EditSettings[K] extends number ? K : never }[keyof EditSettings];

type Slider = {
  key: NumberKey;
  label: string;
  min: number;
  max: number;
  step?: number;
};

const ADJUST_GROUPS: [string, Slider[]][] = [
  [
    "色・明るさ",
    [
      { key: "exposure", label: "露出 (EV)", min: -5, max: 5, step: 0.1 },
      { key: "brightness", label: "明るさ", min: -100, max: 100 },
      { key: "contrast", label: "コントラスト", min: -100, max: 100 },
      { key: "temperature", label: "色温度 (K)", min: 2000, max: 10000, step: 100 },
      { key: "saturation", label: "彩度", min: -100, max: 100 },
      { key: "vignette", label: "周辺減光", min: 0, max: 100 },
      { key: "aging", label: "経年劣化", min: 0, max: 100 },
    ],
  ],
  [
    "ディテール",
    [
      { key: "sharpen", label: "シャープ", min: 0, max: 100 },
      { key: "blur", label: "ぼかし", min: 0, max: 100 },
      { key: "denoise", label: "ノイズ除去", min: 0, max: 100 },
    ],
  ],
];

const DIORAMA_GROUPS: [string, Slider[]][] = [
  [
    "ジオラマ",
    [
      { key: "dioramaBlur", label: "ぼかし", min: 0, max: 100 },
      { key: "dioramaPosition", label: "ピントの位置", min: 0, max: 100 },
      { key: "dioramaWidth", label: "ピントの幅", min: 0, max: 100 },
      { key: "dioramaVivid", label: "鮮やかさ", min: 0, max: 100 },
    ],
  ],
];

/** スライダーを作り、値を変えるたびに onChange を呼ぶ。 */
export class Panel {
  private readonly inputs = new Map<NumberKey, HTMLInputElement>();

  constructor(
    adjustPage: HTMLElement,
    dioramaPage: HTMLElement,
    private readonly settings: EditSettings,
    private readonly onChange: () => void,
  ) {
    this.build(adjustPage, ADJUST_GROUPS);
    this.build(dioramaPage, DIORAMA_GROUPS);
    this.show();
  }

  private build(page: HTMLElement, groups: [string, Slider[]][]) {
    for (const [title, sliders] of groups) {
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
        const output = document.createElement("output");
        input.addEventListener("input", () => {
          this.settings[slider.key] = Number(input.value);
          output.textContent = input.value;
          this.onChange();
        });
        this.inputs.set(slider.key, input);
        row.append(slider.label, input, output);
        page.append(row);
      }
    }
  }

  /** settings の値をスライダーに反映する。 */
  show() {
    for (const [key, input] of this.inputs) {
      input.value = String(this.settings[key]);
      const output = input.nextElementSibling;
      if (output instanceof HTMLOutputElement) output.textContent = input.value;
    }
  }
}
