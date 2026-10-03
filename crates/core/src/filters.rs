//! テイスト（フィルター 23 種）。旧版の core/filters.py を移したもの。
//!
//! どれも色だけを変え、大きさは変えない（フレームは別に付ける）。R・G・B だけを変え、アルファは
//! 元のまま残す。係数は旧版と同じ（旧版 §5.4）。

use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::adjust::{self, apply_lut, clip, curve_lut, s_curve, smoothstep, Lut};
use crate::blur::{gaussian_blur, unsharp_mask};
use crate::pillow::{self, blend, colorize_table, convert_l_matrix, luma, map_pixels, screen};
use crate::PIXELS_PER_TASK;

const SEPIA_FACTORS: [f64; 3] = [1.07, 0.74, 0.43];
const HIGH_TONE_BRIGHTNESS: f64 = 1.2;
const HIGH_TONE_CONTRAST: f64 = 1.3;
const POLAROID_CONTRAST: f64 = 0.9;
const POLAROID_RED_FACTOR: f64 = 1.05;
const POLAROID_BLUE_FACTOR: f64 = 0.90;
const POSITIVE_SATURATION: f64 = 1.35;
const POSITIVE_CURVE: f64 = 0.5;
const POSITIVE_HIGHLIGHT_WARM: f64 = 6.0;
const POSITIVE_SHADOW_COOL: f64 = 10.0;
const RETRO_SATURATION: f64 = 0.8;
const RETRO_BLACK: f64 = 28.0;
const RETRO_WHITE: f64 = 232.0;
const RETRO_TINT: [f64; 3] = [1.04, 1.0, 0.88];
const RETRO_GRAIN: i32 = 10;
const RETRO_GRAIN_SEED: u64 = 20_260_928;
const HIGH_KEY_GAMMA: f64 = 0.6;
const HIGH_KEY_BLACK: f64 = 20.0;
const HIGH_KEY_SATURATION: f64 = 0.85;
const LOW_KEY_GAMMA: f64 = 1.8;
const LOW_KEY_SATURATION: f64 = 0.9;
const DRAMATIC_CURVE: f64 = 0.7;
const DRAMATIC_GAIN: f64 = 0.95;
const DRAMATIC_SATURATION: f64 = 0.45;
const MODERN_CURVE: f64 = 0.3;
const MODERN_BLACK: f64 = 18.0;
const MODERN_WHITE: f64 = 245.0;
const MODERN_TINT: [f64; 3] = [0.97, 1.0, 1.05];
const MODERN_SATURATION: f64 = 0.85;
const NATURAL_CURVE: f64 = 0.15;
const NATURAL_TINT: [f64; 3] = [1.02, 1.0, 0.98];
const NATURAL_SATURATION: f64 = 1.12;
const CINEMATIC_CURVE: f64 = 0.3;
const CINEMATIC_SPLIT: f64 = 0.10;
const CINEMATIC_SHADOW_GREEN: f64 = 0.04;
const CINEMATIC_SATURATION: f64 = 0.9;
const NOIR_CURVE: f64 = 0.9;
const NOIR_GAMMA: f64 = 1.25;
const BLEACH_SATURATION: f64 = 0.25;
const BLEACH_CURVE: f64 = 0.8;
const BLEACH_GRAIN: i32 = 8;
const BLEACH_GRAIN_SEED: u64 = 19_440_606;
const PASTEL_BLACK: f64 = 70.0;
const PASTEL_GAMMA: f64 = 0.8;
const PASTEL_TINT: [f64; 3] = [1.02, 0.99, 1.03];
const PASTEL_SATURATION: f64 = 0.7;
const CROSS_RED_CURVE: f64 = 0.8;
const CROSS_GREEN_GAMMA: f64 = 0.85;
const CROSS_BLUE_RANGE: (f64, f64) = (0.2, 0.75);
const CROSS_SATURATION: f64 = 1.15;
const CYANOTYPE_DARK: [u8; 3] = [10, 35, 80];
const CYANOTYPE_LIGHT: [u8; 3] = [220, 236, 248];
const SUMMER_GAMMA: f64 = 0.85;
const SUMMER_TINT: [f64; 3] = [0.97, 1.03, 1.07];
const SUMMER_SATURATION: f64 = 1.25;
const AUTUMN_CURVE: f64 = 0.2;
const AUTUMN_GAMMA: f64 = 1.05;
const AUTUMN_TINT: [f64; 3] = [1.08, 0.99, 0.82];
const AUTUMN_SATURATION: f64 = 1.05;
const SOFT_RADIUS_RATIO: f64 = 0.015;
const SOFT_GLOW_THRESHOLD: f64 = 0.55;
const SOFT_GLOW_STRENGTH: f64 = 0.7;
const SOFT_BLUR_MIX: f32 = 0.25;
const HDR_RADIUS_RATIO: f64 = 0.02;
const HDR_DETAIL_PERCENT: u32 = 90;
const HDR_GAMMA: f64 = 0.85;
const HDR_SATURATION: f64 = 1.15;
const INFRARED_WEIGHTS: [f32; 4] = [-0.2, 1.5, -0.9, 0.0];
const INFRARED_GAMMA: f64 = 0.6;
const INFRARED_COLOR_MIX: f32 = 0.3;

/// テイスト（加工の種類）。JSON では旧版と同じ名前（"high_tone" など）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterType {
    #[default]
    None,
    Sepia,
    Monotone,
    HighTone,
    Polaroid,
    PositiveFilm,
    RetroCamera,
    HighKey,
    LowKey,
    Dramatic,
    Modern,
    Natural,
    Cinematic,
    Noir,
    BleachBypass,
    Pastel,
    CrossProcess,
    Cyanotype,
    Summer,
    Autumn,
    SoftFocus,
    Hdr,
    Infrared,
}

impl FilterType {
    /// すべてのテイスト（画面のプルダウンの順）。
    pub const ALL: [FilterType; 23] = [
        Self::None,
        Self::Sepia,
        Self::Monotone,
        Self::HighTone,
        Self::Polaroid,
        Self::PositiveFilm,
        Self::RetroCamera,
        Self::HighKey,
        Self::LowKey,
        Self::Dramatic,
        Self::Modern,
        Self::Natural,
        Self::Cinematic,
        Self::Noir,
        Self::BleachBypass,
        Self::Pastel,
        Self::CrossProcess,
        Self::Cyanotype,
        Self::Summer,
        Self::Autumn,
        Self::SoftFocus,
        Self::Hdr,
        Self::Infrared,
    ];

    /// 画面に出す名前。
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "なし",
            Self::Sepia => "セピア",
            Self::Monotone => "モノトーン",
            Self::HighTone => "ハイトーン",
            Self::Polaroid => "ポラロイド風",
            Self::PositiveFilm => "ポジフィルム風",
            Self::RetroCamera => "レトロカメラ風",
            Self::HighKey => "ハイキー",
            Self::LowKey => "ローキー",
            Self::Dramatic => "ドラマチック",
            Self::Modern => "モダン",
            Self::Natural => "ナチュラル",
            Self::Cinematic => "シネマティック",
            Self::Noir => "ノワール",
            Self::BleachBypass => "ブリーチバイパス",
            Self::Pastel => "パステル",
            Self::CrossProcess => "クロスプロセス",
            Self::Cyanotype => "青写真",
            Self::Summer => "夏らしい",
            Self::Autumn => "秋らしい",
            Self::SoftFocus => "ソフトフォーカス",
            Self::Hdr => "HDR 風",
            Self::Infrared => "赤外線風",
        }
    }
}

/// 画像にテイストをかける（なしなら何もしない）。ぼかしの半径は画像の短辺に比例させる。
/// テイストを強さ strength（0〜100%）でかける: 元の写真とテイストをかけた写真を R・G・B ごとに混ぜる
/// （四捨五入。アルファは元のまま）。100 以上は apply_filter と同じ、0 はテイストなし。
pub fn apply_filter_with_strength(image: &mut RgbaImage, filter: FilterType, strength: u32) {
    if filter == FilterType::None || strength == 0 {
        return;
    }
    if strength >= 100 {
        apply_filter(image, filter);
        return;
    }
    let original = image.clone();
    apply_filter(image, filter);
    let (keep, take) = (100 - strength, strength);
    image
        .as_mut()
        .par_chunks_exact_mut(4)
        .zip(original.as_raw().par_chunks_exact(4))
        .with_min_len(PIXELS_PER_TASK)
        .for_each(|(out, before)| {
            for c in 0..3 {
                out[c] = ((u32::from(before[c]) * keep + u32::from(out[c]) * take + 50) / 100) as u8;
            }
            out[3] = before[3];
        });
}

pub fn apply_filter(image: &mut RgbaImage, filter: FilterType) {
    match filter {
        FilterType::None => {}
        FilterType::Sepia => {
            let tables = SEPIA_FACTORS.map(scale_table);
            map_pixels(image, |p| {
                let gray = luma(p[0], p[1], p[2]) as usize;
                for (c, v) in p.iter_mut().enumerate() {
                    *v = tables[c][gray];
                }
            });
        }
        FilterType::Monotone => map_pixels(image, |p| p.fill(luma(p[0], p[1], p[2]))),
        FilterType::HighTone => {
            pillow::enhance_brightness(image, HIGH_TONE_BRIGHTNESS);
            pillow::enhance_contrast(image, HIGH_TONE_CONTRAST);
        }
        FilterType::Polaroid => {
            pillow::enhance_contrast(image, POLAROID_CONTRAST);
            let identity = adjust::identity_lut()[1];
            apply_lut(
                image,
                &[scale_table(POLAROID_RED_FACTOR), identity, scale_table(POLAROID_BLUE_FACTOR)],
            );
        }
        FilterType::PositiveFilm => {
            pillow::enhance_color(image, POSITIVE_SATURATION);
            let curve = |v: usize| s_curve(v as f64 / 255.0, POSITIVE_CURVE) * 255.0;
            let warm = |v: usize| POSITIVE_HIGHLIGHT_WARM * (v as f64 / 255.0).powi(2);
            let cool = |v: usize| POSITIVE_SHADOW_COOL * (1.0 - v as f64 / 255.0).powi(2);
            apply_lut(
                image,
                &[
                    std::array::from_fn(|v| clip(curve(v) + warm(v))),
                    std::array::from_fn(|v| clip(curve(v))),
                    std::array::from_fn(|v| clip(curve(v) + cool(v))),
                ],
            );
        }
        FilterType::RetroCamera => {
            pillow::enhance_color(image, RETRO_SATURATION);
            let lut: Lut = std::array::from_fn(|c| {
                std::array::from_fn(|v| {
                    clip((RETRO_BLACK + v as f64 * (RETRO_WHITE - RETRO_BLACK) / 255.0) * RETRO_TINT[c])
                })
            });
            apply_lut(image, &lut);
            adjust::add_grain(image, RETRO_GRAIN, RETRO_GRAIN_SEED);
        }
        FilterType::HighKey => {
            let black = HIGH_KEY_BLACK / 255.0;
            tone(image, |x| black + (1.0 - black) * x.powf(HIGH_KEY_GAMMA), [1.0; 3], HIGH_KEY_SATURATION);
        }
        FilterType::LowKey => tone(image, |x| x.powf(LOW_KEY_GAMMA), [1.0; 3], LOW_KEY_SATURATION),
        FilterType::Dramatic => {
            let curve = |x: f64| {
                let once = smoothstep(x);
                ((1.0 - DRAMATIC_CURVE) * once + DRAMATIC_CURVE * smoothstep(once)) * DRAMATIC_GAIN
            };
            tone(image, curve, [1.0; 3], DRAMATIC_SATURATION);
        }
        FilterType::Modern => {
            let (black, white) = (MODERN_BLACK / 255.0, MODERN_WHITE / 255.0);
            tone(
                image,
                |x| black + (white - black) * s_curve(x, MODERN_CURVE),
                MODERN_TINT,
                MODERN_SATURATION,
            );
        }
        FilterType::Natural => tone(image, |x| s_curve(x, NATURAL_CURVE), NATURAL_TINT, NATURAL_SATURATION),
        FilterType::Cinematic => {
            pillow::enhance_color(image, CINEMATIC_SATURATION);
            apply_lut(
                image,
                &[
                    curve_lut(|x| s_curve(x, CINEMATIC_CURVE) + CINEMATIC_SPLIT * (2.0 * x - 1.0))[0],
                    curve_lut(|x| s_curve(x, CINEMATIC_CURVE) + CINEMATIC_SHADOW_GREEN * (1.0 - x).powi(2))
                        [0],
                    curve_lut(|x| s_curve(x, CINEMATIC_CURVE) - CINEMATIC_SPLIT * (2.0 * x - 1.0))[0],
                ],
            );
        }
        FilterType::Noir => {
            let table = curve_lut(|x| {
                let once = smoothstep(x);
                ((1.0 - NOIR_CURVE) * once + NOIR_CURVE * smoothstep(once)).powf(NOIR_GAMMA)
            })[0];
            map_pixels(image, |p| p.fill(table[luma(p[0], p[1], p[2]) as usize]));
        }
        FilterType::BleachBypass => {
            tone(image, |x| s_curve(x, BLEACH_CURVE), [1.0; 3], BLEACH_SATURATION);
            adjust::add_grain(image, BLEACH_GRAIN, BLEACH_GRAIN_SEED);
        }
        FilterType::Pastel => {
            let black = PASTEL_BLACK / 255.0;
            tone(image, |x| black + (1.0 - black) * x.powf(PASTEL_GAMMA), PASTEL_TINT, PASTEL_SATURATION);
        }
        FilterType::CrossProcess => {
            let (low, high) = CROSS_BLUE_RANGE;
            pillow::enhance_color(image, CROSS_SATURATION);
            apply_lut(
                image,
                &[
                    curve_lut(|x| s_curve(x, CROSS_RED_CURVE))[0],
                    curve_lut(|x| x.powf(CROSS_GREEN_GAMMA))[0],
                    curve_lut(|x| low + (high - low) * x)[0],
                ],
            );
        }
        FilterType::Cyanotype => {
            let table = colorize_table(CYANOTYPE_DARK, CYANOTYPE_LIGHT);
            map_pixels(image, |p| {
                let gray = luma(p[0], p[1], p[2]) as usize;
                for (c, v) in p.iter_mut().enumerate() {
                    *v = table[c][gray];
                }
            });
        }
        FilterType::Summer => tone(image, |x| x.powf(SUMMER_GAMMA), SUMMER_TINT, SUMMER_SATURATION),
        FilterType::Autumn => {
            tone(image, |x| s_curve(x, AUTUMN_CURVE).powf(AUTUMN_GAMMA), AUTUMN_TINT, AUTUMN_SATURATION);
        }
        FilterType::SoftFocus => soft_focus(image),
        FilterType::Hdr => {
            *image = unsharp_mask(image, radius(image, HDR_RADIUS_RATIO), HDR_DETAIL_PERCENT, 0);
            tone(image, |x| x.powf(HDR_GAMMA), [1.0; 3], HDR_SATURATION);
        }
        FilterType::Infrared => {
            let curve = curve_lut(|x| x.powf(INFRARED_GAMMA))[0];
            map_pixels(image, |p| {
                let gray = curve[convert_l_matrix(p[0], p[1], p[2], INFRARED_WEIGHTS) as usize];
                // R と B を入れ替えた非現実的な色を、明るさの灰色に少し混ぜる
                let swapped = [p[2], p[1], p[0]];
                for (v, s) in p.iter_mut().zip(swapped) {
                    *v = blend(gray, s, INFRARED_COLOR_MIX);
                }
            });
        }
    }
}

/// 明るい部分がにじみ、光があふれたような柔らかさにする（ソフトフォーカス）。
fn soft_focus(image: &mut RgbaImage) {
    let blurred = gaussian_blur(image, radius(image, SOFT_RADIUS_RATIO));
    let threshold = SOFT_GLOW_THRESHOLD;
    let glow: [u8; 256] = std::array::from_fn(|v| {
        clip(((v as f64 / 255.0 - threshold) / (1.0 - threshold)).max(0.0) * 255.0 * SOFT_GLOW_STRENGTH)
    });
    for (p, b) in image.pixels_mut().zip(blurred.pixels()) {
        for c in 0..3 {
            p[c] = screen(blend(p[c], b[c], SOFT_BLUR_MIX), glow[b[c] as usize]);
        }
    }
}

/// ぼかしの半径: 画像の短辺 × ratio（最小 1px）。縮小プレビューと原寸で見た目をそろえる。
/// 旧版と同じく倍精度で計算してから、Pillow に渡すときの float（32bit）にする。
fn radius(image: &RgbaImage, ratio: f64) -> f32 {
    (f64::from(image.width().min(image.height())) * ratio).max(1.0) as f32
}

/// 彩度を変えてから、トーンカーブ (0〜1 → 0〜1) と色味の係数を 1 回の表でかける（旧版の _tone）。
fn tone(image: &mut RgbaImage, curve: impl Fn(f64) -> f64, tint: [f64; 3], saturation: f64) {
    pillow::enhance_color(image, saturation);
    let levels: [f64; 256] = std::array::from_fn(|v| curve(v as f64 / 255.0) * 255.0);
    let lut: Lut = std::array::from_fn(|c| std::array::from_fn(|v| clip(levels[v] * tint[c])));
    apply_lut(image, &lut);
}

/// 各値に factor を掛けて 0〜255 に収める表（旧版の _scale_table）。
fn scale_table(factor: f64) -> [u8; 256] {
    std::array::from_fn(|v| clip(v as f64 * factor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_blends_with_the_original() {
        let image = RgbaImage::from_fn(30, 20, |x, y| image::Rgba([(x * 8) as u8, (y * 12) as u8, 90, 200]));
        let mut full = image.clone();
        apply_filter(&mut full, FilterType::Sepia);
        // 100% は今までと同じ、0% はテイストなし
        let at = |strength| {
            let mut out = image.clone();
            apply_filter_with_strength(&mut out, FilterType::Sepia, strength);
            out
        };
        assert_eq!(at(100), full);
        assert_eq!(at(0), image);
        // 途中は R・G・B を混ぜる（四捨五入）。アルファは元のまま
        let half = at(50);
        for ((h, o), f) in half.pixels().zip(image.pixels()).zip(full.pixels()) {
            for c in 0..3 {
                assert_eq!(u32::from(h[c]), (u32::from(o[c]) + u32::from(f[c])).div_ceil(2), "{c}");
            }
            assert_eq!(h[3], o[3]);
        }
        let quarter = at(25);
        let p = (quarter.get_pixel(10, 5), image.get_pixel(10, 5), full.get_pixel(10, 5));
        assert_eq!(u32::from(p.0[0]), (u32::from(p.1[0]) * 75 + u32::from(p.2[0]) * 25 + 50) / 100);
        // テイストなしは強さによらず何もしない
        let mut none = image.clone();
        apply_filter_with_strength(&mut none, FilterType::None, 40);
        assert_eq!(none, image);
    }

    #[test]
    fn names_are_unique() {
        let labels: std::collections::HashSet<_> = FilterType::ALL.iter().map(|f| f.label()).collect();
        assert_eq!(labels.len(), FilterType::ALL.len());
        assert_eq!(serde_json::to_string(&FilterType::HighTone).unwrap(), "\"high_tone\"");
        assert_eq!(serde_json::from_str::<FilterType>("\"soft_focus\"").unwrap(), FilterType::SoftFocus);
    }
}
