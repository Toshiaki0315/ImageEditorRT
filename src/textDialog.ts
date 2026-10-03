// 「文字・透かし」のダイアログ（旧版 FR-UI-47）。開いたまま調整でき、変更はすぐ設定とプレビューに反映する。

import type { EditSettings, TextFont, TextPosition } from "./types";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const toHex = ([r, g, b]: [number, number, number]) =>
  `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
const fromHex = (hex: string): [number, number, number] => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as [number, number, number];

export class TextDialog {
  private readonly dialog = $<HTMLDialogElement>("text-dialog");
  private readonly text = $<HTMLTextAreaElement>("text-input");
  private readonly font = $<HTMLSelectElement>("text-font");
  private readonly size = $<HTMLInputElement>("text-size");
  private readonly color = $<HTMLInputElement>("text-color");
  private readonly opacity = $<HTMLInputElement>("text-opacity");
  private readonly opacityValue = $<HTMLOutputElement>("text-opacity-value");
  private readonly position = $<HTMLSelectElement>("text-position");

  constructor(
    private readonly settings: EditSettings,
    fonts: [TextFont, string][],
    positions: [TextPosition, string][],
    private readonly onChange: () => void,
  ) {
    for (const [value, label] of fonts) this.font.add(new Option(label, value));
    for (const [value, label] of positions) this.position.add(new Option(label, value));
    this.text.addEventListener("input", () => this.update({ text: this.text.value }));
    this.font.addEventListener("change", () => this.update({ font: this.font.value as TextFont }));
    this.size.addEventListener("input", () => {
      const size = Number(this.size.value);
      if (size >= 1 && size <= 30) this.update({ size });
    });
    this.color.addEventListener("input", () => this.update({ color: fromHex(this.color.value) }));
    this.opacity.addEventListener("input", () => this.update({ opacity: Number(this.opacity.value) }));
    this.position.addEventListener("change", () => this.update({ position: this.position.value as TextPosition }));
    $<HTMLButtonElement>("text-clear").addEventListener("click", () => this.update({ text: "" }));
    $<HTMLButtonElement>("text-close").addEventListener("click", () => this.dialog.close());
    this.show();
  }

  /** ダイアログを開く（画像がないときは開けない）。 */
  open() {
    this.show();
    if (!this.dialog.open) this.dialog.show();
    this.text.focus();
  }

  close() {
    this.dialog.close();
  }

  /** 設定の値をダイアログに反映する（画像を開いたときなど）。 */
  show() {
    const t = this.settings.text;
    if (this.text.value !== t.text) this.text.value = t.text;
    this.font.value = t.font;
    if (Number(this.size.value) !== t.size) this.size.value = String(t.size);
    this.color.value = toHex(t.color);
    this.opacity.value = String(t.opacity);
    this.opacityValue.textContent = `${t.opacity}%`;
    this.position.value = t.position;
  }

  private update(change: Partial<EditSettings["text"]>) {
    this.settings.text = { ...this.settings.text, ...change };
    this.show();
    this.onChange();
  }
}
