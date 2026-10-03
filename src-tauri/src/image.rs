//! 画像を開く・閉じる・プレビュー・100% 表示・クリップボードの画像・ジオラマのガイド。

use std::path::PathBuf;
use std::time::Instant;

use imageeditorrt_core::exif_info::ExifInfo;
use imageeditorrt_core::formats::Format;
use imageeditorrt_core::pipeline::{self, EditSettings};
use imageeditorrt_core::{decode, load, save};
use tauri::ipc::Response;
use tauri::{State, WebviewWindow};

#[cfg(target_os = "macos")]
use crate::clipboard;
use crate::state::{blocking, elapsed_ms, file_name, prepare, store, AppState, Loaded, OpenInfo, Source};
use crate::APP_NAME;

/// 画像のファイルを読み込む。読めなければ、ダイアログに出す説明を返す（旧版 FR-IO-10）。
#[tauri::command]
pub async fn open_path(
    path: String,
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> Result<OpenInfo, String> {
    let path = PathBuf::from(path);
    let name = file_name(&path);
    let prepared = blocking(move || {
        let start = Instant::now();
        let loaded = load::load_file(&path).map_err(|e| e.message(&name))?;
        let decode_ms = elapsed_ms(start);
        let source = Source {
            format: Some(loaded.decoded.format),
            exif: loaded.raw_exif,
            path: Some(path),
            pasted: false,
        };
        Ok(prepare(name, loaded.decoded, decode_ms, loaded.exif, source))
    })
    .await?;
    store(&state, &window, prepared)
}

/// プレビューに設定をかけて返す。trimmed なら切り抜いた範囲だけを表示する。
///
/// 返すバイト列: 先頭 12 バイトが幅・高さ・処理の時間 (µs)（どれも u32 リトルエンディアン）、
/// その後ろが RGBA の画素、最後が保存される写真のヒストグラム（R・G・B・輝度の順に 256 個ずつの
/// u32 リトルエンディアン）。JSON にしないので、1600px の画像でも受け渡しは数 ms で済む。
#[tauri::command]
pub async fn render_preview(
    settings: EditSettings,
    trimmed: bool,
    comparing: bool,
    state: State<'_, AppState>,
) -> Result<Response, String> {
    // 鍵は画像を取り出すあいだだけ持ち、処理は別のスレッドで行う（その間もほかの問い合わせに答えられる）
    let (preview, factor, settings) = {
        let loaded = state.0.lock().map_err(|e| e.to_string())?;
        let preview = loaded.preview.clone().ok_or("画像が読み込まれていません")?;
        (preview, loaded.factor, shown_settings(&loaded, settings, comparing))
    };
    let ((image, histogram), render_time) = blocking(move || {
        let start = Instant::now();
        Ok((pipeline::render_preview_with_histogram(&preview, &settings, factor, trimmed), start.elapsed()))
    })
    .await?;
    let (width, height) = image.dimensions();
    let pixels = image.into_raw();
    let histogram = histogram.to_le_bytes();
    let mut body = Vec::with_capacity(12 + pixels.len() + histogram.len());
    for v in [width, height, render_time.as_micros() as u32] {
        body.extend_from_slice(&v.to_le_bytes());
    }
    body.extend_from_slice(&pixels);
    body.extend_from_slice(&histogram);
    Ok(Response::new(body))
}

/// リセット（旧版 FR-UI-42）: 読み込んだ画像を捨てて、未読込の状態に戻す。
#[tauri::command]
pub fn close_image(window: WebviewWindow, state: State<'_, AppState>) -> Result<(), String> {
    let _ = window.set_title(APP_NAME);
    *state.0.lock().map_err(|e| e.to_string())? = Loaded::default();
    Ok(())
}

/// 表示に使う設定。comparing（加工前の表示）なら、向きと切り抜く範囲だけを残す。
pub(crate) fn shown_settings(loaded: &Loaded, settings: EditSettings, comparing: bool) -> EditSettings {
    match (&loaded.original, comparing) {
        (Some(original), true) => pipeline::before_settings(original.dimensions(), &settings),
        _ => settings,
    }
}

/// 100% 表示: 原寸で処理した保存結果（comparing なら加工前）を返す（処理は別のスレッド）。
///
/// 返すバイト列: 先頭 8 バイトが幅・高さ（u32 リトルエンディアン）、その後ろが RGBA の画素。
#[tauri::command]
pub async fn render_actual_size(
    settings: EditSettings,
    comparing: bool,
    state: State<'_, AppState>,
) -> Result<Response, String> {
    let (original, settings) = {
        let loaded = state.0.lock().map_err(|e| e.to_string())?;
        let original = loaded.original.clone().ok_or("画像が読み込まれていません")?;
        let settings = shown_settings(&loaded, settings, comparing);
        (original, settings)
    };
    let image =
        blocking(move || pipeline::apply_edits(&original, &settings).map_err(|e| e.to_string())).await?;
    let (width, height) = image.dimensions();
    let pixels = image.into_raw();
    let mut body = Vec::with_capacity(8 + pixels.len());
    body.extend_from_slice(&width.to_le_bytes());
    body.extend_from_slice(&height.to_le_bytes());
    body.extend_from_slice(&pixels);
    Ok(Response::new(body))
}

/// クリップボードにあるもの（Finder でコピーしたファイル・画像・文字があるか）。
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn clipboard_contents() -> clipboard::Contents {
    clipboard::contents()
}

/// クリップボードの文字（入力欄に貼り付ける）。
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn clipboard_text() -> Option<String> {
    clipboard::text()
}

/// クリップボードの画像を、元のファイルのない画像として開く（PNG 扱い・EXIF なし・透過は残す）。
#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn open_clipboard_image(
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> Result<OpenInfo, String> {
    let start = Instant::now();
    let data = clipboard::image_data().ok_or("クリップボードに画像がありません")?;
    let prepared = blocking(move || {
        let decoded = decode::decode_file(&data).map_err(|e| format!("画像を貼り付けられません\n({e})"))?;
        let decoded = decode::Decoded { format: Format::Png, frame_count: 1, ..decoded };
        let exif = ExifInfo { empty: true, ..ExifInfo::default() };
        let source = Source { format: Some(Format::Png), pasted: true, ..Source::default() };
        Ok(prepare(save::PASTED_NAME.into(), decoded, elapsed_ms(start), exif, source))
    })
    .await?;
    store(&state, &window, prepared)
}

/// プレビューに重ねる、ジオラマのピントの帯のガイドの線。trimmed（切り抜いた範囲だけの表示）なら
/// 表示している写真そのものに対する位置。zoomed（100% 表示）なら保存結果の写真の部分に対する位置。
#[tauri::command]
pub fn diorama_guide(
    settings: EditSettings,
    trimmed: bool,
    zoomed: bool,
    state: State<'_, AppState>,
) -> Result<pipeline::DioramaGuide, String> {
    let loaded = state.0.lock().map_err(|e| e.to_string())?;
    if zoomed {
        let original = loaded.original.as_ref().ok_or("画像が読み込まれていません")?;
        return pipeline::actual_size_diorama_guide(original.dimensions(), &settings)
            .map_err(|e| e.to_string());
    }
    let preview = loaded.preview.as_ref().ok_or("画像が読み込まれていません")?;
    let settings = if trimmed { EditSettings { crop: None, ..settings } } else { settings };
    Ok(pipeline::diorama_guide(preview.dimensions(), &settings, loaded.factor))
}
