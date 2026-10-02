//! ガウスぼかしとアンシャープマスク。旧版（Pillow の GaussianBlur・UnsharpMask）と画素まで同じ。
//!
//! Pillow のガウスぼかしは「端数のある半径の箱ぼかし」を横・縦それぞれ 3 回かける
//! （libImaging/BoxBlur.c）。その計算（固定小数点の重み・端の画素の伸ばし方）をそのまま移し、
//! 行ごとの処理を rayon で並列にする。R・G・B にだけかけ、アルファは元のまま残す。

use image::RgbaImage;
use rayon::prelude::*;

/// Pillow の GaussianBlur の箱ぼかしの回数。
const PASSES: u32 = 3;

/// 半径 radius（Pillow の GaussianBlur(radius) と同じ意味）のガウスぼかしをかけた新しい画像を返す。
pub fn gaussian_blur(image: &RgbaImage, radius: f32) -> RgbaImage {
    let (width, height) = (image.width() as usize, image.height() as usize);
    if radius <= 0.0 || width == 0 || height == 0 {
        return image.clone();
    }
    let box_radius = box_radius(radius);
    let mut rgb: Vec<[u8; 3]> =
        image.as_raw().as_chunks::<4>().0.iter().map(|p| [p[0], p[1], p[2]]).collect();
    blur_rows(&mut rgb, width, box_radius);
    // 縦は、並べ替えて（転置して）横と同じ計算をし、元に戻す（Pillow と同じ）
    let mut transposed = transpose(&rgb, width, height);
    blur_rows(&mut transposed, height, box_radius);
    let rgb = transpose(&transposed, height, width);
    let mut out = image.clone();
    for (p, c) in out.as_mut().as_chunks_mut::<4>().0.iter_mut().zip(&rgb) {
        p[..3].copy_from_slice(c);
    }
    out
}

/// アンシャープマスク: 元 + (元 − ぼかし) × percent / 100（整数で切り捨て）。差が threshold 以下なら変えない。
pub fn unsharp_mask(image: &RgbaImage, radius: f32, percent: u32, threshold: u8) -> RgbaImage {
    let blurred = gaussian_blur(image, radius);
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

/// ガウスぼかしの半径を、箱ぼかしの（端数のある）半径にする（Pillow の _gaussian_blur_radius）。
fn box_radius(radius: f32) -> f32 {
    let sigma2 = radius * radius / PASSES as f32;
    let length = (12.0 * f64::from(sigma2) + 1.0).sqrt() as f32;
    let l = ((f64::from(length) - 1.0) / 2.0).floor() as f32;
    let a = (2.0 * l + 1.0) * (l * (l + 1.0) - 3.0 * sigma2);
    let a = a / (6.0 * (sigma2 - (l + 1.0) * (l + 1.0)));
    l + a
}

fn transpose(pixels: &[[u8; 3]], width: usize, height: usize) -> Vec<[u8; 3]> {
    let mut out = vec![[0u8; 3]; pixels.len()];
    out.par_chunks_exact_mut(height).enumerate().for_each(|(x, column)| {
        for (y, value) in column.iter_mut().enumerate() {
            *value = pixels[y * width + x];
        }
    });
    out
}

/// 各行に箱ぼかしを 3 回かける。
fn blur_rows(pixels: &mut [[u8; 3]], width: usize, radius: f32) {
    let whole = radius as usize;
    // 端数のある半径の重み（固定小数点、合計で 1 << 24）: 内側の画素は ww、両端の 1 つずつは fw
    let ww = (16_777_216f32 / (radius * 2.0 + 1.0)) as u32;
    let fw = (16_777_216 - (whole as u32 * 2 + 1) * ww) / 2;
    pixels.par_chunks_exact_mut(width).for_each(|row| {
        let mut line = vec![[0u8; 3]; width];
        for _ in 0..PASSES {
            blur_line(row, &mut line, whole, ww, fw);
            row.copy_from_slice(&line);
        }
    });
}

/// 1 行の箱ぼかし（Pillow の ImagingLineBoxBlur）。画像の外は端の画素を伸ばす。
fn blur_line(input: &[[u8; 3]], output: &mut [[u8; 3]], radius: usize, ww: u32, fw: u32) {
    let last = input.len() as isize - 1;
    let at = |i: isize| &input[i.clamp(0, last) as usize];
    let r = radius as isize;
    // x = -1 のときの窓（-1-r 〜 -1+r）の合計
    let mut acc = [0u32; 3];
    for i in -1 - r..r {
        for (sum, &v) in acc.iter_mut().zip(at(i)) {
            *sum += u32::from(v);
        }
    }
    for (x, out) in output.iter_mut().enumerate() {
        let x = x as isize;
        let (remove, add) = (at(x - r - 1), at(x + r));
        let right = at(x + r + 1);
        for c in 0..3 {
            acc[c] = acc[c] + u32::from(add[c]) - u32::from(remove[c]);
            let bulk = acc[c] * ww + (u32::from(remove[c]) + u32::from(right[c])) * fw;
            out[c] = ((bulk + (1 << 23)) >> 24) as u8;
        }
    }
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
            let r = box_radius(radius);
            let ww = (16_777_216f32 / (r * 2.0 + 1.0)) as u32;
            let fw = (16_777_216 - (r as u32 * 2 + 1) * ww) / 2;
            let total = (r as u32 * 2 + 1) * ww + 2 * fw;
            assert!(16_777_216 - total <= 1, "{radius}: {total}");
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
