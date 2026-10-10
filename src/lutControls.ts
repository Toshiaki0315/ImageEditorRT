// 「加工」タブの「LUT」（.cube ファイルを選んでテイストの後にかける。旧版にはない）。
// ファイルが LUT として読めるかは、選んだときに Rust に確かめる。

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { percentText } from "./panel";
import { type EditSettings, defaultSettings } from "./types";

export class LutControls {
  private readonly name = document.createElement("span");
  private readonly choose = document.createElement("button");
  private readonly clear = document.createElement("button");
  private readonly strength = document.createElement("input");
  private readonly strengthValue = document.createElement("output");
  /** 読めなかったときに知らせる（ダイアログ） */
  onError: (error: unknown) => void = () => {};
  private readonly settings: EditSettings;
  private readonly loaded: () => boolean;
  private readonly onChange: () => void;

  /**
   * @param container 欄を作る場所（「加工」タブのテイストの下）
   * @param loaded 画像を開いているか
   * @param onChange 設定を変えたとき（履歴に積み、プレビューを描き直す）
   */
  constructor(container: HTMLElement, settings: EditSettings, loaded: () => boolean, onChange: () => void) {
    this.settings = settings;
    this.loaded = loaded;
    this.onChange = onChange;
    const row = document.createElement("div");
    row.className = "row lut-row";
    this.name.className = "lut-name";
    this.choose.type = this.clear.type = "button";
    this.choose.textContent = "選ぶ…";
    this.choose.title = "カラーグレーディング用の LUT（.cube ファイル）を選んで、テイストの後にかけます";
    this.clear.textContent = "外す";
    this.choose.addEventListener("click", () => void this.chooseFile());
    this.clear.addEventListener("click", () => this.update({ path: "" }));
    row.append("LUT", this.name, this.choose, this.clear);
    const strengthRow = document.createElement("label");
    strengthRow.className = "row";
    this.strength.type = "range";
    this.strength.min = "0";
    this.strength.max = "100";
    this.strength.title = "ダブルクリックで 100% に戻す";
    this.strength.addEventListener("input", () => this.update({ strength: Number(this.strength.value) }));
    this.strength.addEventListener("dblclick", () => this.update({ strength: defaultSettings().lut.strength }));
    strengthRow.append("LUT の強さ", this.strength, this.strengthValue);
    container.append(row, strengthRow);
    this.show();
  }

  /** 設定の値を欄に反映する（画像を開いた・元に戻した・プリセットを当てはめたときなど）。 */
  show() {
    const { path, strength } = this.settings.lut;
    const loaded = this.loaded();
    this.name.textContent = path ? (path.split("/").pop() ?? path) : "（なし）";
    this.name.title = path;
    this.strength.value = String(strength);
    this.strengthValue.textContent = percentText(strength);
    this.choose.disabled = !loaded;
    this.clear.disabled = !loaded || !path;
    this.strength.disabled = !loaded || !path;
  }

  private async chooseFile() {
    const path = await open({
      title: "LUT（.cube）を選ぶ",
      multiple: false,
      directory: false,
      filters: [{ name: "LUT（.cube）", extensions: ["cube"] }],
    });
    if (typeof path !== "string") return;
    try {
      await invoke<string>("check_lut", { path });
    } catch (error) {
      this.onError(error);
      return;
    }
    this.update({ path });
  }

  private update(change: Partial<EditSettings["lut"]>) {
    this.settings.lut = { ...this.settings.lut, ...change };
    this.show();
    this.onChange();
  }
}
