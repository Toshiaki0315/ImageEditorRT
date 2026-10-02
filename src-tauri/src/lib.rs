//! ImageEditorRT のアプリ本体（試作）。画像処理は imageeditorrt-core に任せ、
//! ここでは画面（TypeScript）との受け渡しだけを行う。

use std::sync::Mutex;
use std::time::Instant;

use image::RgbaImage;
use imageeditorrt_core::{decode, encode, preview, resize};
use serde::Serialize;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, State};

/// プレビューの長辺（Python 版と同じ 1600px）。
const PREVIEW_MAX_SIDE: u32 = 1600;
/// プレビューを JPEG で渡すときの画質。
const PREVIEW_JPEG_QUALITY: u8 = 85;
/// 計測用の画像の大きさ（Python 版のベンチマークと同じ 6000×4000）。
const SAMPLE_SIZE: (u32, u32) = (6000, 4000);
/// この環境変数があると、起動後に画面が受け渡しの計測をして結果を出力し、終了する。
const BENCH_ENV: &str = "IMAGEEDITORRT_BENCH";

/// 読み込んだ原本（不変）と、それを縮めたプレビュー用の画像。
#[derive(Default)]
struct Loaded {
    original: Option<RgbaImage>,
    preview: Option<RgbaImage>,
}

#[derive(Default)]
struct AppState(Mutex<Loaded>);

/// 読み込みの結果（画面に出す情報）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenInfo {
    width: u32,
    height: u32,
    preview_width: u32,
    preview_height: u32,
    decode_ms: f64,
    resize_ms: f64,
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn store(state: &AppState, original: RgbaImage, decode_ms: f64) -> Result<OpenInfo, String> {
    let start = Instant::now();
    let small = resize::fit_long_side(&original, PREVIEW_MAX_SIDE);
    let info = OpenInfo {
        width: original.width(),
        height: original.height(),
        preview_width: small.width(),
        preview_height: small.height(),
        decode_ms,
        resize_ms: elapsed_ms(start),
    };
    let mut loaded = state.0.lock().map_err(|e| e.to_string())?;
    loaded.original = Some(original);
    loaded.preview = Some(small);
    Ok(info)
}

fn decode_bytes(bytes: &[u8]) -> Result<RgbaImage, String> {
    decode::decode(bytes).map_err(|e| format!("画像を読み込めません: {e:?}"))
}

/// 画面から渡されたファイルの中身（バイト列のまま）を読み込む。
#[tauri::command]
async fn open_bytes(request: Request<'_>, state: State<'_, AppState>) -> Result<OpenInfo, String> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("ファイルの中身がバイト列で渡されていません".into());
    };
    let start = Instant::now();
    let image = decode_bytes(bytes)?;
    store(&state, image, elapsed_ms(start))
}

/// ドロップされたファイルを読み込む。
#[tauri::command]
async fn open_path(path: String, state: State<'_, AppState>) -> Result<OpenInfo, String> {
    let start = Instant::now();
    let bytes = std::fs::read(&path).map_err(|e| format!("{path} を読めません: {e}"))?;
    let image = decode_bytes(&bytes)?;
    store(&state, image, elapsed_ms(start))
}

/// 計測用の画像（6000×4000）を作って読み込んだことにする。
#[tauri::command]
async fn open_sample(state: State<'_, AppState>) -> Result<OpenInfo, String> {
    let start = Instant::now();
    let image = preview::synthetic_photo(SAMPLE_SIZE.0, SAMPLE_SIZE.1);
    store(&state, image, elapsed_ms(start))
}

/// プレビューに設定をかけて返す。
///
/// 返すバイト列: 先頭 16 バイトが幅・高さ・処理の時間 (µs)・変換の時間 (µs)（どれも u32 リトルエンディアン）、
/// その後ろが画素（format が "rgba" なら RGBA のまま、"jpeg" なら JPEG）。
#[tauri::command]
async fn render_preview(
    settings: preview::Settings,
    format: String,
    state: State<'_, AppState>,
) -> Result<Response, String> {
    let rendered = {
        let loaded = state.0.lock().map_err(|e| e.to_string())?;
        let image = loaded.preview.as_ref().ok_or("画像が読み込まれていません")?;
        let start = Instant::now();
        (preview::render(image, &settings), start.elapsed())
    };
    let (image, render_time) = rendered;
    let (width, height) = image.dimensions();
    let start = Instant::now();
    let pixels = match format.as_str() {
        "jpeg" => encode::to_jpeg(&image, PREVIEW_JPEG_QUALITY),
        "rgba" => image.into_raw(),
        other => return Err(format!("知らない形式です: {other}")),
    };
    let encode_time = start.elapsed();
    let mut body = Vec::with_capacity(16 + pixels.len());
    for v in [width, height, render_time.as_micros() as u32, encode_time.as_micros() as u32] {
        body.extend_from_slice(&v.to_le_bytes());
    }
    body.extend_from_slice(&pixels);
    Ok(Response::new(body))
}

/// 計測モードで起動したか。
#[tauri::command]
fn bench_mode() -> bool {
    std::env::var_os(BENCH_ENV).is_some()
}

/// 画面での計測の結果を標準出力に書き、計測モードなら終了する。
#[tauri::command]
fn report(text: String, app: AppHandle) {
    println!("{text}");
    if bench_mode() {
        app.exit(0);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_bytes,
            open_path,
            open_sample,
            render_preview,
            bench_mode,
            report
        ])
        .run(tauri::generate_context!())
        .expect("ImageEditorRT を起動できませんでした");
}
