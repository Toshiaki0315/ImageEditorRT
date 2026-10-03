// ⌘V（編集 > ペースト）で何をするかを決める（旧版 FR-UI-64）。画面の部品に依存しない（tests-ts/paste.test.ts）。

/** クリップボードにあるもの（Rust の clipboard_contents）。 */
export type ClipboardContents = { files: string[]; hasImage: boolean; hasText: boolean };

export type PasteAction =
  /** Finder でコピーしたファイルを、ドロップと同じく開く（複数なら先頭の 1 枚） */
  | { kind: "files"; paths: string[] }
  /** 画像のデータを、元のファイルのない画像として開く */
  | { kind: "image" }
  /** 入力欄に文字を貼り付ける */
  | { kind: "text" }
  /** 開けるものがない（ステータスバーで知らせる） */
  | { kind: "nothing" };

/**
 * editing は文字・数値の入力欄で編集中か。isSupported は対応形式のファイルか。
 *
 * - 入力欄では、対応形式のファイルか「文字がなく画像がある」ときだけ画像を開き、ほかは文字を貼り付ける
 * - それ以外では、ファイルがあればそれを開き（対応形式でなければ開くときに知らせる）、なければ画像を開く
 */
export function pasteAction(
  contents: ClipboardContents,
  editing: boolean,
  isSupported: (path: string) => boolean,
): PasteAction {
  if (editing) {
    const supported = contents.files.filter(isSupported);
    if (supported.length > 0) return { kind: "files", paths: supported };
    if (contents.hasImage && !contents.hasText) return { kind: "image" };
    return { kind: "text" };
  }
  if (contents.files.length > 0) return { kind: "files", paths: contents.files };
  if (contents.hasImage) return { kind: "image" };
  return { kind: "nothing" };
}

/** 貼り付けた画像の保存の初期の名前に使う日時（"20261002-064500" の形、この Mac の時刻）。 */
export function pasteStamp(now: Date): string {
  const two = (n: number) => String(n).padStart(2, "0");
  const date = `${now.getFullYear()}${two(now.getMonth() + 1)}${two(now.getDate())}`;
  return `${date}-${two(now.getHours())}${two(now.getMinutes())}${two(now.getSeconds())}`;
}
