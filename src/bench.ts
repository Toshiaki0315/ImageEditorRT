// 計測モード（IMAGEEDITORRT_BENCH=1 で起動）: 受け渡しを含めたプレビューの更新の時間を測る。

import { invoke } from "@tauri-apps/api/core";
import type { Preview, Timing } from "./preview";
import { defaultSettings, defaultText, type EditSettings } from "./types";

/** 旧版のベンチマークの「重い設定」（Rust の Settings::heavy と同じ）。 */
const HEAVY: EditSettings = {
  ...defaultSettings(),
  exposure: 0.5,
  brightness: 10,
  contrast: 20,
  temperature: 5000,
  saturation: 20,
  denoise: 50,
  blur: 10,
  sharpen: 50,
  dioramaBlur: 80,
  filter: "hdr",
  vignette: 50,
  aging: 30,
  text: { ...defaultText(), text: "© 2026 写真" },
};

const fmt = (ms: number) => `${ms.toFixed(1)}ms`;
const median = (values: number[]) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];

/** 旧版の NFR-01・02 の目標 (ms)。 */
const TARGET_LOAD_MS = 1000;
const TARGET_UPDATE_MS = 200;

/** 目標の時間に収まったかの判定の行。 */
export const judge = (label: string, ms: number, target: number) =>
  `${label}: ${fmt(ms)}（目標 ${target}ms 以内）… ${ms <= target ? "OK" : "NG"}`;

/** 計測の最後に出す判定（NFR-01: 12MP の JPEG を開いて表示するまで、NFR-02: 重い設定のプレビュー更新の最大）。 */
export function verdict(loadMs: number, heavyMax: number): string {
  return [
    "判定",
    judge("NFR-01 読み込み→プレビュー表示（12MP の JPEG）", loadMs, TARGET_LOAD_MS),
    judge("NFR-02 プレビュー更新（重い設定の最大）", heavyMax, TARGET_UPDATE_MS),
  ].join("\n");
}

/** 重い設定と軽い設定で 20 回ずつ描き直し、中央値の行と、重い設定の最大 (ms) を返す。 */
export async function bench(preview: Preview, size: string): Promise<{ text: string; heavyMax: number }> {
  const light: EditSettings = { ...defaultSettings(), exposure: 0.7, saturation: 20 };
  const lines = [`プレビュー ${size}・各 20 回の中央値`];
  let heavyMax = 0;
  for (const [name, base] of [
    ["重い設定", HEAVY],
    ["軽い設定", light],
  ] as [string, EditSettings][]) {
    const runs: Timing[] = [];
    for (let i = 0; i < 23; i++) {
      // 毎回少しだけ設定を変える（同じ結果の使い回しをさせない）
      const t = await preview.render({ ...base, exposure: base.exposure + (i % 2) * 0.1 });
      if (i >= 3) runs.push(t); // 最初の 3 回は慣らし
    }
    const m = (key: keyof Timing) => median(runs.map((r) => r[key]));
    if (base === HEAVY) heavyMax = Math.max(...runs.map((r) => r.total));
    lines.push(
      `${name}: 合計 ${fmt(m("total"))}（最大 ${fmt(Math.max(...runs.map((r) => r.total)))}）` +
        ` = 処理 ${fmt(m("render"))} + 受け渡し ${fmt(m("transfer"))} + 描画 ${fmt(m("draw"))}`,
    );
  }
  return { text: lines.join("\n"), heavyMax };
}

/** 保存（原寸 6000×4000・重い設定）をしている間も、プレビューを描き直せるか（画面が固まらないか）。 */
export async function benchSave(preview: Preview): Promise<string> {
  const path = await invoke<string>("bench_save_path");
  const options = { quality: 90, keepExif: true, keepGps: false };
  const start = performance.now();
  let done = false;
  const saving = invoke("save_image", { path, settings: HEAVY, options }).finally(() => (done = true));
  const runs: number[] = [];
  while (!done) {
    runs.push((await preview.render({ ...HEAVY, exposure: (runs.length % 2) * 0.1 })).total);
  }
  await saving;
  const saveMs = performance.now() - start;
  return (
    `保存（原寸・重い設定）: ${fmt(saveMs)}。その間のプレビュー更新 ${runs.length} 回、` +
    `中央値 ${fmt(median(runs))}・最大 ${fmt(Math.max(...runs))}`
  );
}
