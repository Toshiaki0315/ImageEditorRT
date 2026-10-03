//! ジオラマ風（ミニチュア風・ティルトシフト）。旧版の core/diorama.py を移したもの。
//!
//! ピントの合う帯だけをくっきり残し、帯の外側に向かってなめらかにぼかし、色を少し鮮やかに
//! して、ミニチュア模型を接写したように見せる。旧版と画素まで同じ。

use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::adjust::{apply_lut, curve_lut, s_curve, smoothstep};
use crate::blur::gaussian_blur;
use crate::pillow;
use crate::transform::{round_half_even, CropRect};
use crate::PIXELS_PER_TASK;

/// ぼかし 100 のときのガウスぼかしの半径（短辺に対する比率。縮小プレビューと原寸でそろえる）。
const MAX_RADIUS_RATIO: f64 = 0.02;
/// ピントの帯の端から、ぼけきるまでの長さ（写真の高さ・幅に対する比率）。
pub const TRANSITION: f64 = 0.25;
/// 鮮やかさ 100 のとき: 彩度を何割上げるか、S 字カーブ（コントラスト）をどれだけ混ぜるか。
const MAX_SATURATION: f64 = 0.5;
const MAX_CONTRAST: f64 = 0.4;

/// ピントの帯の向き。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DioramaDirection {
    /// 横の帯（上下をぼかす）
    #[default]
    Horizontal,
    /// 縦の帯（左右をぼかす）
    Vertical,
}

/// ジオラマ風の設定。blur が 0 なら何もしない（鮮やかさも効かない）。位置・幅は写真の高さ（幅）に対する %。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DioramaSettings {
    pub blur: u32,
    pub direction: DioramaDirection,
    pub position: u32,
    pub width: u32,
    pub vivid: u32,
}

/// ピントの帯の位置（写真の高さ・幅に対する割合。0〜1 の外にはみ出すこともある）。
/// sharp_start〜sharp_end はくっきり残す範囲、その外側の blur_start・blur_end でぼけきる。
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DioramaBand {
    pub sharp_start: f64,
    pub sharp_end: f64,
    pub blur_start: f64,
    pub blur_end: f64,
}

/// 設定からピントの帯の位置を返す（プレビューのガイドにも使う）。
pub fn diorama_band(settings: &DioramaSettings) -> DioramaBand {
    let center = f64::from(settings.position) / 100.0;
    let half = f64::from(settings.width) / 200.0;
    DioramaBand {
        sharp_start: center - half,
        sharp_end: center + half,
        blur_start: center - half - TRANSITION,
        blur_end: center + half + TRANSITION,
    }
}

/// ジオラマ風にした新しい画像を返す（入力画像は変更しない）。
///
/// area は、ピントの位置・幅の基準にする写真の範囲（省略時は画像全体）。帯はその外側にも続けて描く。
/// ぼかしの半径は reference（省略時は area の短辺）に比例させる。R・G・B だけを変え、アルファは残す。
pub fn diorama(
    image: &RgbaImage,
    settings: &DioramaSettings,
    area: Option<CropRect>,
    reference: Option<f64>,
) -> RgbaImage {
    if settings.blur == 0 {
        return image.clone();
    }
    let area = area.unwrap_or(CropRect::whole(image.dimensions()));
    let reference = reference.unwrap_or(area.short_side() as f64);
    let radius = reference * MAX_RADIUS_RATIO * f64::from(settings.blur) / 100.0;
    let mask = sharpness_line(image.dimensions(), settings, area);
    let horizontal = settings.direction == DioramaDirection::Horizontal;
    let width = image.width() as usize;
    // くっきり残す度合い（255 = そのまま、0 = ぼけきる）で、元の画像とぼかした画像を混ぜる
    let mut out = gaussian_blur(image, radius as f32);
    out.as_mut()
        .par_chunks_exact_mut(4)
        .zip(image.as_raw().par_chunks_exact(4))
        .enumerate()
        .with_min_len(PIXELS_PER_TASK)
        .for_each(|(i, (soft, pixel))| {
            let m = if horizontal { mask[i / width] } else { mask[i % width] };
            for c in 0..3 {
                soft[c] = composite(pixel[c], soft[c], m);
            }
            soft[3] = pixel[3];
        });
    let vivid = f64::from(settings.vivid) / 100.0;
    if vivid > 0.0 {
        pillow::enhance_color(&mut out, 1.0 + MAX_SATURATION * vivid);
        apply_lut(&mut out, &curve_lut(|x| s_curve(x, MAX_CONTRAST * vivid)));
    }
    out
}

/// 帯に垂直な向き（横の帯なら縦）の 1 列分の、くっきり残す度合い。帯に沿う向きには変わらない。
fn sharpness_line((width, height): (u32, u32), settings: &DioramaSettings, area: CropRect) -> Vec<u8> {
    let horizontal = settings.direction == DioramaDirection::Horizontal;
    let (length, start, span) =
        if horizontal { (height, area.y, area.height) } else { (width, area.x, area.width) };
    let band = diorama_band(settings);
    let center = (band.sharp_start + band.sharp_end) / 2.0;
    let half = (band.sharp_end - band.sharp_start) / 2.0;
    let span = span.max(1) as f64;
    (0..length)
        .map(|i| {
            // 写真の高さ・幅に対する位置（画素の中心）
            let u = (f64::from(i) + 0.5 - start as f64) / span;
            let outside = ((u - center).abs() - half).max(0.0);
            round_half_even(255.0 * (1.0 - smoothstep((outside / TRANSITION).min(1.0)))) as u8
        })
        .collect()
}

/// Image.composite(a, b, mask) の 1 つの値: a × mask + b × (255 − mask) を 255 で割って四捨五入（Pillow の DIV255）。
pub(crate) fn composite(a: u8, b: u8, mask: u8) -> u8 {
    let t = u32::from(b) * (255 - u32::from(mask)) + u32::from(a) * u32::from(mask) + 128;
    (((t >> 8) + t) >> 8) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn stripes() -> RgbaImage {
        RgbaImage::from_fn(100, 100, |x, _| {
            if x % 4 == 0 {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        })
    }

    fn settings(direction: DioramaDirection) -> DioramaSettings {
        DioramaSettings { blur: 100, direction, position: 50, width: 20, vivid: 0 }
    }

    #[test]
    fn keeps_band_sharp() {
        let image = stripes();
        let out = diorama(&image, &settings(DioramaDirection::Horizontal), None, None);
        assert_eq!(out.get_pixel(0, 50), image.get_pixel(0, 50)); // 帯の中
        assert!(out.get_pixel(0, 0)[0] < 200); // 帯から遠い上端はぼける
    }

    #[test]
    fn band_follows_area() {
        let image = stripes();
        let s = DioramaSettings { width: 10, ..settings(DioramaDirection::Horizontal) };
        // 写真の範囲が上の 20px なら、帯は y = 10 のあたり（ぼかしの半径の基準は 100px にする）
        let out = diorama(&image, &s, Some(CropRect::new(0, 0, 100, 20)), Some(100.0));
        assert_eq!(out.get_pixel(0, 10), image.get_pixel(0, 10));
        assert!(out.get_pixel(0, 90)[0] < 200);
    }

    #[test]
    fn off_does_nothing() {
        let image = stripes();
        let s = DioramaSettings { blur: 0, vivid: 100, ..settings(DioramaDirection::Vertical) };
        assert_eq!(diorama(&image, &s, None, None), image);
    }

    #[test]
    fn band_positions() {
        let band = diorama_band(&DioramaSettings {
            position: 30,
            width: 20,
            ..settings(DioramaDirection::Vertical)
        });
        assert!((band.sharp_start - 0.2).abs() < 1e-12 && (band.sharp_end - 0.4).abs() < 1e-12);
        assert!((band.blur_start + 0.05).abs() < 1e-12 && (band.blur_end - 0.65).abs() < 1e-12);
    }
}
