//! 文字・透かし（ウォーターマーク）を描く。旧版の core/text.py を移したもの。
//!
//! 配置の計算（大きさ・余白・行間・そろえ方・収まらなければ小さくする・不透明度）は旧版と同じ。
//! 文字の形は ab_glyph で描く（旧版の Pillow / FreeType とは縁のなめらかさが少し違う）。

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use ab_glyph::{point, Font, FontVec, PxScale, Rect, ScaleFont};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::transform::round_half_even;

const FONT_DIR: &str = "/System/Library/Fonts";
/// 文字の大きさ（短辺に対する %）の範囲と既定値。
pub const TEXT_SIZE_MIN: f32 = 1.0;
pub const TEXT_SIZE_MAX: f32 = 30.0;
pub const TEXT_SIZE_DEFAULT: f32 = 5.0;
/// 不透明度（%）の既定値。
pub const TEXT_OPACITY_DEFAULT: u32 = 80;
/// 写真の端からの余白（短辺に対する比率）。
pub(crate) const TEXT_MARGIN_RATIO: f64 = 0.03;
/// 行間（文字の大きさに対する比率）。
const LINE_SPACING_RATIO: f64 = 0.25;
/// 縁取りの太さ・影のずれ・影のぼかし（文字の大きさに対する比率）。
const OUTLINE_RATIO: f32 = 0.06;
const SHADOW_OFFSET_RATIO: f32 = 0.08;
const SHADOW_BLUR_RATIO: f32 = 0.04;
/// 影の濃さ（文字の不透明度に対する比率）。
const SHADOW_STRENGTH: f32 = 0.75;

/// 文字のフォント（どの Mac にも入っているもの）。JSON では旧版のプリセットと同じ名前（GOTHIC など）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TextFont {
    #[default]
    Gothic,
    GothicBold,
    Mincho,
    MaruGothic,
    Helvetica,
    Times,
}

impl TextFont {
    /// すべてのフォント（画面のプルダウンの順）。
    pub const ALL: [TextFont; 6] =
        [Self::Gothic, Self::GothicBold, Self::Mincho, Self::MaruGothic, Self::Helvetica, Self::Times];

    /// 画面に出す名前。
    pub fn label(self) -> &'static str {
        match self {
            Self::Gothic => "ヒラギノ角ゴシック",
            Self::GothicBold => "ヒラギノ角ゴシック（太字）",
            Self::Mincho => "ヒラギノ明朝",
            Self::MaruGothic => "ヒラギノ丸ゴ",
            Self::Helvetica => "Helvetica",
            Self::Times => "Times",
        }
    }

    /// フォントのファイル（.ttc の 0 番目を使う）。
    pub fn path(self) -> PathBuf {
        let name = match self {
            Self::Gothic => "ヒラギノ角ゴシック W3.ttc",
            Self::GothicBold => "ヒラギノ角ゴシック W6.ttc",
            Self::Mincho => "ヒラギノ明朝 ProN.ttc",
            Self::MaruGothic => "ヒラギノ丸ゴ ProN W4.ttc",
            Self::Helvetica => "Helvetica.ttc",
            Self::Times => "Times.ttc",
        };
        Path::new(FONT_DIR).join(name)
    }
}

/// 文字を置く場所。写真の上の 9 か所と、フレームの余白。JSON では旧版と同じ名前。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextPosition {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    #[default]
    BottomRight,
    /// ポラロイド・チェキの広い余白（フレームがなければ下中央）
    FrameMargin,
    /// 写真全体に斜めに繰り返して敷く（旧版にはない）
    Tiled,
}

impl TextPosition {
    /// すべての位置（画面のプルダウンの順）。
    pub const ALL: [TextPosition; 11] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Left,
        Self::Center,
        Self::Right,
        Self::BottomLeft,
        Self::Bottom,
        Self::BottomRight,
        Self::FrameMargin,
        Self::Tiled,
    ];

    /// 画面に出す名前。
    pub fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "左上",
            Self::Top => "上",
            Self::TopRight => "右上",
            Self::Left => "左",
            Self::Center => "中央",
            Self::Right => "右",
            Self::BottomLeft => "左下",
            Self::Bottom => "下",
            Self::BottomRight => "右下",
            Self::FrameMargin => "フレームの余白",
            Self::Tiled => "全体に繰り返す（斜め）",
        }
    }

    /// 横・縦のそろえ方（0 = 左・上、0.5 = 中央、1 = 右・下）。
    pub(crate) fn anchor(self) -> (f64, f64) {
        match self {
            Self::TopLeft => (0.0, 0.0),
            Self::Top => (0.5, 0.0),
            Self::TopRight => (1.0, 0.0),
            Self::Left => (0.0, 0.5),
            Self::Center | Self::FrameMargin | Self::Tiled => (0.5, 0.5),
            Self::Right => (1.0, 0.5),
            Self::BottomLeft => (0.0, 1.0),
            Self::Bottom => (0.5, 1.0),
            Self::BottomRight => (1.0, 1.0),
        }
    }
}

/// 文字の飾り（明るい写真でも暗い写真でも読みやすくする）。色は文字の色から決める（明るい文字には黒、暗い文字には白）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextEffect {
    #[default]
    None,
    /// 縁取り
    Outline,
    /// 右下に落とす影
    Shadow,
}

impl TextEffect {
    /// 飾りがないか（設定のファイルには、飾りがあるときだけ書く）。
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
}

/// 文字の色に合う飾りの色（明るい文字には黒、暗い文字には白）。
pub fn effect_color(color: [u8; 3]) -> [u8; 3] {
    let [r, g, b] = color.map(f32::from);
    if 0.299 * r + 0.587 * g + 0.114 * b >= 128.0 {
        [0, 0, 0]
    } else {
        [255, 255, 255]
    }
}

/// 文字・透かしの設定。text が空（空白だけも）なら何も描かない。size は写真の短辺に対する %。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TextSettings {
    pub text: String,
    pub font: TextFont,
    pub size: f32,
    pub color: [u8; 3],
    /// 不透明度 0〜100（%）
    pub opacity: u32,
    pub position: TextPosition,
    /// 飾り（旧版にはない）
    #[serde(skip_serializing_if = "TextEffect::is_none")]
    pub effect: TextEffect,
}

impl Default for TextSettings {
    fn default() -> Self {
        Self {
            text: String::new(),
            font: TextFont::Gothic,
            size: TEXT_SIZE_DEFAULT,
            color: [255, 255, 255],
            opacity: TEXT_OPACITY_DEFAULT,
            position: TextPosition::BottomRight,
            effect: TextEffect::None,
        }
    }
}

impl TextSettings {
    /// 描く文字がないか（空白だけも含む）。
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// 撮影日時の差し込み（文字の中に書くと、写真の撮影日時に置き換える）。
pub const DATE_PLACEHOLDER: &str = "{日付}";
pub const DATE_TIME_PLACEHOLDER: &str = "{日時}";

/// 文字の {日付}（2026.10.09）・{日時}（2026.10.09 14:23）を撮影日時に置き換える。撮影日時がなければ空にする。
pub fn expand_placeholders(text: &str, date: Option<&crate::exif_info::CaptureDate>) -> String {
    if !text.contains('{') {
        return text.to_string();
    }
    let (day, minute) = match date {
        Some(d) => (
            format!("{}.{:02}.{:02}", d.year, d.month, d.day),
            format!("{}.{:02}.{:02} {:02}:{:02}", d.year, d.month, d.day, d.hour, d.minute),
        ),
        None => (String::new(), String::new()),
    };
    text.replace(DATE_TIME_PLACEHOLDER, &minute).replace(DATE_PLACEHOLDER, &day)
}

/// フォントを読む。同じフォントは 2 回目から読み直さない。読めなければヒラギノ角ゴシック W3、
/// それも読めなければ None（旧版は Pillow の組み込みのフォントで代わりに描いていた）。
pub fn load_font(font: TextFont) -> Option<&'static FontVec> {
    load_font_file(&font.path(), 0).or_else(|| load_font_file(&TextFont::Gothic.path(), 0))
}

/// 読み込んだフォント（パス・.ttc の何番目か・フォント）。
type FontCache = Vec<(PathBuf, u32, &'static FontVec)>;

/// フォントのファイルを読む（.ttc は index 番目）。
pub fn load_font_file(path: &Path, index: u32) -> Option<&'static FontVec> {
    static CACHE: OnceLock<Mutex<FontCache>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    if let Some((_, _, font)) = cache.iter().find(|(p, i, _)| p == path && *i == index) {
        return Some(font);
    }
    let bytes = std::fs::read(path).ok()?;
    let font: &'static FontVec = Box::leak(Box::new(FontVec::try_from_vec_and_index(bytes, index).ok()?));
    cache.push((path.to_path_buf(), index, font));
    Some(font)
}

/// 文字の大きさ（px）。reference は基準にする写真の短辺。
pub fn text_size_px(settings: &TextSettings, reference: f64) -> u32 {
    let size = f64::from(settings.size.clamp(TEXT_SIZE_MIN, TEXT_SIZE_MAX));
    round_half_even(reference * size / 100.0).max(1) as u32
}

/// 並べた文字の形（px）。
struct Layout {
    /// 文字の大きさ（収まるように小さくした後）
    size_px: u32,
    scale: PxScale,
    /// 行の間隔（1 行目の上端から次の行の上端まで）
    line_height: f32,
    /// 各行の左端の位置（いちばん長い行の幅に対して、そろえ方に合わせてずらす）
    offsets: Vec<f32>,
    /// 文字が描かれる範囲（左上を (0, 0) にして並べたとき）
    bounds: Rect,
}

/// size_px の大きさで並べたときの形。
fn layout(font: &FontVec, lines: &[&str], size_px: u32, align: f64) -> Layout {
    // 旧版（Pillow）と同じく、大きさは 1 文字分の高さ（em）で指定する
    let scale = font.pt_to_px_scale(size_px as f32).unwrap_or(PxScale::from(size_px as f32));
    let scaled = font.as_scaled(scale);
    let spacing = round_half_even(f64::from(size_px) * LINE_SPACING_RATIO).max(0) as f32;
    // Pillow と同じく、行の間隔は「A」の下端（ほぼベースライン）＋行間
    let a_bottom = font
        .outline_glyph(font.glyph_id('A').with_scale_and_position(scale, point(0.0, scaled.ascent())))
        .map_or(scaled.ascent(), |g| g.px_bounds().max.y);
    let line_height = a_bottom + spacing;
    let widths: Vec<f32> =
        lines.iter().map(|l| l.chars().map(|c| scaled.h_advance(font.glyph_id(c))).sum()).collect();
    let widest = widths.iter().cloned().fold(0.0f32, f32::max);
    let offsets: Vec<f32> = widths.iter().map(|w| (widest - w) * align as f32).collect();
    let mut bounds = Rect { min: point(f32::MAX, f32::MAX), max: point(f32::MIN, f32::MIN) };
    for_each_glyph(font, scale, lines, &offsets, line_height, (0.0, 0.0), |outline| {
        let b = outline.px_bounds();
        bounds.min.x = bounds.min.x.min(b.min.x);
        bounds.min.y = bounds.min.y.min(b.min.y);
        bounds.max.x = bounds.max.x.max(b.max.x);
        bounds.max.y = bounds.max.y.max(b.max.y);
    });
    if bounds.min.x > bounds.max.x {
        bounds = Rect::default(); // 形のある文字がない（空白だけなど）
    }
    Layout { size_px, scale, line_height, offsets, bounds }
}

/// 並べた文字の 1 つずつの形について f を呼ぶ。origin は 1 行目の左上。
fn for_each_glyph(
    font: &FontVec,
    scale: PxScale,
    lines: &[&str],
    offsets: &[f32],
    line_height: f32,
    origin: (f32, f32),
    mut f: impl FnMut(ab_glyph::OutlinedGlyph),
) {
    let scaled = font.as_scaled(scale);
    for (row, line) in lines.iter().enumerate() {
        let mut x = origin.0 + offsets[row];
        let baseline = origin.1 + row as f32 * line_height + scaled.ascent();
        for ch in line.chars() {
            let glyph = font.glyph_id(ch).with_scale_and_position(scale, point(x, baseline));
            x += scaled.h_advance(glyph.id);
            if let Some(outline) = font.outline_glyph(glyph) {
                f(outline);
            }
        }
    }
}

/// available に収まる大きさで並べる（収まらなければ小さくする。旧版 FR-TXT-04）。
fn fit(font: &FontVec, lines: &[&str], mut size_px: u32, available: (f64, f64), align: f64) -> Layout {
    loop {
        let layout = layout(font, lines, size_px, align);
        let width = f64::from(layout.bounds.width());
        let height = f64::from(layout.bounds.height());
        if (width <= available.0 && height <= available.1) || size_px <= 1 {
            return layout;
        }
        let scale = (available.0 / width.max(1.0)).min(available.1 / height.max(1.0));
        size_px = ((size_px - 1).min((f64::from(size_px) * scale) as u32)).max(1);
    }
}

/// 文字を描いた範囲（左, 上, 右, 下。画像の座標）。
pub type DrawnBox = (f32, f32, f32, f32);

/// 画像に文字を描き、描いた範囲を返す（描かなかったら None）。
///
/// area（左, 上, 右, 下）の中に、設定の位置に合わせて置く（省略時は画像全体）。写真の上の位置では
/// area の端から短辺の 3% 内側に置く。大きさ・余白の基準は reference（省略時は area の短辺）。
/// 収まらないときは収まるまで小さくする。不透明度に合わせて半透明で重ね、元の透過も残す
/// （文字は透明な部分の上にも見える。Pillow の alpha_composite と同じ重ね方）。
pub fn draw_text(
    image: &mut RgbaImage,
    settings: &TextSettings,
    area: Option<(i64, i64, i64, i64)>,
    reference: Option<f64>,
) -> Option<DrawnBox> {
    if settings.is_empty() {
        return None;
    }
    let (left, top, right, bottom) =
        area.unwrap_or((0, 0, i64::from(image.width()), i64::from(image.height())));
    let (width, height) = ((right - left) as f64, (bottom - top) as f64);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let font = load_font(settings.font)?;
    let reference = reference.unwrap_or(width.min(height));
    let margin =
        if settings.position == TextPosition::FrameMargin { 0.0 } else { reference * TEXT_MARGIN_RATIO };
    let available = ((width - margin * 2.0).max(1.0), (height - margin * 2.0).max(1.0));
    let (ax, ay) = settings.position.anchor();
    let text = settings.text.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let layout = fit(font, &lines, text_size_px(settings, reference), available, ax);
    let b = layout.bounds;
    let x = left as f64 + margin + (available.0 - f64::from(b.width())) * ax - f64::from(b.min.x);
    let y = top as f64 + margin + (available.1 - f64::from(b.height())) * ay - f64::from(b.min.y);
    let alpha = round_half_even(255.0 * f64::from(settings.opacity.min(100)) / 100.0) as f32 / 255.0;
    let color = settings.color.map(f32::from);
    let (image_width, image_height) = image.dimensions();
    let origin = (x as f32, y as f32);
    draw_effect(image, settings, font, &lines, &layout, origin, alpha);
    for_each_glyph(font, layout.scale, &lines, &layout.offsets, layout.line_height, origin, |outline| {
        let bounds = outline.px_bounds();
        outline.draw(|gx, gy, coverage| {
            let px = bounds.min.x as i64 + i64::from(gx);
            let py = bounds.min.y as i64 + i64::from(gy);
            if px < 0 || py < 0 || px >= i64::from(image_width) || py >= i64::from(image_height) {
                return;
            }
            let source_alpha = coverage.clamp(0.0, 1.0) * alpha;
            if source_alpha <= 0.0 {
                return;
            }
            source_over(image.get_pixel_mut(px as u32, py as u32), color, source_alpha);
        });
    });
    Some((origin.0 + b.min.x, origin.1 + b.min.y, origin.0 + b.max.x, origin.1 + b.max.y))
}

/// 画素に色 color を不透明度 source_alpha で「上に重ねる」（source over）。
pub(crate) fn source_over(pixel: &mut image::Rgba<u8>, color: [f32; 3], source_alpha: f32) {
    let dest_alpha = f32::from(pixel[3]) / 255.0;
    let out_alpha = source_alpha + dest_alpha * (1.0 - source_alpha);
    for c in 0..3 {
        let under = f32::from(pixel[c]) * dest_alpha * (1.0 - source_alpha);
        pixel[c] = ((color[c] * source_alpha + under) / out_alpha).round().clamp(0.0, 255.0) as u8;
    }
    pixel[3] = (out_alpha * 255.0).round() as u8;
}

/// 文字の下に飾り（縁取り・影）を描く。文字の形を濃さの板（マスク）に描き、縁取りなら太らせ、影ならぼかしてずらす。
fn draw_effect(
    image: &mut RgbaImage,
    settings: &TextSettings,
    font: &FontVec,
    lines: &[&str],
    layout: &Layout,
    origin: (f32, f32),
    alpha: f32,
) {
    let size = layout.size_px as f32;
    let (pad, shift, strength) = match settings.effect {
        TextEffect::None => return,
        TextEffect::Outline => ((size * OUTLINE_RATIO).round().max(1.0) as i64, 0, 1.0),
        TextEffect::Shadow => {
            let blur = (size * SHADOW_BLUR_RATIO).max(0.5);
            (
                (blur * 3.0).ceil() as i64,
                (size * SHADOW_OFFSET_RATIO).round().max(1.0) as i64,
                SHADOW_STRENGTH,
            )
        }
    };
    let b = layout.bounds;
    let left = (origin.0 + b.min.x).floor() as i64 - pad - 1;
    let top = (origin.1 + b.min.y).floor() as i64 - pad - 1;
    let width = b.width().ceil() as i64 + 2 * pad + 3;
    let height = b.height().ceil() as i64 + 2 * pad + 3;
    let mut mask = image::GrayImage::new(width as u32, height as u32);
    for_each_glyph(font, layout.scale, lines, &layout.offsets, layout.line_height, origin, |outline| {
        let bounds = outline.px_bounds();
        outline.draw(|gx, gy, coverage| {
            let mx = bounds.min.x as i64 + i64::from(gx) - left;
            let my = bounds.min.y as i64 + i64::from(gy) - top;
            if mx < 0 || my < 0 || mx >= width || my >= height {
                return;
            }
            let value = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
            let pixel = mask.get_pixel_mut(mx as u32, my as u32);
            pixel[0] = pixel[0].max(value);
        });
    });
    let mask = match settings.effect {
        TextEffect::Outline => dilate(&mask, pad),
        _ => image::imageops::blur(&mask, (size * SHADOW_BLUR_RATIO).max(0.5)),
    };
    let color = effect_color(settings.color).map(f32::from);
    let (image_width, image_height) = (i64::from(image.width()), i64::from(image.height()));
    for (mx, my, value) in mask.enumerate_pixels() {
        let px = left + i64::from(mx) + shift;
        let py = top + i64::from(my) + shift;
        let source_alpha = f32::from(value[0]) / 255.0 * alpha * strength;
        if source_alpha <= 0.0 || px < 0 || py < 0 || px >= image_width || py >= image_height {
            continue;
        }
        source_over(image.get_pixel_mut(px as u32, py as u32), color, source_alpha);
    }
}

/// 濃さの板を半径 radius の円で太らせる（各画素を、円の中でいちばん濃い値にする）。
fn dilate(mask: &image::GrayImage, radius: i64) -> image::GrayImage {
    let (width, height) = (i64::from(mask.width()), i64::from(mask.height()));
    // 円の各段（dy）の横の半幅
    let spans: Vec<(i64, i64)> = (-radius..=radius)
        .map(|dy| (dy, ((radius * radius - dy * dy) as f64).sqrt().floor() as i64))
        .collect();
    let mut out = image::GrayImage::new(mask.width(), mask.height());
    for y in 0..height {
        for x in 0..width {
            let mut best = 0u8;
            for &(dy, half) in &spans {
                let sy = y + dy;
                if sy < 0 || sy >= height {
                    continue;
                }
                for sx in (x - half).max(0)..=(x + half).min(width - 1) {
                    best = best.max(mask.get_pixel(sx as u32, sy as u32)[0]);
                }
                if best == 255 {
                    break;
                }
            }
            out.put_pixel(x as u32, y as u32, image::Luma([best]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn settings(text: &str, position: TextPosition) -> TextSettings {
        TextSettings { text: text.into(), position, opacity: 100, size: 10.0, ..TextSettings::default() }
    }

    fn lit(image: &RgbaImage, x: std::ops::Range<u32>, y: std::ops::Range<u32>) -> usize {
        x.flat_map(|x| y.clone().map(move |y| (x, y)))
            .filter(|&(x, y)| image.get_pixel(x, y)[0] > 128)
            .count()
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn fonts_load() {
        for font in TextFont::ALL {
            assert!(load_font(font).is_some(), "{font:?}");
        }
        assert_ne!(load_font(TextFont::Gothic).unwrap().glyph_id('写').0, 0);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn positions_follow_the_anchor() {
        for position in TextPosition::ALL {
            let mut image = RgbaImage::from_pixel(400, 300, Rgba([0, 0, 0, 255]));
            let (l, t, r, b) = draw_text(&mut image, &settings("写真", position), None, None).unwrap();
            let (ax, ay) = position.anchor();
            // 写真の上では余白は短辺の 3% = 9px、フレームの余白では 0
            let margin = if position == TextPosition::FrameMargin { 0.0 } else { 9.0 };
            let expected_x = margin + (400.0 - 2.0 * margin - f64::from(r - l)) * ax;
            let expected_y = margin + (300.0 - 2.0 * margin - f64::from(b - t)) * ay;
            assert!((f64::from(l) - expected_x).abs() < 0.01, "{position:?}: x {l} / {expected_x}");
            assert!((f64::from(t) - expected_y).abs() < 0.01, "{position:?}: y {t} / {expected_y}");
            assert!(lit(&image, l as u32..r.ceil() as u32, t as u32..b.ceil() as u32) > 50, "{position:?}");
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn size_follows_the_short_side_and_shrinks_to_fit() {
        let s = TextSettings { size: 10.0, ..settings("A", TextPosition::Center) };
        assert_eq!(text_size_px(&s, 300.0), 30);
        assert_eq!(text_size_px(&TextSettings { size: 99.0, ..s.clone() }, 300.0), 90); // 30% まで
                                                                                        // 長い文字は、範囲に収まるまで小さくする
        let mut image = RgbaImage::from_pixel(200, 100, Rgba([0, 0, 0, 255]));
        let long = TextSettings {
            size: 30.0,
            ..settings("とても長い文字の透かしです", TextPosition::Center)
        };
        let (l, _, r, _) = draw_text(&mut image, &long, None, None).unwrap();
        assert!(l >= 2.9 && r <= 197.1, "{l}〜{r}");
        // 2 行は 1 行より高い
        let mut two = RgbaImage::new(400, 300);
        let (_, t1, _, b1) =
            draw_text(&mut two, &settings("写真", TextPosition::Center), None, None).unwrap();
        let (_, t2, _, b2) =
            draw_text(&mut two, &settings("写真\n二行目", TextPosition::Center), None, None).unwrap();
        assert!(b2 - t2 > 1.8 * (b1 - t1), "{} / {}", b2 - t2, b1 - t1);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn opacity_and_transparency() {
        // 不透明度 50%: 黒の上の白い文字は灰色になる
        let mut image = RgbaImage::from_pixel(200, 100, Rgba([0, 0, 0, 255]));
        let half = TextSettings { opacity: 50, size: 30.0, ..settings("■", TextPosition::Center) };
        draw_text(&mut image, &half, None, None);
        let brightest = image.pixels().map(|p| p[0]).max().unwrap();
        assert!((120..=135).contains(&brightest), "{brightest}");
        // 透明な画像の上にも描ける（文字の部分は不透明になる）
        let mut clear = RgbaImage::new(200, 100);
        draw_text(&mut clear, &TextSettings { opacity: 100, ..half }, None, None);
        assert!(clear.pixels().any(|p| p[3] == 255));
        assert_eq!(clear.get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn empty_text_draws_nothing() {
        let mut image = RgbaImage::from_pixel(20, 10, Rgba([1, 2, 3, 255]));
        assert!(draw_text(&mut image, &settings("  \n ", TextPosition::Center), None, None).is_none());
        assert!(image.pixels().all(|p| p.0 == [1, 2, 3, 255]));
    }

    #[test]
    fn outline_and_shadow_surround_the_text() {
        // 白い文字を白い背景に描く: 飾りがなければ見えず、縁取り・影なら黒が文字のまわりに出る
        let base = TextSettings { color: [255, 255, 255], ..settings("写真", TextPosition::Center) };
        let dark = |image: &RgbaImage| image.pixels().filter(|p| p[0] < 200).count();
        let draw = |effect| {
            let mut image = RgbaImage::from_pixel(400, 200, Rgba([255, 255, 255, 255]));
            let drawn = draw_text(&mut image, &TextSettings { effect, ..base.clone() }, None, None).unwrap();
            (image, drawn)
        };
        let (plain, plain_box) = draw(TextEffect::None);
        assert_eq!(dark(&plain), 0);
        let (outline, outline_box) = draw(TextEffect::Outline);
        let (shadow, _) = draw(TextEffect::Shadow);
        assert!(dark(&outline) > 50 && dark(&shadow) > 50, "{} {}", dark(&outline), dark(&shadow));
        // 文字の位置は変わらない。影は右下にずれる
        assert_eq!(plain_box, outline_box);
        let center = |image: &RgbaImage| {
            let points: Vec<(u32, u32)> =
                image.enumerate_pixels().filter(|(_, _, p)| p[0] < 200).map(|(x, y, _)| (x, y)).collect();
            let n = points.len() as f64;
            let sum = points.iter().fold((0.0, 0.0), |a, &(x, y)| (a.0 + f64::from(x), a.1 + f64::from(y)));
            (sum.0 / n, sum.1 / n)
        };
        let (ox, oy) = center(&outline);
        let (sx, sy) = center(&shadow);
        assert!(sx > ox && sy > oy, "影 ({sx}, {sy}) は縁取り ({ox}, {oy}) より右下");
        // 暗い文字には白の飾り。なしは JSON に書かない
        assert_eq!(effect_color([20, 30, 40]), [255, 255, 255]);
        assert_eq!(effect_color([250, 250, 200]), [0, 0, 0]);
        assert!(!serde_json::to_string(&base).unwrap().contains("effect"));
        let shadowed = TextSettings { effect: TextEffect::Shadow, ..base };
        assert!(serde_json::to_string(&shadowed).unwrap().contains("\"effect\":\"shadow\""));
    }

    #[test]
    fn json_names_match_old_presets() {
        assert_eq!(serde_json::to_string(&TextFont::GothicBold).unwrap(), "\"GOTHIC_BOLD\"");
        assert_eq!(serde_json::to_string(&TextPosition::FrameMargin).unwrap(), "\"frame_margin\"");
    }

    #[test]
    fn placeholders_become_the_capture_date() {
        let date = crate::exif_info::CaptureDate { year: 2026, month: 10, day: 9, hour: 7, minute: 5 };
        assert_eq!(expand_placeholders("撮影 {日付}", Some(&date)), "撮影 2026.10.09");
        assert_eq!(expand_placeholders("{日時}\n© me", Some(&date)), "2026.10.09 07:05\n© me");
        assert_eq!(expand_placeholders("{日付}", None), "");
        assert_eq!(expand_placeholders("{ほか}", Some(&date)), "{ほか}");
    }
}
