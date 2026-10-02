//! 旧版が使っていた Pillow の処理を、画素まで同じ結果になるよう移したもの。
//!
//! Pillow の C の計算は float（32bit）で行って整数に切り捨てるものが多い。Apple Silicon の Pillow は
//! 掛け算と足し算を 1 回でまとめて計算する（FMA）ので、`mul_add` で同じ丸め方にする。
//! どれも R・G・B だけを変え、アルファはそのまま残す（旧版の map_rgb と同じ）。

use image::RgbaImage;
use rayon::prelude::*;

use crate::PIXELS_PER_TASK;

/// RGB → L の変換（ImageOps.grayscale・convert("L")。ITU-R 601-2 の係数を 16bit の整数にしたもの）。
pub fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((u32::from(r) * 19595 + u32::from(g) * 38470 + u32::from(b) * 7471 + 0x8000) >> 16) as u8
}

/// Image.blend の 1 つの値: a + alpha × (b − a)。alpha が 0〜1 なら切り捨て、外なら 0〜255 に収める。
pub fn blend(a: u8, b: u8, alpha: f32) -> u8 {
    let value = alpha.mul_add((i32::from(b) - i32::from(a)) as f32, f32::from(a));
    if (0.0..=1.0).contains(&alpha) {
        value as u8
    } else if value <= 0.0 {
        0
    } else if value >= 255.0 {
        255
    } else {
        value as u8
    }
}

/// ImageChops.screen: 255 − (255 − a)(255 − b) / 255（切り捨て）。
pub fn screen(a: u8, b: u8) -> u8 {
    255 - ((255 - u32::from(a)) * (255 - u32::from(b)) / 255) as u8
}

/// RGB の画素ごとに処理する（アルファはそのまま）。
pub fn map_pixels(image: &mut RgbaImage, f: impl Fn(&mut [u8]) + Sync) {
    image.as_mut().par_chunks_exact_mut(4).with_min_len(PIXELS_PER_TASK).for_each(|p| f(&mut p[..3]));
}

/// ImageEnhance.Color: L に変換した灰色と、元の色を factor で混ぜる。
pub fn enhance_color(image: &mut RgbaImage, factor: f64) {
    if factor == 1.0 {
        return;
    }
    let alpha = factor as f32;
    map_pixels(image, |p| {
        let gray = luma(p[0], p[1], p[2]);
        for v in p {
            *v = blend(gray, *v, alpha);
        }
    });
}

/// ImageEnhance.Brightness: 黒と元の色を factor で混ぜる。
pub fn enhance_brightness(image: &mut RgbaImage, factor: f64) {
    let alpha = factor as f32;
    map_pixels(image, |p| {
        for v in p {
            *v = blend(0, *v, alpha);
        }
    });
}

/// ImageEnhance.Contrast: 画像全体の明るさ (L) の平均の灰色と、元の色を factor で混ぜる。
pub fn enhance_contrast(image: &mut RgbaImage, factor: f64) {
    let mean = luma_mean(image);
    let alpha = factor as f32;
    map_pixels(image, |p| {
        for v in p {
            *v = blend(mean, *v, alpha);
        }
    });
}

/// 明るさ (L) の平均を四捨五入した値（ImageStat.Stat(L).mean を int(mean + 0.5) にしたもの）。
fn luma_mean(image: &RgbaImage) -> u8 {
    let pixels = u64::from(image.width()) * u64::from(image.height());
    if pixels == 0 {
        return 0;
    }
    let sum: u64 = image
        .as_raw()
        .par_chunks_exact(4)
        .with_min_len(PIXELS_PER_TASK)
        .map(|p| u64::from(luma(p[0], p[1], p[2])))
        .sum();
    (sum as f64 / pixels as f64 + 0.5) as u8
}

/// ImageOps.colorize（2 色）の表: 0 → black、255 → white の間を整数で割り切り捨てて結ぶ。
pub fn colorize_table(black: [u8; 3], white: [u8; 3]) -> [[u8; 256]; 3] {
    std::array::from_fn(|c| {
        std::array::from_fn(|i| {
            if i == 255 {
                white[c]
            } else {
                let (b, w) = (i32::from(black[c]), i32::from(white[c]));
                (b + (i as i32 * (w - b)).div_euclid(255)) as u8
            }
        })
    })
}

/// Image.convert("L", matrix): 明るさ = R・G・B の重み付き和 + 定数（0〜255 に収める）。
///
/// Apple Silicon の Pillow のコンパイル結果と同じく、G の積を先に計算し、R・B の積を FMA で足す。
pub fn convert_l_matrix(r: u8, g: u8, b: u8, matrix: [f32; 4]) -> u8 {
    let mut v = matrix[1] * f32::from(g);
    v = matrix[0].mul_add(f32::from(r), v);
    v = matrix[2].mul_add(f32::from(b), v);
    let v = (f64::from(v + matrix[3]) + 0.5) as f32;
    if v <= 0.0 {
        0
    } else if v >= 255.0 {
        255
    } else {
        v as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(luma(255, 255, 255), 255);
        assert_eq!(luma(0, 0, 0), 0);
        assert_eq!(blend(10, 20, 0.5), 15);
        assert_eq!(blend(100, 200, 2.0), 255);
        assert_eq!(blend(100, 0, 2.0), 0);
        assert_eq!(screen(0, 0), 0);
        assert_eq!(screen(255, 10), 255);
        let table = colorize_table([10, 35, 80], [220, 236, 248]);
        assert_eq!([table[0][0], table[1][0], table[2][0]], [10, 35, 80]);
        assert_eq!([table[0][255], table[1][255], table[2][255]], [220, 236, 248]);
    }
}
