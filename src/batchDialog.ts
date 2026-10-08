// まとめて処理（旧版 FR-UI-45）のダイアログと進み具合。処理は Rust（core/batch.rs）が 1 枚ずつ行う。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { EditSettings } from "./types";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

/** 「今の加工」の選択肢の値（プリセットはその名前） */
const CURRENT = "";
const NO_OUTPUT_TEXT = "（まだ選んでいません）";
const CANCELLING_TEXT = "中止しています…（処理中の 1 枚が終わるまでお待ちください）";
const MIN_SIDE = 1;
const MAX_SIDE = 20000;

type Progress = { done: number; total: number; name: string };
export type BatchFinished = { saved: number; cancelled: boolean; message: string };

/** ダイアログを開くときの初期値。 */
export type BatchStart = {
  presets: string[];
  /** 長辺の初期値（開いている画像の出力の長いほう。なければ 2048） */
  longSide: number;
  /** 今の画像をリサイズしていれば、リサイズする設定で開く */
  resize: boolean;
  settings: EditSettings;
  save: unknown;
  /** 処理を始めたとき・終えたとき（実行中は保存・開く・まとめて処理を止める） */
  onBusy: (busy: boolean) => void;
};

export class BatchDialog {
  private readonly dialog = $<HTMLDialogElement>("batch-dialog");
  private readonly files = $<HTMLSelectElement>("batch-files");
  private readonly look = $<HTMLSelectElement>("batch-look");
  private readonly resize = $<HTMLInputElement>("batch-resize");
  private readonly longSide = $<HTMLInputElement>("batch-long-side");
  private readonly outLabel = $<HTMLElement>("batch-out-dir");
  private readonly start = $<HTMLButtonElement>("batch-start");
  private readonly progress = $<HTMLDialogElement>("batch-progress");
  private readonly progressText = $<HTMLElement>("batch-progress-text");
  private readonly progressBar = $<HTMLProgressElement>("batch-progress-bar");
  private readonly cancel = $<HTMLButtonElement>("batch-cancel");
  private readonly removeGps = $<HTMLInputElement>("batch-remove-gps");
  private readonly faces = $<HTMLInputElement>("batch-faces");
  private readonly text = $<HTMLInputElement>("batch-text");
  private readonly cover = $<HTMLSelectElement>("batch-cover");
  private readonly strength = $<HTMLInputElement>("batch-strength");
  private readonly strengthValue = $<HTMLOutputElement>("batch-strength-value");
  private outDir: string | null = null;

  constructor(private readonly extensions: string[]) {
    $("batch-add-files").addEventListener("click", () => void this.chooseFiles());
    $("batch-add-folder").addEventListener("click", () => void this.chooseFolders());
    $("batch-remove").addEventListener("click", () => {
      for (const option of [...this.files.selectedOptions]) option.remove();
      this.updateStart();
    });
    $("batch-choose-out").addEventListener("click", () => void this.chooseOutDir());
    this.resize.addEventListener("change", () => (this.longSide.disabled = !this.resize.checked));
    this.longSide.addEventListener("input", () => this.updateStart());
    this.strength.addEventListener("input", () => (this.strengthValue.textContent = this.strength.value));
    for (const check of [this.faces, this.text]) check.addEventListener("change", () => this.updatePrivacy());
    this.cancel.addEventListener("click", () => {
      this.cancel.disabled = true;
      this.progressText.textContent = CANCELLING_TEXT;
      void invoke("cancel_batch");
    });
    // 実行中は Esc で閉じない（「中止」で止める）
    this.progress.addEventListener("cancel", (event) => event.preventDefault());
  }

  /** ダイアログを開き、「開始」なら処理して結果を返す（キャンセルなら null）。 */
  async run(start: BatchStart): Promise<BatchFinished | null> {
    this.files.replaceChildren();
    this.look.replaceChildren(new Option("今の加工", CURRENT));
    for (const name of start.presets) this.look.add(new Option(`プリセット: ${name}`, name));
    this.resize.checked = start.resize;
    this.longSide.value = String(Math.min(Math.max(Math.round(start.longSide), MIN_SIDE), MAX_SIDE));
    this.longSide.disabled = !start.resize;
    this.setOutDir(null);
    this.updatePrivacy();
    // 開いている画像があれば最初から一覧に入れておく（貼り付けた画像などファイルがなければ入れない）
    const current = await invoke<string | null>("batch_current_source");
    if (current) await this.addPaths([current]);
    this.updateStart();
    this.dialog.returnValue = "";
    this.dialog.showModal();
    const answer = await new Promise<string>((resolve) =>
      this.dialog.addEventListener("close", () => resolve(this.dialog.returnValue), { once: true }),
    );
    const sources = [...this.files.options].map((o) => o.value);
    if (answer !== "start" || !this.outDir || sources.length === 0 || !this.validSide()) return null;
    return this.process(sources, this.outDir, start);
  }

  private async process(sources: string[], outDir: string, start: BatchStart): Promise<BatchFinished> {
    this.progressBar.max = sources.length;
    this.progressBar.value = 0;
    this.progressText.textContent = "まとめて処理しています…";
    this.cancel.disabled = false;
    this.progress.showModal();
    const unlisten = await listen<Progress>("batch-progress", (event) => {
      const { done, total, name } = event.payload;
      this.progressBar.value = done;
      if (!this.cancel.disabled) this.progressText.textContent = `${done + 1} / ${total} 枚目を処理しています… ${name}`;
    });
    start.onBusy(true);
    try {
      return await invoke<BatchFinished>("run_batch", {
        sources,
        outDir,
        preset: this.look.value === CURRENT ? null : this.look.value,
        settings: start.settings,
        longSide: this.resize.checked ? Number(this.longSide.value) : null,
        save: start.save,
        privacy: {
          removeGps: this.removeGps.checked,
          faces: this.faces.checked,
          text: this.text.checked,
          kind: this.cover.value,
          strength: Number(this.strength.value),
        },
      });
    } finally {
      unlisten();
      this.progress.close();
      start.onBusy(false);
    }
  }

  private validSide(): boolean {
    const value = Number(this.longSide.value);
    return !this.resize.checked || (Number.isInteger(value) && value >= MIN_SIDE && value <= MAX_SIDE);
  }

  private async addPaths(paths: string[]) {
    const existing = [...this.files.options].map((o) => o.value);
    const added = await invoke<string[]>("batch_collect", { paths, existing });
    for (const path of added) {
      const option = new Option(path.split("/").pop() ?? path, path);
      option.title = path;
      this.files.add(option);
    }
    this.updateStart();
  }

  private async chooseFiles() {
    const paths = await open({
      title: "処理する画像を追加",
      multiple: true,
      filters: [{ name: "画像ファイル", extensions: this.extensions }],
    });
    if (paths) await this.addPaths(paths);
  }

  private async chooseFolders() {
    const paths = await open({ title: "画像のあるフォルダを追加", directory: true, multiple: true });
    if (paths) await this.addPaths(paths);
  }

  private async chooseOutDir() {
    const path = await open({ title: "保存先のフォルダを選ぶ", directory: true, multiple: false });
    if (typeof path === "string") this.setOutDir(path);
  }

  private setOutDir(path: string | null) {
    this.outDir = path;
    this.outLabel.textContent = path ?? NO_OUTPUT_TEXT;
    this.outLabel.title = path ?? "";
    this.updateStart();
  }

  /** 顔・文字を隠すときだけ、隠し方と強さを選べる。 */
  private updatePrivacy() {
    const hiding = this.faces.checked || this.text.checked;
    this.cover.disabled = this.strength.disabled = !hiding;
  }

  /** 画像と保存先がそろうまで「開始」は押せない。 */
  private updateStart() {
    this.start.disabled = this.files.options.length === 0 || this.outDir === null || !this.validSide();
  }
}
