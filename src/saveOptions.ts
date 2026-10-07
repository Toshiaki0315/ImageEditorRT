// 「出力」タブの「保存の設定」（JPEG 品質・EXIF・位置情報）と、「投稿加工」タブの「位置情報を消して保存する」
// （位置情報を残すかの逆。どちらで変えても両方に反映する）。
// 保存の好みなので画像を開いても戻さず、アプリを終了しても残す（旧版 FR-UI-56）。

/** Rust の save::SaveOptions。 */
export type SaveOptions = { quality: number; keepExif: boolean; keepGps: boolean };

const DEFAULT_OPTIONS: SaveOptions = { quality: 90, keepExif: true, keepGps: false };
const STORAGE_KEY = "saveOptions";

export class SaveOptionsPanel {
  private options: SaveOptions;

  constructor(
    private readonly quality: HTMLInputElement,
    private readonly qualityValue: HTMLOutputElement,
    private readonly keepExif: HTMLInputElement,
    private readonly keepGps: HTMLInputElement,
    private readonly removeGps: HTMLInputElement,
  ) {
    this.options = load();
    quality.addEventListener("input", () => this.update({ quality: Number(quality.value) }));
    quality.addEventListener("dblclick", () => this.update({ quality: DEFAULT_OPTIONS.quality }));
    keepExif.addEventListener("change", () => this.update({ keepExif: keepExif.checked }));
    keepGps.addEventListener("change", () => this.update({ keepGps: keepGps.checked }));
    removeGps.addEventListener("change", () => this.update({ keepGps: !removeGps.checked }));
    this.show();
  }

  /** 今の保存の設定。 */
  value(): SaveOptions {
    return { ...this.options };
  }

  private update(change: Partial<SaveOptions>) {
    this.options = { ...this.options, ...change };
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.options));
    } catch {
      // 保存先が使えなくても動く
    }
    this.show();
  }

  private show() {
    this.quality.value = String(this.options.quality);
    this.qualityValue.textContent = String(this.options.quality);
    this.keepExif.checked = this.options.keepExif;
    this.keepGps.checked = this.options.keepGps;
    // 位置情報は EXIF を残すときだけ選べる（残さなければ位置情報も消える）
    this.keepGps.disabled = !this.options.keepExif;
    this.removeGps.checked = !this.options.keepExif || !this.options.keepGps;
    this.removeGps.disabled = !this.options.keepExif;
  }
}

function load(): SaveOptions {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null");
    if (saved && typeof saved === "object") {
      const quality = Number(saved.quality);
      return {
        quality: Number.isInteger(quality) && quality >= 1 && quality <= 100 ? quality : DEFAULT_OPTIONS.quality,
        keepExif: typeof saved.keepExif === "boolean" ? saved.keepExif : DEFAULT_OPTIONS.keepExif,
        keepGps: typeof saved.keepGps === "boolean" ? saved.keepGps : DEFAULT_OPTIONS.keepGps,
      };
    }
  } catch {
    // 壊れていれば既定値
  }
  return { ...DEFAULT_OPTIONS };
}
