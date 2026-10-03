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
const TEXT_MARGIN_RATIO: f64 = 0.03;
/// 行間（文字の大きさに対する比率）。
const LINE_SPACING_RATIO: f64 = 0.25;

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
}

impl TextPosition {
    /// すべての位置（画面のプルダウンの順）。
    pub const ALL: [TextPosition; 10] = [
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
        }
    }

    /// 横・縦のそろえ方（0 = 左・上、0.5 = 中央、1 = 右・下）。
    fn anchor(self) -> (f64, f64) {
        match self {
            Self::TopLeft => (0.0, 0.0),
            Self::Top => (0.5, 0.0),
            Self::TopRight => (1.0, 0.0),
            Self::Left => (0.0, 0.5),
            Self::Center | Self::FrameMargin => (0.5, 0.5),
            Self::Right => (1.0, 0.5),
            Self::BottomLeft => (0.0, 1.0),
            Self::Bottom => (0.5, 1.0),
            Self::BottomRight => (1.0, 1.0),
        }
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
        }
    }
}

impl TextSettings {
    /// 描く文字がないか（空白だけも含む）。
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
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
    Layout { scale, line_height, offsets, bounds }
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
            let pixel = image.get_pixel_mut(px as u32, py as u32);
            let dest_alpha = f32::from(pixel[3]) / 255.0;
            // 「上に重ねる」合成（source over）
            let out_alpha = source_alpha + dest_alpha * (1.0 - source_alpha);
            for c in 0..3 {
                let under = f32::from(pixel[c]) * dest_alpha * (1.0 - source_alpha);
                pixel[c] = ((color[c] * source_alpha + under) / out_alpha).round().clamp(0.0, 255.0) as u8;
            }
            pixel[3] = (out_alpha * 255.0).round() as u8;
        });
    });
    Some((origin.0 + b.min.x, origin.1 + b.min.y, origin.0 + b.max.x, origin.1 + b.max.y))
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
    fn json_names_match_old_presets() {
        assert_eq!(serde_json::to_string(&TextFont::GothicBold).unwrap(), "\"GOTHIC_BOLD\"");
        assert_eq!(serde_json::to_string(&TextPosition::FrameMargin).unwrap(), "\"frame_margin\"");
    }
}
