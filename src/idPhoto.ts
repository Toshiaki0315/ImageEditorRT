// 「ファイル > 証明写真…」（旧版にはない）のダイアログ。切り抜き・背景・並べる処理は Rust（core/id_photo.rs）が行う。

import { invoke } from "@tauri-apps/api/core";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

/** Rust の id_photo::IdSize。 */
export type IdSize = "resume" | "passport" | "license";

/** Rust の make_id_photo に渡すもの（settings は呼ぶ側が足す）。 */
export type IdPhotoRequest = { size: IdSize; whiteBackground: boolean; sheet: boolean };

/** ダイアログで大きさ・背景・並べ方を選んでもらう（キャンセルなら null）。 */
export async function askIdPhoto(): Promise<IdPhotoRequest | null> {
  const dialog = $<HTMLDialogElement>("id-photo-dialog");
  const size = $<HTMLSelectElement>("id-photo-size");
  if (size.options.length === 0) {
    const sizes = await invoke<[IdSize, string][]>("id_photo_sizes");
    for (const [value, label] of sizes) size.add(new Option(label, value));
  }
  dialog.returnValue = "";
  dialog.showModal();
  return new Promise((resolve) =>
    dialog.addEventListener(
      "close",
      () =>
        resolve(
          dialog.returnValue === "ok"
            ? {
                size: size.value as IdSize,
                whiteBackground: $<HTMLInputElement>("id-photo-white").checked,
                sheet: $<HTMLInputElement>("id-photo-sheet").checked,
              }
            : null,
        ),
      { once: true },
    ),
  );
}
