//! 縮小（fast_image_resize の Lanczos3。SIMD と rayon で速く縮める）。

use fast_image_resize::{images::Image, images::ImageRef, PixelType, Resizer};
use image::RgbaImage;
use rayon::prelude::*;

/// 長辺が max_side px 以下になるように縦横比を保って縮める（もともと小さければそのまま）。
pub fn fit_long_side(image: &RgbaImage, max_side: u32) -> RgbaImage {
    let (width, height) = image.dimensions();
    let long = width.max(height);
    if long <= max_side || max_side == 0 {
        return image.clone();
    }
    let scale = max_side as f64 / long as f64;
    let new_width = ((width as f64 * scale).round() as u32).max(1);
    let new_height = ((height as f64 * scale).round() as u32).max(1);
    resize(image, new_width, new_height)
}

/// 指定の大きさに縮める・広げる（Lanczos3）。
pub fn resize(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let src = ImageRef::new(image.width(), image.height(), image.as_raw(), PixelType::U8x4)
        .expect("RgbaImage の大きさと画素の並びは正しい");
    let mut dst = Image::new(width, height, PixelType::U8x4);
    Resizer::new().resize(&src, &mut dst, None).expect("同じ画素の形式どうしなので縮められる");
    RgbaImage::from_raw(width, height, dst.into_vec()).expect("大きさは指定のとおり")
}

/// 1 チャンネル（グレー）の画素をバイリニアで指定の大きさにする。
///
/// Pillow の Image.resize(BILINEAR) と画素まで同じにするため、Pillow の計算（固定小数点の係数、
/// 横 → 縦の 2 回）をそのまま移している（周辺減光の濃淡を旧版とそろえるのに使う）。
pub fn resize_gray_bilinear(
    pixels: &[u8],
    (width, height): (u32, u32),
    (new_width, new_height): (u32, u32),
) -> Vec<u8> {
    let (width, height, new_width, new_height) =
        (width as usize, height as usize, new_width as usize, new_height as usize);
    let horizontal = PillowCoeffs::new(width, new_width);
    let vertical = PillowCoeffs::new(height, new_height);
    // 縦の計算に使う行だけを横に引き伸ばす
    let first = vertical.bounds[0].0;
    let last = vertical.bounds.last().map_or(0, |&(start, count)| start + count);
    let mut temp = vec![0u8; new_width * (last - first)];
    temp.par_chunks_exact_mut(new_width).enumerate().for_each(|(row, out)| {
        let source = &pixels[(row + first) * width..(row + first + 1) * width];
        for (x, value) in out.iter_mut().enumerate() {
            *value = horizontal.apply(x, |i| source[i]);
        }
    });
    let mut out = vec![0u8; new_width * new_height];
    out.par_chunks_exact_mut(new_width).enumerate().for_each(|(y, row)| {
        for (x, value) in row.iter_mut().enumerate() {
            *value = vertical.apply(y, |i| temp[(i - first) * new_width + x]);
        }
    });
    out
}

/// Pillow の precompute_coeffs と normalize_coeffs_8bpc（BILINEAR、8bit）。
struct PillowCoeffs {
    /// 出力の各画素の、元の画素の範囲 (最初, 数)
    bounds: Vec<(usize, usize)>,
    /// 出力の各画素の係数（固定小数点）
    weights: Vec<Vec<i64>>,
}

impl PillowCoeffs {
    const PRECISION_BITS: u32 = 32 - 8 - 2;

    fn new(in_size: usize, out_size: usize) -> Self {
        let scale = in_size as f64 / out_size as f64;
        let filter_scale = scale.max(1.0);
        let support = filter_scale; // BILINEAR の support は 1
        let mut bounds = Vec::with_capacity(out_size);
        let mut weights = Vec::with_capacity(out_size);
        for xx in 0..out_size {
            let center = (xx as f64 + 0.5) * scale;
            let xmin = ((center - support + 0.5) as i64).max(0) as usize;
            let xmax = ((center + support + 0.5) as i64).min(in_size as i64) as usize - xmin;
            let raw: Vec<f64> = (0..xmax)
                .map(|x| {
                    let d = ((x + xmin) as f64 - center + 0.5) / filter_scale;
                    (1.0 - d.abs()).max(0.0)
                })
                .collect();
            let total: f64 = raw.iter().sum();
            let k = raw
                .iter()
                .map(|&w| {
                    let w = if total != 0.0 { w / total } else { w };
                    let fixed = w * f64::from(1u32 << Self::PRECISION_BITS);
                    (if w < 0.0 { fixed - 0.5 } else { fixed + 0.5 }) as i64
                })
                .collect();
            bounds.push((xmin, xmax));
            weights.push(k);
        }
        Self { bounds, weights }
    }

    fn apply(&self, index: usize, pixel: impl Fn(usize) -> u8) -> u8 {
        let (start, _) = self.bounds[index];
        let sum = self.weights[index]
            .iter()
            .enumerate()
            .fold(1i64 << (Self::PRECISION_BITS - 1), |s, (i, &k)| s + i64::from(pixel(start + i)) * k);
        (sum >> Self::PRECISION_BITS).clamp(0, 255) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn fits_long_side_and_keeps_aspect() {
        let image = RgbaImage::from_pixel(600, 400, Rgba([10, 20, 30, 255]));
        let small = fit_long_side(&image, 300);
        assert_eq!(small.dimensions(), (300, 200));
        assert_eq!(small.get_pixel(150, 100), &Rgba([10, 20, 30, 255]));
        let tall = fit_long_side(&RgbaImage::new(400, 600), 300);
        assert_eq!(tall.dimensions(), (200, 300));
    }

    #[test]
    fn small_image_is_unchanged() {
        let image = RgbaImage::from_pixel(100, 50, Rgba([1, 2, 3, 4]));
        assert_eq!(fit_long_side(&image, 1600), image);
    }
}
