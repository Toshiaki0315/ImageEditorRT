//! ガウスぼかしとアンシャープマスク。旧版（Pillow の GaussianBlur・UnsharpMask）と画素まで同じ。
//!
//! Pillow のガウスぼかしは「端数のある半径の箱ぼかし」を横・縦それぞれ 3 回かける
//! （libImaging/BoxBlur.c）。その計算（固定小数点の重み・端の画素の伸ばし方）をそのまま移し、
//! 行ごとの処理を rayon で並列にする。R・G・B にだけかけ、アルファは元のまま残す。

use image::RgbaImage;
use rayon::prelude::*;

use crate::PIXELS_PER_TASK;

/// Pillow の GaussianBlur の箱ぼかしの回数。
const PASSES: u32 = 3;

/// 半径 radius（Pillow の GaussianBlur(radius) と同じ意味）のガウスぼかしをかけた新しい画像を返す。
pub fn gaussian_blur(image: &RgbaImage, radius: f32) -> RgbaImage {
    let (width, height) = (image.width() as usize, image.height() as usize);
    if radius <= 0.0 || width == 0 || height == 0 {
        return image.clone();
    }
    let weights = Weights::new(box_radius(radius));
    // 4 チャンネルをまとめて計算し（アルファの分は最後に捨てる）、画素の並びのまま横 → 縦にかける
    let mut data = image.as_raw().clone();
    blur_rows(&mut data, width, weights);
    let mut scratch = vec![0u8; data.len()];
    for _ in 0..PASSES {
        blur_columns(&data, &mut scratch, width, height, weights);
        std::mem::swap(&mut data, &mut scratch);
    }
    for (out, src) in data.as_chunks_mut::<4>().0.iter_mut().zip(image.as_raw().as_chunks::<4>().0) {
        out[3] = src[3];
    }
    RgbaImage::from_raw(width as u32, height as u32, data).expect("大きさは元と同じ")
}

/// アンシャープマスク: 元 + (元 − ぼかし) × percent / 100（整数で切り捨て）。差が threshold 以下なら変えない。
pub fn unsharp_mask(image: &RgbaImage, radius: f32, percent: u32, threshold: u8) -> RgbaImage {
    // 画像を複製せず、ぼかした画像の上に結果を書く
    let mut out = gaussian_blur(image, radius);
    let amount = percent as i32;
    out.as_mut()
        .par_chunks_exact_mut(4)
        .zip(image.as_raw().par_chunks_exact(4))
        .with_min_len(PIXELS_PER_TASK)
        .for_each(|(soft, pixel)| {
            for c in 0..3 {
                let original = i32::from(pixel[c]);
                let diff = original - i32::from(soft[c]);
                soft[c] = if diff.unsigned_abs() > u32::from(threshold) {
                    (original + diff * amount / 100).clamp(0, 255) as u8
                } else {
                    pixel[c]
                };
            }
        });
    out
}

/// ガウスぼかしの半径を、箱ぼかしの（端数のある）半径にする（Pillow の _gaussian_blur_radius）。
fn box_radius(radius: f32) -> f32 {
    let sigma2 = radius * radius / PASSES as f32;
    let length = (12.0 * f64::from(sigma2) + 1.0).sqrt() as f32;
    let l = ((f64::from(length) - 1.0) / 2.0).floor() as f32;
    let a = (2.0 * l + 1.0) * (l * (l + 1.0) - 3.0 * sigma2);
    let a = a / (6.0 * (sigma2 - (l + 1.0) * (l + 1.0)));
    l + a
}

/// 端数のある半径の箱の重み（固定小数点、合計で 1 << 24）: 内側の 2r+1 画素は ww、その外の両端は fw。
#[derive(Clone, Copy)]
struct Weights {
    radius: usize,
    ww: u32,
    fw: u32,
}

impl Weights {
    fn new(radius: f32) -> Self {
        let whole = radius as usize;
        let ww = (16_777_216f32 / (radius * 2.0 + 1.0)) as u32;
        let fw = (16_777_216 - (whole as u32 * 2 + 1) * ww) / 2;
        Self { radius: whole, ww, fw }
    }

    /// 窓の合計 acc と、窓のすぐ外の両端の値から、出力の値を求める。
    #[inline(always)]
    fn value(self, acc: u32, left: u8, right: u8) -> u8 {
        let bulk = acc * self.ww + (u32::from(left) + u32::from(right)) * self.fw;
        ((bulk + (1 << 23)) >> 24) as u8
    }
}

/// 各行に横の箱ぼかしを 3 回かける（Pillow の ImagingLineBoxBlur。画像の外は端の画素を伸ばす）。
fn blur_rows(data: &mut [u8], width: usize, weights: Weights) {
    data.par_chunks_exact_mut(width * 4).for_each(|row| {
        let mut line = vec![0u8; row.len()];
        // row → line → row → line と 3 回かけ、最後に row へ写す
        blur_line(row, &mut line, weights);
        blur_line(&line, row, weights);
        blur_line(row, &mut line, weights);
        row.copy_from_slice(&line);
    });
}

fn blur_line(input: &[u8], output: &mut [u8], w: Weights) {
    let input = input.as_chunks::<4>().0;
    let output = output.as_chunks_mut::<4>().0;
    let width = input.len();
    let last = width as isize - 1;
    let r = w.radius as isize;
    let at = |i: isize| &input[i.clamp(0, last) as usize];
    // x = -1 のときの窓（-1-r 〜 -1+r）の合計
    // アルファは最後に元に戻すので、R・G・B の 3 つだけを計算する
    let mut acc = [0u32; 3];
    for i in -1 - r..r {
        for (sum, &v) in acc.iter_mut().zip(at(i)) {
            *sum += u32::from(v);
        }
    }
    let mut step = |out: &mut [u8; 4], remove: &[u8; 4], add: &[u8; 4], right: &[u8; 4]| {
        for c in 0..3 {
            acc[c] = acc[c] + u32::from(add[c]) - u32::from(remove[c]);
            out[c] = w.value(acc[c], remove[c], right[c]);
        }
    };
    // 窓が画像の端にかからない真ん中（x - r - 1 ≥ 0 かつ x + r + 1 ≤ last）は、端の判定なしで計算する
    let start = (w.radius + 1).min(width);
    let end = width.saturating_sub(w.radius + 1).max(start);
    for x in 0..start {
        let x = x as isize;
        step(&mut output[x as usize], at(x - r - 1), at(x + r), at(x + r + 1));
    }
    for x in start..end {
        step(&mut output[x], &input[x - w.radius - 1], &input[x + w.radius], &input[x + w.radius + 1]);
    }
    for x in end..width {
        let x = x as isize;
        step(&mut output[x as usize], at(x - r - 1), at(x + r), at(x + r + 1));
    }
}

/// 各列に縦の箱ぼかしを 1 回かける（横と同じ計算）。画像を横長の帯に分けて並列にし、
/// 帯の中では窓の合計を 1 行ずつ下にずらしながら、行全体をまとめて計算する（メモリを順に読める）。
fn blur_columns(src: &[u8], dst: &mut [u8], width: usize, height: usize, w: Weights) {
    let stride = width * 4;
    let bands = (rayon::current_num_threads() * 2).max(1);
    let band_rows = height.div_ceil(bands).max(1);
    let row = |y: isize| {
        let y = y.clamp(0, height as isize - 1) as usize;
        &src[y * stride..(y + 1) * stride]
    };
    let r = w.radius as isize;
    dst.par_chunks_mut(band_rows * stride).enumerate().for_each(|(band, out)| {
        let first = (band * band_rows) as isize;
        // この帯の最初の行の 1 つ前（y = first - 1）の窓の合計
        let mut acc = vec![0u32; stride];
        for y in first - 1 - r..first + r {
            for (sum, &v) in acc.iter_mut().zip(row(y)) {
                *sum += u32::from(v);
            }
        }
        for (i, out_row) in out.chunks_exact_mut(stride).enumerate() {
            let y = first + i as isize;
            let (remove, add, right) = (row(y - r - 1), row(y + r), row(y + r + 1));
            for k in 0..stride {
                acc[k] = acc[k] + u32::from(add[k]) - u32::from(remove[k]);
                out_row[k] = w.value(acc[k], remove[k], right[k]);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn flat_image_stays_flat() {
        let image = RgbaImage::from_pixel(40, 30, Rgba([100, 150, 200, 255]));
        assert_eq!(gaussian_blur(&image, 5.0), image);
    }

    #[test]
    fn edge_is_softened_and_symmetric() {
        let mut image = RgbaImage::from_pixel(41, 5, Rgba([0, 0, 0, 255]));
        for y in 0..5 {
            image.put_pixel(20, y, Rgba([255, 255, 255, 255]));
        }
        let blurred = gaussian_blur(&image, 2.0);
        let center = blurred.get_pixel(20, 2)[0];
        // 半径 2 のガウス分布の山の高さは 255 / (√(2π) × 2) ≈ 51
        assert!((45..=57).contains(&center), "{center}");
        assert_eq!(blurred.get_pixel(18, 2), blurred.get_pixel(22, 2));
        assert!(blurred.get_pixel(18, 2)[0] > 0);
        assert_eq!(blurred.get_pixel(5, 2)[0], 0);
    }

    #[test]
    fn keeps_alpha() {
        let mut image = RgbaImage::from_pixel(10, 10, Rgba([10, 20, 30, 77]));
        image.put_pixel(5, 5, Rgba([250, 250, 250, 200]));
        let blurred = gaussian_blur(&image, 3.0);
        assert_eq!(blurred.get_pixel(5, 5)[3], 200);
        assert_eq!(blurred.get_pixel(0, 0)[3], 77);
    }

    #[test]
    fn weights_sum_to_one() {
        for radius in [0.5f32, 1.0, 1.7, 3.2, 10.0] {
            let w = Weights::new(box_radius(radius));
            let total = (w.radius as u32 * 2 + 1) * w.ww + 2 * w.fw;
            assert!(16_777_216 - total <= 1, "{radius}: {total}");
        }
    }

    #[test]
    fn small_images_and_large_radius() {
        // 半径が画像より大きくても、端の画素を伸ばして計算できる
        for (w, h) in [(1, 1), (3, 2), (2, 7)] {
            let image = RgbaImage::from_fn(w, h, |x, y| Rgba([(x * 90) as u8, (y * 40) as u8, 7, 255]));
            let blurred = gaussian_blur(&image, 20.0);
            assert_eq!(blurred.dimensions(), (w, h));
        }
    }

    #[test]
    fn unsharp_increases_contrast() {
        let mut image = RgbaImage::from_pixel(20, 3, Rgba([100, 100, 100, 255]));
        for x in 10..20 {
            for y in 0..3 {
                image.put_pixel(x, y, Rgba([150, 150, 150, 255]));
            }
        }
        let sharp = unsharp_mask(&image, 2.0, 150, 2);
        assert!(sharp.get_pixel(9, 1)[0] < 100);
        assert!(sharp.get_pixel(10, 1)[0] > 150);
        assert_eq!(sharp.get_pixel(0, 1)[0], 100); // 平らなところは変えない
    }
}
