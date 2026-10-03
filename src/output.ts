// 「出力」タブのサイズ変更（幅・高さ・縦横比を保持。旧版 FR-UI-10〜12）。
// 欄に出す値と編集設定に渡す値の計算は Rust（core/output.rs）に任せる。

import { invoke } from "@tauri-apps/api/core";
import type { EditSettings } from "./types";

type Side = "width" | "height";
type SizeState = { width: number; height: number; edited: boolean; last: Side; keepAspect: boolean };
type SizeResult = { state: SizeState; base: [number, number]; width: number | null; height: number | null };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const initialState = (): SizeState => ({ width: 1, height: 1, edited: false, last: "width", keepAspect: true });

export class OutputSize {
  private state = initialState();
  private loaded = false;
  private readonly width = $<HTMLInputElement>("out-width");
  private readonly height = $<HTMLInputElement>("out-height");
  private readonly keepAspect = $<HTMLInputElement>("keep-aspect");

  /** @param onChange 欄を変えたとき（出力の大きさの表示などを更新する） */
  constructor(
    private readonly settings: EditSettings,
    private readonly onChange: () => void,
  ) {
    // 旧版の数値の欄と同じく、入力のたびに反映する
    this.width.addEventListener("input", () => this.edited("width", this.width));
    this.height.addEventListener("input", () => this.edited("height", this.height));
    this.keepAspect.addEventListener("change", () => {
      this.state.keepAspect = this.keepAspect.checked;
      this.onChange();
    });
    this.reset(false);
  }

  /** 画像を開いたとき: 手で変えていない状態（トリミング後の大きさ）・縦横比を保持に戻す。 */
  reset(loaded: boolean) {
    this.loaded = loaded;
    this.state = initialState();
    this.keepAspect.checked = true;
    this.width.disabled = this.height.disabled = this.keepAspect.disabled = !loaded;
    if (!loaded) this.width.value = this.height.value = "";
  }

  /** 範囲・フレーム・形などが変わったとき: 欄と編集設定の幅・高さを今の設定に合わせる。 */
  async refresh() {
    if (!this.loaded) return;
    const result = await invoke<SizeResult>("resolve_size", { settings: this.settings, state: this.state });
    this.state = result.state;
    this.settings.width = result.width;
    this.settings.height = result.height;
    // 入力中の欄は、値が同じなら書き換えない（カーソルの位置を保つ）
    if (Number(this.width.value) !== result.state.width) this.width.value = String(result.state.width);
    if (Number(this.height.value) !== result.state.height) this.height.value = String(result.state.height);
  }

  /** 90° 回転したとき: 手で変えた幅・高さを入れ替える。 */
  async rotate() {
    this.state = await invoke<SizeState>("rotate_size", { state: this.state });
  }

  private edited(side: Side, input: HTMLInputElement) {
    const value = Math.round(Number(input.value));
    if (!(value >= 1)) return; // 空欄・0 は入力の途中として待つ
    this.state = { ...this.state, [side]: value, edited: true, last: side };
    this.onChange();
  }
}
