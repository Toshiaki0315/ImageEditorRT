// 「出力」タブの「保存の設定」（JPEG 品質・EXIF・位置情報・ファイルの大きさの上限）と、「投稿加工」タブの「位置情報を消して保存する」
// （位置情報を残すかの逆。どちらで変えても両方に反映する）。
// 保存の好みなので画像を開いても戻さず、アプリを終了しても残す（旧版 FR-UI-56）。

/** Rust の save::SaveOptions。maxKb はファイルの大きさの上限（KB、JPEG のとき。なければ null）。 */
export type SaveOptions = { quality: number; keepExif: boolean; keepGps: boolean; maxKb: number | null };

/** 画面で持つ設定（上限を外しても、入れた MB は覚えておく）。 */
type Stored = Omit<SaveOptions, "maxKb"> & { limit: boolean; limitMb: number };

const DEFAULT_OPTIONS: Stored = { quality: 90, keepExif: true, keepGps: false, limit: false, limitMb: 1 };
const LIMIT_MIN_MB = 0.05;
const LIMIT_MAX_MB = 100;
const STORAGE_KEY = "saveOptions";

export class SaveOptionsPanel {
  private options: Stored;

  constructor(
    private readonly quality: HTMLInputElement,
    private readonly qualityValue: HTMLOutputElement,
    private readonly keepExif: HTMLInputElement,
    private readonly keepGps: HTMLInputElement,
    private readonly removeGps: HTMLInputElement,
    private readonly limit: HTMLInputElement,
    private readonly limitMb: HTMLInputElement,
  ) {
    this.options = load();
    quality.addEventListener("input", () => this.update({ quality: Number(quality.value) }));
    quality.addEventListener("dblclick", () => this.update({ quality: DEFAULT_OPTIONS.quality }));
    keepExif.addEventListener("change", () => this.update({ keepExif: keepExif.checked }));
    keepGps.addEventListener("change", () => this.update({ keepGps: keepGps.checked }));
    removeGps.addEventListener("change", () => this.update({ keepGps: !removeGps.checked }));
    limit.addEventListener("change", () => this.update({ limit: limit.checked }));
    limitMb.addEventListener("change", () => {
      const mb = Number(limitMb.value);
      if (mb >= LIMIT_MIN_MB && mb <= LIMIT_MAX_MB) this.update({ limitMb: mb });
      else this.show();
    });
    this.show();
  }

  /** 今の保存の設定。 */
  value(): SaveOptions {
    const { limit, limitMb, ...rest } = this.options;
    return { ...rest, maxKb: limit ? Math.max(1, Math.round(limitMb * 1024)) : null };
  }

  private update(change: Partial<Stored>) {
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
    this.limit.checked = this.options.limit;
    this.limitMb.value = String(this.options.limitMb);
    this.limitMb.disabled = !this.options.limit;
  }
}

function load(): Stored {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null");
    if (saved && typeof saved === "object") {
      const quality = Number(saved.quality);
      return {
        quality: Number.isInteger(quality) && quality >= 1 && quality <= 100 ? quality : DEFAULT_OPTIONS.quality,
        keepExif: typeof saved.keepExif === "boolean" ? saved.keepExif : DEFAULT_OPTIONS.keepExif,
        keepGps: typeof saved.keepGps === "boolean" ? saved.keepGps : DEFAULT_OPTIONS.keepGps,
        limit: typeof saved.limit === "boolean" ? saved.limit : DEFAULT_OPTIONS.limit,
        limitMb:
          Number(saved.limitMb) >= LIMIT_MIN_MB && Number(saved.limitMb) <= LIMIT_MAX_MB
            ? Number(saved.limitMb)
            : DEFAULT_OPTIONS.limitMb,
      };
    }
  } catch {
    // 壊れていれば既定値
  }
  return { ...DEFAULT_OPTIONS };
}
