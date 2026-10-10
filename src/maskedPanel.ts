// 「加工」タブの「被写体／背景の補正」（被写体だけ・背景だけに露出・コントラスト・色温度・彩度をかける。旧版にはない）。
// 被写体のマスクは「背景を消す」と同じもの（Vision）。初めて動かしたときに作る。

import { evText, kelvinText, signedText } from "./panel";
import type { EditSettings, MaskedAdjust } from "./types";

type Target = "subjectAdjust" | "backgroundAdjust";
type Key = keyof MaskedAdjust;

const SLIDERS: { key: Key; label: string; min: number; max: number; step?: number; initial: number; text: (v: number) => string }[] = [
  { key: "exposure", label: "対象の露出", min: -5, max: 5, step: 0.1, initial: 0, text: evText },
  { key: "contrast", label: "対象のコントラスト", min: -100, max: 100, initial: 0, text: signedText },
  { key: "temperature", label: "対象の色温度", min: 2000, max: 10000, step: 100, initial: 6500, text: kelvinText },
  { key: "saturation", label: "対象の彩度", min: -100, max: 100, initial: 0, text: signedText },
];

/** 何も変えない補正。 */
export const neutralMasked = (): MaskedAdjust => ({ exposure: 0, contrast: 0, temperature: 6500, saturation: 0 });

const isNeutral = (adjust: MaskedAdjust) => SLIDERS.every(({ key, initial }) => adjust[key] === initial);

export class MaskedPanel {
  private readonly target = document.createElement("select");
  private readonly rows = new Map<Key, { input: HTMLInputElement; output: HTMLOutputElement }>();
  private readonly resetButton = document.createElement("button");
  /** 被写体のマスクを作る。被写体があれば true、なければ false、作れなければ null */
  prepareMask: () => Promise<boolean | null> = async () => true;
  /** 被写体が見つからなかったとき */
  onNoSubject: () => void = () => {};
  /** マスクを作っている間（もう一度は作らない） */
  private preparing = false;
  private readonly settings: EditSettings;
  private readonly loaded: () => boolean;
  private readonly onChange: () => void;

  /**
   * @param container 欄を作る場所（「加工」タブの部分補正の下）
   * @param loaded 画像を開いているか
   * @param onChange 設定を変えたとき（履歴に積み、プレビューを描き直す）
   */
  constructor(container: HTMLElement, settings: EditSettings, loaded: () => boolean, onChange: () => void) {
    this.settings = settings;
    this.loaded = loaded;
    this.onChange = onChange;
    const heading = document.createElement("h2");
    heading.textContent = "被写体／背景の補正";
    const targetRow = document.createElement("label");
    targetRow.className = "row";
    for (const [value, label] of [
      ["subjectAdjust", "被写体"],
      ["backgroundAdjust", "背景"],
    ]) {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = label;
      this.target.append(option);
    }
    this.target.title = "どちらに補正をかけるか（被写体と背景で別々に覚えます）";
    this.target.addEventListener("change", () => this.show());
    targetRow.append("対象", this.target);
    container.append(heading, targetRow);
    for (const slider of SLIDERS) {
      const row = document.createElement("label");
      row.className = "row";
      const input = document.createElement("input");
      input.type = "range";
      input.min = String(slider.min);
      input.max = String(slider.max);
      input.step = String(slider.step ?? 1);
      input.title = "ダブルクリックで既定値に戻す";
      const output = document.createElement("output");
      input.addEventListener("input", () => void this.set(slider.key, Number(input.value)));
      input.addEventListener("dblclick", () => void this.set(slider.key, slider.initial));
      row.append(slider.label, input, output);
      container.append(row);
      this.rows.set(slider.key, { input, output });
    }
    const buttons = document.createElement("div");
    buttons.className = "buttons";
    this.resetButton.type = "button";
    this.resetButton.textContent = "被写体／背景の補正をリセット";
    this.resetButton.addEventListener("click", () => this.reset());
    buttons.append(this.resetButton);
    const note = document.createElement("p");
    note.className = "note";
    note.textContent =
      "人・動物・ものなど目立つ被写体と、その背景に別々の補正をかけます（「背景を消す」と同じ被写体の見つけ方。macOS 14 以降。処理はこの Mac の中だけ）。";
    container.append(buttons, note);
    this.show();
  }

  /** 設定の値を欄に反映する。 */
  show() {
    const loaded = this.loaded();
    const adjust = this.settings[this.target.value as Target];
    for (const slider of SLIDERS) {
      const { input, output } = this.rows.get(slider.key)!;
      input.value = String(adjust[slider.key]);
      output.textContent = slider.text(adjust[slider.key]);
      input.disabled = !loaded || this.preparing;
    }
    this.target.disabled = !loaded;
    this.resetButton.disabled =
      !loaded || (isNeutral(this.settings.subjectAdjust) && isNeutral(this.settings.backgroundAdjust));
  }

  /** 選んでいる対象の値を変える。初めて補正をかけるときは、先に被写体のマスクを作る（被写体がなければ変えない）。 */
  private async set(key: Key, value: number) {
    const target = this.target.value as Target;
    if (this.settings[target][key] === value) return;
    const first = isNeutral(this.settings.subjectAdjust) && isNeutral(this.settings.backgroundAdjust);
    if (first) {
      if (this.preparing) return;
      this.preparing = true;
      this.show();
      try {
        const found = await this.prepareMask();
        if (found !== true) {
          if (found === false) this.onNoSubject();
          return;
        }
      } finally {
        this.preparing = false;
        this.show();
      }
    }
    this.settings[target] = { ...this.settings[target], [key]: value };
    this.show();
    this.onChange();
  }

  private reset() {
    this.settings.subjectAdjust = neutralMasked();
    this.settings.backgroundAdjust = neutralMasked();
    this.show();
    this.onChange();
  }
}
