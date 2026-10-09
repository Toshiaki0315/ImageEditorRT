// キーの判定（画面の部品に依存しない。tests-ts/ でテストする）。

/** 範囲の指定を解除する Esc キー（日本語の変換中の Esc は変換の取り消しなので含めない）。 */
export function isEscapeKey(event: Pick<KeyboardEvent, "key" | "isComposing">): boolean {
  return event.key === "Escape" && !event.isComposing;
}

/** 加工前を表示する \ キー（JIS 配列で同じ位置の ¥ キーも。旧版 FR-UI-44）。 */
export function isCompareKey(event: Pick<KeyboardEvent, "key" | "code">): boolean {
  return event.key === "\\" || event.key === "¥" || event.code === "Backslash" || event.code === "IntlYen";
}
