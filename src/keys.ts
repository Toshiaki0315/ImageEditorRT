// キーの判定（画面の部品に依存しない。tests-ts/ でテストする）。

/** 加工前を表示する \ キー（JIS 配列で同じ位置の ¥ キーも。旧版 FR-UI-44）。 */
export function isCompareKey(event: Pick<KeyboardEvent, "key" | "code">): boolean {
  return event.key === "\\" || event.key === "¥" || event.code === "Backslash" || event.code === "IntlYen";
}
