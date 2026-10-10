//! 参考の写真に色を合わせる（旧版にはない）: Lab の明るさ・色の傾きの平均と広がりを、参考の写真のものに近づける
//! （Reinhard らの色の移し替え）。
//!
//! 参考の写真は、選んだときに色の情報（`ColorStats`）だけを測って設定に数値で持つ（ファイルは覚えない）。
//! かけるときは、今の画像の色の情報を測って、参考の値との差を埋める。色の情報は画像の大きさによらない格子で
//! 測るので、プレビューと保存（原寸）でほぼ同じ見え方になる。

use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PIXELS_PER_TASK;

/// 色の情報（Lab の L・a・b のそれぞれの平均と標準偏差）。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorStats {
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

/// 色を合わせる設定。reference がなければかけない。strength は 0〜100%。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorMatch {
    pub reference: Option<ColorStats>,
    pub strength: u32,
}

impl Default for ColorMatch {
    fn default() -> Self {
        Self { reference: None, strength: STRENGTH_DEFAULT }
    }
}

/// 強さの既定（%）。
pub const STRENGTH_DEFAULT: u32 = 100;
/// 色の情報を測る格子の数（縦・横）。
const GRID: u32 = 128;
/// 広がりを何倍まで変えるか（極端な写真どうしで色が壊れないよう抑える）。
const SCALE_MIN: f32 = 0.4;
const SCALE_MAX: f32 = 2.5;
/// 広がりがこれより小さい（ほぼ一色の）ときは、広がりを変えない。
const STD_EPSILON: f32 = 0.5;

/// sRGB（0〜255）を線形の値にする表。
fn linear_table() -> &'static [f32; 256] {
    static TABLE: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|i| {
            let v = i as f32 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        })
    })
}

/// D65 の白。
const WHITE: [f32; 3] = [0.950_47, 1.0, 1.088_83];

fn lab_f(t: f32) -> f32 {
    if t > 0.008_856 {
        t.cbrt()
    } else {
        7.787 * t + 16.0 / 116.0
    }
}

fn lab_f_inv(t: f32) -> f32 {
    let cube = t * t * t;
    if cube > 0.008_856 {
        cube
    } else {
        (t - 16.0 / 116.0) / 7.787
    }
}

/// sRGB の画素を Lab にする。
fn to_lab(p: [u8; 3]) -> [f32; 3] {
    let table = linear_table();
    let [r, g, b] = p.map(|v| table[usize::from(v)]);
    let x = (0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b) / WHITE[0];
    let y = (0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b) / WHITE[1];
    let z = (0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b) / WHITE[2];
    let (fx, fy, fz) = (lab_f(x), lab_f(y), lab_f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// Lab を sRGB の画素にする（範囲の外は切る）。
fn from_lab([l, a, b]: [f32; 3]) -> [u8; 3] {
    let fy = (l + 16.0) / 116.0;
    let x = lab_f_inv(fy + a / 500.0) * WHITE[0];
    let y = lab_f_inv(fy) * WHITE[1];
    let z = lab_f_inv(fy - b / 200.0) * WHITE[2];
    let linear = [
        3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z,
        -0.969_266 * x + 1.876_010_8 * y + 0.041_556 * z,
        0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z,
    ];
    linear.map(|v| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (s * 255.0).round().clamp(0.0, 255.0) as u8
    })
}

/// 画像の色の情報を測る（大きさによらない格子の画素で。透明な画素は数えない）。測れる画素がなければ None。
pub fn measure(image: &RgbaImage) -> Option<ColorStats> {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let mut sum = [0f64; 3];
    let mut squares = [0f64; 3];
    let mut count = 0u32;
    for gy in 0..GRID {
        let y = ((u64::from(gy) * 2 + 1) * u64::from(height) / u64::from(GRID * 2)) as u32;
        for gx in 0..GRID {
            let x = ((u64::from(gx) * 2 + 1) * u64::from(width) / u64::from(GRID * 2)) as u32;
            let p = image.get_pixel(x, y);
            if p[3] < 128 {
                continue;
            }
            let lab = to_lab([p[0], p[1], p[2]]);
            for c in 0..3 {
                sum[c] += f64::from(lab[c]);
                squares[c] += f64::from(lab[c]) * f64::from(lab[c]);
            }
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let n = f64::from(count);
    let mean = sum.map(|s| s / n);
    let std = std::array::from_fn(|c| (squares[c] / n - mean[c] * mean[c]).max(0.0).sqrt() as f32);
    Some(ColorStats { mean: mean.map(|m| m as f32), std })
}

/// 画像の色を参考の色の情報に近づける（strength は 0〜100%。透過はそのまま）。
pub fn apply_color_match(image: &mut RgbaImage, settings: &ColorMatch) {
    let Some(reference) = settings.reference else { return };
    let amount = settings.strength.min(100) as f32 / 100.0;
    if amount == 0.0 {
        return;
    }
    let Some(source) = measure(image) else { return };
    let scale: [f32; 3] = std::array::from_fn(|c| {
        if source.std[c] < STD_EPSILON || reference.std[c] < STD_EPSILON {
            1.0
        } else {
            (reference.std[c] / source.std[c]).clamp(SCALE_MIN, SCALE_MAX)
        }
    });
    image.as_mut().par_chunks_exact_mut(4).with_min_len(PIXELS_PER_TASK).for_each(|p| {
        if p[3] == 0 {
            return;
        }
        let lab = to_lab([p[0], p[1], p[2]]);
        let moved: [f32; 3] = std::array::from_fn(|c| {
            let target = (lab[c] - source.mean[c]) * scale[c] + reference.mean[c];
            lab[c] + (target - lab[c]) * amount
        });
        let rgb = from_lab([moved[0].clamp(0.0, 100.0), moved[1], moved[2]]);
        p[..3].copy_from_slice(&rgb);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// 横に明るさが変わる、色 tint の写真。
    fn gradient(width: u32, height: u32, tint: [f32; 3]) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, _| {
            let v = 40.0 + 160.0 * x as f32 / (width - 1) as f32;
            let [r, g, b] = tint.map(|t| (v * t).clamp(0.0, 255.0) as u8);
            Rgba([r, g, b, 255])
        })
    }

    #[test]
    fn lab_round_trips() {
        for p in [[0, 0, 0], [255, 255, 255], [200, 30, 90], [12, 140, 250], [128, 128, 128]] {
            let back = from_lab(to_lab(p));
            assert!(p.iter().zip(back).all(|(&a, b)| a.abs_diff(b) <= 1), "{p:?} -> {back:?}");
        }
        let white = to_lab([255, 255, 255]);
        assert!((white[0] - 100.0).abs() < 0.1 && white[1].abs() < 0.1 && white[2].abs() < 0.1, "{white:?}");
    }

    #[test]
    fn stats_do_not_depend_on_the_image_size() {
        let small = measure(&gradient(400, 300, [1.0, 0.9, 0.7])).unwrap();
        let large = measure(&gradient(1600, 1200, [1.0, 0.9, 0.7])).unwrap();
        for c in 0..3 {
            assert!((small.mean[c] - large.mean[c]).abs() < 0.5, "{small:?} {large:?}");
            assert!((small.std[c] - large.std[c]).abs() < 0.5, "{small:?} {large:?}");
        }
        assert_eq!(measure(&RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 0]))), None);
    }

    #[test]
    fn colors_move_toward_the_reference() {
        // 青っぽく暗い写真を、暖かく明るい参考に近づける
        let reference = measure(&gradient(200, 100, [1.25, 1.05, 0.7])).unwrap();
        let cold = gradient(200, 100, [0.6, 0.7, 1.0]);
        let mut matched = cold.clone();
        apply_color_match(&mut matched, &ColorMatch { reference: Some(reference), strength: 100 });
        let after = measure(&matched).unwrap();
        let before = measure(&cold).unwrap();
        for c in 0..3 {
            assert!(
                (after.mean[c] - reference.mean[c]).abs() < (before.mean[c] - reference.mean[c]).abs() * 0.3,
                "{c}: {before:?} -> {after:?} (目標 {reference:?})"
            );
        }
        // 強さ 50% なら半分くらい
        let mut half = cold.clone();
        apply_color_match(&mut half, &ColorMatch { reference: Some(reference), strength: 50 });
        let middle = measure(&half).unwrap();
        assert!(middle.mean[2] > before.mean[2] && middle.mean[2] < after.mean[2], "{middle:?}");
        // 参考がない・強さ 0 なら変えない
        let mut same = cold.clone();
        apply_color_match(&mut same, &ColorMatch::default());
        apply_color_match(&mut same, &ColorMatch { reference: Some(reference), strength: 0 });
        assert_eq!(same, cold);
    }

    #[test]
    fn transparency_is_kept() {
        let mut image = RgbaImage::from_fn(20, 20, |x, _| {
            if x < 10 {
                Rgba([30, 60, 200, 255])
            } else {
                Rgba([30, 60, 200, 0])
            }
        });
        let reference = measure(&RgbaImage::from_pixel(8, 8, Rgba([220, 160, 60, 255]))).unwrap();
        apply_color_match(&mut image, &ColorMatch { reference: Some(reference), strength: 100 });
        assert_eq!(image.get_pixel(15, 5).0, [30, 60, 200, 0]);
        let p = image.get_pixel(5, 5).0;
        assert!(p[0] > p[2] && p[3] == 255, "暖かい色に寄る: {p:?}");
    }

    #[test]
    fn settings_round_trip_as_numbers() {
        let settings = ColorMatch {
            reference: Some(ColorStats { mean: [50.0, 5.5, -3.0], std: [20.0, 4.0, 6.0] }),
            strength: 70,
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<ColorMatch>(&json).unwrap(), settings);
        assert_eq!(serde_json::from_str::<ColorMatch>("{}").unwrap(), ColorMatch::default());
    }
}
