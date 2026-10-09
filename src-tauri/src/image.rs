//! 画像を開く・閉じる・プレビュー・100% 表示・クリップボードの画像・ジオラマのガイド。

use std::path::PathBuf;
use std::time::Instant;

use imageeditorrt_core::exif_info::ExifInfo;
use imageeditorrt_core::formats::Format;
use imageeditorrt_core::pipeline::{self, EditSettings};
use imageeditorrt_core::transform::CropRect;
use imageeditorrt_core::{collage, decode, load, save};
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
    let opened_path = path.clone();
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
    let info = store(&state, &window, prepared)?;
    // 開けたファイルを「最近使った項目」に足す
    crate::recent::opened(tauri::Manager::app_handle(&window), &opened_path);
    Ok(info)
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
    let opened = state.opened()?;
    let settings = opened.shown(settings, comparing);
    let ((image, histogram), render_time) = blocking(move || {
        let start = Instant::now();
        let preview = opened.prepared(&opened.preview, &settings);
        let rendered = pipeline::render_preview_with_histogram(&preview, &settings, opened.factor, trimmed);
        Ok((rendered, start.elapsed()))
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

/// テイストの一覧の見本（今の設定のまま、テイストだけを替えた小さな完成形。順は filter_types と同じ）。
///
/// 返すバイト列: 先頭 4 バイトが見本の数、その後ろに見本ごとの幅・高さ（u32 リトルエンディアン）と
/// RGBA の画素を続ける。
#[tauri::command]
pub async fn filter_thumbnails(
    settings: EditSettings,
    state: State<'_, AppState>,
) -> Result<Response, String> {
    let opened = state.opened()?;
    let settings = opened.shown(settings, false);
    let thumbnails = blocking(move || {
        let preview = opened.prepared(&opened.preview, &settings);
        Ok(pipeline::filter_thumbnails(&preview, &settings, opened.factor, pipeline::THUMBNAIL_MAX_SIDE))
    })
    .await?;
    let total: usize = thumbnails.iter().map(|t| 8 + t.as_raw().len()).sum();
    let mut body = Vec::with_capacity(4 + total);
    body.extend_from_slice(&(thumbnails.len() as u32).to_le_bytes());
    for thumbnail in thumbnails {
        let (width, height) = thumbnail.dimensions();
        body.extend_from_slice(&width.to_le_bytes());
        body.extend_from_slice(&height.to_le_bytes());
        body.extend_from_slice(thumbnail.as_raw());
    }
    Ok(Response::new(body))
}

/// 顔の自動認識（投稿加工）: 隠す範囲（顔より少し大きい正方形）を、回転・反転した後の原寸の座標で返す。
#[tauri::command]
pub async fn detect_faces(
    settings: EditSettings,
    state: State<'_, AppState>,
) -> Result<Vec<CropRect>, String> {
    use imageeditorrt_core::faces;
    find_regions(settings, state, faces::detect_faces, faces::cover_rect).await
}

/// 文字の自動認識（投稿加工）: 隠す範囲（文字の範囲を少し広げたもの）を、回転・反転した後の原寸の座標で返す。
#[tauri::command]
pub async fn detect_text(
    settings: EditSettings,
    state: State<'_, AppState>,
) -> Result<Vec<CropRect>, String> {
    use imageeditorrt_core::{text_regions, transform};
    find_regions(settings, state, text_regions::detect_text, transform::clamp_crop).await
}

/// おまかせ切り抜き: プレビュー用の画像を今の向きにして目立つ部分を探し、選んでいる比でその部分が中央寄りに入る
/// 範囲を、回転・反転した後の原寸の座標で返す。目立つ部分が見つからなければ None。
#[tauri::command]
pub async fn auto_crop(
    settings: EditSettings,
    aspect: imageeditorrt_core::crop::AspectChoice,
    state: State<'_, AppState>,
) -> Result<Option<CropRect>, String> {
    let opened = state.opened()?;
    blocking(move || {
        let image = pipeline::straightened(&opened.preview, &settings);
        let Some(found) = imageeditorrt_core::saliency::salient_rect(&image)? else { return Ok(None) };
        let size = settings.orientation.size(opened.original.dimensions());
        let to_original = |v: i64| (v as f64 / opened.factor).round() as i64;
        let subject = CropRect::new(
            to_original(found.x),
            to_original(found.y),
            to_original(found.width),
            to_original(found.height),
        );
        Ok(imageeditorrt_core::crop::subject_crop(size, subject, aspect))
    })
    .await
}

/// プレビュー用の画像を今の向き（回転・反転・水平の補正）にして detect で範囲を探し、原寸の座標に直してから
/// finish（原寸の画像の大きさで、隠す範囲に仕上げる）にかけて返す（処理は別のスレッド）。
async fn find_regions(
    settings: EditSettings,
    state: State<'_, AppState>,
    detect: fn(&image::RgbaImage) -> Result<Vec<CropRect>, String>,
    finish: fn(CropRect, (u32, u32)) -> Option<CropRect>,
) -> Result<Vec<CropRect>, String> {
    let opened = state.opened()?;
    blocking(move || {
        let image = pipeline::straightened(&opened.preview, &settings);
        let size = settings.orientation.size(opened.original.dimensions());
        let to_original = |v: i64| (v as f64 / opened.factor).round() as i64;
        Ok(detect(&image)?
            .into_iter()
            .filter_map(|r| {
                let r = CropRect::new(
                    to_original(r.x),
                    to_original(r.y),
                    to_original(r.width),
                    to_original(r.height),
                );
                finish(r, size)
            })
            .collect())
    })
    .await
}

/// 傾きの自動補正（水平の補正の「自動」）: プレビュー用の画像を今の回転・反転にして（水平の補正はかけずに）傾きを
/// 求め、それを打ち消す水平の補正の角度（度）を返す。分からなければ None。
#[tauri::command]
pub async fn auto_straighten(
    settings: EditSettings,
    state: State<'_, AppState>,
) -> Result<Option<f64>, String> {
    let opened = state.opened()?;
    blocking(move || {
        imageeditorrt_core::horizon::straighten_angle(&settings.orientation.transpose(&opened.preview))
    })
    .await
}

/// 自動補正: 今の写真（切り抜く範囲。色の調整をかける前）から、露出・コントラスト・色温度のちょうどよい値を求める。
#[tauri::command]
pub async fn auto_adjust(
    settings: EditSettings,
    state: State<'_, AppState>,
) -> Result<imageeditorrt_core::auto::AutoAdjust, String> {
    let opened = state.opened()?;
    blocking(move || {
        let photo = pipeline::photo_for_analysis(&opened.preview, &settings, opened.factor);
        Ok(imageeditorrt_core::auto::auto_adjust(&photo))
    })
    .await
}

/// 背景を消す準備: 被写体のマスクをまだ作っていなければ、プレビュー用の画像から作って覚える（画像ごとに 1 回）。
/// 被写体があれば true。
#[tauri::command]
pub async fn prepare_background(state: State<'_, AppState>) -> Result<bool, String> {
    state
        .remember(
            |loaded| loaded.mask.as_ref().map(Option::is_some),
            imageeditorrt_core::foreground::foreground_mask,
            Option::is_some,
            |loaded, mask| loaded.mask = Some(mask.map(std::sync::Arc::new)),
        )
        .await
}

/// 肌をなめらかにする準備: 顔をまだ探していなければ、プレビュー用の画像から探して覚える（画像ごとに 1 回）。
/// 見つけた顔の数を返す。
#[tauri::command]
pub async fn prepare_faces(state: State<'_, AppState>) -> Result<usize, String> {
    state
        .remember(
            |loaded| loaded.faces.as_ref().map(|faces| faces.len()),
            imageeditorrt_core::faces::detect_faces,
            Vec::len,
            |loaded, faces| loaded.faces = Some(std::sync::Arc::new(faces)),
        )
        .await
}

/// リセット（旧版 FR-UI-42）: 読み込んだ画像を捨てて、未読込の状態に戻す。
#[tauri::command]
pub fn close_image(window: WebviewWindow, state: State<'_, AppState>) -> Result<(), String> {
    let _ = window.set_title(APP_NAME);
    *state.0.lock().map_err(|e| e.to_string())? = Loaded::default();
    Ok(())
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
    let opened = state.opened()?;
    let settings = opened.shown(settings, comparing);
    let image = blocking(move || {
        let original = opened.prepared(&opened.original, &settings);
        pipeline::apply_edits(&original, &settings).map_err(|e| e.to_string())
    })
    .await?;
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

/// 「並べて 1 枚に」の並べ方（値・名前・枚数）。
type LayoutChoice = (collage::CollageLayout, &'static str, usize);

/// 「並べて 1 枚に」の選択肢: 並べ方と縦横比。
#[tauri::command]
pub fn collage_choices() -> (Vec<LayoutChoice>, Vec<(u32, u32)>) {
    let layouts = collage::CollageLayout::ALL.iter().map(|&l| (l, l.label(), l.count())).collect();
    (layouts, collage::ASPECTS.to_vec())
}

/// 「並べて 1 枚に」: paths の写真を並べた 1 枚を作り、元のファイルのない画像として開く（PNG 扱い・EXIF なし）。
#[tauri::command]
pub async fn make_collage(
    paths: Vec<String>,
    options: collage::CollageOptions,
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> Result<OpenInfo, String> {
    let start = Instant::now();
    let prepared = blocking(move || {
        let mut images = Vec::with_capacity(paths.len());
        for path in &paths {
            let path = PathBuf::from(path);
            let loaded = load::load_file(&path).map_err(|e| e.message(&file_name(&path)))?;
            images.push(loaded.decoded.image);
        }
        let image = collage::make_collage(&images, &options);
        let decoded = decode::Decoded { image, format: Format::Png, frame_count: 1 };
        let exif = ExifInfo { empty: true, ..ExifInfo::default() };
        let source = Source { format: Some(Format::Png), pasted: true, ..Source::default() };
        Ok(prepare(collage::COLLAGE_NAME.into(), decoded, elapsed_ms(start), exif, source))
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
