//! 編集設定 (EditSettings) と、それを画像にかける処理の流れ（旧版の core/pipeline.py を移したもの）。
//!
//! 処理順（旧版 §5.1）: 回転・反転 → トリミング → リサイズ → 露出 → 明るさ → コントラスト →
//! 色温度 → 彩度 → ディテール（ノイズ除去 → ぼかし → シャープ） → ジオラマ → フィルター →
//! 周辺減光 → 経年劣化 → 文字。
//! フレーム・形（#13）は、トリミングの後の比への切り抜きと、経年劣化の後にここへ足す。

use std::path::Path;

use image::{imageops, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::adjust::{self, Lut};
pub use crate::diorama::DioramaDirection;
use crate::diorama::{self, DioramaSettings};
use crate::effects;
use crate::filters;
pub use crate::filters::FilterType;
use crate::text;
use crate::transform::{self, round_half_even, CropRect, Orientation, SizeError};

/// プレビューの長辺（px）。
pub const PREVIEW_MAX_SIDE: u32 = 1600;

/// 文字・透かし（空なら描かない）。フォント・位置・色などは #15 で足す。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TextSettings {
    pub text: String,
    /// 写真の短辺に対する文字の大きさ (%)
    pub size: f32,
}

impl Default for TextSettings {
    fn default() -> Self {
        Self { text: String::new(), size: 5.0 }
    }
}

impl TextSettings {
    /// 描く文字がないか。
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// 編集設定。トリミング範囲は、回転・反転した後の原寸画像の座標で持つ。JSON では camelCase。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EditSettings {
    pub orientation: Orientation,
    pub crop: Option<CropRect>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub keep_aspect: bool,
    pub filter: FilterType,
    /// 周辺減光 0〜100（0 = なし）
    pub vignette: u32,
    /// 経年劣化 0〜100（0 = なし）
    pub aging: u32,
    /// 色温度（ケルビン、6500 = 変化なし）
    pub temperature: u32,
    /// 彩度 -100〜+100（0 = 変化なし）
    pub saturation: i32,
    /// 明るさ -100〜+100（0 = 変化なし）
    pub brightness: i32,
    /// コントラスト -100〜+100（0 = 変化なし）
    pub contrast: i32,
    /// 露出 -5.0〜+5.0 EV（0 = 変化なし）
    pub exposure: f64,
    /// シャープ・ぼかし・ノイズ除去 0〜100（0 = なし）
    pub sharpen: u32,
    pub blur: u32,
    pub denoise: u32,
    /// ジオラマ風（ぼかし 0 = なし）。位置・幅は写真の高さに対する %
    pub diorama_blur: u32,
    pub diorama_direction: DioramaDirection,
    pub diorama_position: u32,
    pub diorama_width: u32,
    pub diorama_vivid: u32,
    pub text: TextSettings,
}

impl Default for EditSettings {
    fn default() -> Self {
        Self {
            orientation: Orientation::default(),
            crop: None,
            width: None,
            height: None,
            keep_aspect: true,
            filter: FilterType::None,
            vignette: 0,
            aging: 0,
            temperature: adjust::TEMPERATURE_NEUTRAL,
            saturation: 0,
            brightness: 0,
            contrast: 0,
            exposure: 0.0,
            sharpen: 0,
            blur: 0,
            denoise: 0,
            diorama_blur: 0,
            diorama_direction: DioramaDirection::Horizontal,
            diorama_position: 50,
            diorama_width: 20,
            diorama_vivid: 30,
            text: TextSettings::default(),
        }
    }
}

impl EditSettings {
    /// 旧版のベンチマークの「重い設定」（ほぼすべての効果をかける）。
    pub fn heavy() -> Self {
        Self {
            exposure: 0.5,
            brightness: 10,
            contrast: 20,
            temperature: 5000,
            saturation: 20,
            denoise: 50,
            blur: 10,
            sharpen: 50,
            diorama_blur: 80,
            filter: FilterType::Hdr,
            vignette: 50,
            aging: 30,
            text: TextSettings { text: "© 2026 写真".into(), ..TextSettings::default() },
            ..Self::default()
        }
    }

    /// ジオラマ風の加工の設定をまとめて返す。
    pub fn diorama(&self) -> DioramaSettings {
        DioramaSettings {
            blur: self.diorama_blur,
            direction: self.diorama_direction,
            position: self.diorama_position,
            width: self.diorama_width,
            vivid: self.diorama_vivid,
        }
    }
}

/// 原画像に編集をかけた新しい画像を返す（原画像は変更しない）。保存に使う。
pub fn apply_edits(original: &RgbaImage, settings: &EditSettings) -> Result<RgbaImage, SizeError> {
    let mut image = settings.orientation.transpose(original);
    if let Some(rect) = effective_crop(image.dimensions(), settings.crop) {
        image = transform::crop(&image, rect);
    }
    let size =
        transform::fit_size(image.dimensions(), settings.width, settings.height, settings.keep_aspect)?;
    let image = transform::resize_to(&image, size);

    let reference = f64::from(image.width().min(image.height()));
    let image = apply_detail(apply_basic_adjustments(image, settings), settings, reference, None);
    let image = apply_diorama_and_filter(image, settings, None);
    let mut image = image;
    adjust::vignette(&mut image, settings.vignette);
    adjust::aging(&mut image, settings.aging);
    draw_text(&mut image, &settings.text, None);
    Ok(image)
}

/// プレビュー表示用の画像を返す（入力画像は変更しない）。
///
/// image は原画像を factor 倍に縮めたプレビュー用の画像（回転・反転する前のもの）。
/// 回転・反転と色の加工をかけ、リサイズはかけない（出力の大きさは output_size で求める）。
///
/// - trimmed = false: 元の画角全体を表示する。周辺減光はトリミング範囲を基準にその中だけにかけ、
///   文字もトリミング範囲に描く（範囲はいつでも選び直せる）
/// - trimmed = true: 切り抜いた範囲だけを表示する（範囲がなければ全体）
pub fn render_preview(image: &RgbaImage, settings: &EditSettings, factor: f64, trimmed: bool) -> RgbaImage {
    let image = settings.orientation.transpose(image);
    let scaled = scale_settings(settings, factor);
    let mut rect = effective_crop(image.dimensions(), scaled.crop);
    // ディテール・ジオラマの半径は、保存時と同じく実際に切り抜く範囲（なければ全体）の短辺を基準にする
    let reference = rect.map_or(f64::from(image.width().min(image.height())), |r| r.short_side() as f64);
    // シャープの半径の下限は保存時の写真に対するものなので、保存する写真の短辺も渡す
    let output = saved_photo_short_side(image.dimensions(), settings, factor);
    let adjusted = apply_detail(apply_basic_adjustments(image, settings), settings, reference, output);
    // ジオラマの帯は写真（実際に切り抜く範囲）に対する位置に置き、全体表示では外側にも続ける
    let mut rendered = apply_diorama_and_filter(adjusted, settings, rect);
    if trimmed {
        if let Some(r) = rect.take() {
            rendered = transform::crop(&rendered, r);
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
    draw_text(&mut rendered, &settings.text, rect);
    rendered
}

/// プレビューに重ねる、ジオラマのピントの帯のガイドの線。
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DioramaGuide {
    /// 横の帯なら true（線は横に引く）
    pub horizontal: bool,
    /// 線の位置（表示している画像の高さ（縦の帯なら幅）に対する割合）と、実線かどうか。
    /// 実線はくっきり残す範囲の端、破線はぼけきる位置。写真の範囲の外にも続ける
    pub lines: Vec<(f64, bool)>,
}

/// 全体表示のプレビューに重ねるジオラマのガイド。帯の位置は、実際に切り抜く範囲（なければ全体）に対する。
///
/// preview_size は回転・反転する前のプレビューの大きさ、factor は原寸に対する縮小率。
pub fn diorama_guide(preview_size: (u32, u32), settings: &EditSettings, factor: f64) -> DioramaGuide {
    let size = settings.orientation.size(preview_size);
    let scaled = scale_settings(settings, factor);
    let area = effective_crop(size, scaled.crop).unwrap_or(CropRect::whole(size));
    let horizontal = settings.diorama_direction == DioramaDirection::Horizontal;
    let (start, length, total) = if horizontal {
        (area.y as f64, area.height as f64, f64::from(size.1))
    } else {
        (area.x as f64, area.width as f64, f64::from(size.0))
    };
    let band = diorama::diorama_band(&settings.diorama());
    let at = |fraction: f64| (start + fraction * length) / total;
    DioramaGuide {
        horizontal,
        lines: vec![
            (at(band.blur_start), false),
            (at(band.sharp_start), true),
            (at(band.sharp_end), true),
            (at(band.blur_end), false),
        ],
    }
}

/// 画像を処理せずに、apply_edits の出力の大きさを求める。
pub fn output_size(original_size: (u32, u32), settings: &EditSettings) -> Result<(u32, u32), SizeError> {
    let mut size = settings.orientation.size(original_size);
    if let Some(rect) = effective_crop(size, settings.crop) {
        size = (rect.width as u32, rect.height as u32);
    }
    transform::fit_size(size, settings.width, settings.height, settings.keep_aspect)
}

/// 実際に切り抜く範囲を返す（size の画像の座標）。切り抜かないなら None。
///
/// トリミング範囲は画像内に収まるよう補正する。フレーム・円の比への切り抜きは #13 で足す。
pub fn effective_crop(size: (u32, u32), crop: Option<CropRect>) -> Option<CropRect> {
    transform::clamp_crop(crop?, size)
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

/// フィルターの前にかける基本補正（露出 → 明るさ → コントラスト → 色温度 → 彩度）。
/// 露出〜色温度は 1 つの変換表にまとめて 1 回でかける。
fn apply_basic_adjustments(mut image: RgbaImage, settings: &EditSettings) -> RgbaImage {
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
    if lut != adjust::identity_lut() {
        adjust::apply_lut(&mut image, &lut);
    }
    if settings.saturation != 0 {
        adjust::saturation(&mut image, settings.saturation);
    }
    image
}

/// 保存する写真（切り抜き・リサイズ後）の短辺を、プレビュー（回転・反転した後の大きさ）から求める。
fn saved_photo_short_side(preview_size: (u32, u32), settings: &EditSettings, factor: f64) -> Option<f64> {
    let original = (
        round_half_even(f64::from(preview_size.0) / factor) as u32,
        round_half_even(f64::from(preview_size.1) / factor) as u32,
    );
    let mut size = original;
    if let Some(rect) = effective_crop(original, settings.crop) {
        size = (rect.width as u32, rect.height as u32);
    }
    let (width, height) =
        transform::fit_size(size, settings.width, settings.height, settings.keep_aspect).ok()?;
    Some(f64::from(width.min(height)))
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
    filters::apply_filter(&mut image, settings.filter);
    image
}

/// 文字を描く（area があればその範囲の右下。省略時は画像全体）。
fn draw_text(image: &mut RgbaImage, settings: &TextSettings, area: Option<CropRect>) {
    if settings.is_empty() {
        return;
    }
    let Some(font) = text::load_font(Path::new(text::HIRAGINO_W3), 0) else { return };
    let (color, opacity) = ([255, 255, 255], 0.8);
    match area {
        Some(r) => {
            let mut region = transform::crop(image, r);
            text::draw_text_bottom_right(&mut region, font, &settings.text, settings.size, color, opacity);
            imageops::replace(image, &region, r.x, r.y);
        }
        None => {
            text::draw_text_bottom_right(image, font, &settings.text, settings.size, color, opacity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn sample() -> RgbaImage {
        RgbaImage::from_fn(120, 80, |x, y| Rgba([(x * 2) as u8, (y * 3) as u8, ((x + y) % 256) as u8, 200]))
    }

    #[test]
    fn default_settings_change_nothing() {
        let image = sample();
        assert_eq!(apply_edits(&image, &EditSettings::default()).unwrap(), image);
        assert_eq!(render_preview(&image, &EditSettings::default(), 1.0, false), image);
    }

    #[test]
    fn heavy_settings_keep_size_and_alpha() {
        let image = sample();
        let out = apply_edits(&image, &EditSettings::heavy()).unwrap();
        assert_eq!(out.dimensions(), image.dimensions());
        assert_ne!(out, image);
        // 透過（アルファ）はすべての処理の後も残る（FR-PRC-03）。文字を描いた画素だけは別
        let same_alpha = out.pixels().zip(image.pixels()).filter(|(a, b)| a[3] == b[3]).count();
        assert!(same_alpha as f64 > 0.95 * f64::from(image.width() * image.height()));
    }

    #[test]
    fn settings_from_json_use_defaults() {
        let s: EditSettings = serde_json::from_str(
            r#"{"exposure": 1.5, "dioramaBlur": 40, "orientation": {"rotation": 90},
                "crop": {"x": 1, "y": 2, "width": 3, "height": 4}, "keepAspect": false, "filter": "hdr"}"#,
        )
        .unwrap();
        assert_eq!(s.exposure, 1.5);
        assert_eq!(s.diorama_blur, 40);
        assert_eq!(s.temperature, 6500);
        assert_eq!(s.orientation, Orientation::new(90, false));
        assert_eq!(s.crop, Some(CropRect::new(1, 2, 3, 4)));
        assert!(!s.keep_aspect);
        assert_eq!(s.filter, FilterType::Hdr);
        assert_eq!(s.width, None);
    }

    #[test]
    fn preview_vignette_stays_inside_crop() {
        let image = RgbaImage::from_pixel(100, 80, Rgba([200, 200, 200, 255]));
        let settings = EditSettings {
            vignette: 100,
            crop: Some(CropRect::new(20, 20, 40, 30)),
            ..EditSettings::default()
        };
        let out = render_preview(&image, &settings, 1.0, false);
        assert_eq!(out.get_pixel(5, 5), image.get_pixel(5, 5)); // 範囲の外は変えない
        assert!(out.get_pixel(20, 20)[0] < 200); // 範囲の角は暗くなる
        assert_eq!(out.get_pixel(40, 35), image.get_pixel(40, 35)); // 範囲の中心はそのまま
    }

    #[test]
    fn trimmed_preview_is_the_crop() {
        let image = sample();
        let settings = EditSettings { crop: Some(CropRect::new(10, 20, 30, 40)), ..EditSettings::default() };
        let out = render_preview(&image, &settings, 0.5, true);
        assert_eq!(out.dimensions(), (15, 20));
        assert_eq!(out.get_pixel(0, 0), image.get_pixel(5, 10));
    }

    #[test]
    fn make_preview_keeps_small_images() {
        let image = sample();
        let (preview, factor) = make_preview(&image, 1600);
        assert_eq!((preview.dimensions(), factor), (image.dimensions(), 1.0));
        let (preview, factor) = make_preview(&image, 60);
        assert_eq!(preview.dimensions(), (60, 40));
        assert_eq!(factor, 0.5);
    }

    #[test]
    fn diorama_guide_follows_crop() {
        // 原寸 400×200 を 0.5 倍にしたプレビュー（200×100）。範囲は原寸で y = 40〜120
        let settings = EditSettings {
            diorama_position: 50,
            diorama_width: 20,
            crop: Some(CropRect::new(0, 40, 400, 80)),
            ..EditSettings::default()
        };
        let guide = diorama_guide((200, 100), &settings, 0.5);
        assert!(guide.horizontal);
        let solid: Vec<f64> = guide.lines.iter().filter(|l| l.1).map(|l| l.0).collect();
        // 範囲（プレビューで y = 20〜60）の 40%〜60% → y = 36〜44 → 高さ 100 に対して 0.36〜0.44
        assert!((solid[0] - 0.36).abs() < 1e-9 && (solid[1] - 0.44).abs() < 1e-9, "{solid:?}");
        // 縦の帯・90° 回転: 幅は回転後のもの
        let vertical = EditSettings {
            diorama_direction: DioramaDirection::Vertical,
            orientation: Orientation::new(90, false),
            ..EditSettings::default()
        };
        let guide = diorama_guide((200, 100), &vertical, 1.0);
        assert!(!guide.horizontal);
        assert!((guide.lines[1].0 - 0.4).abs() < 1e-9);
    }

    #[test]
    fn invalid_size_is_an_error() {
        let settings = EditSettings { width: Some(0), ..EditSettings::default() };
        assert!(apply_edits(&sample(), &settings).is_err());
        assert!(output_size((10, 10), &settings).is_err());
    }
}
