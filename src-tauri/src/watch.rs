//! フォルダの見張り（旧版にはない）: 見張るフォルダに新しく入った画像に、まとめて処理と同じ加工をかけて保存する。
//!
//! 別のスレッドで一定の間隔でフォルダを見て（`core::watch::WatchTracker`）、書き込みが終わった新しい画像を
//! 1 枚ずつ処理し、結果を "watch-processed" のイベントで画面に送る。止めるまで続ける（別のフォルダを見張り始めると、
//! 前の見張りは止める）。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use imageeditorrt_core::batch::{
    self, BatchAutoCrop, BatchOptions, BatchPrivacy, OutputFormat, OutputNaming,
};
use imageeditorrt_core::pipeline::EditSettings;
use imageeditorrt_core::presets::Preset;
use imageeditorrt_core::save::SaveOptions;
use imageeditorrt_core::watch::{folder_images, WatchTracker};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::batch::{guarded, CURRENT_LOOK};
use crate::presets::PresetStore;
use crate::saving::save_pool;

/// 処理した結果のイベントの名前。
const PROCESSED_EVENT: &str = "watch-processed";
/// フォルダを見る間隔。
const INTERVAL: Duration = Duration::from_secs(2);

/// 見張りの止める印（見張っていなければ None）。
#[derive(Default)]
pub struct WatchState(Mutex<Option<Arc<AtomicBool>>>);

impl WatchState {
    /// 見張っていれば止める。
    fn stop(&self) {
        let stop = self.0.lock().ok().and_then(|mut current| current.take());
        if let Some(stop) = stop {
            stop.store(true, Ordering::SeqCst);
        }
    }
}

/// 1 枚処理した結果（name は元のファイル名、output は保存したファイル名、error は処理できなかった理由）。
#[derive(Clone, Serialize)]
struct Processed {
    name: String,
    output: Option<String>,
    error: Option<String>,
    count: usize,
}

/// フォルダの見張りを始める。加工・保存の選び方はまとめて処理と同じ。保存先が見張るフォルダと同じならエラー
/// （保存したファイルをまた処理しないよう、別のフォルダにする）。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn start_watch(
    folder: String,
    out_dir: String,
    preset: Option<String>,
    settings: EditSettings,
    long_side: Option<u32>,
    save: SaveOptions,
    privacy: BatchPrivacy,
    naming: OutputNaming,
    format: OutputFormat,
    auto_crop: Option<BatchAutoCrop>,
    app: AppHandle,
    presets: State<'_, PresetStore>,
    watch: State<'_, WatchState>,
) -> Result<(), String> {
    let folder = PathBuf::from(folder);
    let out_dir = PathBuf::from(out_dir);
    if !folder.is_dir() {
        return Err("見張るフォルダがありません".into());
    }
    if imageeditorrt_core::save::is_same_file(&folder, &out_dir) {
        return Err("保存先は、見張るフォルダとは別のフォルダにしてください".into());
    }
    let look = match &preset {
        Some(name) => presets.find(name).ok_or_else(|| format!("プリセット「{name}」がありません"))?,
        None => Preset::from_settings(CURRENT_LOOK, &settings),
    };
    let options = BatchOptions { look, long_side, save, privacy, naming, format, auto_crop };
    watch.stop();
    let stop = Arc::new(AtomicBool::new(false));
    *watch.0.lock().map_err(|e| e.to_string())? = Some(stop.clone());
    let existing = folder_images(&folder).into_iter().map(|(path, _)| path);
    let mut tracker = WatchTracker::new(existing);
    std::thread::spawn(move || {
        let mut count = 0;
        while !stop.load(Ordering::SeqCst) {
            for source in tracker.poll(&folder_images(&folder)) {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let result = save_pool()
                    .install(|| guarded(|| batch::process_image_at(&source, count, &out_dir, &options)));
                count += 1;
                if let Ok(path) = &result {
                    tracker.mark_done(path);
                }
                let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let output = result
                    .as_ref()
                    .ok()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned());
                let _ = app.emit(PROCESSED_EVENT, Processed { name, output, error: result.err(), count });
            }
            std::thread::sleep(INTERVAL);
        }
    });
    Ok(())
}

/// フォルダの見張りをやめる。
#[tauri::command]
pub fn stop_watch(watch: State<'_, WatchState>) {
    watch.stop();
}
