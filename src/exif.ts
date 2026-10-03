// 「EXIF」タブ（旧版 FR-UI-63）: グループごとに折りたためる一覧、選んだ行のコピー、撮影地をマップで開く。

import { invoke } from "@tauri-apps/api/core";
import type { ExifEntry, ExifInfo } from "./types";

const NO_EXIF_TEXT = "この画像には EXIF がありません";
/** 最初は閉じておくグループ（項目が多く、あまり見ないもの） */
const COLLAPSED_GROUPS = new Set(["互換性", "サムネイル"]);

type Row = { element: HTMLElement; entry: ExifEntry | null; group: string };

export class ExifView {
  private info: ExifInfo | null = null;
  private rows: Row[] = [];
  /** shift で範囲を選ぶときの起点 */
  private anchor = -1;
  private readonly summary = document.createElement("p");
  private readonly list = document.createElement("div");
  private readonly mapButton = document.createElement("button");
  private readonly menu = document.createElement("div");

  constructor(page: HTMLElement) {
    this.summary.className = "exif-summary";
    this.list.className = "exif-list";
    this.list.tabIndex = 0;
    this.list.setAttribute("role", "tree");
    this.mapButton.type = "button";
    this.mapButton.textContent = "マップで開く";
    this.mapButton.title = "撮影した場所を macOS のマップアプリで開きます";
    this.mapButton.addEventListener("click", () => void this.openMap());
    const buttons = document.createElement("div");
    buttons.className = "buttons exif-buttons";
    buttons.append(this.mapButton);
    // 右クリックの「コピー」
    this.menu.className = "context-menu";
    this.menu.hidden = true;
    const copy = document.createElement("button");
    copy.type = "button";
    copy.textContent = "コピー";
    copy.addEventListener("click", () => {
      this.menu.hidden = true;
      void this.copySelected();
    });
    this.menu.append(copy);
    page.replaceChildren(this.summary, this.list, buttons, this.menu);

    this.list.addEventListener("keydown", (event) => {
      if (event.metaKey && event.key.toLowerCase() === "c") {
        event.preventDefault();
        void this.copySelected();
      }
    });
    this.list.addEventListener("contextmenu", (event) => {
      if (!this.rows.some((r) => this.isSelected(r))) return;
      event.preventDefault();
      const box = page.getBoundingClientRect();
      this.menu.style.left = `${event.clientX - box.left}px`;
      this.menu.style.top = `${event.clientY - box.top + page.scrollTop}px`;
      this.menu.hidden = false;
    });
    document.addEventListener("click", (event) => {
      if (!this.menu.contains(event.target as Node)) this.menu.hidden = true;
    });
    this.show(null);
  }

  /** 表示する EXIF を設定する。null・空なら一覧を消す。 */
  show(info: ExifInfo | null) {
    this.info = info && !info.empty ? info : null;
    this.list.replaceChildren();
    this.rows = [];
    this.anchor = -1;
    if (!this.info) {
      this.summary.textContent = NO_EXIF_TEXT;
      this.mapButton.disabled = true;
      return;
    }
    const groups = new Map<string, ExifEntry[]>();
    for (const entry of this.info.entries) {
      if (!groups.has(entry.group)) groups.set(entry.group, []);
      groups.get(entry.group)!.push(entry);
    }
    for (const [group, entries] of groups) {
      let title = group;
      if (group === "MakerNote" && this.info.makerNote) title = `${title}（${this.info.makerNote}）`;
      const header = document.createElement("div");
      header.className = "exif-group";
      header.setAttribute("aria-expanded", String(!COLLAPSED_GROUPS.has(group)));
      const toggle = document.createElement("span");
      toggle.className = "toggle";
      toggle.addEventListener("click", (event) => {
        event.stopPropagation();
        const expanded = header.getAttribute("aria-expanded") !== "true";
        header.setAttribute("aria-expanded", String(expanded));
        for (const row of this.rows) if (row.entry && row.group === group) row.element.hidden = !expanded;
      });
      header.append(toggle, `${title}  ${entries.length} 項目`);
      this.addRow({ element: header, entry: null, group });
      for (const entry of entries) {
        const row = document.createElement("div");
        row.className = "exif-row";
        row.hidden = COLLAPSED_GROUPS.has(group);
        const label = document.createElement("span");
        label.textContent = entry.label;
        label.title = entry.label;
        const value = document.createElement("span");
        value.textContent = entry.value;
        value.title = entry.value;
        row.append(label, value);
        this.addRow({ element: row, entry, group });
      }
    }
    const maker = this.info.makerNote ? `・MakerNote: ${this.info.makerNote}` : "";
    this.summary.textContent = `${this.info.entries.length} 項目${maker}`;
    this.mapButton.disabled = this.info.gps === null;
  }

  /** 選んだ行を「項目: 値」の行にした文字列（グループを選んだら、その中の全項目）。 */
  selectedText(): string {
    const lines: string[] = [];
    const wholeGroups = new Set(this.rows.filter((r) => !r.entry && this.isSelected(r)).map((r) => r.group));
    for (const row of this.rows) {
      if (row.entry && (wholeGroups.has(row.group) || this.isSelected(row))) {
        lines.push(`${row.entry.label}: ${row.entry.value}`);
      }
    }
    return lines.join("\n");
  }

  private addRow(row: Row) {
    const index = this.rows.length;
    row.element.setAttribute("role", "treeitem");
    row.element.addEventListener("mousedown", (event) => {
      if (event.button === 2 && this.isSelected(row)) return; // 選んだ行の上の右クリックは選択を保つ
      this.select(index, event);
    });
    this.rows.push(row);
    this.list.append(row.element);
  }

  /** クリックで 1 行、⌘ で追加・解除、shift で範囲を選ぶ。 */
  private select(index: number, event: MouseEvent) {
    if (event.shiftKey && this.anchor >= 0) {
      const [from, to] = [Math.min(this.anchor, index), Math.max(this.anchor, index)];
      this.rows.forEach((r, i) => this.setSelected(r, i >= from && i <= to && !r.element.hidden));
    } else if (event.metaKey) {
      this.setSelected(this.rows[index], !this.isSelected(this.rows[index]));
      this.anchor = index;
    } else {
      this.rows.forEach((r, i) => this.setSelected(r, i === index));
      this.anchor = index;
    }
    this.list.focus();
  }

  private isSelected(row: Row): boolean {
    return row.element.getAttribute("aria-selected") === "true";
  }

  private setSelected(row: Row, selected: boolean) {
    row.element.setAttribute("aria-selected", String(selected));
  }

  private async copySelected() {
    const text = this.selectedText();
    if (text) await navigator.clipboard.writeText(text);
  }

  private async openMap() {
    const gps = this.info?.gps;
    if (gps) await invoke("open_map", { latitude: gps.latitude, longitude: gps.longitude });
  }
}
