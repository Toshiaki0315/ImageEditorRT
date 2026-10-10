// 「加工」タブの「色を合わせる」（参考の写真の明るさ・色の傾きに近づける。旧版にはない）。
// 参考の写真は選んだときに Rust で色の情報を測り、その数値だけを設定に入れる（ファイルは覚えない）。

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { percentText } from "./panel";
import { type ColorMatch, type ColorStats, type EditSettings, defaultSettings } from "./types";

export class ColorMatchControls {
  private readonly state = document.createElement("span");
  private readonly choose = document.createElement("button");
  private readonly clear = document.createElement("button");
  private readonly strength = document.createElement("input");
  private readonly strengthValue = document.createElement("output");
  /** 選べる画像の拡張子 */
  extensions: () => string[] = () => [];
  /** 読めなかったときに知らせる（ダイアログ） */
  onError: (error: unknown) => void = () => {};
  /** 参考の写真の名前（その場の表示だけ。設定には入れない） */
  private chosenName = "";
  private readonly settings: EditSettings;
  private readonly loaded: () => boolean;
  private readonly onChange: () => void;

  /**
   * @param container 欄を作る場所（「加工」タブの LUT の下）
   * @param loaded 画像を開いているか
   * @param onChange 設定を変えたとき（履歴に積み、プレビューを描き直す）
   */
  constructor(container: HTMLElement, settings: EditSettings, loaded: () => boolean, onChange: () => void) {
    this.settings = settings;
    this.loaded = loaded;
    this.onChange = onChange;
    const row = document.createElement("div");
    row.className = "row lut-row";
    this.state.className = "lut-name";
    this.choose.type = this.clear.type = "button";
    this.choose.textContent = "選ぶ…";
    this.choose.title = "参考にしたい写真を選ぶと、明るさ・色の傾きをその写真に近づけます（写真の色の情報だけを覚えます）";
    this.clear.textContent = "外す";
    this.choose.addEventListener("click", () => void this.chooseFile());
    this.clear.addEventListener("click", () => this.update({ reference: null }));
    row.append("色を合わせる", this.state, this.choose, this.clear);
    const strengthRow = document.createElement("label");
    strengthRow.className = "row";
    this.strength.type = "range";
    this.strength.min = "0";
    this.strength.max = "100";
    this.strength.title = "ダブルクリックで 100% に戻す";
    this.strength.addEventListener("input", () => this.update({ strength: Number(this.strength.value) }));
    this.strength.addEventListener("dblclick", () => this.update({ strength: defaultSettings().colorMatch.strength }));
    strengthRow.append("合わせる強さ", this.strength, this.strengthValue);
    container.append(row, strengthRow);
    this.show();
  }

  /** 設定の値を欄に反映する（画像を開いた・元に戻した・プリセットを当てはめたときなど）。 */
  show() {
    const { reference, strength } = this.settings.colorMatch;
    const loaded = this.loaded();
    if (!reference) this.chosenName = "";
    this.state.textContent = reference ? this.chosenName || "参考の色あり" : "（なし）";
    this.state.title = reference ? referenceSummary(reference) : "";
    this.strength.value = String(strength);
    this.strengthValue.textContent = percentText(strength);
    this.choose.disabled = !loaded;
    this.clear.disabled = !loaded || !reference;
    this.strength.disabled = !loaded || !reference;
  }

  private async chooseFile() {
    const path = await open({
      title: "色を合わせる参考の写真を選ぶ",
      multiple: false,
      directory: false,
      filters: [{ name: "画像ファイル", extensions: this.extensions() }],
    });
    if (typeof path !== "string") return;
    let reference: ColorStats;
    try {
      reference = await invoke<ColorStats>("measure_reference", { path });
    } catch (error) {
      this.onError(error);
      return;
    }
    this.chosenName = path.split("/").pop() ?? "";
    this.update({ reference });
  }

  private update(change: Partial<ColorMatch>) {
    this.settings.colorMatch = { ...this.settings.colorMatch, ...change };
    this.show();
    this.onChange();
  }
}

/** 参考の色の情報の説明（明るさの平均と色の傾き）。 */
function referenceSummary({ mean }: ColorStats): string {
  const [l, a, b] = mean.map((v) => Math.round(v));
  return `参考の色: 明るさ ${l}・緑〜赤 ${a}・青〜黄 ${b}（Lab の平均）`;
}
