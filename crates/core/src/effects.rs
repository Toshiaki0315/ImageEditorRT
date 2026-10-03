//! ディテール（シャープ・ぼかし・ノイズ除去）。旧版の core/effects.py と画素まで同じ。
//!
//! 半径はどれも reference（基準の短辺、px）に比例させ、縮小プレビューと原寸で効き方をそろえる。

use image::RgbaImage;
use rayon::prelude::*;

use crate::PIXELS_PER_TASK;

use crate::adjust;
use crate::blur::{gaussian_blur, unsharp_mask};
use crate::diorama::composite;
use crate::pillow::luma;
use crate::transform::round_half_even;

const SHARPEN_RADIUS_RATIO: f64 = 0.0012;
/// シャープの半径の下限（px）。小さい画像でも効くようにする
const SHARPEN_MIN_RADIUS: f64 = 1.0;
const SHARPEN_MAX_PERCENT: f64 = 250.0;
const SHARPEN_THRESHOLD: u8 = 2;
const BLUR_MAX_RADIUS_RATIO: f64 = 0.01;
const DENOISE_RADIUS_RATIO: f64 = 0.002;
const DENOISE_EDGE_THRESHOLD: f64 = 40.0;

/// シャープ（0〜100）: 輪郭をくっきりさせる（アンシャープマスク）。
///
/// 半径は reference（基準の短辺、px）に比例させる。output は保存する写真の短辺で、縮小プレビューで
/// 渡すと、保存時の半径（下限込み）を reference / output 倍に換算して保存結果と同じ効き方にする。
pub fn sharpen(image: &RgbaImage, amount: u32, reference: f64, output: Option<f64>) -> RgbaImage {
    if amount == 0 {
        return image.clone();
    }
    let output = output.unwrap_or(reference);
    let saved = SHARPEN_MIN_RADIUS.max(output * SHARPEN_RADIUS_RATIO);
    let percent = round_half_even(SHARPEN_MAX_PERCENT * f64::from(amount) / 100.0) as u32;
    unsharp_mask(image, (saved * reference / output) as f32, percent, SHARPEN_THRESHOLD)
}

/// ぼかし（0〜100）: 全体をぼかす（100 で短辺の 1%）。
pub fn blur(image: &RgbaImage, amount: u32, reference: f64) -> RgbaImage {
    if amount == 0 {
        return image.clone();
    }
    gaussian_blur(image, (reference * BLUR_MAX_RADIUS_RATIO * f64::from(amount) / 100.0) as f32)
}

/// ノイズ除去（0〜100）: ぼかした画像との差が小さい（なめらかな）部分ほどぼかした画像を混ぜ、
/// 差が大きい輪郭は元のまま残す。
pub fn denoise(image: &RgbaImage, amount: u32, reference: f64) -> RgbaImage {
    if amount == 0 {
        return image.clone();
    }
    let mut smooth = gaussian_blur(image, (reference * DENOISE_RADIUS_RATIO) as f32);
    let strength = f64::from(amount) / 100.0;
    // 差が小さいほどぼかした画像を多く混ぜ、しきい値に向かってなめらかに元の画像に戻す
    let weights: [u8; 256] = std::array::from_fn(|v| {
        let t = (v as f64 / DENOISE_EDGE_THRESHOLD).clamp(0.0, 1.0);
        round_half_even(255.0 * strength * (1.0 - adjust::smoothstep(t))) as u8
    });
    // 旧版と同じく、差（ImageChops.difference）を L にして重みにし、Image.composite で混ぜる。
    // 画像を複製せず、ぼかした画像の上に結果を書く
    smooth
        .as_mut()
        .par_chunks_exact_mut(4)
        .zip(image.as_raw().par_chunks_exact(4))
        .with_min_len(PIXELS_PER_TASK)
        .for_each(|(s, p)| {
            let mask = weights[luma(p[0].abs_diff(s[0]), p[1].abs_diff(s[1]), p[2].abs_diff(s[2])) as usize];
            for c in 0..3 {
                s[c] = composite(s[c], p[c], mask);
            }
            s[3] = p[3];
        });
    smooth
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn effects_keep_alpha() {
        let image =
            RgbaImage::from_fn(30, 20, |x, y| Rgba([(x * 8) as u8, (y * 12) as u8, 90, (x * 8) as u8]));
        for out in [sharpen(&image, 50, 20.0, None), blur(&image, 50, 20.0), denoise(&image, 50, 20.0)] {
            assert!(out.pixels().zip(image.pixels()).all(|(a, b)| a[3] == b[3]));
        }
    }
}
