//! 編集設定 (EditSettings) と、それを画像にかける処理の流れ（旧版の core/pipeline.py を移したもの）。
//!
//! 処理順（旧版 §5.1）: 回転・反転 →（水平の補正 → 遠近の補正 → 投稿加工のぼかし・モザイク）→ トリミング → リサイズ → 露出 → 明るさ → コントラスト →
//! 色温度 →（トーンカーブ → ハイライト／シャドウ）→ 彩度 →（色ごとの調整 → 部分補正） → ディテール（ノイズ除去 → ぼかし → シャープ） → ジオラマ →
//! フィルター →（LUT）→ 周辺減光 → 経年劣化 →（投稿加工のスタンプ）→ 文字（ロゴ・全体に繰り返す透かしも）。
//! 顔の枠・被写体のマスクが要る赤目・肌・背景は、ここより前に元の画像にかける（`prepare`）。
//! フレーム・形（#13）は、トリミングの後の比への切り抜きと、経年劣化の後にここへ足す。

use image::{imageops, RgbaImage};
use rayon::prelude::*;

use crate::adjust::{self, Lut};
pub use crate::background::Background;
use crate::curve;
pub use crate::curve::{HslAdjust, HSL_BANDS};
use crate::diorama;
pub use crate::diorama::DioramaDirection;
use crate::effects;
use crate::filters;
pub use crate::filters::FilterType;
pub use crate::frames::FrameType;
use crate::frames::{self, FRAME_COLOR};
use crate::histogram::{compute_histogram, Histogram};
use crate::local;
use crate::logo;
pub use crate::logo::LogoSettings;
use crate::lut;
use crate::perspective;
use crate::privacy;
use crate::shapes;
pub use crate::shapes::ShapeType;
use crate::text;
pub use crate::text::{TextFont, TextPosition, TextSettings};
use crate::tile;
use crate::transform::{self, CropRect, SizeError};

mod guide;
mod settings;
mod sizes;
#[cfg(test)]
mod tests;

pub use guide::{actual_size_diorama_guide, diorama_guide, DioramaGuide};
pub use settings::{
    EditSettings, FILTER_STRENGTH_FULL, FILTER_STRENGTH_MAX, PREVIEW_MAX_SIDE, THUMBNAIL_MAX_SIDE,
};
use sizes::saved_photo_short_side;
pub use sizes::{
    before_settings, effective_crop, long_side_settings, make_preview, output_size, scale_settings,
};

/// 回転・反転し、水平の補正と投稿加工の範囲（ぼかし・モザイク）をかけた画像を返す（大きさは回転・反転した
/// 後のもの）。image は原寸を factor 倍にした画像。
fn orient(image: &RgbaImage, settings: &EditSettings, factor: f64) -> RgbaImage {
    let mut image = straightened(image, settings);
    privacy::cover(&mut image, &settings.regions, factor);
    image
}

/// 自動補正で調べる写真: プレビュー用の画像 image（原寸を factor 倍にしたもの）を今の向き（回転・反転・水平の
/// 補正）にし、実際に切り抜く範囲（なければ全体）だけにしたもの。色の調整はかけない。
pub fn photo_for_analysis(image: &RgbaImage, settings: &EditSettings, factor: f64) -> RgbaImage {
    let image = straightened(image, settings);
    let scaled = scale_settings(settings, factor);
    match effective_crop(image.dimensions(), scaled.crop, settings.frame, settings.shape) {
        Some(rect) => transform::crop(&image, rect),
        None => image,
    }
}

/// 回転・反転し、水平の補正・遠近の補正をかけた画像（投稿加工の範囲の座標と同じ向き。顔の認識に使う）。
pub fn straightened(image: &RgbaImage, settings: &EditSettings) -> RgbaImage {
    let image = settings.orientation.transpose(image);
    let image =
        if settings.straighten == 0.0 { image } else { transform::straighten(&image, settings.straighten) };
    // 遠近の補正（大きさは変わらないので、範囲の座標はそのまま）
    if settings.perspective_vertical == 0 && settings.perspective_horizontal == 0 {
        return image;
    }
    perspective::correct(&image, settings.perspective_vertical, settings.perspective_horizontal)
}

/// 原画像に編集をかけた新しい画像を返す（原画像は変更しない）。保存に使う。
pub fn apply_edits(original: &RgbaImage, settings: &EditSettings) -> Result<RgbaImage, SizeError> {
    let mut image = orient(original, settings, 1.0);
    let photo = effective_crop(image.dimensions(), settings.crop, settings.frame, settings.shape);
    if let Some(rect) = photo {
        image = transform::crop(&image, rect);
    }
    let photo = photo.unwrap_or(CropRect::whole(image.dimensions()));
    let size =
        transform::fit_size(image.dimensions(), settings.width, settings.height, settings.keep_aspect)?;
    let image = transform::resize_to(&image, size);
    // スタンプの位置: 原寸の座標を、切り抜いてリサイズした後の画像の座標にする
    let scale = (f64::from(size.0) / photo.width as f64, f64::from(size.1) / photo.height as f64);
    let stamp_place = (scale, (photo.x as f64 * scale.0, photo.y as f64 * scale.1));

    let reference = f64::from(image.width().min(image.height()));
    let adjusted = apply_local_adjustments(apply_basic_adjustments(image, settings), settings, stamp_place);
    let image = apply_detail(adjusted, settings, reference, None);
    let image = apply_diorama_and_filter(image, settings, None);
    let mut image = image;
    adjust::vignette(&mut image, settings.vignette);
    adjust::aging(&mut image, settings.aging);
    privacy::stamp(&mut image, &settings.regions, stamp_place.0, stamp_place.1);
    Ok(apply_shape_and_frame(image, settings))
}

/// 形で切り抜き、文字・ロゴを描き、フレームを付ける（形の外側は、フレームがあればフレームの白、なければ透明）。
///
/// 文字は写真の上ならフレームの前に、フレームの余白ならフレームを付けた後にその余白へ描く
/// （大きさの基準はどちらも写真の短辺。旧版 FR-TXT-05）。
fn apply_shape_and_frame(mut image: RgbaImage, settings: &EditSettings) -> RgbaImage {
    let has_frame = settings.frame != FrameType::None;
    if settings.shape != ShapeType::Rectangle {
        let fill = has_frame.then_some(FRAME_COLOR);
        image = shapes::apply_shape(&image, settings.shape, settings.corner_radius, fill);
    }
    let on_margin = settings.text.position == TextPosition::FrameMargin && has_frame;
    let logo_on_margin = settings.logo.position == TextPosition::FrameMargin && has_frame;
    if settings.text.position == TextPosition::Tiled {
        tile::draw_tiled_text(&mut image, &settings.text);
    } else if !on_margin {
        text::draw_text(&mut image, &photo_text(settings), None, None);
    }
    if settings.logo.position == TextPosition::Tiled {
        tile::draw_tiled_logo(&mut image, &settings.logo);
    } else if !logo_on_margin {
        logo::draw_logo(&mut image, &photo_logo(settings), None, None);
    }
    let photo_size = image.dimensions();
    let mut framed = frames::add_frame(&image, settings.frame);
    if on_margin || logo_on_margin {
        if let Some((l, t, r, b)) = frames::margin_box(photo_size, settings.frame) {
            let area = (i64::from(l), i64::from(t), i64::from(r), i64::from(b));
            let reference = f64::from(photo_size.0.min(photo_size.1));
            if on_margin {
                text::draw_text(&mut framed, &settings.text, Some(area), Some(reference));
            }
            if logo_on_margin {
                logo::draw_logo(&mut framed, &settings.logo, Some(area), Some(reference));
            }
        }
    }
    framed
}

/// 写真の上に描くロゴの設定（フレームがないのに「フレームの余白」なら下中央に描く）。
fn photo_logo(settings: &EditSettings) -> LogoSettings {
    match settings.logo.position {
        TextPosition::FrameMargin => LogoSettings { position: TextPosition::Bottom, ..settings.logo.clone() },
        _ => settings.logo.clone(),
    }
}

/// 写真の上に描く文字の設定（フレームがないのに「フレームの余白」なら下中央に描く）。
fn photo_text(settings: &EditSettings) -> TextSettings {
    match settings.text.position {
        TextPosition::FrameMargin => TextSettings { position: TextPosition::Bottom, ..settings.text.clone() },
        _ => settings.text.clone(),
    }
}

/// プレビュー表示用の画像を返す（入力画像は変更しない）。
///
/// image は原画像を factor 倍に縮めたプレビュー用の画像（回転・反転する前のもの）。
/// 回転・反転と色の加工をかけ、リサイズはかけない（出力の大きさは output_size で求める）。
///
/// - trimmed = false: 元の画角全体を表示する。周辺減光はトリミング範囲を基準にその中だけにかけ、
///   文字もトリミング範囲に描く（範囲はいつでも選び直せる）
/// - trimmed = true: 切り抜いた範囲だけを表示し、形とフレームも付けて完成形を見せる（範囲がなければ全体）
pub fn render_preview(image: &RgbaImage, settings: &EditSettings, factor: f64, trimmed: bool) -> RgbaImage {
    render_preview_with_histogram(image, settings, factor, trimmed).0
}

/// プレビュー表示用の画像と、保存される写真のヒストグラムを返す（画像の作り方は render_preview）。
///
/// ヒストグラムは、実際に切り抜く範囲（なければ全体）の、形・フレームを付ける前の写真から数える
/// （範囲の外・フレームの白・形の外側・透明な画素・文字は数えない）。
pub fn render_preview_with_histogram(
    image: &RgbaImage,
    settings: &EditSettings,
    factor: f64,
    trimmed: bool,
) -> (RgbaImage, Histogram) {
    let image = orient(image, settings, factor);
    let scaled = scale_settings(settings, factor);
    let mut rect = effective_crop(image.dimensions(), scaled.crop, settings.frame, settings.shape);
    // ディテール・ジオラマの半径は、保存時と同じく実際に切り抜く範囲（なければ全体）の短辺を基準にする
    let reference = rect.map_or(f64::from(image.width().min(image.height())), |r| r.short_side() as f64);
    // シャープの半径の下限は保存時の写真に対するものなので、保存する写真の短辺も渡す
    let output = saved_photo_short_side(image.dimensions(), settings, factor);
    let adjusted = apply_local_adjustments(
        apply_basic_adjustments(image, settings),
        settings,
        ((factor, factor), (0.0, 0.0)),
    );
    let adjusted = apply_detail(adjusted, settings, reference, output);
    // ジオラマの帯は写真（実際に切り抜く範囲）に対する位置に置き、全体表示では外側にも続ける
    let mut rendered = apply_diorama_and_filter(adjusted, settings, rect);
    // スタンプの位置のずれ（切り抜いた表示なら、切り抜いた範囲の左上の分）
    let mut stamp_offset = (0.0, 0.0);
    if trimmed {
        if let Some(r) = rect.take() {
            rendered = transform::crop(&rendered, r);
            stamp_offset = (r.x as f64, r.y as f64);
        }
    }
    match rect {
        // トリミング範囲の中心を基準に暗くし、元の位置に戻す
        Some(r) if settings.vignette > 0 => {
            let mut region = transform::crop(&rendered, r);
            adjust::vignette(&mut region, settings.vignette);
            imageops::replace(&mut rendered, &region, r.x, r.y);
        }
        _ => adjust::vignette(&mut rendered, settings.vignette),
    }
    // 経年劣化は画素ごとの色の変化と固定模様の粒子なので、表示範囲全体にかける
    adjust::aging(&mut rendered, settings.aging);
    privacy::stamp(&mut rendered, &settings.regions, (factor, factor), stamp_offset);
    let photo = rect.unwrap_or(CropRect::whole(rendered.dimensions()));
    let mask =
        shapes::shape_mask((photo.width as u32, photo.height as u32), settings.shape, settings.corner_radius);
    let histogram = compute_histogram(&rendered, Some(photo), mask.as_ref());
    if trimmed {
        return (apply_shape_and_frame(rendered, settings), histogram);
    }
    // 全体表示ではフレーム・形は付けない（形は画面でマスクと輪郭を重ねて見せる）。写真の上の文字は
    // 切り抜く範囲（なければ全体）に描く。フレームの余白の文字は「トリミング実行」の表示で見える
    let on_margin = settings.text.position == TextPosition::FrameMargin && settings.frame != FrameType::None;
    let area = rect.map(|r| (r.x, r.y, r.right(), r.bottom()));
    if !on_margin {
        text::draw_text(&mut rendered, &photo_text(settings), area, None);
    }
    if !(settings.logo.position == TextPosition::FrameMargin && settings.frame != FrameType::None) {
        logo::draw_logo(&mut rendered, &photo_logo(settings), area, None);
    }
    (rendered, histogram)
}

/// テイストの一覧の見本: 今の設定のまま、テイストだけを FilterType::ALL の順に替えた小さな完成形を返す。
///
/// image・factor は render_preview と同じ（縮小したプレビューと、その縮小率）。切り抜いた写真の長辺が
/// max_side になるまで縮めてから（大きくはしない）、切り抜き・形・フレームも付けた表示（trimmed）を作る。
/// 強さは 100%（テイストを選び直すと 100% に戻るので、選んだときと同じ見た目）。テイストごとに並列に作る。
pub fn filter_thumbnails(
    image: &RgbaImage,
    settings: &EditSettings,
    factor: f64,
    max_side: u32,
) -> Vec<RgbaImage> {
    let size = settings.orientation.size(image.dimensions());
    let scaled = scale_settings(settings, factor);
    let photo =
        effective_crop(size, scaled.crop, settings.frame, settings.shape).unwrap_or(CropRect::whole(size));
    let long = photo.width.max(photo.height).max(1) as f64;
    // 縮める倍率（切り抜いた写真の長辺を max_side に）。画像全体の長辺に直して make_preview に渡す
    let shrink = (f64::from(max_side) / long).min(1.0);
    let whole_side = (f64::from(image.width().max(image.height())) * shrink).round().max(1.0) as u32;
    let (small, small_factor) = make_preview(image, whole_side);
    let factor = factor * small_factor;
    FilterType::ALL
        .par_iter()
        .map(|&filter| {
            let settings = EditSettings { filter, filter_strength: FILTER_STRENGTH_FULL, ..settings.clone() };
            render_preview(&small, &settings, factor, true)
        })
        .collect()
}

/// 部分補正をかける。place は原寸の座標からこの画像の座標への換算（倍率, ずれ）。
fn apply_local_adjustments(
    image: RgbaImage,
    settings: &EditSettings,
    (scale, offset): ((f64, f64), (f64, f64)),
) -> RgbaImage {
    if settings.local_adjustments.is_empty() {
        return image;
    }
    local::apply_local(image, &settings.local_adjustments, scale, offset, |image, a| {
        let only = EditSettings {
            exposure: a.exposure,
            contrast: a.contrast,
            temperature: a.temperature,
            saturation: a.saturation,
            ..EditSettings::default()
        };
        apply_basic_adjustments(image.clone(), &only)
    })
}

/// フィルターの前にかける基本補正（参考の写真に色を合わせる → 露出 → 明るさ → コントラスト → 色温度 → トーンカーブ → ハイライト／シャドウ →
/// 彩度 → 色ごとの調整）。露出〜トーンカーブは 1 つの変換表にまとめて 1 回でかける。
fn apply_basic_adjustments(mut image: RgbaImage, settings: &EditSettings) -> RgbaImage {
    crate::color_match::apply_color_match(&mut image, &settings.color_match);
    let mut lut: Lut = adjust::identity_lut();
    if settings.exposure != 0.0 {
        lut = adjust::compose(&lut, &adjust::exposure_lut(settings.exposure));
    }
    if settings.brightness != 0 {
        lut = adjust::compose(&lut, &adjust::brightness_lut(settings.brightness));
    }
    if settings.contrast != 0 {
        lut = adjust::compose(&lut, &adjust::contrast_lut(settings.contrast));
    }
    if settings.temperature != adjust::TEMPERATURE_NEUTRAL {
        lut = adjust::compose(&lut, &adjust::temperature_lut(settings.temperature));
    }
    if settings.tone_curve != curve::identity_curve() {
        lut = adjust::compose(&lut, &curve::curve_lut(&settings.tone_curve));
    }
    if lut != adjust::identity_lut() {
        adjust::apply_lut(&mut image, &lut);
    }
    adjust::highlights_shadows(&mut image, settings.highlights, settings.shadows);
    if settings.saturation != 0 {
        adjust::saturation(&mut image, settings.saturation);
    }
    curve::apply_hsl(&mut image, &settings.hsl);
    image
}

/// ディテール（ノイズ除去 → ぼかし → シャープ）。半径は reference（短辺）に比例させる。
/// output は保存する写真の短辺（プレビューでシャープの効き方を保存時とそろえる）。
fn apply_detail(
    mut image: RgbaImage,
    settings: &EditSettings,
    reference: f64,
    output: Option<f64>,
) -> RgbaImage {
    if settings.denoise > 0 {
        image = effects::denoise(&image, settings.denoise, reference);
    }
    if settings.blur > 0 {
        image = effects::blur(&image, settings.blur, reference);
    }
    if settings.sharpen > 0 {
        image = effects::sharpen(&image, settings.sharpen, reference, output);
    }
    image
}

/// ジオラマ → フィルター。ジオラマの帯は area（写真の範囲）に対する位置に置く。
fn apply_diorama_and_filter(
    mut image: RgbaImage,
    settings: &EditSettings,
    area: Option<CropRect>,
) -> RgbaImage {
    if settings.diorama_blur > 0 {
        // ぼかしの半径の基準は写真の範囲（なければ画像全体）の短辺
        image = diorama::diorama(&image, &settings.diorama(), area, None);
    }
    // テイストのぼかしの半径は、旧版と同じく画像そのもの（プレビューでは表示している全体）の短辺に比例させる
    filters::apply_filter_with_strength(&mut image, settings.filter, settings.filter_strength);
    // LUT はテイストの後（読めなければかけない）
    lut::apply_settings(&mut image, &settings.lut);
    image
}
