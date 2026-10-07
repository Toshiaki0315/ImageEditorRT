//! 設定パネルの選択肢（テイスト・フレーム・形・文字・比）と、範囲・出力の大きさの計算（計算は core）。

use imageeditorrt_core::crop::{self, AspectChoice, DragMode, Oriented, SpinField};
use imageeditorrt_core::filters::FilterType;
use imageeditorrt_core::formats;
use imageeditorrt_core::frames::FrameType;
use imageeditorrt_core::output::{self, SizeResult, SizeState};
use imageeditorrt_core::pipeline::{self, EditSettings};
use imageeditorrt_core::privacy::Region;
use imageeditorrt_core::save;
use imageeditorrt_core::shapes::ShapeType;
use imageeditorrt_core::text::{TextFont, TextPosition};
use imageeditorrt_core::transform::{AspectRatio, CropRect, OrientOp, Orientation};
use tauri::State;

use crate::state::AppState;

/// 読み込める拡張子（ファイルを選ぶダイアログ・ドロップの判定に使う）・保存できる拡張子・対応形式の説明。
#[tauri::command]
pub fn supported_formats() -> (Vec<&'static str>, Vec<&'static str>, &'static str) {
    (formats::SUPPORTED_EXTENSIONS.to_vec(), save::SAVABLE_EXTENSIONS.to_vec(), formats::FORMATS_TEXT)
}

/// テイストの一覧（画面のプルダウンの順。JSON の名前と表示名）。
#[tauri::command]
pub fn filter_types() -> Vec<(FilterType, &'static str)> {
    FilterType::ALL.iter().map(|&f| (f, f.label())).collect()
}

/// 画面のプルダウンの選択肢（JSON の名前と表示名）。
type Choices<T> = Vec<(T, &'static str)>;

/// フレームと形の選択肢。
#[tauri::command]
pub fn frame_shape_types() -> (Choices<FrameType>, Choices<ShapeType>) {
    (
        FrameType::ALL.iter().map(|&f| (f, f.label())).collect(),
        ShapeType::ALL.iter().map(|&s| (s, s.label())).collect(),
    )
}

/// 文字・透かしのフォントと位置の選択肢。
#[tauri::command]
pub fn text_options() -> (Choices<TextFont>, Choices<TextPosition>) {
    (
        TextFont::ALL.iter().map(|&f| (f, f.label())).collect(),
        TextPosition::ALL.iter().map(|&p| (p, p.label())).collect(),
    )
}

/// 実際に切り抜く範囲（フレーム・円の比に合わせた範囲。回転・反転した後の原寸画像の座標）。
/// 切り抜かないなら None。画面で形の輪郭を重ねるのに使う。
#[tauri::command]
pub fn effective_crop(settings: EditSettings, size: (u32, u32)) -> Option<CropRect> {
    pipeline::effective_crop(size, settings.crop, settings.frame, settings.shape)
}

/// トリミングの比の選択肢（JSON の名前と表示名）。
#[tauri::command]
pub fn aspect_ratios() -> Vec<(AspectRatio, String)> {
    AspectRatio::ALL.iter().map(|&r| (r, r.label())).collect()
}

/// プレビュー上のドラッグ中の範囲（座標は回転・反転した後の原寸画像の座標）。
#[tauri::command]
pub fn crop_drag(
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
pub fn crop_spin(
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
pub fn crop_fit(rect: Option<CropRect>, aspect: AspectChoice, size: (u32, u32)) -> Option<CropRect> {
    crop::fit_to_aspect(rect, aspect, size)
}

/// 投稿加工で選べるスタンプの絵文字（画面の並びの順）。
#[tauri::command]
pub fn stamp_list() -> Vec<&'static str> {
    imageeditorrt_core::privacy::STAMPS.to_vec()
}

/// 回転・反転（トリミング範囲・投稿加工の範囲も一緒に回す）。
#[tauri::command]
pub fn crop_orient(
    orientation: Orientation,
    op: OrientOp,
    crop: Option<CropRect>,
    regions: Vec<Region>,
    size: (u32, u32),
) -> Oriented {
    crop::orient(orientation, op, crop, &regions, size)
}

/// 「出力」タブのサイズ変更: 欄に出す幅・高さと、編集設定に渡す幅・高さ。
#[tauri::command]
pub fn resolve_size(
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
pub fn rotate_size(state: SizeState) -> SizeState {
    output::rotate(state)
}

/// 設定をかけたときの出力の大きさ（ステータスバーに出す）。大きさの指定が範囲外ならエラー。
#[tauri::command]
pub fn output_size(settings: EditSettings, state: State<'_, AppState>) -> Result<(u32, u32), String> {
    let loaded = state.0.lock().map_err(|e| e.to_string())?;
    let original = loaded.original.as_ref().ok_or("画像が読み込まれていません")?;
    pipeline::output_size(original.dimensions(), &settings).map_err(|e| e.to_string())
}
