//! ガウスぼかし（箱ぼかし 3 回で近似。Pillow と同じ考え方）とアンシャープマスク。
//!
//! RGB にだけかけ、アルファはそのまま残す。行ごと・列ごとの処理を rayon で並列にする。

use image::RgbaImage;
use rayon::prelude::*;

/// 標準偏差 sigma（px）のガウスぼかしをかけた新しい画像を返す。
pub fn gaussian_blur(image: &RgbaImage, sigma: f32) -> RgbaImage {
    if sigma <= 0.0 {
        return image.clone();
    }
    let (width, height) = image.dimensions();
    let mut data = image.as_raw().clone();
    let mut scratch = vec![0u8; data.len()];
    for size in box_sizes(sigma) {
        let radius = (size - 1) / 2;
        // 横にぼかしてから縦にぼかす（どちらも行の並びのまま読むので、メモリを順に読める）
        blur_rows(&data, &mut scratch, width as usize, height as usize, radius);
        blur_columns(&scratch, &mut data, width as usize, height as usize, radius);
    }
    // アルファは元のまま
    for (out, src) in data.chunks_exact_mut(4).zip(image.as_raw().chunks_exact(4)) {
        out[3] = src[3];
    }
    RgbaImage::from_raw(width, height, data).expect("大きさは元と同じ")
}

/// アンシャープマスク: 元 + (元 - ぼかし) × percent / 100。差が threshold 以下なら変えない。
pub fn unsharp_mask(image: &RgbaImage, sigma: f32, percent: u32, threshold: u8) -> RgbaImage {
    let blurred = gaussian_blur(image, sigma);
    let amount = percent as i32;
    let mut out = image.clone();
    out.as_mut().par_chunks_exact_mut(4).zip(blurred.as_raw().par_chunks_exact(4)).for_each(
        |(pixel, soft)| {
            for c in 0..3 {
                let original = i32::from(pixel[c]);
                let diff = original - i32::from(soft[c]);
                if diff.unsigned_abs() > u32::from(threshold) {
                    pixel[c] = (original + diff * amount / 100).clamp(0, 255) as u8;
                }
            }
        },
    );
    out
}

/// sigma のガウスぼかしに近い、3 回の箱ぼかしの大きさ（奇数）を返す。
fn box_sizes(sigma: f32) -> [usize; 3] {
    let n = 3.0_f32;
    let ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut lower = ideal.floor() as usize;
    if lower.is_multiple_of(2) {
        lower = lower.saturating_sub(1).max(1);
    }
    let upper = lower + 2;
    let lf = lower as f32;
    let m = ((12.0 * sigma * sigma - n * lf * lf - 4.0 * n * lf - 3.0 * n) / (-4.0 * lf - 4.0))
        .round()
        .clamp(0.0, 3.0) as usize;
    std::array::from_fn(|i| if i < m { lower } else { upper })
}

/// 箱の合計を平均にする割り算を、掛け算とシフトで行う（四捨五入）。
#[derive(Clone, Copy)]
struct Average {
    inverse: u64,
    half: u64,
}

impl Average {
    const SHIFT: u32 = 32;

    fn new(window: u32) -> Self {
        Self { inverse: (1u64 << Self::SHIFT).div_ceil(u64::from(window)), half: u64::from(window / 2) }
    }

    #[inline(always)]
    fn of(self, sum: u32) -> u8 {
        (((u64::from(sum) + self.half) * self.inverse) >> Self::SHIFT) as u8
    }
}

/// 各行を半径 radius の箱でぼかす（端は端の画素を伸ばす）。RGBA の 4 チャンネルをまとめて扱う。
fn blur_rows(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize) {
    let stride = width * 4;
    let average = Average::new((2 * radius + 1) as u32);
    dst.par_chunks_exact_mut(stride).zip(src.par_chunks_exact(stride)).take(height).for_each(|(out, row)| {
        let last = width - 1;
        let pixel = |x: usize| -> &[u8] { &row[x * 4..x * 4 + 4] };
        let mut sums = [0u32; 4];
        for x in 0..=2 * radius {
            // 窓の最初の位置: -radius..=radius（左端より外は左端の画素）
            let p = pixel(x.saturating_sub(radius).min(last));
            for c in 0..4 {
                sums[c] += u32::from(p[c]);
            }
        }
        for x in 0..width {
            let o = x * 4;
            for c in 0..4 {
                out[o + c] = average.of(sums[c]);
            }
            // 窓を 1 つ右へ: x + radius + 1 を足し、x - radius を引く（端は伸ばす）
            let add = pixel((x + radius + 1).min(last));
            let remove = pixel(x.saturating_sub(radius));
            for c in 0..4 {
                sums[c] = sums[c] + u32::from(add[c]) - u32::from(remove[c]);
            }
        }
    });
}

/// 各列を半径 radius の箱でぼかす（端は端の画素を伸ばす）。
///
/// 画像を横長の帯に分けて並列に処理する。帯の中では、窓の合計を 1 行ずつ下にずらしながら
/// 行全体を一度に足し引きするので、メモリは行の並びのまま順に読める。
fn blur_columns(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize) {
    let stride = width * 4;
    let average = Average::new((2 * radius + 1) as u32);
    let bands = (rayon::current_num_threads() * 4).max(1);
    let band_rows = height.div_ceil(bands).max(1);
    let row = |y: isize| -> &[u8] {
        let y = y.clamp(0, height as isize - 1) as usize;
        &src[y * stride..(y + 1) * stride]
    };
    dst.par_chunks_mut(band_rows * stride).enumerate().for_each(|(band, out)| {
        let first = (band * band_rows) as isize;
        let mut sums = vec![0u32; stride];
        for y in first - radius as isize..=first + radius as isize {
            for (sum, &v) in sums.iter_mut().zip(row(y)) {
                *sum += u32::from(v);
            }
        }
        for (i, out_row) in out.chunks_exact_mut(stride).enumerate() {
            let y = first + i as isize;
            for (o, &sum) in out_row.iter_mut().zip(&sums) {
                *o = average.of(sum);
            }
            let add = row(y + radius as isize + 1);
            let remove = row(y - radius as isize);
            for ((sum, &a), &r) in sums.iter_mut().zip(add).zip(remove) {
                *sum = *sum + u32::from(a) - u32::from(r);
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
        // σ = 2 のガウス分布の山の高さは 255 / (√(2π) × 2) ≈ 51
        assert!((48..=54).contains(&center), "{center}");
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
    fn average_matches_division() {
        for window in [1u32, 3, 5, 7, 41, 255, 1001] {
            let average = Average::new(window);
            for sum in (0..=255 * window).step_by(7) {
                assert_eq!(u32::from(average.of(sum)), (sum + window / 2) / window, "{window} {sum}");
            }
        }
    }

    #[test]
    fn box_sizes_grow_with_sigma() {
        assert_eq!(box_sizes(1.0).iter().sum::<usize>() % 2, 1);
        assert!(box_sizes(10.0)[0] > box_sizes(2.0)[0]);
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
