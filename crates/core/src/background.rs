//! 背景を消す（旧版にはない）: 被写体のマスクで、背景を透明か白にする。
//!
//! マスク（被写体 255・背景 0。途中の値は境目のなめらかさ）は macOS の Vision で作る（`foreground`）。
//! ここでは、どの大きさの画像にもマスクを合わせて（拡大・縮小して）かける。処理は元の画像に対して、ほかの
//! すべての加工の前にかける（回転・切り抜き・テイストなどは、背景を消した後の画像にかかる）。

use image::{GrayImage, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PIXELS_PER_TASK;

/// 背景の扱い。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Background {
    /// そのまま（消さない）
    #[default]
    Keep,
    /// 透明にする（PNG・TIFF などで保存すると透明のまま。JPEG では白になる）
    Transparent,
    /// 白にする
    White,
}

/// 画像に mask（画像と違う大きさなら合わせる）をかけて、背景を透明か白にした新しい画像を返す。
/// Keep なら複製を返す。
pub fn apply_background(image: &RgbaImage, mask: &GrayImage, mode: Background) -> RgbaImage {
    let mut out = image.clone();
    if mode == Background::Keep {
        return out;
    }
    let (width, height) = image.dimensions();
    let fitted = if mask.dimensions() == (width, height) {
        mask.as_raw().clone()
    } else {
        crate::resize::resize_gray_bilinear(mask.as_raw(), mask.dimensions(), (width, height))
    };
    out.as_mut().par_chunks_exact_mut(4).zip(fitted.par_iter()).with_min_len(PIXELS_PER_TASK).for_each(
        |(p, &m)| {
            // 被写体である度合い（元の透明度も掛ける）
            let keep = u32::from(p[3]) * u32::from(m);
            match mode {
                Background::Transparent => p[3] = ((keep + 127) / 255) as u8,
                Background::White => {
                    // 白の上に重ねる（被写体の度合いで、元の色と白を混ぜる）
                    for c in &mut p[..3] {
                        let mixed = u32::from(*c) * keep + 255 * (255 * 255 - keep);
                        *c = ((mixed + 255 * 255 / 2) / (255 * 255)) as u8;
                    }
                    p[3] = 255;
                }
                Background::Keep => {}
            }
        },
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgba};

    /// 左半分が被写体（255）、右半分が背景（0）のマスク。
    fn half_mask(width: u32, height: u32) -> GrayImage {
        GrayImage::from_fn(width, height, |x, _| if x < width / 2 { Luma([255]) } else { Luma([0]) })
    }

    #[test]
    fn transparent_and_white_backgrounds() {
        let image = RgbaImage::from_pixel(8, 4, Rgba([10, 100, 200, 255]));
        let mask = half_mask(8, 4);
        let clear = apply_background(&image, &mask, Background::Transparent);
        assert_eq!(clear.get_pixel(1, 1).0, [10, 100, 200, 255]);
        assert_eq!(clear.get_pixel(6, 1)[3], 0);
        let white = apply_background(&image, &mask, Background::White);
        assert_eq!(white.get_pixel(1, 1).0, [10, 100, 200, 255]);
        assert_eq!(white.get_pixel(6, 1).0, [255, 255, 255, 255]);
        assert_eq!(apply_background(&image, &mask, Background::Keep), image);
    }

    #[test]
    fn mask_is_fitted_to_the_image_size() {
        // 4 × 2 のマスクを 40 × 20 の画像に合わせる。左の端は被写体、右の端は背景
        let image = RgbaImage::from_pixel(40, 20, Rgba([50, 50, 50, 255]));
        let clear = apply_background(&image, &half_mask(4, 2), Background::Transparent);
        assert_eq!(clear.dimensions(), (40, 20));
        assert_eq!(clear.get_pixel(2, 10)[3], 255);
        assert_eq!(clear.get_pixel(37, 10)[3], 0);
        // 境目は途中の透明度（なめらか）
        assert!(clear.pixels().any(|p| p[3] > 0 && p[3] < 255));
    }

    #[test]
    fn soft_edges_and_existing_transparency() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 128]));
        let mask = GrayImage::from_pixel(1, 1, Luma([128]));
        // 元の透明度（半分）× マスク（半分）= 4 分の 1
        assert_eq!(apply_background(&image, &mask, Background::Transparent).get_pixel(0, 0)[3], 64);
        // 白: 黒を 4 分の 1 だけ残して白に混ぜる
        let white = apply_background(&image, &mask, Background::White);
        assert_eq!(white.get_pixel(0, 0).0, [191, 191, 191, 255]);
    }
}
