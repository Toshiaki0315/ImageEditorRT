//! まとめて処理（一括処理。旧版 FR-UI-45）の画面とのやりとり。
//!
//! 処理は保存と同じスレッドの組で 1 枚ずつ行い、進み具合を "batch-progress" のイベントで画面に送る。
//! 中止すると、処理中の 1 枚を終えたところで止まる。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use imageeditorrt_core::batch::{self, BatchOptions};
use imageeditorrt_core::pipeline::EditSettings;
use imageeditorrt_core::presets::Preset;
use imageeditorrt_core::save::SaveOptions;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::presets::PresetStore;
use crate::saving::save_pool;
use crate::state::AppState;

/// 進み具合のイベントの名前。
const PROGRESS_EVENT: &str = "batch-progress";
/// 「今の加工」の名前（プリセットの代わりに今の設定をかける）。
const CURRENT_LOOK: &str = "今の加工";

/// 中止の印（実行中に「中止」が押されたら true）。
#[derive(Default)]
pub struct BatchState(AtomicBool);

/// 進み具合: done 枚を処理し終え、次に name を処理する。
#[derive(Clone, Serialize)]
struct Progress {
    done: usize,
    total: usize,
    name: String,
}

/// 終わったときの結果。message は知らせる文（保存した枚数と、処理できなかった画像）。
#[derive(Serialize)]
pub struct Finished {
    saved: usize,
    cancelled: bool,
    message: String,
}

/// 1 枚の処理で想定外のエラー（パニック）が起きても、その画像を「処理できなかった画像」にして残りを続ける
/// （パニックの中身はパニックのフックがログに書く）。
fn guarded(process: impl FnOnce() -> Result<PathBuf, String>) -> Result<PathBuf, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(process))
        .unwrap_or_else(|_| Err("予期しないエラー（詳しくはログ）".to_string()))
}

/// 開いている画像のファイル（一覧に最初から入れておく。ファイルがなければ None）。
#[tauri::command]
pub fn batch_current_source(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let loaded = state.0.lock().map_err(|e| e.to_string())?;
    Ok(loaded.source.path.as_ref().map(|p| p.to_string_lossy().into_owned()))
}

/// 一覧に加える画像（フォルダは直下の対応形式の画像を名前順に。existing と同じものは加えない）。
#[tauri::command]
pub fn batch_collect(paths: Vec<String>, existing: Vec<String>) -> Vec<String> {
    let to_paths = |list: Vec<String>| list.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    batch::collect_images(&to_paths(paths), &to_paths(existing))
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// まとめて処理する。preset が None なら今の設定（settings）の加工をかける。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn run_batch(
    sources: Vec<String>,
    out_dir: String,
    preset: Option<String>,
    settings: EditSettings,
    long_side: Option<u32>,
    save: SaveOptions,
    app: AppHandle,
    presets: State<'_, PresetStore>,
    batch_state: State<'_, BatchState>,
) -> Result<Finished, String> {
    let look = match &preset {
        Some(name) => presets.find(name).ok_or_else(|| format!("プリセット「{name}」がありません"))?,
        None => Preset::from_settings(CURRENT_LOOK, &settings),
    };
    let options = BatchOptions { look, long_side, save };
    let sources: Vec<PathBuf> = sources.into_iter().map(PathBuf::from).collect();
    let out_dir = PathBuf::from(out_dir);
    batch_state.0.store(false, Ordering::SeqCst);
    let cancel = app.clone();
    let results = tauri::async_runtime::spawn_blocking({
        let out_dir = out_dir.clone();
        move || {
            let total = sources.len();
            save_pool().install(|| {
                batch::run_batch(
                    &sources,
                    |source| guarded(|| batch::process_image(source, &out_dir, &options)),
                    |done, source| {
                        let name =
                            source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let _ = app.emit(PROGRESS_EVENT, Progress { done, total, name });
                    },
                    || cancel.state::<BatchState>().0.load(Ordering::SeqCst),
                )
            })
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    let cancelled = batch_state.0.load(Ordering::SeqCst);
    Ok(Finished {
        saved: results.iter().filter(|r| r.output.is_some()).count(),
        cancelled,
        message: batch::summary(&results, cancelled, &out_dir),
    })
}

/// 「中止」: 処理中の 1 枚を終えたところで止める。
#[tauri::command]
pub fn cancel_batch(state: State<'_, BatchState>) {
    state.0.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_becomes_a_failure_of_that_image() {
        assert_eq!(guarded(|| Ok(PathBuf::from("a.png"))), Ok(PathBuf::from("a.png")));
        assert_eq!(guarded(|| Err("読めません".into())), Err("読めません".to_string()));
        // パニックの表示（テストの出力）は既定のフックに任せる
        assert_eq!(guarded(|| panic!("壊れた画像")), Err("予期しないエラー（詳しくはログ）".to_string()));
    }
}
