// 「編集 > 履歴…」: 操作の一覧（何を変えたか）を出し、選んだ時点に戻す・進める（元に戻す・やり直すと同じ履歴）。
// プレビューを見ながら選べるよう、モーダルにしないで出す。

import type { Snapshot } from "./app";
import type { HistoryRecorder } from "./history";
import { describeHistory, type LabeledState } from "./historyLabels";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class HistoryPanel {
  private readonly dialog = $<HTMLDialogElement>("history-dialog");
  private readonly list = $<HTMLSelectElement>("history-list");
  private readonly recorder: () => HistoryRecorder<Snapshot> | undefined;

  constructor(recorder: () => HistoryRecorder<Snapshot> | undefined) {
    this.recorder = recorder;
    this.list.addEventListener("change", () => {
      this.recorder()?.goto(Number(this.list.value));
      this.refresh();
    });
    $<HTMLButtonElement>("history-close").addEventListener("click", () => this.dialog.close());
  }

  /** 一覧を出す。 */
  open() {
    if (!this.dialog.open) this.dialog.show();
    this.refresh();
  }

  close() {
    this.dialog.close();
  }

  /** 履歴が変わったとき（出していれば一覧を作り直す）。 */
  refresh() {
    const recorder = this.recorder();
    if (!this.dialog.open || !recorder) return;
    // 積み終えた分だけ出す（スライダーを動かしている途中は、落ち着いて積んだときに出し直す）
    const { states, current } = recorder.committedEntries();
    const labels = describeHistory(states as unknown as LabeledState[]);
    // 新しいものを上に並べる
    const options = labels.map((label, i) => {
      const suffix = i === current ? "（今）" : i > current ? "（やり直せる）" : "";
      return new Option(`${i + 1}. ${label}${suffix}`, String(i));
    });
    this.list.replaceChildren(...options.reverse());
    this.list.value = String(current);
  }
}
