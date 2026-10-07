//! 投稿加工（旧版にはない）: 写真の一部をぼかし・モザイク・絵文字のスタンプで隠す。
//!
//! 範囲はトリミング範囲と同じく、回転・反転（と水平の補正）をした後の原寸の画像の座標で持つ。
//! ぼかしの半径・モザイクの目の大きさは範囲の短辺に比例させるので、縮小したプレビューでも原寸でも
//! 同じ見え方になる。範囲の外の画素は変えない。
//!
//! ぼかし・モザイクは回転・反転の直後に（`cover`）、スタンプはテイスト・周辺減光・経年劣化の後に
//! （`stamp`。色を変えない）かける。スタンプは macOS の Apple Color Emoji の絵で描く。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use image::{imageops, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::blur::gaussian_blur;
use crate::transform::{clamp_crop, CropRect};

/// 強さの既定・下限・上限。
pub const STRENGTH_DEFAULT: u32 = 50;
pub const STRENGTH_MIN: u32 = 1;
pub const STRENGTH_MAX: u32 = 100;

/// 隠し方。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    #[default]
    Blur,
    Mosaic,
    /// 絵文字のスタンプ（範囲に収まる正方形で中央に置く）
    Stamp,
}

/// スタンプの既定の絵文字。
pub const STAMP_DEFAULT: &str = "😊";

/// 選べるスタンプ（画面の並びの順）。どれも Apple Color Emoji の 1 文字（ハートは異体字セレクタ付き）。
pub const STAMPS: [&str; 20] = [
    "😊", "😀", "😆", "😎", "🥰", "😇", "🙂", "😺", "🐱", "🐶", "🐰", "🐻", "🐼", "🐵", "❤️", "⭐", "🌸",
    "✨", "🍀", "🙈",
];

/// Apple Color Emoji のファイル。
const EMOJI_FONT: &str = "/System/Library/Fonts/Apple Color Emoji.ttc";

/// 隠す範囲 1 つ。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Region {
    pub kind: RegionKind,
    /// 回転・反転した後の原寸の画像の座標（px）
    pub rect: CropRect,
    /// 強さ 1〜100（ぼかしの半径・モザイクの目の大きさ。スタンプでは使わない）
    pub strength: u32,
    /// スタンプの絵文字（スタンプのときだけ使う）
    pub stamp: String,
}

impl Default for Region {
    fn default() -> Self {
        Self {
            kind: RegionKind::Blur,
            rect: CropRect::new(0, 0, 0, 0),
            strength: STRENGTH_DEFAULT,
            stamp: STAMP_DEFAULT.into(),
        }
    }
}

/// 範囲の短辺に対する、ぼかしの半径・モザイクの目の大きさの割合（強さ 0 → 最小、100 → 最大）。
const SIZE_RATIO_MIN: f64 = 0.02;
const SIZE_RATIO_MAX: f64 = 0.2;

fn size_ratio(strength: u32) -> f64 {
    let s = f64::from(strength.clamp(STRENGTH_MIN, STRENGTH_MAX)) / 100.0;
    SIZE_RATIO_MIN + (SIZE_RATIO_MAX - SIZE_RATIO_MIN) * s
}

/// 原寸の座標の範囲を、factor 倍に縮めた画像の座標にする（左上と右下をそれぞれ四捨五入）。
pub fn scale_rect(rect: CropRect, factor: f64) -> CropRect {
    let scale = |v: i64| (v as f64 * factor).round() as i64;
    let (left, top) = (scale(rect.x), scale(rect.y));
    let (right, bottom) = (scale(rect.right()), scale(rect.bottom()));
    CropRect::new(left, top, right - left, bottom - top)
}

/// 画像に範囲のぼかし・モザイクをかける。image は原寸を factor 倍にした画像（回転・反転した後のもの）。
/// 範囲が重なっていれば、並んでいる順にかける。
pub fn cover(image: &mut RgbaImage, regions: &[Region], factor: f64) {
    for region in regions {
        if region.kind == RegionKind::Stamp {
            continue;
        }
        let Some(rect) = clamp_crop(scale_rect(region.rect, factor), image.dimensions()) else { continue };
        let area =
            imageops::crop_imm(image, rect.x as u32, rect.y as u32, rect.width as u32, rect.height as u32)
                .to_image();
        let size = rect.short_side() as f64 * size_ratio(region.strength);
        let covered = match region.kind {
            RegionKind::Blur => gaussian_blur(&area, size as f32),
            RegionKind::Mosaic => mosaic(&area, size.round().max(1.0) as u32),
            RegionKind::Stamp => continue,
        };
        imageops::replace(image, &covered, rect.x, rect.y);
    }
}

/// 画像にスタンプを描く。原寸の座標 (x, y) は画像の (x × scale.0 − offset.0, y × scale.1 − offset.1) に当たる
/// （切り抜き・リサイズの後の画像に描くため）。スタンプは範囲に収まる正方形で、範囲の中央に置く。
/// 画像の外にはみ出す部分は描かない。絵文字を描けなければ（macOS 以外など）何もしない。
pub fn stamp(image: &mut RgbaImage, regions: &[Region], scale: (f64, f64), offset: (f64, f64)) {
    for region in regions.iter().filter(|r| r.kind == RegionKind::Stamp) {
        let r = region.rect;
        let (left, top) = (r.x as f64 * scale.0 - offset.0, r.y as f64 * scale.1 - offset.1);
        let (width, height) = (r.width as f64 * scale.0, r.height as f64 * scale.1);
        let side = width.min(height).round();
        if side < 1.0 {
            continue;
        }
        let Some(picture) = emoji(&region.stamp, side as u32) else { continue };
        let x = (left + (width - side) / 2.0).round() as i64;
        let y = (top + (height - side) / 2.0).round() as i64;
        imageops::overlay(image, &picture, x, y);
    }
}

/// 絵文字の絵を side × side px で返す（Apple Color Emoji の 160px の絵を Lanczos で拡大・縮小する）。
fn emoji(text: &str, side: u32) -> Option<RgbaImage> {
    let picture = emoji_picture(text)?;
    Some(crate::transform::resize_to(&picture, (side, side)))
}

/// 絵文字の元の絵（1 文字目。異体字セレクタは見ない）。一度読んだ絵文字は覚えておく。
fn emoji_picture(text: &str) -> Option<RgbaImage> {
    use ab_glyph::Font;
    static CACHE: OnceLock<Mutex<HashMap<char, Option<RgbaImage>>>> = OnceLock::new();
    let c = text.chars().next()?;
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    cache
        .entry(c)
        .or_insert_with(|| {
            let font = crate::text::load_font_file(std::path::Path::new(EMOJI_FONT), 0)?;
            let id = font.glyph_id(c);
            if id.0 == 0 {
                return None;
            }
            let raster = font.glyph_raster_image2(id, u16::MAX)?;
            if !matches!(raster.format, ab_glyph::GlyphImageFormat::Png) {
                return None;
            }
            image::load_from_memory_with_format(raster.data, image::ImageFormat::Png)
                .ok()
                .map(|i| i.to_rgba8())
        })
        .clone()
}

/// モザイク: block × block の目ごとに、その中の色の平均で塗る（透明度を考えて平均する）。
fn mosaic(image: &RgbaImage, block: u32) -> RgbaImage {
    let (width, height) = image.dimensions();
    let mut out = RgbaImage::new(width, height);
    for top in (0..height).step_by(block as usize) {
        for left in (0..width).step_by(block as usize) {
            let (right, bottom) = ((left + block).min(width), (top + block).min(height));
            let mut sum = [0u64; 4];
            for y in top..bottom {
                for x in left..right {
                    let p = image.get_pixel(x, y).0;
                    let alpha = u64::from(p[3]);
                    for c in 0..3 {
                        sum[c] += u64::from(p[c]) * alpha;
                    }
                    sum[3] += alpha;
                }
            }
            let count = u64::from((right - left) * (bottom - top));
            let color = if sum[3] == 0 {
                [0, 0, 0, 0]
            } else {
                let channel = |c: usize| ((sum[c] + sum[3] / 2) / sum[3]) as u8;
                [channel(0), channel(1), channel(2), ((sum[3] + count / 2) / count) as u8]
            };
            for y in top..bottom {
                for x in left..right {
                    image::Pixel::channels_mut(out.get_pixel_mut(x, y)).copy_from_slice(&color);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn checker(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            if (x / 2 + y / 2) % 2 == 0 {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        })
    }

    fn region(kind: RegionKind, rect: CropRect, strength: u32) -> Region {
        Region { kind, rect, strength, ..Region::default() }
    }

    #[test]
    fn outside_of_the_region_is_unchanged() {
        for kind in [RegionKind::Blur, RegionKind::Mosaic] {
            let original = checker(100, 80);
            let mut image = original.clone();
            let rect = CropRect::new(20, 10, 40, 30);
            cover(&mut image, &[region(kind, rect, 60)], 1.0);
            let inside = |x: u32, y: u32| (20..60).contains(&x) && (10..40).contains(&y);
            for (x, y, p) in image.enumerate_pixels() {
                if !inside(x, y) {
                    assert_eq!(p, original.get_pixel(x, y), "{kind:?} {x} {y}");
                }
            }
            assert_ne!(image, original, "{kind:?}");
        }
    }

    #[test]
    fn mosaic_blocks_grow_with_strength() {
        let distinct = |strength: u32| {
            let mut image = RgbaImage::from_fn(200, 200, |x, y| Rgba([x as u8, y as u8, 100, 255]));
            cover(&mut image, &[region(RegionKind::Mosaic, CropRect::new(0, 0, 200, 200), strength)], 1.0);
            let mut colors: Vec<[u8; 4]> = image.pixels().map(|p| p.0).collect();
            colors.sort_unstable();
            colors.dedup();
            colors.len()
        };
        // 強さ 100 で目は短辺の 20%（5 × 5）、弱いほど細かい
        assert_eq!(distinct(100), 25);
        assert!(distinct(10) > distinct(50) && distinct(50) > distinct(100));
    }

    #[test]
    fn blur_gets_stronger_with_strength() {
        let spread = |strength: u32| {
            let mut image = RgbaImage::from_fn(120, 120, |x, y| {
                if (x / 20 + y / 20) % 2 == 0 {
                    Rgba([255, 255, 255, 255])
                } else {
                    Rgba([0, 0, 0, 255])
                }
            });
            cover(&mut image, &[region(RegionKind::Blur, CropRect::new(0, 0, 120, 120), strength)], 1.0);
            // 白黒の市松模様がどれだけ灰色に近づいたか（小さいほど強くぼけている）
            image.pixels().map(|p| (i32::from(p[0]) - 128).abs()).sum::<i32>()
        };
        assert!(spread(80) < spread(5));
    }

    #[test]
    fn preview_and_full_size_look_the_same() {
        // 原寸 400×400 とその半分のプレビュー。モザイクの目の数は同じ
        let full_image = RgbaImage::from_fn(400, 400, |x, y| Rgba([(x / 2) as u8, (y / 2) as u8, 0, 255]));
        let half_image = RgbaImage::from_fn(200, 200, |x, y| Rgba([x as u8, y as u8, 0, 255]));
        let regions = [region(RegionKind::Mosaic, CropRect::new(100, 100, 200, 200), 100)];
        let (mut full, mut half) = (full_image, half_image);
        cover(&mut full, &regions, 1.0);
        cover(&mut half, &regions, 0.5);
        let count = |image: &RgbaImage| {
            let mut colors: Vec<[u8; 4]> = image.pixels().map(|p| p.0).collect();
            colors.sort_unstable();
            colors.dedup();
            colors.len()
        };
        let blocks = |image: &RgbaImage, side: u32, offset: u32| {
            count(&imageops::crop_imm(image, offset, offset, side, side).to_image())
        };
        assert_eq!(blocks(&full, 200, 100), 25);
        assert_eq!(blocks(&half, 100, 50), 25);
    }

    #[test]
    fn regions_outside_or_empty_are_skipped() {
        let original = checker(20, 20);
        let mut image = original.clone();
        let regions = [
            region(RegionKind::Blur, CropRect::new(50, 50, 10, 10), 50),
            region(RegionKind::Mosaic, CropRect::new(5, 5, 0, 10), 50),
        ];
        cover(&mut image, &regions, 1.0);
        assert_eq!(image, original);
    }

    #[test]
    fn mosaic_keeps_transparency_weighted_colors() {
        // 透明な画素の色は平均に入れない
        let mut image =
            RgbaImage::from_fn(
                2,
                1,
                |x, _| if x == 0 { Rgba([255, 0, 0, 0]) } else { Rgba([0, 0, 255, 255]) },
            );
        let out = mosaic(&image, 2);
        assert_eq!(out.get_pixel(0, 0).0, [0, 0, 255, 128]);
        image = out;
        assert_eq!(image.get_pixel(1, 0).0, [0, 0, 255, 128]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn every_stamp_can_be_drawn() {
        for text in STAMPS {
            let picture = emoji(text, 64).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(picture.dimensions(), (64, 64));
            // 色のついた、不透明な部分がある
            assert!(picture.pixels().filter(|p| p[3] > 200).count() > 200, "{text}");
        }
        // 絵文字のない文字は描かない
        assert!(emoji("\u{E000}", 64).is_none());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn stamp_is_a_centered_square_inside_the_region() {
        let original = RgbaImage::from_pixel(100, 60, Rgba([0, 0, 255, 255]));
        let mut image = original.clone();
        let regions =
            [Region { kind: RegionKind::Stamp, rect: CropRect::new(10, 10, 60, 40), ..Region::default() }];
        stamp(&mut image, &regions, (1.0, 1.0), (0.0, 0.0));
        // 40 × 40 の正方形を、範囲の中央（x = 20〜60, y = 10〜50）に置く
        for (x, y, p) in image.enumerate_pixels() {
            if !((20..60).contains(&x) && (10..50).contains(&y)) {
                assert_eq!(p, original.get_pixel(x, y), "{x} {y}");
            }
        }
        assert_ne!(image.get_pixel(40, 30), original.get_pixel(40, 30));
        // ぼかし・モザイクの cover はスタンプを描かない
        let mut covered = original.clone();
        cover(&mut covered, &regions, 1.0);
        assert_eq!(covered, original);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn stamp_follows_scale_and_offset() {
        let blank = RgbaImage::from_pixel(50, 50, Rgba([0, 0, 0, 255]));
        let regions =
            [Region { kind: RegionKind::Stamp, rect: CropRect::new(40, 40, 40, 40), ..Region::default() }];
        // 原寸を 0.5 倍にして、(10, 10) から切り抜いた画像 → (10, 10)〜(30, 30) に描く
        let mut image = blank.clone();
        stamp(&mut image, &regions, (0.5, 0.5), (10.0, 10.0));
        let changed: Vec<(u32, u32)> = image
            .enumerate_pixels()
            .filter(|(x, y, p)| *p != blank.get_pixel(*x, *y))
            .map(|(x, y, _)| (x, y))
            .collect();
        assert!(!changed.is_empty());
        assert!(changed.iter().all(|&(x, y)| (10..30).contains(&x) && (10..30).contains(&y)));
    }
}
