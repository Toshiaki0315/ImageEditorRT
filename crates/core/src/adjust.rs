//! 色の調整（露出・明るさ・コントラスト・色温度・彩度・周辺減光・経年劣化）。
//!
//! 計算式・丸め方は旧版（core/effects.py・core/tone.py と、その中で使う Pillow）と同じ。
//! チャンネルごとの調整はルックアップテーブル (LUT) にして、続けてかける分は 1 つの表に
//! まとめてから 1 回でかける（整数の表どうしなので、続けてかけた結果と完全に同じ）。

use image::RgbaImage;
use rayon::prelude::*;

use crate::PIXELS_PER_TASK;

use crate::pyrandom::PyRandom;
use crate::resize::resize_gray_bilinear;
use crate::transform::round_half_even;

/// 0〜255 → 0〜255 の表（R・G・B それぞれ）。
pub type Lut = [[u8; 256]; 3];

const LUMA: [f64; 3] = [0.299, 0.587, 0.114];
pub const TEMPERATURE_NEUTRAL: u32 = 6500;
const TEMPERATURE_STRENGTH: f64 = 0.5;
const CONTRAST_MIN_SLOPE: f64 = 0.5;
const VIGNETTE_MAX_DARKEN: f64 = 0.8;
const VIGNETTE_START: f64 = 0.35;
const AGING_MAX_DESATURATE: f64 = 0.6;
const AGING_MAX_BLACK: f64 = 45.0;
const AGING_MAX_WHITE_DROP: f64 = 25.0;
const AGING_MAX_TINT: [f64; 3] = [1.08, 1.0, 0.72];
const AGING_MAX_GRAIN: f64 = 14.0;
/// 粒子の模様を毎回同じにするための乱数の種（旧版と同じ）。
const AGING_GRAIN_SEED: u64 = 19_700_101;
/// Pillow の Image.radial_gradient の大きさ（中心からの距離 × √2 の濃淡、四隅で 255）。
const GRADIENT_SIZE: u32 = 256;

pub fn identity_lut() -> Lut {
    let row: [u8; 256] = std::array::from_fn(|v| v as u8);
    [row; 3]
}

/// 0〜1 のトーンカーブを、3 チャンネル共通の表にする。
pub fn curve_lut(curve: impl Fn(f64) -> f64) -> Lut {
    let row: [u8; 256] = std::array::from_fn(|v| clip(curve(v as f64 / 255.0) * 255.0));
    [row; 3]
}

/// a の後に b をかけるのと同じ表を返す（整数の表どうしなので、続けてかけた結果と完全に同じ）。
pub fn compose(a: &Lut, b: &Lut) -> Lut {
    std::array::from_fn(|c| std::array::from_fn(|v| b[c][a[c][v] as usize]))
}

pub fn exposure_lut(ev: f64) -> Lut {
    let gain = 2f64.powf(ev);
    curve_lut(|x| linear_to_srgb((srgb_to_linear(x) * gain).min(1.0)))
}

pub fn brightness_lut(amount: i32) -> Lut {
    let gamma = 2f64.powf(-f64::from(amount) / 50.0);
    curve_lut(|x| x.powf(gamma))
}

pub fn contrast_lut(amount: i32) -> Lut {
    let t = f64::from(amount.abs()) / 100.0;
    if amount >= 0 {
        curve_lut(|x| s_curve(x, t))
    } else {
        let slope = 1.0 - (1.0 - CONTRAST_MIN_SLOPE) * t;
        curve_lut(|x| 0.5 + (x - 0.5) * slope)
    }
}

pub fn temperature_lut(kelvin: u32) -> Lut {
    let multipliers = temperature_multipliers(kelvin);
    std::array::from_fn(|c| std::array::from_fn(|v| clip(v as f64 * multipliers[c])))
}

/// 色温度に対する R / G / B の倍率（6500K で 1、輝度は 1 に正規化）。
pub fn temperature_multipliers(kelvin: u32) -> [f64; 3] {
    let reference = kelvin_to_rgb(f64::from(TEMPERATURE_NEUTRAL));
    let color = kelvin_to_rgb(f64::from(kelvin));
    let raw: [f64; 3] = std::array::from_fn(|c| 1.0 + (color[c] / reference[c] - 1.0) * TEMPERATURE_STRENGTH);
    let luma: f64 = (0..3).map(|c| LUMA[c] * raw[c]).sum();
    raw.map(|m| m / luma)
}

fn kelvin_to_rgb(kelvin: f64) -> [f64; 3] {
    let t = kelvin / 100.0;
    let (red, green) = if t <= 66.0 {
        (255.0, 99.470_802_586_1 * t.ln() - 161.119_568_166_1)
    } else {
        (
            329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2),
            288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2),
        )
    };
    let blue = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7
    };
    [red, green, blue].map(|v| v.clamp(1.0, 255.0))
}

/// 表をかける（アルファはそのまま）。
pub fn apply_lut(image: &mut RgbaImage, lut: &Lut) {
    image.as_mut().par_chunks_exact_mut(4).with_min_len(PIXELS_PER_TASK).for_each(|p| {
        p[0] = lut[0][p[0] as usize];
        p[1] = lut[1][p[1] as usize];
        p[2] = lut[2][p[2] as usize];
    });
}

/// 彩度を factor 倍にする（Pillow の ImageEnhance.Color と同じ。`pillow::enhance_color`）。
pub fn enhance_color(image: &mut RgbaImage, factor: f64) {
    crate::pillow::enhance_color(image, factor);
}

/// 彩度 -100〜+100（-100 で白黒、0 で変化なし、+100 で鮮やかさ 2 倍）。
pub fn saturation(image: &mut RgbaImage, amount: i32) {
    if amount != 0 {
        enhance_color(image, 1.0 + f64::from(amount) / 100.0);
    }
}

/// Pillow の ImageChops.multiply と同じ、a × b / 255 の切り捨て。
fn mul_div_255(a: u8, b: u8) -> u8 {
    (u32::from(a) * u32::from(b) / 255) as u8
}

/// 周辺減光 0〜100。画像の中心を基準に、縦横比に合わせた楕円状に四隅へ向かって暗くする。
///
/// 旧版と同じく、中心からの距離の濃淡（Pillow の radial_gradient、256×256）を画像の大きさに
/// バイリニアで引き伸ばし、明るさの倍率の表をかけて、画素に掛ける。
pub fn vignette(image: &mut RgbaImage, amount: u32) {
    if amount == 0 {
        return;
    }
    let (width, height) = image.dimensions();
    let gradient: Vec<u8> = (0..GRADIENT_SIZE * GRADIENT_SIZE)
        .map(|i| {
            let (x, y) = (f64::from(i % GRADIENT_SIZE) - 128.0, f64::from(i / GRADIENT_SIZE) - 128.0);
            ((x * x + y * y) * 2.0).sqrt().min(255.0) as u8
        })
        .collect();
    let distance = resize_gray_bilinear(&gradient, (GRADIENT_SIZE, GRADIENT_SIZE), (width, height));
    let strength = VIGNETTE_MAX_DARKEN * f64::from(amount) / 100.0;
    let corner = std::f64::consts::SQRT_2;
    // radial_gradient は四隅が 255 なので、上下左右の端（距離 1）は 255 / √2
    let edge = 255.0 / corner;
    let multiplier: [u8; 256] = std::array::from_fn(|v| {
        let t = ((v as f64 / edge - VIGNETTE_START) / (corner - VIGNETTE_START)).clamp(0.0, 1.0);
        round_half_even(255.0 * (1.0 - strength * smoothstep(t))) as u8
    });
    image.as_mut().par_chunks_exact_mut(4).zip(distance.par_iter()).with_min_len(PIXELS_PER_TASK).for_each(
        |(p, &d)| {
            let m = multiplier[d as usize];
            for v in &mut p[..3] {
                *v = mul_div_255(*v, m);
            }
        },
    );
}

/// 経年劣化 0〜100（退色・フェード・黄ばみと青の抜け・粒子）。粒子の模様は固定で、同じ入力なら毎回同じ。
pub fn aging(image: &mut RgbaImage, amount: u32) {
    if amount == 0 {
        return;
    }
    let t = f64::from(amount) / 100.0;
    enhance_color(image, 1.0 - AGING_MAX_DESATURATE * t);
    // フェードと色かぶりを 1 回の表でかける
    let black = AGING_MAX_BLACK * t;
    let white = 255.0 - AGING_MAX_WHITE_DROP * t;
    let lut: Lut = std::array::from_fn(|c| {
        let tint = 1.0 + (AGING_MAX_TINT[c] - 1.0) * t;
        std::array::from_fn(|v| clip((black + v as f64 * (white - black) / 255.0) * tint))
    });
    apply_lut(image, &lut);
    let grain = round_half_even(AGING_MAX_GRAIN * t) as i32;
    if grain > 0 {
        add_grain(image, grain, AGING_GRAIN_SEED);
    }
}

/// モノクロの粒子を重ねる（明るさは最大で ±strength）。旧版と同じく、同じ seed と大きさなら同じ模様。
///
/// Python の random.Random(seed).randbytes で一様な乱数の画像を 2 枚作って平均し（中央に寄った
/// 自然な粒子にする）、128 より明るい分を足し、暗い分を引く。
pub fn add_grain(image: &mut RgbaImage, strength: i32, seed: u64) {
    let count = (image.width() * image.height()) as usize;
    let mut random = PyRandom::new(seed);
    let first = random.bytes(count);
    let second = random.bytes(count);
    let scale = f64::from(strength) / 128.0;
    let lift: [u8; 256] = std::array::from_fn(|v| ((v as f64 - 128.0).max(0.0) * scale + 0.5) as u8);
    let drop: [u8; 256] = std::array::from_fn(|v| ((128.0 - v as f64).max(0.0) * scale + 0.5) as u8);
    image
        .as_mut()
        .par_chunks_exact_mut(4)
        .zip(first.par_iter().zip(second.par_iter()))
        .with_min_len(PIXELS_PER_TASK)
        .for_each(|(p, (&a, &b))| {
            // Pillow の Image.blend(a, b, 0.5) と同じく切り捨て
            let noise = 0.5f32.mul_add((i32::from(b) - i32::from(a)) as f32, f32::from(a)) as usize;
            let (up, down) = (lift[noise], drop[noise]);
            for v in &mut p[..3] {
                *v = v.saturating_add(up).saturating_sub(down);
            }
        });
}

pub fn smoothstep(x: f64) -> f64 {
    x * x * (3.0 - 2.0 * x)
}

pub fn s_curve(x: f64, strength: f64) -> f64 {
    (1.0 - strength) * x + strength * smoothstep(x)
}

pub fn clip(value: f64) -> u8 {
    (value + 0.5).floor().clamp(0.0, 255.0) as u8
}

fn srgb_to_linear(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(v: f64) -> f64 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn neutral_settings_change_nothing() {
        let id = identity_lut();
        assert_eq!(exposure_lut(0.0), id);
        assert_eq!(brightness_lut(0), id);
        assert_eq!(contrast_lut(0), id);
        assert_eq!(temperature_lut(TEMPERATURE_NEUTRAL), id);
    }

    #[test]
    fn exposure_one_stop_brightens_midtones() {
        // Python 版と同じ値（+1 EV で 128 → 176、-1 EV で 255 → 188）
        assert_eq!(exposure_lut(1.0)[0][128], 176);
        assert_eq!(exposure_lut(1.0)[0][0], 0);
        assert_eq!(exposure_lut(-1.0)[0][255], 188);
    }

    #[test]
    fn compose_equals_sequential() {
        let a = brightness_lut(40);
        let b = contrast_lut(30);
        let both = compose(&a, &b);
        for v in 0..256 {
            assert_eq!(both[1][v], b[1][a[1][v] as usize]);
        }
    }

    #[test]
    fn temperature_multipliers_match_python() {
        // Python 版: (1.137364999206921, 0.9652456275761113, 0.8186726478066079)
        let m = temperature_multipliers(3000);
        assert!((m[0] - 1.137_364_999).abs() < 1e-6);
        assert!((m[1] - 0.965_245_627).abs() < 1e-6);
        assert!((m[2] - 0.818_672_647).abs() < 1e-6);
    }

    #[test]
    fn warm_temperature_raises_red() {
        let lut = temperature_lut(3000);
        assert!(lut[0][128] > 128 && lut[2][128] < 128);
    }

    #[test]
    fn saturation_minus_100_is_gray() {
        let mut image = RgbaImage::from_pixel(2, 2, Rgba([200, 100, 50, 255]));
        saturation(&mut image, -100);
        let p = image.get_pixel(0, 0);
        assert_eq!((p[0], p[1], p[2]), (124, 124, 124));
    }

    #[test]
    fn vignette_darkens_corners_only() {
        let mut image = RgbaImage::from_pixel(101, 101, Rgba([200, 200, 200, 255]));
        vignette(&mut image, 100);
        assert_eq!(image.get_pixel(50, 50)[0], 200);
        assert!(image.get_pixel(0, 0)[0] < 60);
    }

    #[test]
    fn grain_is_repeatable() {
        let base = RgbaImage::from_pixel(30, 20, Rgba([128, 128, 128, 255]));
        let mut a = base.clone();
        let mut b = base.clone();
        add_grain(&mut a, 10, 1);
        add_grain(&mut b, 10, 1);
        assert_eq!(a, b);
        assert_ne!(a, base);
    }
}
