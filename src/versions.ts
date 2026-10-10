// 加工の途中の版（旧版にはない）: 今の加工（切り抜き・大きさを含む）に名前を付けて残し、一覧から選んで
// 当てはめて見比べる。版は写真を開いている間だけ覚える（別の写真を開くと消える）。

import type { Snapshot } from "./app";

/** 残した版。 */
export type Version = { name: string; snapshot: Snapshot };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class Versions {
  private list: Version[] = [];
  /** 次に付ける番号（「版 1」「版 2」…。消しても同じ番号は使わない） */
  private next = 1;
  private readonly dialog = $<HTMLDialogElement>("versions-dialog");
  private readonly items = $<HTMLSelectElement>("versions-list");
  private readonly name = $<HTMLInputElement>("version-name");
  private readonly applyButton = $<HTMLButtonElement>("version-apply");
  private readonly renameButton = $<HTMLButtonElement>("version-rename");
  private readonly deleteButton = $<HTMLButtonElement>("version-delete");
  private readonly keepButton = $<HTMLButtonElement>("version-keep");
  /** 今の状態を取る・当てはめる（app の履歴と同じ形）・知らせる */
  private readonly take: () => Snapshot;
  private readonly apply: (snapshot: Snapshot) => void;
  private readonly notify: (message: string) => void;

  constructor(take: () => Snapshot, apply: (snapshot: Snapshot) => void, notify: (message: string) => void) {
    this.take = take;
    this.apply = apply;
    this.notify = notify;
    this.keepButton.addEventListener("click", () => this.keep());
    this.applyButton.addEventListener("click", () => this.applySelected());
    // 一覧で選ぶとすぐ当てはめる（選び直して見比べる）
    this.items.addEventListener("change", () => {
      this.applySelected();
      this.show();
    });
    this.renameButton.addEventListener("click", () => this.rename());
    this.name.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        this.rename();
      }
    });
    this.deleteButton.addEventListener("click", () => this.remove());
    $<HTMLButtonElement>("versions-close").addEventListener("click", () => this.dialog.close());
    this.show();
  }

  /** 残している版の名前（テスト・表示用）。 */
  names(): string[] {
    return this.list.map((v) => v.name);
  }

  /** 今の加工を版として残す。 */
  keep() {
    const name = `版 ${this.next}`;
    this.next += 1;
    this.list.push({ name, snapshot: structuredClone(this.take()) });
    this.items.value = String(this.list.length - 1);
    this.show();
    this.notify(`今の加工を「${name}」として残しました（「編集 > 版の一覧…」で見比べられます）`);
  }

  /** 一覧を出す（プレビューを見ながら選べるよう、ほかの操作もできるまま出す）。 */
  open() {
    this.show();
    if (!this.dialog.open) this.dialog.show();
  }

  /** 別の写真を開いた・閉じたとき: 版を消し、一覧を閉じる。 */
  reset() {
    this.list = [];
    this.next = 1;
    this.dialog.close();
    this.show();
  }

  private selected(): number | null {
    const index = Number(this.items.value);
    return this.items.value !== "" && index >= 0 && index < this.list.length ? index : null;
  }

  private applySelected() {
    const index = this.selected();
    if (index === null) return;
    this.apply(structuredClone(this.list[index].snapshot));
    this.notify(`「${this.list[index].name}」を当てはめました（⌘Z で戻せます）`);
  }

  private rename() {
    const index = this.selected();
    const name = this.name.value.trim();
    if (index === null || !name) return;
    this.list[index].name = name;
    this.show();
  }

  private remove() {
    const index = this.selected();
    if (index === null) return;
    this.list.splice(index, 1);
    this.items.value = this.list.length ? String(Math.min(index, this.list.length - 1)) : "";
    this.show();
  }

  private show() {
    const chosen = this.items.value;
    this.items.replaceChildren(...this.list.map((v, i) => new Option(v.name, String(i))));
    this.items.value = chosen;
    const index = this.selected();
    this.name.value = index === null ? "" : this.list[index].name;
    this.name.disabled = this.applyButton.disabled = this.renameButton.disabled = this.deleteButton.disabled = index === null;
  }
}
