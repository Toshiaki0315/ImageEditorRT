//! 計測モード（IMAGEEDITORRT_BENCH=1 で起動）: 画面が受け渡しを含めた速さを測り、結果を出して終わる。

use std::time::Instant;

use imageeditorrt_core::decode;
use imageeditorrt_core::exif_info::ExifInfo;
use imageeditorrt_core::formats::Format;
use imageeditorrt_core::{sample, save};
use tauri::{AppHandle, State, WebviewWindow};

use crate::state::{blocking, elapsed_ms, prepare, store, AppState, OpenInfo, Source};

/// 計測用の画像の大きさ（旧版のベンチマークと同じ 6000×4000）。
const SAMPLE_SIZE: (u32, u32) = (6000, 4000);
/// この環境変数があると、起動後に画面が受け渡しの計測をして結果を出力し、終了する。
const BENCH_ENV: &str = "IMAGEEDITORRT_BENCH";

/// 計測用の画像（6000×4000）を作って読み込んだことにする。
#[tauri::command]
pub async fn open_sample(state: State<'_, AppState>, window: WebviewWindow) -> Result<OpenInfo, String> {
    let prepared = blocking(|| {
        let start = Instant::now();
        let image = sample::synthetic_photo(SAMPLE_SIZE.0, SAMPLE_SIZE.1);
        let decoded = decode::Decoded { image, format: Format::Png, frame_count: 1 };
        Ok(prepare("計測用の画像".into(), decoded, elapsed_ms(start), ExifInfo::default(), Source::default()))
    })
    .await?;
    store(&state, &window, prepared)
}

/// 計測モードで起動したか。
#[tauri::command]
pub fn bench_mode() -> bool {
    std::env::var_os(BENCH_ENV).is_some()
}

/// 計測モードで保存を試すときの保存先（一時フォルダ）。
#[tauri::command]
pub fn bench_save_path() -> String {
    std::env::temp_dir().join("imageeditorrt-bench.jpg").to_string_lossy().into_owned()
}

/// 計測モードで読み込みを測る 12MP（4000×3000）の JPEG（一時フォルダに作る。旧版の NFR-01 と同じ大きさ）。
#[tauri::command]
pub async fn bench_jpeg_path() -> Result<String, String> {
    let path = std::env::temp_dir().join("imageeditorrt-bench-12mp.jpg");
    if !path.exists() {
        let image = sample::synthetic_photo(4000, 3000);
        let bytes = save::encode(&image, save::SaveFormat::Jpeg, 90, None).map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(path.to_string_lossy().into_owned())
}

/// 画面での計測の結果を標準出力に書き、計測モードなら終了する。
#[tauri::command]
pub fn report(text: String, app: AppHandle) {
    println!("{text}");
    if bench_mode() {
        app.exit(0);
    }
}

/// 画面での途中経過・エラーを標準エラー出力に書く（計測モードの確認用）。
#[tauri::command]
pub fn log(text: String) {
    eprintln!("[画面] {text}");
}
