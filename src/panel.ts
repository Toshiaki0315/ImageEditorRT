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
  { key: "highlights", label: "ハイライト", min: -100, max: 100, initial: 0, text: signedText },
  { key: "shadows", label: "シャドウ", min: -100, max: 100, initial: 0, text: signedText },
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

/** 割合を「50%」のように表示する。 */
export const percentText = (value: number) => `${value}%`;

/**
 * テイストの強さ（旧版にはない）。100% がテイストのまま、下げると元の写真に近づき、上げる（最大 200%）と
 * テイストによる変化を強める。テイストが「なし」のときは使えない。
 */
const STRENGTH: Slider[] = [{ key: "filterStrength", label: "強さ", min: 0, max: 200, initial: 100, text: percentText }];

/** 「ジオラマ」タブのスライダー（旧版 FR-UI-62）。ぼかしの次に「帯の向き」を置く。 */
const DIORAMA_BLUR: Slider[] = [{ key: "dioramaBlur", label: "ぼかし", min: 0, max: 100, initial: 0 }];
const DIORAMA_BAND: Slider[] = [
  { key: "dioramaPosition", label: "ピントの位置", min: 0, max: 100, initial: 50, text: percentText },
  { key: "dioramaWidth", label: "ピントの幅", min: 0, max: 100, initial: 20, text: percentText },
  { key: "dioramaVivid", label: "鮮やかさ", min: 0, max: 100, initial: 30 },
];

const DIORAMA_NOTE =
  "ぼかしを 0 より大きくすると、ピントの帯（プレビューの実線の間）だけをくっきり残し、" +
  "外側に向かってぼかします（点線でぼけきります）。街並みを見下ろした写真に向いています。";

/** スライダーを作り、値を変えるたびに onChange を呼ぶ。 */
export class Panel {
  private readonly rows = new Map<NumberKey, { slider: Slider; input: HTMLInputElement; output: HTMLOutputElement }>();
  private readonly filter = document.createElement("select");
  private readonly direction = document.createElement("select");

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
    this.filter.addEventListener("change", () => this.selectFilter(this.filter.value as FilterType));
    // 旧版と同じく、テイストの行に「文字…」のボタンを置く（⌘T と同じ）
    const row = document.createElement("div");
    row.className = "filter-row";
    const textButton = document.createElement("button");
    textButton.type = "button";
    textButton.id = "text-button";
    textButton.textContent = "文字…";
    textButton.title = "文字・透かし（⌘T）";
    textButton.disabled = true;
    // 名前付きの加工の組み合わせ（旧版 FR-UI-59）。押すとメニューを出す
    const presetButton = document.createElement("button");
    presetButton.type = "button";
    presetButton.id = "preset-button";
    presetButton.textContent = "プリセット ▾";
    presetButton.title = "加工の組み合わせを保存・呼び出し・削除します";
    // 今の写真にテイストをかけた見本を並べて選ぶ（旧版にはない）
    const galleryButton = document.createElement("button");
    galleryButton.type = "button";
    galleryButton.id = "taste-button";
    galleryButton.textContent = "一覧…";
    galleryButton.title = "今の写真にテイストをかけた見本を並べて選びます";
    galleryButton.disabled = true;
    row.append(this.filter, galleryButton, presetButton, textButton);
    adjustPage.append(heading, row);
    this.build(adjustPage, STRENGTH);
    this.heading(adjustPage, "加工");
    this.build(adjustPage, COLOR);
    this.heading(adjustPage, "ディテール");
    this.build(adjustPage, DETAIL);
    const reset = document.createElement("button");
    reset.type = "button";
    reset.className = "reset-adjustments";
    reset.textContent = "加工をリセット";
    reset.title = "テイスト・色の調整・ディテールを既定値に戻します（切り抜き・サイズ・ジオラマはそのまま）";
    reset.addEventListener("click", () => this.resetAdjustments());
    adjustPage.append(reset);
    this.heading(dioramaPage, "ジオラマ（ミニチュア風）");
    this.build(dioramaPage, DIORAMA_BLUR);
    const directionRow = document.createElement("label");
    directionRow.className = "row";
    this.direction.add(new Option("横の帯", "horizontal"));
    this.direction.add(new Option("縦の帯", "vertical"));
    this.direction.addEventListener("change", () => {
      this.settings.dioramaDirection = this.direction.value as EditSettings["dioramaDirection"];
      this.onChange();
    });
    directionRow.append("帯の向き", this.direction);
    dioramaPage.append(directionRow);
    this.build(dioramaPage, DIORAMA_BAND);
    const note = document.createElement("p");
    note.className = "note";
    note.textContent = DIORAMA_NOTE;
    dioramaPage.append(note);
    this.show();
  }

  private heading(page: HTMLElement, title: string) {
    const heading = document.createElement("h2");
    heading.textContent = title;
    page.append(heading);
  }

  private build(page: HTMLElement, sliders: Slider[]) {
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

  /** テイストを選ぶ（プルダウン・一覧から）。強さは既定（100%）に戻す（前のテイストに合わせた強さを持ち越さない）。 */
  selectFilter(filter: FilterType) {
    this.settings.filter = filter;
    this.filter.value = filter;
    this.settings.filterStrength = STRENGTH[0].initial;
    this.showRow("filterStrength");
    this.updateStrength();
    this.onChange();
  }

  /** テイスト・色の調整・ディテールを既定値に戻す（旧版 FR-UI-54。確認は出さない）。 */
  resetAdjustments() {
    this.settings.filter = "none";
    for (const slider of [...STRENGTH, ...COLOR, ...DETAIL]) this.settings[slider.key] = slider.initial;
    this.show();
    this.onChange();
  }

  /** settings の値をスライダーに反映する。 */
  show() {
    this.filter.value = this.settings.filter;
    this.direction.value = this.settings.dioramaDirection;
    for (const key of this.rows.keys()) this.showRow(key);
    this.updateStrength();
  }

  /** テイストが「なし」のときは強さを使えない。 */
  private updateStrength() {
    this.rows.get("filterStrength")!.input.disabled = this.settings.filter === "none";
  }

  private showRow(key: NumberKey) {
    const row = this.rows.get(key)!;
    const value = this.settings[key];
    row.input.value = String(value);
    row.output.textContent = row.slider.text ? row.slider.text(value) : String(value);
  }
}
