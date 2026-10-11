// 「ImageEditorRT > キーボードショートカット…」（旧版にはない）: メニューの項目ごとのショートカットを変える・外す・
// 既定に戻す。覚えるのとメニューに当てるのは Rust（src-tauri/src/shortcuts.rs）。

import { invoke } from "@tauri-apps/api/core";
import { acceleratorFromKey, displayAccelerator } from "./shortcutKeys";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

/** Rust の ShortcutItem。 */
type Item = { id: string; menu: string; label: string; default: string; current: string };

export class ShortcutsDialog {
  private readonly dialog = $<HTMLDialogElement>("shortcuts-dialog");
  private readonly table = $<HTMLTableSectionElement>("shortcuts-list");
  private readonly message = $<HTMLElement>("shortcuts-message");
  /** キーを待っている項目 */
  private recording: string | null = null;

  constructor() {
    $<HTMLButtonElement>("shortcuts-reset").addEventListener("click", () => void this.call("reset_shortcuts", {}));
    $<HTMLButtonElement>("shortcuts-close").addEventListener("click", () => this.dialog.close());
    // 「変える」の後に押したキーを、その項目のショートカットにする（Esc でやめる）
    this.dialog.addEventListener(
      "keydown",
      (event) => {
        if (this.recording === null) return;
        event.preventDefault();
        event.stopPropagation();
        if (event.key === "Escape") {
          this.recording = null;
          this.message.textContent = "";
          void this.load();
          return;
        }
        const accelerator = acceleratorFromKey(event);
        if (!accelerator) return; // 修飾キーだけのときは、続けて押すのを待つ
        const id = this.recording;
        this.recording = null;
        void this.call("set_shortcut", { id, accelerator });
      },
      { capture: true },
    );
  }

  async open() {
    this.message.textContent = "";
    this.recording = null;
    await this.load();
    if (!this.dialog.open) this.dialog.showModal();
  }

  private async load() {
    this.render(await invoke<Item[]>("shortcut_list"));
  }

  /** Rust に変えさせて、一覧を作り直す（重なっているなどのときは理由を出す）。 */
  private async call(command: string, args: Record<string, unknown>) {
    try {
      this.render(await invoke<Item[]>(command, args));
      this.message.textContent = "";
    } catch (error) {
      this.message.textContent = String(error);
      await this.load();
    }
  }

  private render(items: Item[]) {
    const rows = items.map((item) => {
      const row = document.createElement("tr");
      const name = document.createElement("td");
      name.textContent = `${item.menu} > ${item.label}`;
      const key = document.createElement("td");
      key.className = "shortcut-key";
      key.textContent = this.recording === item.id ? "キーを押してください…（Esc でやめる）" : displayAccelerator(item.current);
      if (item.current !== item.default) key.classList.add("changed");
      const actions = document.createElement("td");
      actions.className = "buttons";
      const change = this.button("変える", () => {
        this.recording = item.id;
        this.message.textContent = "";
        this.render(items);
      });
      const remove = this.button("外す", () => void this.call("set_shortcut", { id: item.id, accelerator: "" }));
      remove.disabled = !item.current;
      const reset = this.button("既定", () => void this.call("set_shortcut", { id: item.id, accelerator: item.default }));
      reset.disabled = item.current === item.default;
      reset.title = `既定: ${displayAccelerator(item.default)}`;
      actions.append(change, remove, reset);
      row.append(name, key, actions);
      return row;
    });
    this.table.replaceChildren(...rows);
  }

  private button(label: string, onClick: () => void): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.addEventListener("click", onClick);
    return button;
  }
}
