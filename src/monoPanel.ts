// 「加工」タブの「白黒」（白黒にし、色ごとにどれだけ明るく写すかを決める。旧版にはない）。

import { signedText } from "./panel";
import type { EditSettings } from "./types";

/** 色（Rust の curve::HSL_CENTERS と同じ順）。 */
const BANDS = ["赤", "オレンジ", "黄", "緑", "水色", "青", "紫", "マゼンタ"];

/** 白黒にしない既定の設定。 */
export const defaultMono = (): EditSettings["mono"] => ({ enabled: false, mix: [0, 0, 0, 0, 0, 0, 0, 0] });

export class MonoPanel {
  private readonly enabled = document.createElement("input");
  private readonly sliders: { input: HTMLInputElement; output: HTMLOutputElement }[] = [];
  private readonly resetButton = document.createElement("button");
  private readonly settings: EditSettings;
  private readonly loaded: () => boolean;
  private readonly onChange: () => void;

  /**
   * @param container 欄を作る場所（「加工」タブの色ごとの調整の下）
   * @param loaded 画像を開いているか
   * @param onChange 設定を変えたとき（履歴に積み、プレビューを描き直す）
   */
  constructor(container: HTMLElement, settings: EditSettings, loaded: () => boolean, onChange: () => void) {
    this.settings = settings;
    this.loaded = loaded;
    this.onChange = onChange;
    const heading = document.createElement("h2");
    heading.textContent = "白黒";
    const check = document.createElement("label");
    check.className = "check";
    this.enabled.type = "checkbox";
    this.enabled.addEventListener("change", () => this.update({ enabled: this.enabled.checked }));
    check.append(this.enabled, " 白黒にする");
    container.append(heading, check);
    BANDS.forEach((name, band) => {
      const row = document.createElement("label");
      row.className = "row";
      const input = document.createElement("input");
      input.type = "range";
      input.min = "-100";
      input.max = "100";
      input.title = `白黒にしたとき、${name}の部分をどれだけ明るく写すか（ダブルクリックで 0 に戻す）`;
      const output = document.createElement("output");
      input.addEventListener("input", () => this.setMix(band, Number(input.value)));
      input.addEventListener("dblclick", () => this.setMix(band, 0));
      row.append(`白黒の${name}`, input, output);
      container.append(row);
      this.sliders.push({ input, output });
    });
    const buttons = document.createElement("div");
    buttons.className = "buttons";
    this.resetButton.type = "button";
    this.resetButton.textContent = "白黒の色を元に戻す";
    this.resetButton.addEventListener("click", () => this.update({ mix: defaultMono().mix }));
    buttons.append(this.resetButton);
    container.append(buttons);
    this.show();
  }

  /** 設定の値を欄に反映する。 */
  show() {
    const { enabled, mix } = this.settings.mono;
    const loaded = this.loaded();
    this.enabled.checked = enabled;
    this.enabled.disabled = !loaded;
    this.sliders.forEach(({ input, output }, band) => {
      input.value = String(mix[band]);
      output.textContent = signedText(mix[band]);
      input.disabled = !loaded || !enabled;
    });
    this.resetButton.disabled = !loaded || !enabled || mix.every((v) => v === 0);
  }

  private setMix(band: number, value: number) {
    if (this.settings.mono.mix[band] === value) return;
    const mix = [...this.settings.mono.mix];
    mix[band] = value;
    this.update({ mix });
  }

  private update(change: Partial<EditSettings["mono"]>) {
    this.settings.mono = { ...this.settings.mono, ...change };
    this.show();
    this.onChange();
  }
}
