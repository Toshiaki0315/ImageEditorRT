//! かすみの除去（旧版にはない）: 霧や逆光で白っぽくかすんだ写真をくっきりさせる（負の値ではかすみを足す）。
//!
//! He らの暗いチャンネル（dark channel prior）を使う。かすみのない部分では、まわりのどれかの色の値がほぼ 0 になる
//! ので、まわりの最小値（暗いチャンネル）が大きいところほどかすんでいるとみなし、大気の光（かすみの色）と
//! 透過率を求めて、かすみを引く。透過率は大きさによらない小さな画像（長辺 `WORK_SIDE`）で求めて広げるので、
//! プレビューと保存（原寸）で同じ見え方になる。

use image::RgbaImage;
use rayon::prelude::*;

use crate::PIXELS_PER_TASK;

/// 強さの範囲。
pub const DEHAZE_MIN: i32 = -100;
pub const DEHAZE_MAX: i32 = 100;
/// 透過率を求める画像の長辺（px）。
const WORK_SIDE: u32 = 256;
/// 暗いチャンネルを取るまわりの大きさ（作業用の画像の長辺に対する割合）。
const PATCH_RATIO: f32 = 0.02;
/// 強さ 100 のときに引くかすみの割合（1 にすると不自然になる）。
const OMEGA_MAX: f32 = 0.9;
/// 透過率の下限（小さすぎると色が飛ぶ）。
const T_MIN: f32 = 0.15;
/// 強さ -100 のときに足すかすみの割合。
const ADD_MAX: f32 = 0.5;

/// かすみの除去をかける（amount は -100〜100。0 なら何もしない。透過はそのまま）。
pub fn dehaze(image: &mut RgbaImage, amount: i32) {
    let amount = amount.clamp(DEHAZE_MIN, DEHAZE_MAX);
    let (width, height) = image.dimensions();
    if amount == 0 || width == 0 || height == 0 {
        return;
    }
    let (small, size) = work_image(image);
    let atmosphere = atmospheric_light(&small, size);
    if amount < 0 {
        // かすみを足す: 大気の光の色に寄せる（明るい・低いコントラスト）
        let k = ADD_MAX * (-amount) as f32 / 100.0;
        image.as_mut().par_chunks_exact_mut(4).with_min_len(PIXELS_PER_TASK).for_each(|p| {
            for c in 0..3 {
                let v = f32::from(p[c]);
                p[c] = (v + (atmosphere[c] * 255.0 - v) * k).round().clamp(0.0, 255.0) as u8;
            }
        });
        return;
    }
    let omega = OMEGA_MAX * amount as f32 / 100.0;
    let transmission = transmission_map(&small, size, atmosphere, omega);
    let fitted = crate::resize::resize_gray_bilinear(&transmission, size, (width, height));
    image.as_mut().par_chunks_exact_mut(4).zip(fitted.par_iter()).with_min_len(PIXELS_PER_TASK).for_each(
        |(p, &t)| {
            let t = (f32::from(t) / 255.0).max(T_MIN);
            for c in 0..3 {
                let a = atmosphere[c];
                let v = f32::from(p[c]) / 255.0;
                p[c] = (((v - a) / t + a) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        },
    );
}

/// 作業用の小さな画像（0〜1 の RGB）とその大きさ。
fn work_image(image: &RgbaImage) -> (Vec<[f32; 3]>, (u32, u32)) {
    let small = crate::resize::fit_long_side(image, WORK_SIDE);
    let size = small.dimensions();
    let pixels = small.pixels().map(|p| [0, 1, 2].map(|c| f32::from(p[c]) / 255.0)).collect();
    (pixels, size)
}

/// 画素ごとの RGB の最小値を、まわり（正方形）の最小値にしたもの（暗いチャンネル）。
fn dark_channel(pixels: &[[f32; 3]], (width, height): (u32, u32)) -> Vec<f32> {
    let mins: Vec<f32> = pixels.iter().map(|p| p[0].min(p[1]).min(p[2])).collect();
    let radius = ((width.max(height) as f32 * PATCH_RATIO).round() as usize).max(1);
    min_filter(&mins, (width as usize, height as usize), radius)
}

/// 横・縦に分けて最小値を取る（半径 radius の正方形）。
fn min_filter(values: &[f32], (width, height): (usize, usize), radius: usize) -> Vec<f32> {
    let mut rows = vec![0.0; values.len()];
    for y in 0..height {
        for x in 0..width {
            let (from, to) = (x.saturating_sub(radius), (x + radius).min(width - 1));
            rows[y * width + x] =
                values[y * width + from..=y * width + to].iter().copied().fold(1.0, f32::min);
        }
    }
    let mut out = vec![0.0; values.len()];
    for y in 0..height {
        let (from, to) = (y.saturating_sub(radius), (y + radius).min(height - 1));
        for x in 0..width {
            out[y * width + x] = (from..=to).map(|yy| rows[yy * width + x]).fold(1.0, f32::min);
        }
    }
    out
}

/// 大気の光: 暗いチャンネルのいちばん明るい 0.1% の画素の色の平均。
fn atmospheric_light(pixels: &[[f32; 3]], size: (u32, u32)) -> [f32; 3] {
    let dark = dark_channel(pixels, size);
    let mut order: Vec<usize> = (0..dark.len()).collect();
    order.sort_unstable_by(|&a, &b| dark[b].total_cmp(&dark[a]));
    let count = (dark.len() / 1000).max(1);
    let mut sum = [0.0f32; 3];
    for &i in &order[..count] {
        for c in 0..3 {
            sum[c] += pixels[i][c];
        }
    }
    sum.map(|s| (s / count as f32).max(0.05))
}

/// 透過率（0〜255）。1 − omega × （大気の光で割った画像の暗いチャンネル）を、境目が目立たないようぼかす。
fn transmission_map(pixels: &[[f32; 3]], size: (u32, u32), atmosphere: [f32; 3], omega: f32) -> Vec<u8> {
    let normalized: Vec<[f32; 3]> =
        pixels.iter().map(|p| std::array::from_fn(|c| (p[c] / atmosphere[c]).min(1.0))).collect();
    let dark = dark_channel(&normalized, size);
    let t: Vec<f32> = dark.iter().map(|d| 1.0 - omega * d).collect();
    let radius = ((size.0.max(size.1) as f32 * PATCH_RATIO * 2.0).round() as usize).max(1);
    let smooth = box_blur(
        &box_blur(&t, (size.0 as usize, size.1 as usize), radius),
        (size.0 as usize, size.1 as usize),
        radius,
    );
    smooth.iter().map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8).collect()
}

/// 半径 radius の箱形のぼかし（横・縦に分けて）。
fn box_blur(values: &[f32], (width, height): (usize, usize), radius: usize) -> Vec<f32> {
    let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
        let mut out = vec![0.0; src.len()];
        let (outer, inner) = if horizontal { (height, width) } else { (width, height) };
        let at = |o: usize, i: usize| if horizontal { o * width + i } else { i * width + o };
        for o in 0..outer {
            for i in 0..inner {
                let (from, to) = (i.saturating_sub(radius), (i + radius).min(inner - 1));
                let sum: f32 = (from..=to).map(|j| src[at(o, j)]).sum();
                out[at(o, i)] = sum / (to - from + 1) as f32;
            }
        }
        out
    };
    pass(&pass(values, true), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// くっきりした模様に、白いかすみを重ねた写真。
    fn hazy(width: u32, height: u32, haze: f32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            let clear = if (x / 8 + y / 8) % 2 == 0 { [30.0, 90.0, 40.0] } else { [160.0, 60.0, 30.0] };
            let [r, g, b] = clear.map(|v: f32| (v * (1.0 - haze) + 230.0 * haze) as u8);
            Rgba([r, g, b, 255])
        })
    }

    fn contrast(image: &RgbaImage) -> f32 {
        let lum: Vec<f32> = image
            .pixels()
            .map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
            .collect();
        let mean = lum.iter().sum::<f32>() / lum.len() as f32;
        (lum.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / lum.len() as f32).sqrt()
    }

    #[test]
    fn haze_is_removed_and_added() {
        let image = hazy(200, 150, 0.6);
        let before = contrast(&image);
        let mut clear = image.clone();
        dehaze(&mut clear, 80);
        assert!(contrast(&clear) > before * 1.5, "{} -> {}", before, contrast(&clear));
        let mut foggy = image.clone();
        dehaze(&mut foggy, -80);
        assert!(contrast(&foggy) < before, "かすみを足すとコントラストが下がる");
        let mut same = image.clone();
        dehaze(&mut same, 0);
        assert_eq!(same, image);
    }

    #[test]
    fn preview_and_full_size_look_alike() {
        let full = hazy(800, 600, 0.5);
        let mut preview = crate::resize::fit_long_side(&full, 400);
        let mut saved = full.clone();
        dehaze(&mut preview, 70);
        dehaze(&mut saved, 70);
        let saved_small = crate::resize::fit_long_side(&saved, 400);
        let diff: f64 = preview
            .as_raw()
            .iter()
            .zip(saved_small.as_raw())
            .map(|(&a, &b)| f64::from(a.abs_diff(b)))
            .sum::<f64>()
            / preview.as_raw().len() as f64;
        assert!(diff < 6.0, "平均の差 {diff}");
    }

    #[test]
    fn transparency_is_kept() {
        let mut image = hazy(60, 40, 0.5);
        image.put_pixel(3, 3, Rgba([200, 200, 200, 0]));
        dehaze(&mut image, 60);
        assert_eq!(image.get_pixel(3, 3)[3], 0);
    }
}
