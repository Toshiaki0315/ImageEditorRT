// 「文字・透かし」のダイアログ（旧版 FR-UI-47）。開いたまま調整でき、変更はすぐ設定とプレビューに反映する。

import { open } from "@tauri-apps/plugin-dialog";
import type { EditSettings, LogoSettings, TextEffect, TextFont, TextPosition } from "./types";
import { fromHex, toHex } from "./saveOptions";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;


export class TextDialog {
  private readonly dialog = $<HTMLDialogElement>("text-dialog");
  private readonly text = $<HTMLTextAreaElement>("text-input");
  private readonly font = $<HTMLSelectElement>("text-font");
  private readonly size = $<HTMLInputElement>("text-size");
  private readonly color = $<HTMLInputElement>("text-color");
  private readonly opacity = $<HTMLInputElement>("text-opacity");
  private readonly opacityValue = $<HTMLOutputElement>("text-opacity-value");
  private readonly position = $<HTMLSelectElement>("text-position");
  private readonly effect = $<HTMLSelectElement>("text-effect");
  private readonly logoName = $<HTMLElement>("logo-name");
  private readonly logoClear = $<HTMLButtonElement>("logo-clear");
  private readonly logoSize = $<HTMLInputElement>("logo-size");
  private readonly logoSizeValue = $<HTMLOutputElement>("logo-size-value");
  private readonly logoOpacity = $<HTMLInputElement>("logo-opacity");
  private readonly logoOpacityValue = $<HTMLOutputElement>("logo-opacity-value");
  private readonly logoPosition = $<HTMLSelectElement>("logo-position");
  /** ロゴに選べる画像の拡張子 */
  logoExtensions: string[] = ["png", "jpg", "jpeg", "gif", "tif", "tiff", "bmp", "heic"];

  constructor(
    private readonly settings: EditSettings,
    fonts: [TextFont, string][],
    positions: [TextPosition, string][],
    private readonly onChange: () => void,
  ) {
    for (const [value, label] of fonts) this.font.add(new Option(label, value));
    for (const [value, label] of positions) {
      this.position.add(new Option(label, value));
      this.logoPosition.add(new Option(label, value));
    }
    $<HTMLButtonElement>("logo-choose").addEventListener("click", () => void this.chooseLogo());
    this.logoClear.addEventListener("click", () => this.updateLogo({ path: "" }));
    this.logoSize.addEventListener("input", () => this.updateLogo({ size: Number(this.logoSize.value) }));
    this.logoOpacity.addEventListener("input", () => this.updateLogo({ opacity: Number(this.logoOpacity.value) }));
    this.logoPosition.addEventListener("change", () => this.updateLogo({ position: this.logoPosition.value as TextPosition }));
    this.text.addEventListener("input", () => this.update({ text: this.text.value }));
    this.font.addEventListener("change", () => this.update({ font: this.font.value as TextFont }));
    this.size.addEventListener("input", () => {
      const size = Number(this.size.value);
      if (size >= 1 && size <= 30) this.update({ size });
    });
    this.color.addEventListener("input", () => this.update({ color: fromHex(this.color.value) }));
    this.opacity.addEventListener("input", () => this.update({ opacity: Number(this.opacity.value) }));
    this.position.addEventListener("change", () => this.update({ position: this.position.value as TextPosition }));
    this.effect.addEventListener("change", () => this.update({ effect: this.effect.value as TextEffect }));
    $<HTMLButtonElement>("text-clear").addEventListener("click", () => this.update({ text: "" }));
    $<HTMLButtonElement>("insert-date").addEventListener("click", () => this.insert("{日付}"));
    $<HTMLButtonElement>("insert-date-time").addEventListener("click", () => this.insert("{日時}"));
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
    this.effect.value = t.effect ?? "none";
    const logo = this.settings.logo;
    this.logoName.textContent = logo.path ? (logo.path.split("/").pop() ?? logo.path) : "（なし）";
    this.logoName.title = logo.path;
    this.logoClear.disabled = !logo.path;
    this.logoSize.value = String(logo.size);
    this.logoSizeValue.textContent = `${logo.size}%`;
    this.logoOpacity.value = String(logo.opacity);
    this.logoOpacityValue.textContent = `${logo.opacity}%`;
    this.logoPosition.value = logo.position;
  }

  private async chooseLogo() {
    const path = await open({
      title: "ロゴの画像を選ぶ",
      multiple: false,
      filters: [{ name: "画像ファイル", extensions: this.logoExtensions }],
    });
    if (typeof path === "string") this.updateLogo({ path });
  }

  private updateLogo(change: Partial<LogoSettings>) {
    this.settings.logo = { ...this.settings.logo, ...change };
    this.show();
    this.onChange();
  }

  /** 文字の欄のカーソルの位置（選んでいればその部分）に、差し込みを入れる。 */
  private insert(placeholder: string) {
    const { selectionStart: start, selectionEnd: end, value } = this.text;
    const text = value.slice(0, start) + placeholder + value.slice(end);
    this.update({ text });
    this.text.focus();
    this.text.setSelectionRange(start + placeholder.length, start + placeholder.length);
  }

  private update(change: Partial<EditSettings["text"]>) {
    this.settings.text = { ...this.settings.text, ...change };
    this.show();
    this.onChange();
  }
}
