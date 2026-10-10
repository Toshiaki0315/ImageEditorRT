// 「切り抜き」タブの「水平の補正」（自動を含む）と「背景」（消す・ぼかす）の欄（旧版にはない）。
// 回転・反転・トリミングは crop.ts。傾きの求め方・被写体の準備（Vision）は assist.ts がつなぐ。

import type { BackgroundMode, EditSettings } from "./types";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const BACKGROUND_BLUR_DEFAULT = 50;

/** 角度を「+1.5°」「-0.3°」「0.0°」のように表示する。 */
export const degreesText = (degrees: number) => `${degrees > 0 ? "+" : ""}${degrees.toFixed(1)}°`;

/** 角度を 0.1° 刻みに丸める（小数の誤差を残さない）。 */
export const roundDegrees = (value: number) => Number((Math.round(value * 10) / 10).toFixed(1));

export class PhotoControls {
  private readonly straighten = $<HTMLInputElement>("straighten");
  private readonly straightenValue = $<HTMLOutputElement>("straighten-value");
  private readonly autoStraighten = $<HTMLButtonElement>("auto-straighten");
  private readonly background = $<HTMLSelectElement>("background");
  private readonly backgroundBlur = $<HTMLInputElement>("background-blur");
  private readonly backgroundBlurValue = $<HTMLOutputElement>("background-blur-value");
  /** 傾きを求める（水平の補正の角度。分からなければ null、求められなければ undefined） */
  findTilt: () => Promise<number | null | undefined> = async () => null;
  /** 傾きの自動補正の結果を知らせる（直した角度。分からなければ null） */
  onAutoStraighten: (degrees: number | null) => void = () => {};
  /** 背景を消す準備（被写体のマスクを作る）。被写体があれば true、なければ false、準備できなければ null */
  prepareBackground: () => Promise<boolean | null> = async () => true;
  /** 被写体が見つからなかったとき */
  onNoSubject: () => void = () => {};
  /** 水平の補正を動かしたとき（格子を出す。pressed はスライダーを押している間） */
  onStraightenAdjust: (pressed: boolean) => void = () => {};
  private readonly settings: EditSettings;
  private readonly loaded: () => boolean;
  private readonly onChange: () => void;

  /**
   * @param loaded 画像を開いているか
   * @param onChange 設定を変えたとき（履歴に積み、プレビューを描き直す）
   */
  constructor(settings: EditSettings, loaded: () => boolean, onChange: () => void) {
    this.settings = settings;
    this.loaded = loaded;
    this.onChange = onChange;
    this.straighten.addEventListener("input", () => this.setStraighten(Number(this.straighten.value)));
    this.straighten.addEventListener("dblclick", () => this.setStraighten(0));
    // 動かしている間（押している間と、離してから少しの間）は格子を出す
    this.straighten.addEventListener("pointerdown", () => this.onStraightenAdjust(true));
    this.straighten.addEventListener("input", () => this.onStraightenAdjust(true));
    for (const type of ["pointerup", "pointercancel", "change"]) {
      this.straighten.addEventListener(type, () => this.onStraightenAdjust(false));
    }
    this.autoStraighten.addEventListener("click", () => void this.straightenAutomatically());
    this.background.addEventListener("change", () => void this.setBackground(this.background.value as BackgroundMode));
    this.backgroundBlur.addEventListener("input", () => this.setBackgroundBlur(Number(this.backgroundBlur.value)));
    this.backgroundBlur.addEventListener("dblclick", () => this.setBackgroundBlur(BACKGROUND_BLUR_DEFAULT));
    this.show();
  }

  /** 設定の値を欄に反映する（画像を開いた・元に戻した・回転・反転したときなど）。 */
  show() {
    const loaded = this.loaded();
    this.straighten.value = String(this.settings.straighten);
    this.straightenValue.textContent = degreesText(this.settings.straighten);
    this.straighten.disabled = this.autoStraighten.disabled = !loaded;
    this.background.value = this.settings.background;
    this.background.disabled = !loaded;
    this.backgroundBlur.value = String(this.settings.backgroundBlur);
    this.backgroundBlurValue.textContent = String(this.settings.backgroundBlur);
    this.backgroundBlur.disabled = !loaded || this.settings.background !== "blur";
  }

  /** 水平の補正（0.1° 刻み。大きさは変わらないので、トリミング範囲はそのまま）。 */
  private setStraighten(value: number) {
    const degrees = roundDegrees(value);
    if (this.settings.straighten === degrees) return;
    this.settings.straighten = degrees;
    this.changed();
  }

  /** 傾きを求めて、水平の補正に入れる（元に戻せる。スライダーで微調整できる）。 */
  private async straightenAutomatically() {
    this.autoStraighten.disabled = true;
    try {
      const degrees = await this.findTilt();
      if (degrees === undefined) return;
      if (degrees !== null) {
        this.setStraighten(degrees);
        this.onStraightenAdjust(false); // 直した結果を格子で見られるよう、少しの間出す
      }
      this.onAutoStraighten(degrees);
    } finally {
      this.show();
    }
  }

  /** 背景の扱いを変える。消す・ぼかすときは先に被写体のマスクを作り、被写体がなければ「そのまま」に戻す。 */
  private async setBackground(mode: BackgroundMode) {
    if (mode !== "keep") {
      this.background.disabled = true;
      try {
        const found = await this.prepareBackground();
        if (found !== true) {
          if (found === false) this.onNoSubject();
          return;
        }
      } finally {
        this.show();
      }
    }
    this.settings.background = mode;
    this.changed();
  }

  /** 背景のぼかしの強さ（背景を「ぼかす」とき）。 */
  private setBackgroundBlur(value: number) {
    if (this.settings.backgroundBlur === value) return;
    this.settings.backgroundBlur = value;
    this.changed();
  }

  private changed() {
    this.show();
    this.onChange();
  }
}
