//! 文字・透かしを描く（macOS のヒラギノなど、OpenType のフォントを ab_glyph で描く）。

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use image::RgbaImage;

/// どの Mac にも入っているヒラギノ角ゴシック W3（.ttc の 0 番目）。
pub const HIRAGINO_W3: &str = "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc";
const LINE_SPACING_RATIO: f32 = 0.25;
const MARGIN_RATIO: f32 = 0.03;

/// フォントを読む（.ttc は index 番目）。同じフォントは 2 回目から読み直さない。
pub fn load_font(path: &Path, index: u32) -> Option<&'static FontVec> {
    static CACHE: OnceLock<Mutex<Vec<(String, u32, &'static FontVec)>>> = OnceLock::new();
    let key = path.to_string_lossy().into_owned();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    if let Some((_, _, font)) = cache.iter().find(|(p, i, _)| *p == key && *i == index) {
        return Some(font);
    }
    let bytes = std::fs::read(path).ok()?;
    let font: &'static FontVec = Box::leak(Box::new(FontVec::try_from_vec_and_index(bytes, index).ok()?));
    cache.push((key, index, font));
    Some(font)
}

/// 文字を右下に描く。size_percent は短辺に対する文字の大きさ (%)、opacity は 0〜1。
/// 改行で複数行にでき、行は右にそろえる。描けたら true。
pub fn draw_text_bottom_right(
    image: &mut RgbaImage,
    font: &FontVec,
    text: &str,
    size_percent: f32,
    color: [u8; 3],
    opacity: f32,
) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    let (width, height) = image.dimensions();
    let short = width.min(height) as f32;
    let px = (short * size_percent / 100.0).max(1.0);
    let scaled = font.as_scaled(PxScale::from(px));
    let margin = short * MARGIN_RATIO;
    let line_height = scaled.height() + px * LINE_SPACING_RATIO;
    let lines: Vec<&str> = text.lines().collect();
    let block_height = line_height * lines.len() as f32 - px * LINE_SPACING_RATIO;
    let top = height as f32 - margin - block_height;
    for (row, line) in lines.iter().enumerate() {
        let line_width: f32 = line.chars().map(|ch| scaled.h_advance(font.glyph_id(ch))).sum();
        let mut x = width as f32 - margin - line_width;
        let baseline = top + row as f32 * line_height + scaled.ascent();
        for ch in line.chars() {
            let glyph = font.glyph_id(ch).with_scale_and_position(px, ab_glyph::point(x, baseline));
            x += scaled.h_advance(glyph.id);
            let Some(outline) = font.outline_glyph(glyph) else { continue };
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let px_x = bounds.min.x as i32 + gx as i32;
                let px_y = bounds.min.y as i32 + gy as i32;
                if px_x < 0 || px_y < 0 || px_x >= width as i32 || px_y >= height as i32 {
                    return;
                }
                let alpha = coverage.clamp(0.0, 1.0) * opacity;
                let pixel = image.get_pixel_mut(px_x as u32, px_y as u32);
                for c in 0..3 {
                    let under = f32::from(pixel[c]);
                    pixel[c] = (under + (f32::from(color[c]) - under) * alpha).round() as u8;
                }
            });
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    #[cfg(target_os = "macos")]
    fn draws_japanese_with_hiragino() {
        let font = load_font(Path::new(HIRAGINO_W3), 0).expect("ヒラギノを読めること");
        // 日本語の字形を持っている（.notdef = 0 ではない）
        assert_ne!(font.glyph_id('写').0, 0);
        assert_ne!(font.glyph_id('ジ').0, 0);

        let mut image = RgbaImage::from_pixel(400, 300, Rgba([0, 0, 0, 255]));
        assert!(draw_text_bottom_right(&mut image, font, "写真 2026\n二行目", 10.0, [255, 255, 255], 1.0));
        let lit_right_bottom = (200..400)
            .flat_map(|x| (150..300).map(move |y| (x, y)))
            .filter(|&(x, y)| image.get_pixel(x, y)[0] > 128)
            .count();
        let lit_left_top = (0..200)
            .flat_map(|x| (0..150).map(move |y| (x, y)))
            .filter(|&(x, y)| image.get_pixel(x, y)[0] > 128)
            .count();
        assert!(lit_right_bottom > 200, "右下に文字がある: {lit_right_bottom}");
        assert_eq!(lit_left_top, 0);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn font_is_cached() {
        let a = load_font(Path::new(HIRAGINO_W3), 0).unwrap() as *const FontVec;
        let b = load_font(Path::new(HIRAGINO_W3), 0).unwrap() as *const FontVec;
        assert_eq!(a, b);
    }

    #[test]
    fn missing_font_is_none() {
        assert!(load_font(Path::new("/no/such/font.ttc"), 0).is_none());
    }
}
