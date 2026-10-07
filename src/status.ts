// ステータスバーとエラーの知らせ（想定外のエラーはログにも書く。旧版 NFR-04）。

import { invoke } from "@tauri-apps/api/core";
import { message } from "@tauri-apps/plugin-dialog";
import { dom, state } from "./app";

const NO_IMAGE_MESSAGE = "画像が読み込まれていません";

/** エラーをダイアログで知らせる。withFormats なら対応形式も添える。 */
export async function showError(title: string, error: unknown, withFormats = false) {
  let text = String(error);
  if (withFormats) text += `\n\n対応形式: ${state.formatsText}`;
  await message(text, { title, kind: "warning" });
}

/** 知らせた文を、続けて起きる描き直し（知らせた操作で設定が変わったときなど）でも残しておく時間（ms） */
const NOTICE_MS = 4000;
/** 最後に知らせた文と、その時刻 */
let notice: { text: string; at: number } | null = null;

/** ステータスバー: ファイル名・原寸・出力の大きさ（と、読み込みのときのお知らせ・extra・少し前に知らせた文）。 */
export async function updateStatus(extra?: string) {
  if (!extra && notice && performance.now() - notice.at < NOTICE_MS) extra = notice.text;
  const { loaded, notes } = state;
  if (!loaded) {
    dom.status.textContent = notes.length ? notes.join("／") : NO_IMAGE_MESSAGE;
    return;
  }
  let size = "出力 —";
  try {
    const [width, height] = await invoke<[number, number]>("output_size", { settings: state.settings });
    size = `出力 ${width}×${height} px`;
  } catch {
    // 大きさの指定が範囲外のときは「—」
  }
  let text = `${loaded.name} ｜ 原寸 ${loaded.width}×${loaded.height} px ｜ ${size}`;
  const all = extra ? [...notes, extra] : notes;
  if (all.length) text += `（${all.join("／")}）`;
  dom.status.textContent = text;
}

/** ステータスバーで知らせる（画像がなければ、知らせる文だけを出す）。 */
export function notify(text: string) {
  notice = { text, at: performance.now() };
  if (state.loaded) void updateStatus(text);
  else dom.status.textContent = text;
}

/** 想定外のエラーを知らせている間（続けて起きても、ダイアログは 1 つだけにする） */
let reportingUnexpected = false;

/**
 * 想定外のエラー: ログ（~/Library/Logs/ImageEditorRT/）に書き、ダイアログで知らせる。アプリは終わらせない。
 * logPath を渡したとき（Rust のパニック）は、もうログに書いてある。
 */
export async function reportUnexpected(error: unknown, logPath?: string) {
  const detail = error instanceof Error ? `${error.name}: ${error.message}` : String(error);
  let path = logPath ?? "";
  try {
    path ||= await invoke<string>("report_unexpected", {
      message: error instanceof Error && error.stack ? `${detail}\n${error.stack}` : detail,
    });
  } catch {
    // ログに書けなくても、ダイアログでは知らせる
  }
  if (reportingUnexpected) return;
  reportingUnexpected = true;
  try {
    await message(`予期しないエラーが発生しました。\n${detail}\n\n詳細はログを参照してください:\n${path}`, {
      title: "予期しないエラー",
      kind: "error",
    });
  } catch {
    // 知らせることもできなければ、何もしない（知らせるためのエラーで繰り返さない）
  } finally {
    reportingUnexpected = false;
  }
}

/** 画面の想定外のエラー（例外・処理されなかった Promise の失敗）を知らせる。 */
export function catchUnexpectedErrors() {
  window.addEventListener("error", (event) => void reportUnexpected(event.error ?? event.message));
  window.addEventListener("unhandledrejection", (event) => void reportUnexpected(event.reason));
}
