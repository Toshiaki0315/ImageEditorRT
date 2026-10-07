// テイストの一覧（旧版にはない）: 今の写真に各テイストをかけた見本を並べ、押したテイストを選ぶ。
// 見本は Rust（pipeline::filter_thumbnails）がまとめて作る。

import { invoke } from "@tauri-apps/api/core";
import { readImages, type RawImage } from "./protocol";
import type { EditSettings, FilterType } from "./types";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class TasteGallery {
  private readonly dialog = $<HTMLDialogElement>("taste-dialog");
  private readonly grid = $<HTMLElement>("taste-grid");
  /** 開くたびに番号を振り、画像を閉じた後などに届いた古い見本は使わない */
  private sequence = 0;
  private readonly filters: [FilterType, string][];
  private readonly settings: EditSettings;
  private readonly onSelect: (filter: FilterType) => void;

  /**
   * @param filters テイストと名前（filter_types と同じ順。見本もこの順で届く）
   * @param onSelect 見本を押したとき
   */
  constructor(filters: [FilterType, string][], settings: EditSettings, onSelect: (filter: FilterType) => void) {
    this.filters = filters;
    this.settings = settings;
    this.onSelect = onSelect;
    $<HTMLButtonElement>("taste-close").addEventListener("click", () => this.dialog.close());
    this.dialog.addEventListener("keydown", (event) => {
      if (event.key === "Escape") this.dialog.close();
    });
  }

  /**
   * 今の設定で見本を作って並べ、ダイアログを開く（画像があるときだけ呼ぶ）。作っている間は「一覧…」のボタンを押せない。
   * モーダルにすると、タブを切り替えた後に開いたときに中身がアクセシビリティ（VoiceOver など）に出なくなる
   * （WebKit の動き）ので、「文字・透かし」と同じく非モーダルで開き、Esc・閉じるのボタンで閉じる。
   */
  async open(button: HTMLButtonElement) {
    const sequence = ++this.sequence;
    button.disabled = true;
    try {
      const buffer = await invoke<ArrayBuffer>("filter_thumbnails", { settings: this.settings });
      if (sequence !== this.sequence) return;
      this.grid.replaceChildren(...readImages(buffer).map((image, i) => this.item(image, ...this.filters[i])));
    } finally {
      // 途中で画像を閉じたときは押せないまま（閉じたときに close が番号を進める）
      if (sequence === this.sequence) button.disabled = false;
    }
    if (!this.dialog.open) this.dialog.show();
    this.grid.querySelector<HTMLButtonElement>('[aria-pressed="true"]')?.focus();
  }

  /** 見本 1 つ（押すとそのテイストを選んで閉じる。今のテイストは枠で示す）。 */
  private item(image: RawImage, filter: FilterType, label: string): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "taste";
    button.setAttribute("aria-pressed", String(filter === this.settings.filter));
    const canvas = document.createElement("canvas");
    canvas.width = image.width;
    canvas.height = image.height;
    canvas.getContext("2d")!.putImageData(new ImageData(image.pixels, image.width, image.height), 0, 0);
    const name = document.createElement("span");
    name.textContent = label;
    button.append(canvas, name);
    button.addEventListener("click", () => {
      this.dialog.close();
      this.onSelect(filter);
    });
    return button;
  }

  /** ダイアログを閉じ、作っている途中の見本も使わない（画像を閉じたとき）。 */
  close() {
    this.sequence++;
    this.dialog.close();
  }
}
