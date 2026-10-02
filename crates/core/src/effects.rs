//! ぼかしを使う加工（ディテール・ジオラマ・HDR 風）。試作から移したもの。
//! 旧版と同じ結果にするのは #10（ディテール）・#11（ジオラマ）・#9（HDR 風）で行う。
//!
//! 半径はどれも reference（基準の短辺、px）に比例させ、縮小プレビューと原寸で効き方をそろえる。

use image::RgbaImage;
use rayon::prelude::*;

use crate::adjust;
use crate::blur::{gaussian_blur, unsharp_mask};
use crate::transform::CropRect;

const SHARPEN_RADIUS_RATIO: f32 = 0.0012;
const SHARPEN_MAX_PERCENT: u32 = 250;
const BLUR_MAX_RADIUS_RATIO: f32 = 0.01;
const DENOISE_RADIUS_RATIO: f32 = 0.002;
const DENOISE_EDGE_THRESHOLD: f64 = 40.0;
const DIORAMA_MAX_RADIUS_RATIO: f32 = 0.02;
const DIORAMA_TRANSITION: f64 = 0.25;
const HDR_RADIUS_RATIO: f32 = 0.02;

/// シャープ（0〜100）。
pub fn sharpen(image: &RgbaImage, amount: u32, reference: f32) -> RgbaImage {
    let sigma = (reference * SHARPEN_RADIUS_RATIO).max(1.0);
    unsharp_mask(image, sigma, SHARPEN_MAX_PERCENT * amount / 100, 2)
}

/// ぼかし（0〜100）。
pub fn blur(image: &RgbaImage, amount: u32, reference: f32) -> RgbaImage {
    gaussian_blur(image, reference * BLUR_MAX_RADIUS_RATIO * amount as f32 / 100.0)
}

/// ノイズ除去（0〜100）: 輪郭を残してざらつきをなめらかにする。
pub fn denoise(image: &RgbaImage, amount: u32, reference: f32) -> RgbaImage {
    let smooth = gaussian_blur(image, reference * DENOISE_RADIUS_RATIO);
    let strength = f64::from(amount) / 100.0;
    let weights: [u16; 256] = std::array::from_fn(|v| {
        let t = (v as f64 / DENOISE_EDGE_THRESHOLD).clamp(0.0, 1.0);
        (255.0 * strength * (1.0 - adjust::smoothstep(t))).round() as u16
    });
    let mut out = image.clone();
    out.as_mut().par_chunks_exact_mut(4).zip(smooth.as_raw().par_chunks_exact(4)).for_each(|(p, s)| {
        let d: [u32; 3] = std::array::from_fn(|c| u32::from(p[c].abs_diff(s[c])));
        let luma = ((d[0] * 299 + d[1] * 587 + d[2] * 114) / 1000) as usize;
        let w = weights[luma];
        for c in 0..3 {
            let mixed = u16::from(s[c]) * w + u16::from(p[c]) * (255 - w);
            p[c] = ((mixed + 127) / 255) as u8;
        }
    });
    out
}

/// ジオラマ風の設定（ぼかし 0 = なし）。位置・幅は写真の高さに対する %。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Diorama {
    pub blur: u32,
    pub position: u32,
    pub width: u32,
    pub vivid: u32,
}

/// ジオラマ風（横の帯）。帯の中はそのまま、外に向かってなめらかにぼかし、色を少し鮮やかにする。
///
/// area は帯の位置・幅の基準にする写真の範囲（省略時は画像全体）。帯はその外側にも続ける。
pub fn diorama(image: &RgbaImage, settings: Diorama, reference: f32, area: Option<CropRect>) -> RgbaImage {
    let sigma = reference * DIORAMA_MAX_RADIUS_RATIO * settings.blur as f32 / 100.0;
    let blurred = gaussian_blur(image, sigma);
    let area = area.unwrap_or(CropRect::whole(image.dimensions()));
    let (top, height) = (area.y as f64, area.height.max(1) as f64);
    let center = f64::from(settings.position) / 100.0;
    let half = f64::from(settings.width) / 200.0;
    let stride = image.width() as usize * 4;
    let mut out = image.clone();
    out.as_mut()
        .par_chunks_exact_mut(stride)
        .zip(blurred.as_raw().par_chunks_exact(stride))
        .enumerate()
        .for_each(|(y, (row, soft))| {
            let u = (y as f64 + 0.5 - top) / height;
            let outside = ((u - center).abs() - half).max(0.0);
            let blur = adjust::smoothstep((outside / DIORAMA_TRANSITION).min(1.0));
            let w = (blur * 256.0).round() as u16;
            for (p, s) in row.as_chunks_mut::<4>().0.iter_mut().zip(soft.as_chunks::<4>().0) {
                for c in 0..3 {
                    p[c] = ((u16::from(s[c]) * w + u16::from(p[c]) * (256 - w)) >> 8) as u8;
                }
            }
        });
    let vivid = f64::from(settings.vivid) / 100.0;
    if vivid > 0.0 {
        adjust::enhance_color(&mut out, 1.0 + 0.5 * vivid);
        adjust::apply_lut(&mut out, &adjust::curve_lut(|x| adjust::s_curve(x, 0.4 * vivid)));
    }
    out
}

/// HDR 風（細部の明暗を強めてくっきりさせる）。
pub fn hdr(image: &RgbaImage, reference: f32) -> RgbaImage {
    let mut out = unsharp_mask(image, (reference * HDR_RADIUS_RATIO).max(1.0), 90, 0);
    adjust::apply_lut(&mut out, &adjust::curve_lut(|x| x.powf(0.85)));
    adjust::enhance_color(&mut out, 1.15);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn stripes() -> RgbaImage {
        let mut image = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 255]));
        for y in 0..100 {
            for x in (0..100).step_by(4) {
                image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        image
    }

    #[test]
    fn diorama_keeps_band_sharp() {
        let image = stripes();
        let settings = Diorama { blur: 100, position: 50, width: 20, vivid: 0 };
        let out = diorama(&image, settings, 100.0, None);
        assert_eq!(out.get_pixel(0, 50), image.get_pixel(0, 50)); // 帯の中
        assert!(out.get_pixel(0, 0)[0] < 200); // 帯から遠い上端はぼける
    }

    #[test]
    fn diorama_band_follows_area() {
        let image = stripes();
        let settings = Diorama { blur: 100, position: 50, width: 10, vivid: 0 };
        // 写真の範囲が上の 20px なら、帯は y = 10 のあたり
        let out = diorama(&image, settings, 100.0, Some(CropRect::new(0, 0, 100, 20)));
        assert_eq!(out.get_pixel(0, 10), image.get_pixel(0, 10));
        assert!(out.get_pixel(0, 90)[0] < 200);
    }

    #[test]
    fn effects_keep_alpha() {
        let image =
            RgbaImage::from_fn(30, 20, |x, y| Rgba([(x * 8) as u8, (y * 12) as u8, 90, (x * 8) as u8]));
        for out in
            [sharpen(&image, 50, 20.0), blur(&image, 50, 20.0), denoise(&image, 50, 20.0), hdr(&image, 20.0)]
        {
            assert!(out.pixels().zip(image.pixels()).all(|(a, b)| a[3] == b[3]));
        }
    }
}
