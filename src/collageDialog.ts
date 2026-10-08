// 「並べて 1 枚に」（コラージュ。旧版にはない）のダイアログ。並べる処理は Rust（core/collage.rs）が行う。

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

type Layout = string;
/** Rust の collage::CollageOptions。 */
export type CollageOptions = {
  layout: Layout;
  aspect: [number, number];
  longSide: number;
  gap: number;
  background: [number, number, number];
};
export type CollageRequest = { paths: string[]; options: CollageOptions };

const LONG_SIDE_DEFAULT = 2048;
const LONG_SIDE_MIN = 200;
const LONG_SIDE_MAX = 8000;
const GAP_DEFAULT = 1;
const BACKGROUNDS: Record<string, [number, number, number]> = { white: [255, 255, 255], black: [0, 0, 0] };

export class CollageDialog {
  private readonly dialog = $<HTMLDialogElement>("collage-dialog");
  private readonly files = $<HTMLSelectElement>("collage-files");
  private readonly layout = $<HTMLSelectElement>("collage-layout");
  private readonly aspect = $<HTMLSelectElement>("collage-aspect");
  private readonly longSide = $<HTMLInputElement>("collage-long-side");
  private readonly gap = $<HTMLInputElement>("collage-gap");
  private readonly gapValue = $<HTMLOutputElement>("collage-gap-value");
  private readonly background = $<HTMLSelectElement>("collage-background");
  private readonly note = $<HTMLElement>("collage-note");
  private readonly make = $<HTMLButtonElement>("collage-make");
  /** 並べ方ごとの枚数 */
  private readonly counts = new Map<Layout, number>();
  private readonly extensions: string[];
  private opened = false;

  constructor(extensions: string[]) {
    this.extensions = extensions;
    $("collage-add").addEventListener("click", () => void this.chooseFiles());
    $("collage-remove").addEventListener("click", () => {
      for (const option of [...this.files.selectedOptions]) option.remove();
      this.update();
    });
    $("collage-up").addEventListener("click", () => this.move(-1));
    $("collage-down").addEventListener("click", () => this.move(1));
    this.layout.addEventListener("change", () => this.update());
    this.longSide.addEventListener("input", () => this.update());
    this.gap.addEventListener("input", () => this.update());
    void this.loadChoices();
  }

  private async loadChoices() {
    const [layouts, aspects] = await invoke<[[Layout, string, number][], [number, number][]]>("collage_choices");
    for (const [value, label, count] of layouts) {
      this.layout.add(new Option(label, value));
      this.counts.set(value, count);
    }
    for (const [w, h] of aspects) this.aspect.add(new Option(`${w}:${h}`, `${w}:${h}`));
  }

  /** ダイアログを開き、「作る」なら並べる写真と設定を返す（キャンセルなら null）。 */
  async run(): Promise<CollageRequest | null> {
    this.files.replaceChildren();
    // 長辺・すき間は、前に開いたときの値を残す（初めては既定値）
    if (!this.opened) {
      this.longSide.value = String(LONG_SIDE_DEFAULT);
      this.gap.value = String(GAP_DEFAULT);
      this.opened = true;
    }
    this.update();
    this.dialog.returnValue = "";
    this.dialog.showModal();
    await new Promise((resolve) => this.dialog.addEventListener("close", resolve, { once: true }));
    if (this.dialog.returnValue !== "make" || this.make.disabled) return null;
    const [w, h] = this.aspect.value.split(":").map(Number);
    return {
      paths: [...this.files.options].map((o) => o.value),
      options: {
        layout: this.layout.value,
        aspect: [w, h],
        longSide: Number(this.longSide.value),
        gap: Number(this.gap.value),
        background: BACKGROUNDS[this.background.value],
      },
    };
  }

  private async chooseFiles() {
    const paths = await open({
      title: "並べる写真を追加",
      multiple: true,
      filters: [{ name: "画像ファイル", extensions: this.extensions }],
    });
    for (const path of paths ?? []) this.files.add(new Option(path.split("/").pop() ?? path, path));
    this.update();
  }

  /** 選んだ写真を 1 つ上・下へ。 */
  private move(step: number) {
    const option = this.files.selectedOptions[0];
    if (!option) return;
    const index = option.index + step;
    if (index < 0 || index >= this.files.options.length) return;
    this.files.remove(option.index);
    this.files.add(option, index);
    option.selected = true;
  }

  /** 枚数・長辺が合っていれば「作る」を押せる。足りない枚数を知らせる。 */
  private update() {
    this.gapValue.textContent = `${this.gap.value}%`;
    const needed = this.counts.get(this.layout.value) ?? 2;
    const count = this.files.options.length;
    const side = Number(this.longSide.value);
    const sideOk = Number.isInteger(side) && side >= LONG_SIDE_MIN && side <= LONG_SIDE_MAX;
    this.note.textContent =
      count < needed
        ? `この並べ方では ${needed} 枚使います（あと ${needed - count} 枚）。`
        : count > needed
          ? `この並べ方では ${needed} 枚使います（上から ${needed} 枚だけを並べます）。`
          : "写真はそれぞれの枠に合わせて、中央を切り抜きます。できた画像はそのまま加工・保存できます。";
    this.make.disabled = count < needed || !sideOk;
  }
}
