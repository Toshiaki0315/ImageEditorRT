// キーボードショートカットの書き方（Tauri のメニューと同じ `CmdOrCtrl+Shift+K`）と、押したキー・表示の変換。
// 画面の部品には依存しない（tests-ts/shortcutKeys.test.ts で確かめる）。

/** 押したキー（KeyboardEvent のうち使うところ）。 */
export type KeyPress = { code: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean };

/** KeyboardEvent.code の記号のキー → 書き方。 */
const SYMBOLS: Record<string, string> = {
  BracketLeft: "[",
  BracketRight: "]",
  Slash: "/",
  Backslash: "\\",
  Comma: ",",
  Period: ".",
  Semicolon: ";",
  Quote: "'",
  Minus: "-",
  Equal: "=",
  Backquote: "`",
  Space: "Space",
  Enter: "Enter",
  Backspace: "Backspace",
  Delete: "Delete",
  Tab: "Tab",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
};

/** 押したキーをショートカットの書き方にする。修飾キーだけ・使えないキーなら null。 */
export function acceleratorFromKey(key: KeyPress): string | null {
  let name: string | undefined;
  if (/^Key[A-Z]$/.test(key.code)) name = key.code.slice(3);
  else if (/^Digit[0-9]$/.test(key.code)) name = key.code.slice(5);
  else if (/^F([1-9]|1[0-2])$/.test(key.code)) name = key.code;
  else name = SYMBOLS[key.code];
  if (!name) return null;
  const parts: string[] = [];
  if (key.metaKey) parts.push("CmdOrCtrl");
  if (key.ctrlKey) parts.push("Ctrl");
  if (key.altKey) parts.push("Alt");
  if (key.shiftKey) parts.push("Shift");
  return [...parts, name].join("+");
}

/** 書き方を、メニューと同じ記号の表示にする（`CmdOrCtrl+Shift+K` → `⇧⌘K`）。空なら「なし」。 */
export function displayAccelerator(accelerator: string): string {
  if (!accelerator) return "なし";
  const parts = accelerator.split("+");
  const key = parts.pop() ?? "";
  const symbol = (m: string) => ({ Ctrl: "⌃", Alt: "⌥", Shift: "⇧", CmdOrCtrl: "⌘", Cmd: "⌘", Super: "⌘" })[m] ?? m;
  // macOS のメニューと同じ並び（⌃⌥⇧⌘）
  const order = ["Ctrl", "Alt", "Shift", "CmdOrCtrl", "Cmd", "Super"];
  const sorted = [...parts].sort((a, b) => order.indexOf(a) - order.indexOf(b));
  const keyText = { Up: "↑", Down: "↓", Left: "←", Right: "→", Space: "Space", Enter: "↩", Backspace: "⌫", Delete: "⌦", Tab: "⇥" }[key] ?? key;
  return sorted.map(symbol).join("") + keyText;
}
