// 設定パネルのタブ。最後に開いていたタブを覚えておき、次に起動したときも開く（旧版 FR-UI-61）。

export type TabName = "adjust" | "crop" | "output" | "diorama" | "exif";

const STORAGE_KEY = "lastTab";
const DEFAULT_TAB: TabName = "adjust";

export class Tabs {
  private readonly buttons: HTMLButtonElement[];
  private remembered: TabName;

  constructor(root: HTMLElement) {
    this.buttons = [...root.querySelectorAll<HTMLButtonElement>(".tab")];
    for (const button of this.buttons) {
      button.addEventListener("click", () => {
        this.remember(button.dataset.tab as TabName);
        this.select(button.dataset.tab as TabName);
      });
    }
    this.remembered = this.load();
    this.select(this.remembered);
  }

  /** タブを選べるかを変える。開いているタブが選べなくなったら「加工」を開く。 */
  setEnabled(name: TabName, enabled: boolean) {
    const button = this.button(name);
    button.disabled = !enabled;
    if (enabled && name === this.remembered) {
      this.select(name);
    } else if (!enabled && button.getAttribute("aria-selected") === "true") {
      this.select(DEFAULT_TAB);
    }
  }

  private select(name: TabName) {
    if (this.button(name).disabled) name = DEFAULT_TAB;
    for (const button of this.buttons) {
      const selected = button.dataset.tab === name;
      button.setAttribute("aria-selected", String(selected));
      document.getElementById(`page-${button.dataset.tab}`)!.hidden = !selected;
    }
  }

  private button(name: TabName): HTMLButtonElement {
    return this.buttons.find((b) => b.dataset.tab === name)!;
  }

  private load(): TabName {
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved && this.buttons.some((b) => b.dataset.tab === saved)) return saved as TabName;
    } catch {
      // 保存先が使えなくても動く
    }
    return DEFAULT_TAB;
  }

  private remember(name: TabName) {
    this.remembered = name;
    try {
      localStorage.setItem(STORAGE_KEY, name);
    } catch {
      // 保存先が使えなくても動く
    }
  }
}
