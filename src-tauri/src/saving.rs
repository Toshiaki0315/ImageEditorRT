//! 保存（原寸で処理して書き出す）と、保存ダイアログの初期のパス。

use std::path::PathBuf;
use std::sync::OnceLock;

use imageeditorrt_core::formats::Format;
use imageeditorrt_core::pipeline::{self, EditSettings};
use imageeditorrt_core::save::{self, SaveError, SaveOptions, SAME_FILE_MESSAGE};
use serde::Serialize;
use tauri::State;

#[cfg(target_os = "macos")]
use crate::clipboard;
use crate::state::{file_name, AppState};

/// 保存ダイアログの初期のパス `<元の名前>_edited.<拡張子>`（重ならない名前）。
///
/// 貼り付けた画像は `~/ピクチャ/クリップボード_<stamp>.png`（stamp は画面が渡す今の日時）。
/// どちらでもなければ None（計測用の画像など）。
#[tauri::command]
pub fn default_save_path(stamp: String, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let loaded = state.0.lock().map_err(|e| e.to_string())?;
    let path = match (&loaded.source.path, loaded.source.pasted) {
        (Some(path), _) => Some(save::default_save_path(path)),
        #[cfg(target_os = "macos")]
        (None, true) => clipboard::pictures_or_home().map(|folder| save::pasted_save_path(&stamp, &folder)),
        _ => None,
    };
    Ok(path.map(|p| p.to_string_lossy().into_owned()))
}

/// 保存できなかったとき、画面に返す理由。kind が "sameFile" なら保存ダイアログを開き直す。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveFailure {
    /// "sameFile"・"extension"・"other"
    kind: &'static str,
    message: String,
}

impl SaveFailure {
    fn other(message: impl ToString) -> Self {
        Self { kind: "other", message: message.to_string() }
    }
}

/// 保存（原寸の処理）に使うスレッドの組。プレビューの描き直しが待たされないよう、
/// プレビュー（rayon の既定の組）とは分け、CPU のコアを 2 つ残す。
pub(crate) fn save_pool() -> &'static rayon::ThreadPool {
    static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
    POOL.get_or_init(|| {
        let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
        rayon::ThreadPoolBuilder::new()
            .num_threads(cores.saturating_sub(2).max(1))
            .thread_name(|i| format!("save-{i}"))
            .build()
            .expect("保存用のスレッドを作れません")
    })
}

/// 今の設定を原寸でかけて保存する。処理はメインスレッドとは別のスレッドで行う。
#[tauri::command]
pub async fn save_image(
    path: String,
    settings: EditSettings,
    options: SaveOptions,
    state: State<'_, AppState>,
) -> Result<String, SaveFailure> {
    let path = PathBuf::from(path);
    if !save::is_savable(&path) {
        let message = SaveError::UnsupportedExtension(
            path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default(),
        );
        return Err(SaveFailure { kind: "extension", message: message.to_string() });
    }
    let (original, source) = {
        let loaded = state.0.lock().map_err(SaveFailure::other)?;
        let original =
            loaded.original.clone().ok_or_else(|| SaveFailure::other("画像が読み込まれていません"))?;
        (original, loaded.source.clone())
    };
    // 元の画像には上書きしない（大文字・小文字の違いも同じファイルとみなす）
    if source.path.as_deref().is_some_and(|p| save::is_same_file(&path, p)) {
        return Err(SaveFailure { kind: "sameFile", message: SAME_FILE_MESSAGE.into() });
    }
    let name = file_name(&path);
    tauri::async_runtime::spawn_blocking(move || {
        save_pool().install(|| {
            let edited = pipeline::apply_edits(&original, &settings).map_err(SaveFailure::other)?;
            let is_tiff = source.format == Some(Format::Tiff);
            save::save_edited(&edited, &path, options, source.exif.as_deref(), is_tiff)
                .map_err(SaveFailure::other)
        })
    })
    .await
    .map_err(SaveFailure::other)??;
    Ok(name)
}
