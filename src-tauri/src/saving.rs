//! 保存（原寸で処理して書き出す）と、保存ダイアログの初期のパス。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use imageeditorrt_core::formats::Format;
use imageeditorrt_core::pipeline::{self, EditSettings};
use imageeditorrt_core::save::{self, SaveError, SaveOptions, SAME_FILE_MESSAGE};
use serde::Serialize;
use tauri::State;

#[cfg(target_os = "macos")]
use crate::clipboard;
use crate::state::{AppState, Source};

/// 保存ダイアログの初期のパス `<元の名前>_edited.<拡張子>`（重ならない名前）。
///
/// 貼り付けた画像は `~/ピクチャ/クリップボード_<stamp>.png`（stamp は画面が渡す今の日時）。
/// どちらでもなければ None（計測用の画像など）。
#[tauri::command]
pub fn default_save_path(stamp: String, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let loaded = state.0.lock().map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let pictures = clipboard::pictures_or_home();
    #[cfg(not(target_os = "macos"))]
    let pictures = None;
    let path = initial_save_path(&loaded.source, &stamp, pictures.as_deref());
    Ok(path.map(|p| p.to_string_lossy().into_owned()))
}

/// 保存ダイアログの初期のパス: 元のファイルがあれば `<元の名前>_edited`、貼り付けた画像なら
/// pictures（ピクチャ、なければホーム）の `クリップボード_<stamp>.png`、どちらでもなければ None。
fn initial_save_path(source: &Source, stamp: &str, pictures: Option<&Path>) -> Option<PathBuf> {
    match (&source.path, source.pasted) {
        (Some(path), _) => Some(save::default_save_path(path)),
        (None, true) => pictures.map(|folder| save::pasted_save_path(stamp, folder)),
        (None, false) => None,
    }
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

/// 保存先を確かめる: path が保存できる拡張子で、書き出す paths のどれも元の画像 source ではないこと。
fn check_targets(path: &Path, paths: &[PathBuf], source: Option<&Path>) -> Result<(), SaveFailure> {
    if !save::is_savable(path) {
        let message = SaveError::UnsupportedExtension(
            path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default(),
        );
        return Err(SaveFailure { kind: "extension", message: message.to_string() });
    }
    // 元の画像には上書きしない（大文字・小文字の違いも同じファイルとみなす）
    if source.is_some_and(|source| paths.iter().any(|p| save::is_same_file(p, source))) {
        return Err(SaveFailure { kind: "sameFile", message: SAME_FILE_MESSAGE.into() });
    }
    Ok(())
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
) -> Result<save::Saved, SaveFailure> {
    let path = PathBuf::from(path);
    let opened = state.opened().map_err(SaveFailure::other)?;
    let source = opened.source.clone();
    check_targets(&path, std::slice::from_ref(&path), source.path.as_deref())?;
    // 文字の {日付}・{日時} を撮影日時に置き換える
    let settings = opened.shown(settings, false);
    let saved = tauri::async_runtime::spawn_blocking(move || {
        save_pool().install(|| {
            // 肌・背景の材料はプレビューの大きさなので、原寸に合わせてかける
            let original = opened.prepared(&opened.original, &settings);
            let edited = pipeline::apply_edits(&original, &settings).map_err(SaveFailure::other)?;
            let is_tiff = source.format == Some(Format::Tiff);
            save::save_edited(&edited, &path, options, source.exif.as_deref(), is_tiff)
                .map_err(SaveFailure::other)
        })
    })
    .await
    .map_err(SaveFailure::other)??;
    Ok(saved)
}

/// 複数の大きさで保存した 1 つ分（保存先と、保存した結果）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizedSaved {
    path: String,
    long_side: u32,
    saved: save::Saved,
}

/// 今の設定を、長辺を long_sides のそれぞれにして原寸で処理し、path の名前に `_<長辺>` を付けて続けて保存する
/// （複数の大きさで保存。旧版にはない）。すでにあるファイルには上書きしない。
#[tauri::command]
pub async fn save_sizes(
    path: String,
    settings: EditSettings,
    options: SaveOptions,
    long_sides: Vec<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<SizedSaved>, SaveFailure> {
    let base = PathBuf::from(path);
    let opened = state.opened().map_err(SaveFailure::other)?;
    let source = opened.source.clone();
    let paths = save::sized_paths(&base, &long_sides);
    check_targets(&base, &paths, source.path.as_deref())?;
    let settings = opened.shown(settings, false);
    tauri::async_runtime::spawn_blocking(move || {
        save_pool().install(|| {
            // 肌・背景などの前もってかける処理は、大きさによらないので 1 回だけ
            let original = opened.prepared(&opened.original, &settings);
            let is_tiff = source.format == Some(Format::Tiff);
            paths
                .into_iter()
                .zip(long_sides)
                .map(|(path, long_side)| {
                    let sized = pipeline::long_side_settings(original.dimensions(), &settings, long_side);
                    let edited = pipeline::apply_edits(&original, &sized).map_err(SaveFailure::other)?;
                    let saved = save::save_edited(&edited, &path, options, source.exif.as_deref(), is_tiff)
                        .map_err(SaveFailure::other)?;
                    Ok(SizedSaved { path: path.to_string_lossy().into_owned(), long_side, saved })
                })
                .collect()
        })
    })
    .await
    .map_err(SaveFailure::other)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_must_be_savable_and_not_the_source() {
        let source = Path::new("/photos/IMG_0001.JPG");
        let ok = Path::new("/photos/IMG_0001_edited.jpg");
        assert!(check_targets(ok, &[ok.to_path_buf()], Some(source)).is_ok());
        assert!(check_targets(ok, &[ok.to_path_buf()], None).is_ok());
        // 保存できない拡張子
        let raw = Path::new("/photos/x.dng");
        assert_eq!(check_targets(raw, &[raw.to_path_buf()], None).unwrap_err().kind, "extension");
        // 書き出すもののどれかが元の画像（大文字・小文字の違いも同じ）
        let paths = [ok.to_path_buf(), PathBuf::from("/photos/img_0001.jpg")];
        assert_eq!(check_targets(ok, &paths, Some(source)).unwrap_err().kind, "sameFile");
    }

    #[test]
    fn initial_save_paths() {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-app-savepath-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // 元のファイルがあれば、同じフォルダの <名前>_edited（すでにあれば _edited_2）
        let photo = dir.join("photo.JPG");
        std::fs::write(&photo, b"x").unwrap();
        let from_file = Source { path: Some(photo.clone()), ..Source::default() };
        assert_eq!(
            initial_save_path(&from_file, "20261004-120000", Some(&dir)),
            Some(dir.join("photo_edited.JPG"))
        );
        std::fs::write(dir.join("photo_edited.JPG"), b"x").unwrap();
        assert_eq!(initial_save_path(&from_file, "s", None), Some(dir.join("photo_edited_2.JPG")));
        // 貼り付けた画像はピクチャ（なければホーム）の クリップボード_<日時>.png
        let pasted = Source { pasted: true, ..Source::default() };
        assert_eq!(
            initial_save_path(&pasted, "20261004-120000", Some(&dir)),
            Some(dir.join("クリップボード_20261004-120000.png"))
        );
        assert_eq!(initial_save_path(&pasted, "s", None), None);
        // 計測用の画像など、どちらでもなければ None（画面が名前を決める）
        assert_eq!(initial_save_path(&Source::default(), "s", Some(&dir)), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
