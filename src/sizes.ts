// 複数の大きさで保存: 選んだ大きさの覚え方と、保存した結果の知らせ方。画面の部品に依存しない。

/** はじめて開いたときに選んでおく大きさ（Instagram と X）。 */
export const DEFAULT_SIZES = [1080, 1600];

/** 環境設定に残した選択を読む（よく使う大きさ choices にあるものだけ。読めなければ既定）。 */
export function parseSizes(stored: string | null, choices: number[]): number[] {
  try {
    const value: unknown = JSON.parse(stored ?? "null");
    if (Array.isArray(value)) return choices.filter((px) => value.includes(px));
  } catch {
    // 壊れていれば既定
  }
  return DEFAULT_SIZES.filter((px) => choices.includes(px));
}

/** 保存した結果のステータスバーの文（ファイルの名前を並べる。多ければ最初の 2 つと数）。 */
export function savedSummary(paths: string[]): string {
  const names = paths.map((p) => p.split("/").pop() ?? p);
  const shown = names.length > 3 ? `${names.slice(0, 2).join("・")} ほか ${names.length - 2} 件` : names.join("・");
  return `${names.length} つの大きさで保存しました: ${shown}`;
}
