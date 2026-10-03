//! ImageEditorRT のアプリ本体。画像処理は imageeditorrt-core に任せ、
//! ここでは画面（TypeScript）との受け渡し・メニュー・ファイルを開く経路だけを扱う。

mod batch;
#[cfg(target_os = "macos")]
mod clipboard;
mod diagnostics;
mod menu;
mod open;
mod presets;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use image::RgbaImage;
use imageeditorrt_core::crop::{self, AspectChoice, DragMode, Oriented, SpinField};
use imageeditorrt_core::decode::{self, DecodeError};
use imageeditorrt_core::exif_info::{raw_exif, read_exif_info, ExifInfo, GpsPosition};
use imageeditorrt_core::filters::FilterType;
use imageeditorrt_core::formats::{self, Format};
use imageeditorrt_core::frames::FrameType;
use imageeditorrt_core::output::{self, SizeResult, SizeState};
use imageeditorrt_core::pipeline::{self, EditSettings, PREVIEW_MAX_SIDE};
use imageeditorrt_core::sample;
use imageeditorrt_core::save::{self, SaveError, SaveOptions, SAME_FILE_MESSAGE};
use imageeditorrt_core::shapes::ShapeType;
use imageeditorrt_core::text::{TextFont, TextPosition};
use imageeditorrt_core::transform::{AspectRatio, CropRect, OrientOp, Orientation};
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

/// 計測用の画像の大きさ（旧版のベンチマークと同じ 6000×4000）。
const SAMPLE_SIZE: (u32, u32) = (6000, 4000);
/// この環境変数があると、起動後に画面が受け渡しの計測をして結果を出力し、終了する。
const BENCH_ENV: &str = "IMAGEEDITORRT_BENCH";
/// ウィンドウのタイトル（画像を開くと「ファイル名 — ImageEditorRT」）。
const APP_NAME: &str = "ImageEditorRT";

/// 読み込んだ原本（不変）と、それを縮めたプレビュー用の画像・縮小率、元のファイルの情報。
#[derive(Default)]
struct Loaded {
    /// 保存のときは別のスレッドで使うので、複製せずに共有する
    original: Option<Arc<RgbaImage>>,
    /// プレビューの処理中に鍵を持ち続けないよう、複製せずに共有する
    preview: Option<Arc<RgbaImage>>,
    factor: f64,
    source: Source,
}

/// 元のファイル（保存の名前・元の画像への上書きの防止・EXIF を残すのに使う）。
#[derive(Clone, Default)]
struct Source {
    /// 元のファイルのパス（計測用の画像などファイルがなければ None）
    path: Option<PathBuf>,
    format: Option<Format>,
    /// 元の EXIF（TIFF の部分）
    exif: Option<Vec<u8>>,
    /// クリップボードから貼り付けた画像（保存の初期の名前を「クリップボード_日時.png」にする）
    pasted: bool,
}

#[derive(Default)]
pub(crate) struct AppState(Mutex<Loaded>);

/// 読み込みの結果（画面に出す情報）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenInfo {
    /// ファイル名（パスなし）
    name: String,
    format: Option<Format>,
    width: u32,
    height: u32,
    preview_width: u32,
    preview_height: u32,
    /// 透明・半透明の画素があるか（プレビューで市松模様を出す）
    has_alpha: bool,
    /// 1 より大きければ先頭のフレーム（ページ）だけを扱っている
    frame_count: usize,
    /// 読み込み（ファイルの読み込み＋画素にする）・縮小にかかった時間 (ms)
    decode_ms: f64,
    resize_ms: f64,
    exif: ExifInfo,
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// 読み込んだ画像を状態に置き、ウィンドウのタイトルを変える。
/// 重い処理（読み込み・縮小など）を、非同期の処理のスレッドを止めないよう別のスレッドで行う。
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| e.to_string())?
}

/// 読み込んだ画像からプレビュー用の縮小版を作り、画面に返す情報と、覚えておく状態をそろえる（別のスレッドで呼ぶ）。
fn prepare(
    name: String,
    decoded: decode::Decoded,
    decode_ms: f64,
    exif: ExifInfo,
    source: Source,
) -> (OpenInfo, Loaded) {
    let start = Instant::now();
    let original = decoded.image;
    let (small, factor) = pipeline::make_preview(&original, PREVIEW_MAX_SIDE);
    let info = OpenInfo {
        format: Some(decoded.format),
        width: original.width(),
        height: original.height(),
        preview_width: small.width(),
        preview_height: small.height(),
        has_alpha: formats::has_transparency(&small),
        frame_count: decoded.frame_count,
        decode_ms,
        resize_ms: elapsed_ms(start),
        exif,
        name,
    };
    let loaded =
        Loaded { original: Some(Arc::new(original)), preview: Some(Arc::new(small)), factor, source };
    (info, loaded)
}

/// 読み込んだ画像を今の画像にし、ウィンドウのタイトルを「ファイル名 — ImageEditorRT」にする。
fn store(
    state: &AppState,
    window: &WebviewWindow,
    (info, loaded): (OpenInfo, Loaded),
) -> Result<OpenInfo, String> {
    let _ = window.set_title(&format!("{} — {APP_NAME}", info.name));
    *state.0.lock().map_err(|e| e.to_string())? = loaded;
    Ok(info)
}

/// 画像のファイルを読み込む。読めなければ、ダイアログに出す説明を返す（旧版 FR-IO-10）。
#[tauri::command]
async fn open_path(
    path: String,
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> Result<OpenInfo, String> {
    let path = PathBuf::from(path);
    let name = file_name(&path);
    if !formats::is_supported(&path) {
        let ext = path.extension().map_or("(なし)".into(), |e| format!(".{}", e.to_string_lossy()));
        return Err(format!("対応していない拡張子です: {ext}"));
    }
    let prepared = blocking(move || {
        let start = Instant::now();
        let bytes = std::fs::read(&path).map_err(|e| format!("画像を読み込めません: {name}\n({e})"))?;
        let decoded = decode::decode_file(&bytes).map_err(|e| match e {
            DecodeError::UnsupportedFormat(_) => e.to_string(),
            _ => format!("画像を読み込めません: {name}\n({e})"),
        })?;
        let decode_ms = elapsed_ms(start);
        // EXIF が壊れていても画像は開く（EXIF なしとして扱う）
        let exif = read_exif_info(&bytes);
        let source =
            Source { format: Some(decoded.format), exif: raw_exif(&bytes), path: Some(path), pasted: false };
        Ok(prepare(name, decoded, decode_ms, exif, source))
    })
    .await?;
    store(&state, &window, prepared)
}

/// 計測用の画像（6000×4000）を作って読み込んだことにする。
#[tauri::command]
async fn open_sample(state: State<'_, AppState>, window: WebviewWindow) -> Result<OpenInfo, String> {
    let prepared = blocking(|| {
        let start = Instant::now();
        let image = sample::synthetic_photo(SAMPLE_SIZE.0, SAMPLE_SIZE.1);
        let decoded = decode::Decoded { image, format: Format::Png, frame_count: 1 };
        Ok(prepare("計測用の画像".into(), decoded, elapsed_ms(start), ExifInfo::default(), Source::default()))
    })
    .await?;
    store(&state, &window, prepared)
}

/// 読み込める拡張子（ファイルを選ぶダイアログ・ドロップの判定に使う）・保存できる拡張子・対応形式の説明。
#[tauri::command]
fn supported_formats() -> (Vec<&'static str>, Vec<&'static str>, &'static str) {
    (formats::SUPPORTED_EXTENSIONS.to_vec(), save::SAVABLE_EXTENSIONS.to_vec(), formats::FORMATS_TEXT)
}

/// テイストの一覧（画面のプルダウンの順。JSON の名前と表示名）。
#[tauri::command]
fn filter_types() -> Vec<(FilterType, &'static str)> {
    FilterType::ALL.iter().map(|&f| (f, f.label())).collect()
}

/// プレビューに設定をかけて返す。trimmed なら切り抜いた範囲だけを表示する。
///
/// 返すバイト列: 先頭 12 バイトが幅・高さ・処理の時間 (µs)（どれも u32 リトルエンディアン）、
/// その後ろが RGBA の画素、最後が保存される写真のヒストグラム（R・G・B・輝度の順に 256 個ずつの
/// u32 リトルエンディアン）。JSON にしないので、1600px の画像でも受け渡しは数 ms で済む。
#[tauri::command]
async fn render_preview(
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
fn close_image(window: WebviewWindow, state: State<'_, AppState>) -> Result<(), String> {
    let _ = window.set_title(APP_NAME);
    *state.0.lock().map_err(|e| e.to_string())? = Loaded::default();
    Ok(())
}

/// 画面が終了してよいと確かめたか（Dock の「終了」などで届く終了の求めを、それまでは止める）。
static QUIT_CONFIRMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// 終了の求めを画面に知らせるイベント（画面が未保存の変更を確かめて、quit_app を呼ぶ）。
const QUIT_EVENT: &str = "quit-requested";

/// アプリを終了する（画面が未保存の変更を確かめた後に呼ぶ）。
#[tauri::command]
fn quit_app(app: AppHandle) {
    QUIT_CONFIRMED.store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
}

/// 表示に使う設定。comparing（加工前の表示）なら、向きと切り抜く範囲だけを残す。
fn shown_settings(loaded: &Loaded, settings: EditSettings, comparing: bool) -> EditSettings {
    match (&loaded.original, comparing) {
        (Some(original), true) => pipeline::before_settings(original.dimensions(), &settings),
        _ => settings,
    }
}

/// 100% 表示: 原寸で処理した保存結果（comparing なら加工前）を返す（処理は別のスレッド）。
///
/// 返すバイト列: 先頭 8 バイトが幅・高さ（u32 リトルエンディアン）、その後ろが RGBA の画素。
#[tauri::command]
async fn render_actual_size(
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

/// メニューの項目を使える・使えないにする（100% で表示・画面に合わせる）。
#[tauri::command]
fn set_menu_enabled(id: String, enabled: bool, app: AppHandle) -> Result<(), String> {
    menu::set_enabled(&app, &id, enabled).map_err(|e| e.to_string())
}

/// メニューのチェックの付く項目の状態を変える（環境設定に残した状態に合わせる）。
#[tauri::command]
fn set_menu_checked(id: String, checked: bool, app: AppHandle) -> Result<(), String> {
    menu::set_checked(&app, &id, checked).map_err(|e| e.to_string())
}

/// 保存ダイアログの初期のパス `<元の名前>_edited.<拡張子>`（重ならない名前）。
///
/// 貼り付けた画像は `~/ピクチャ/クリップボード_<stamp>.png`（stamp は画面が渡す今の日時）。
/// どちらでもなければ None（計測用の画像など）。
#[tauri::command]
fn default_save_path(stamp: String, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let loaded = state.0.lock().map_err(|e| e.to_string())?;
    let path = match (&loaded.source.path, loaded.source.pasted) {
        (Some(path), _) => Some(save::default_save_path(path)),
        #[cfg(target_os = "macos")]
        (None, true) => clipboard::pictures_or_home().map(|folder| save::pasted_save_path(&stamp, &folder)),
        _ => None,
    };
    Ok(path.map(|p| p.to_string_lossy().into_owned()))
}

/// クリップボードにあるもの（Finder でコピーしたファイル・画像・文字があるか）。
#[cfg(target_os = "macos")]
#[tauri::command]
fn clipboard_contents() -> clipboard::Contents {
    clipboard::contents()
}

/// クリップボードの文字（入力欄に貼り付ける）。
#[cfg(target_os = "macos")]
#[tauri::command]
fn clipboard_text() -> Option<String> {
    clipboard::text()
}

/// クリップボードの画像を、元のファイルのない画像として開く（PNG 扱い・EXIF なし・透過は残す）。
#[cfg(target_os = "macos")]
#[tauri::command]
async fn open_clipboard_image(state: State<'_, AppState>, window: WebviewWindow) -> Result<OpenInfo, String> {
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

/// 保存できなかったとき、画面に返す理由。kind が "sameFile" なら保存ダイアログを開き直す。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveFailure {
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
async fn save_image(
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

/// プレビューに重ねる、ジオラマのピントの帯のガイドの線。trimmed（切り抜いた範囲だけの表示）なら
/// 表示している写真そのものに対する位置。zoomed（100% 表示）なら保存結果の写真の部分に対する位置。
#[tauri::command]
fn diorama_guide(
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

/// 画面のプルダウンの選択肢（JSON の名前と表示名）。
type Choices<T> = Vec<(T, &'static str)>;

/// フレームと形の選択肢。
#[tauri::command]
fn frame_shape_types() -> (Choices<FrameType>, Choices<ShapeType>) {
    (
        FrameType::ALL.iter().map(|&f| (f, f.label())).collect(),
        ShapeType::ALL.iter().map(|&s| (s, s.label())).collect(),
    )
}

/// 文字・透かしのフォントと位置の選択肢。
#[tauri::command]
fn text_options() -> (Choices<TextFont>, Choices<TextPosition>) {
    (
        TextFont::ALL.iter().map(|&f| (f, f.label())).collect(),
        TextPosition::ALL.iter().map(|&p| (p, p.label())).collect(),
    )
}

/// 実際に切り抜く範囲（フレーム・円の比に合わせた範囲。回転・反転した後の原寸画像の座標）。
/// 切り抜かないなら None。画面で形の輪郭を重ねるのに使う。
#[tauri::command]
fn effective_crop(settings: EditSettings, size: (u32, u32)) -> Option<CropRect> {
    pipeline::effective_crop(size, settings.crop, settings.frame, settings.shape)
}

/// トリミングの比の選択肢（JSON の名前と表示名）。
#[tauri::command]
fn aspect_ratios() -> Vec<(AspectRatio, String)> {
    AspectRatio::ALL.iter().map(|&r| (r, r.label())).collect()
}

/// プレビュー上のドラッグ中の範囲（座標は回転・反転した後の原寸画像の座標）。
#[tauri::command]
fn crop_drag(
    mode: DragMode,
    anchor: (i64, i64),
    point: (i64, i64),
    start: Option<CropRect>,
    aspect: AspectChoice,
    size: (u32, u32),
) -> Option<CropRect> {
    crop::drag(mode, anchor, point, start, aspect, size)
}

/// トリミングの数値の欄を変えたときの範囲（比を保つ）。
#[tauri::command]
fn crop_spin(
    field: SpinField,
    values: CropRect,
    previous: Option<CropRect>,
    aspect: AspectChoice,
    size: (u32, u32),
) -> Option<CropRect> {
    crop::spin_edit(field, values, previous, aspect, size)
}

/// 比を変えたとき、範囲の中央を新しい比に直す。
#[tauri::command]
fn crop_fit(rect: Option<CropRect>, aspect: AspectChoice, size: (u32, u32)) -> Option<CropRect> {
    crop::fit_to_aspect(rect, aspect, size)
}

/// 回転・反転（範囲も一緒に回す）。
#[tauri::command]
fn crop_orient(orientation: Orientation, op: OrientOp, crop: Option<CropRect>, size: (u32, u32)) -> Oriented {
    crop::orient(orientation, op, crop, size)
}

/// 「出力」タブのサイズ変更: 欄に出す幅・高さと、編集設定に渡す幅・高さ。
#[tauri::command]
fn resolve_size(
    settings: EditSettings,
    state: SizeState,
    app: State<'_, AppState>,
) -> Result<SizeResult, String> {
    let loaded = app.0.lock().map_err(|e| e.to_string())?;
    let original = loaded.original.as_ref().ok_or("画像が読み込まれていません")?;
    Ok(output::resolve(original.dimensions(), &settings, state))
}

/// 90° 回転したときのサイズ変更の欄（手で変えた幅・高さを入れ替える）。
#[tauri::command]
fn rotate_size(state: SizeState) -> SizeState {
    output::rotate(state)
}

/// 撮影した場所を macOS のマップアプリで開く（「EXIF」タブの「マップで開く」。押したときだけ）。
#[tauri::command]
fn open_map(latitude: f64, longitude: f64) -> Result<(), String> {
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return Err("位置情報が正しくありません".into());
    }
    let url = GpsPosition { latitude, longitude }.map_url();
    std::process::Command::new("/usr/bin/open").arg(url).status().map_err(|e| e.to_string())?;
    Ok(())
}

/// 設定をかけたときの出力の大きさ（ステータスバーに出す）。大きさの指定が範囲外ならエラー。
#[tauri::command]
fn output_size(settings: EditSettings, state: State<'_, AppState>) -> Result<(u32, u32), String> {
    let loaded = state.0.lock().map_err(|e| e.to_string())?;
    let original = loaded.original.as_ref().ok_or("画像が読み込まれていません")?;
    pipeline::output_size(original.dimensions(), &settings).map_err(|e| e.to_string())
}

/// 計測モードで起動したか。
#[tauri::command]
fn bench_mode() -> bool {
    std::env::var_os(BENCH_ENV).is_some()
}

/// 計測モードで保存を試すときの保存先（一時フォルダ）。
#[tauri::command]
fn bench_save_path() -> String {
    std::env::temp_dir().join("imageeditorrt-bench.jpg").to_string_lossy().into_owned()
}

/// 計測モードで読み込みを測る 12MP（4000×3000）の JPEG（一時フォルダに作る。旧版の NFR-01 と同じ大きさ）。
#[tauri::command]
async fn bench_jpeg_path() -> Result<String, String> {
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
fn report(text: String, app: AppHandle) {
    println!("{text}");
    if bench_mode() {
        app.exit(0);
    }
}

/// 画面での途中経過・エラーを標準エラー出力に書く（計測モードの確認用）。
#[tauri::command]
fn log(text: String) {
    eprintln!("[画面] {text}");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ビルドの後の起動確認（画面を出さずに、画像を読めるかだけを確かめて終わる）
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if let Some(i) = args.iter().position(|a| a == diagnostics::SMOKE_TEST_FLAG) {
        let file = args.get(i + 1).map(PathBuf::from);
        std::process::exit(diagnostics::smoke_test(file.as_deref()));
    }
    // 想定外のエラー（パニック）はログに書き、画面で知らせる（旧版 NFR-04）
    diagnostics::install_panic_hook();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .manage(presets::PresetStore::default())
        .manage(batch::BatchState::default())
        .manage(open::Pending::from_args(std::env::args_os().skip(1)))
        .menu(menu::build)
        .on_menu_event(menu::on_event)
        .setup(|app| {
            diagnostics::set_app(app.handle().clone());
            // 計測ではウィンドウが隠れていると描画が止まるので、前に出す
            if bench_mode() {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_focus();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_path,
            open_sample,
            supported_formats,
            filter_types,
            render_preview,
            output_size,
            open_map,
            resolve_size,
            rotate_size,
            diorama_guide,
            aspect_ratios,
            frame_shape_types,
            text_options,
            effective_crop,
            crop_drag,
            crop_spin,
            crop_fit,
            crop_orient,
            default_save_path,
            set_menu_checked,
            set_menu_enabled,
            close_image,
            quit_app,
            clipboard_contents,
            clipboard_text,
            open_clipboard_image,
            diagnostics::report_unexpected,
            render_actual_size,
            save_image,
            open::take_pending_paths,
            presets::load_presets,
            presets::default_preset_name,
            presets::check_preset_name,
            presets::save_preset,
            presets::delete_preset,
            presets::apply_preset,
            presets::show_preset_menu,
            batch::batch_current_source,
            batch::batch_collect,
            batch::run_batch,
            batch::cancel_batch,
            bench_mode,
            bench_save_path,
            bench_jpeg_path,
            log,
            report
        ])
        .build(tauri::generate_context!())
        .expect("ImageEditorRT を起動できませんでした");
    app.run(|handle, event| {
        // Dock の「終了」・ログアウトなどで届く終了の求めは、画面が未保存の変更を確かめるまで止める
        if let tauri::RunEvent::ExitRequested { code: None, api, .. } = &event {
            if !QUIT_CONFIRMED.load(std::sync::atomic::Ordering::SeqCst) {
                api.prevent_exit();
                let _ = handle.emit(QUIT_EVENT, ());
                return;
            }
        }
        // Finder の「このアプリケーションで開く」・Dock のアイコンへのドロップ
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Opened { urls } = event {
            open::opened(handle, urls);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (handle, event);
    });
}
