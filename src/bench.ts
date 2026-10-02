// 計測モード（IMAGEEDITORRT_BENCH=1 で起動）: 受け渡しを含めたプレビューの更新の時間を測る。

import type { Preview, Timing } from "./preview";
import { defaultSettings, type EditSettings } from "./types";

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
  text: { text: "© 2026 写真", size: 5 },
};

const fmt = (ms: number) => `${ms.toFixed(1)}ms`;
const median = (values: number[]) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];

/** 重い設定と軽い設定で 20 回ずつ描き直し、中央値を返す。 */
export async function bench(preview: Preview, size: string): Promise<string> {
  const light: EditSettings = { ...defaultSettings(), exposure: 0.7, saturation: 20 };
  const lines = [`プレビュー ${size}・各 20 回の中央値`];
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
    lines.push(
      `${name}: 合計 ${fmt(m("total"))}（最大 ${fmt(Math.max(...runs.map((r) => r.total)))}）` +
        ` = 処理 ${fmt(m("render"))} + 受け渡し ${fmt(m("transfer"))} + 描画 ${fmt(m("draw"))}`,
    );
  }
  return lines.join("\n");
}
