//! 大きさ・範囲の計算（出力の大きさ・実際に切り抜く範囲・プレビュー用の換算・加工前の表示の設定）。

use image::RgbaImage;

use super::EditSettings;
use crate::frames::{self, FrameType};
use crate::shapes::{self, ShapeType};
use crate::transform::{self, round_half_even, CropRect, SizeError};

/// 画像を処理せずに、apply_edits の出力の大きさを求める。
pub fn output_size(original_size: (u32, u32), settings: &EditSettings) -> Result<(u32, u32), SizeError> {
    let mut size = settings.orientation.size(original_size);
    if let Some(rect) = effective_crop(size, settings.crop, settings.frame, settings.shape) {
        size = (rect.width as u32, rect.height as u32);
    }
    let size = transform::fit_size(size, settings.width, settings.height, settings.keep_aspect)?;
    Ok(frames::framed_size(size, settings.frame))
}

/// 出力の長辺を long_side px にした設定（複数の大きさで保存。旧版にはない）。
///
/// 回転・反転して切り抜いた後の写真（フレーム・円の比に合わせた範囲を含む）の向きで、横長なら幅、
/// 縦長なら高さを決め、縦横比は保つ。ほかの設定はそのまま。original_size は回転・反転する前の原寸。
pub fn long_side_settings(
    original_size: (u32, u32),
    settings: &EditSettings,
    long_side: u32,
) -> EditSettings {
    let size = settings.orientation.size(original_size);
    let (width, height) = effective_crop(size, settings.crop, settings.frame, settings.shape)
        .map_or(size, |r| (r.width as u32, r.height as u32));
    let (width, height) = if width >= height { (Some(long_side), None) } else { (None, Some(long_side)) };
    EditSettings { width, height, keep_aspect: true, ..settings.clone() }
}

/// 加工前の表示（旧版 FR-UI-44）の設定: 向き（水平の補正を含む）と、実際に切り抜く範囲だけを残す。
///
/// 色の調整・テイスト・ディテール・ジオラマ・周辺減光・経年劣化・形・フレーム・文字・リサイズは外す。
/// フレーム・円の比に合わせた範囲も、そのままの範囲で見比べられるよう切り抜く範囲として残す。
/// original_size は回転・反転する前の原寸。
pub fn before_settings(original_size: (u32, u32), settings: &EditSettings) -> EditSettings {
    let size = settings.orientation.size(original_size);
    let crop = effective_crop(size, settings.crop, settings.frame, settings.shape);
    EditSettings {
        orientation: settings.orientation,
        straighten: settings.straighten,
        perspective_vertical: settings.perspective_vertical,
        perspective_horizontal: settings.perspective_horizontal,
        crop,
        ..EditSettings::default()
    }
}

/// 実際に切り抜く範囲を返す（size の画像の座標）。切り抜かないなら None。
///
/// トリミング範囲は画像内に収まるよう補正する。フレームがあるときは、トリミング範囲（なければ
/// 画像全体）をフレームの写真部分の縦横比になるよう中央で切り抜く。フレームがなく形が円のときは、
/// 中央を正方形に切り抜く（フレームがあるときの円は写真部分の中に描くので、写真部分の比のまま）。
pub fn effective_crop(
    size: (u32, u32),
    crop: Option<CropRect>,
    frame: FrameType,
    shape: ShapeType,
) -> Option<CropRect> {
    let rect = crop.and_then(|c| transform::clamp_crop(c, size));
    let base = rect.unwrap_or(CropRect::whole(size));
    let base_size = (base.width as u32, base.height as u32);
    let aspect = frames::window_aspect(frame, base_size).or_else(|| shapes::shape_aspect(shape));
    match aspect {
        Some(aspect) => Some(transform::fit_aspect(base, aspect)),
        None => rect,
    }
}

/// 縮小プレビュー用に、トリミング範囲と出力の大きさを factor 倍に換算した設定を返す。
///
/// 換算後の幅・高さ・トリミング範囲の大きさは最小 1px。
pub fn scale_settings(settings: &EditSettings, factor: f64) -> EditSettings {
    assert!(factor > 0.0, "factor は正の数で指定してください: {factor}");
    let crop = settings.crop.map(|r| {
        // 左上と右下をそれぞれ換算し、端がずれないようにする
        let (left, top) = (scale(r.x, factor), scale(r.y, factor));
        let (right, bottom) = (scale(r.right(), factor), scale(r.bottom(), factor));
        let width = if r.width > 0 { (right - left).max(1) } else { r.width };
        let height = if r.height > 0 { (bottom - top).max(1) } else { r.height };
        CropRect::new(left, top, width, height)
    });
    let length =
        |v: Option<u32>| v.map(|v| (scale(i64::from(v), factor).max(1) as u32).min(transform::MAX_SIZE));
    EditSettings { crop, width: length(settings.width), height: length(settings.height), ..settings.clone() }
}

/// プレビュー用に長辺 max_side 以下へ縮めた画像と、その縮小率を返す。
///
/// 元から小さい画像は縮めず、縮小率 1.0 で複製を返す。縮小率は scale_settings にそのまま渡せる。
pub fn make_preview(original: &RgbaImage, max_side: u32) -> (RgbaImage, f64) {
    let (width, height) = original.dimensions();
    let long = width.max(height);
    if long <= max_side {
        return (original.clone(), 1.0);
    }
    let factor = f64::from(max_side) / f64::from(long);
    let size = |v: u32| round_half_even(f64::from(v) * factor).max(1) as u32;
    // プレビュー用の縮小は速さを優先する（旧版も reducing_gap で近似していた）
    (crate::resize::resize(original, size(width), size(height)), factor)
}

fn scale(value: i64, factor: f64) -> i64 {
    if value >= 0 {
        (value as f64 * factor + 0.5) as i64
    } else {
        -((-value as f64 * factor + 0.5) as i64)
    }
}

/// 保存する写真（切り抜き・リサイズ後）の短辺を、プレビュー（回転・反転した後の大きさ）から求める。
pub(super) fn saved_photo_short_side(
    preview_size: (u32, u32),
    settings: &EditSettings,
    factor: f64,
) -> Option<f64> {
    let original = (
        round_half_even(f64::from(preview_size.0) / factor) as u32,
        round_half_even(f64::from(preview_size.1) / factor) as u32,
    );
    let mut size = original;
    if let Some(rect) = effective_crop(original, settings.crop, settings.frame, settings.shape) {
        size = (rect.width as u32, rect.height as u32);
    }
    let (width, height) =
        transform::fit_size(size, settings.width, settings.height, settings.keep_aspect).ok()?;
    Some(f64::from(width.min(height)))
}
