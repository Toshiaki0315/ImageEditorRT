//! トーンカーブと色ごとの調整（HSL）。旧版にはない。
//!
//! - トーンカーブ: 点を通るなめらかな曲線（単調な 3 次補間。Fritsch–Carlson）を、R・G・B 共通の変換表にする。
//!   点の x は 0〜255 で増えていく順、両端は x = 0 と x = 255
//! - 色ごとの調整: 赤・オレンジ・黄・緑・水色・青・紫・マゼンタの 8 色の色相・彩度・明るさ。画素の色相が
//!   となりあう 2 色のあいだにあれば、近いほうを強く効かせる（色の境目がなめらか）。彩度の低い（灰色に近い）
//!   画素ほど効きを弱める（灰色・白・黒は変わらない）

use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::adjust::Lut;
use crate::PIXELS_PER_TASK;

/// トーンカーブの点の数の上限。
pub const CURVE_MAX_POINTS: usize = 16;

/// 既定のトーンカーブ（まっすぐ = 変化なし）。
pub fn identity_curve() -> Vec<[u8; 2]> {
    vec![[0, 0], [255, 255]]
}

/// トーンカーブの点として正しいか（2〜16 点、x は 0 から 255 まで増えていく）。
pub fn is_valid_curve(points: &[[u8; 2]]) -> bool {
    (2..=CURVE_MAX_POINTS).contains(&points.len())
        && points.first().is_some_and(|p| p[0] == 0)
        && points.last().is_some_and(|p| p[0] == 255)
        && points.windows(2).all(|w| w[0][0] < w[1][0])
}

/// トーンカーブの変換表（R・G・B 共通）。点が正しくなければまっすぐ（変化なし）。
pub fn curve_lut(points: &[[u8; 2]]) -> Lut {
    let row = curve_row(points);
    [row; 3]
}

/// 0〜255 の各値を曲線に通した値。
pub fn curve_row(points: &[[u8; 2]]) -> [u8; 256] {
    if !is_valid_curve(points) {
        return std::array::from_fn(|v| v as u8);
    }
    let xs: Vec<f64> = points.iter().map(|p| f64::from(p[0])).collect();
    let ys: Vec<f64> = points.iter().map(|p| f64::from(p[1])).collect();
    let tangents = monotone_tangents(&xs, &ys);
    std::array::from_fn(|v| {
        let x = v as f64;
        let i = xs.windows(2).position(|w| x <= w[1]).unwrap_or(xs.len() - 2);
        let (x0, x1, y0, y1) = (xs[i], xs[i + 1], ys[i], ys[i + 1]);
        let h = x1 - x0;
        let t = (x - x0) / h;
        let (t2, t3) = (t * t, t * t * t);
        // エルミート補間
        let y = (2.0 * t3 - 3.0 * t2 + 1.0) * y0
            + (t3 - 2.0 * t2 + t) * h * tangents[i]
            + (-2.0 * t3 + 3.0 * t2) * y1
            + (t3 - t2) * h * tangents[i + 1];
        y.round().clamp(0.0, 255.0) as u8
    })
}

/// 単調な 3 次補間の各点の傾き（Fritsch–Carlson。点のあいだで曲線が行き過ぎて波打たない）。
fn monotone_tangents(xs: &[f64], ys: &[f64]) -> Vec<f64> {
    let n = xs.len();
    let slopes: Vec<f64> = (0..n - 1).map(|i| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i])).collect();
    let mut m = vec![0.0; n];
    m[0] = slopes[0];
    m[n - 1] = slopes[n - 2];
    for i in 1..n - 1 {
        m[i] = if slopes[i - 1] * slopes[i] <= 0.0 { 0.0 } else { (slopes[i - 1] + slopes[i]) / 2.0 };
    }
    for i in 0..n - 1 {
        if slopes[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let (a, b) = (m[i] / slopes[i], m[i + 1] / slopes[i]);
        let length = a.hypot(b);
        if length > 3.0 {
            let scale = 3.0 / length;
            m[i] = scale * a * slopes[i];
            m[i + 1] = scale * b * slopes[i];
        }
    }
    m
}

/// 色ごとの調整の 1 色分。色相 -30〜30（度）、彩度・明るさ -100〜100。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HslAdjust {
    pub hue: i32,
    pub saturation: i32,
    pub lightness: i32,
}

impl HslAdjust {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }
}

/// 色ごとの調整の色の数と、それぞれの色相の中心（度）。並びは赤・オレンジ・黄・緑・水色・青・紫・マゼンタ。
pub const HSL_BANDS: usize = 8;
pub const HSL_CENTERS: [f64; HSL_BANDS] = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0];
/// 色相をずらす上限（度）。
pub const HSL_HUE_MAX: i32 = 30;

/// 色ごとの調整をかける（すべて 0 なら何もしない）。アルファはそのまま。
pub fn apply_hsl(image: &mut RgbaImage, bands: &[HslAdjust; HSL_BANDS]) {
    if bands.iter().all(HslAdjust::is_neutral) {
        return;
    }
    let bands = bands.map(|b| HslAdjust {
        hue: b.hue.clamp(-HSL_HUE_MAX, HSL_HUE_MAX),
        saturation: b.saturation.clamp(-100, 100),
        lightness: b.lightness.clamp(-100, 100),
    });
    image.as_mut().par_chunks_exact_mut(4).with_min_len(PIXELS_PER_TASK).for_each(|p| {
        let (h, s, l) = rgb_to_hsl(p[0], p[1], p[2]);
        if s <= 0.0 {
            return;
        }
        let (mut hue, mut saturation, mut lightness) = (0.0, 0.0, 0.0);
        for (band, weight) in band_weights(h) {
            hue += weight * f64::from(bands[band].hue);
            saturation += weight * f64::from(bands[band].saturation) / 100.0;
            lightness += weight * f64::from(bands[band].lightness) / 100.0;
        }
        let new_h = (h + hue).rem_euclid(360.0);
        let new_s = (s * (1.0 + saturation)).clamp(0.0, 1.0);
        // 明るさは彩度の高い画素ほど強く効かせる（灰色に近い画素はほとんど変えない）
        let amount = lightness * s;
        let new_l = if amount >= 0.0 { l + (1.0 - l) * amount } else { l + l * amount };
        let [r, g, b] = hsl_to_rgb(new_h, new_s, new_l.clamp(0.0, 1.0));
        p[0] = r;
        p[1] = g;
        p[2] = b;
    });
}

/// 色相 h（度）が、となりあう 2 色のどちらにどれだけ近いか（(色の番号, 重み) を 2 つ。重みの合計は 1）。
pub(crate) fn band_weights(h: f64) -> [(usize, f64); 2] {
    for (i, &start) in HSL_CENTERS.iter().enumerate() {
        let next = (i + 1) % HSL_BANDS;
        let mut end = HSL_CENTERS[next];
        if end <= start {
            end += 360.0;
        }
        let mut x = h;
        if x < start {
            x += 360.0;
        }
        if x >= start && x < end {
            let t = (x - start) / (end - start);
            return [(i, 1.0 - t), (next, t)];
        }
    }
    [(0, 1.0), (1, 0.0)]
}

/// RGB（0〜255）を、色相（度）・彩度・明るさ（0〜1）にする。
pub(crate) fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s, l)
}

/// 色相（度）・彩度・明るさ（0〜1）を RGB（0〜255）にする。
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn straight_curve_changes_nothing() {
        let row = curve_row(&identity_curve());
        assert!(row.iter().enumerate().all(|(v, &y)| y as usize == v));
        // 正しくない点（x が増えていない・端がない）はまっすぐ
        assert!(!is_valid_curve(&[[0, 0], [100, 50], [100, 60], [255, 255]]));
        assert!(!is_valid_curve(&[[10, 0], [255, 255]]));
        assert_eq!(curve_row(&[[10, 0], [255, 255]])[128], 128);
    }

    #[test]
    fn curve_passes_through_the_points_and_stays_monotone() {
        // 中間を持ち上げる（S 字ではなく明るくする）曲線
        let points = [[0, 0], [64, 100], [192, 230], [255, 255]];
        let row = curve_row(&points);
        for p in points {
            assert_eq!(row[p[0] as usize], p[1], "{p:?}");
        }
        assert!(row.windows(2).all(|w| w[0] <= w[1]));
        assert!(row[128] > 128);
        // 平らな部分では行き過ぎない（波打たない）
        let flat = curve_row(&[[0, 0], [100, 120], [150, 120], [255, 255]]);
        assert!((100..=150).all(|x| flat[x] == 120), "{:?}", &flat[100..=150]);
    }

    #[test]
    fn hsl_changes_only_the_chosen_color() {
        let colors = [
            Rgba([220, 40, 40, 255]),   // 赤
            Rgba([40, 40, 220, 255]),   // 青（色相 240°）
            Rgba([128, 128, 128, 255]), // 灰色
            Rgba([255, 255, 255, 200]), // 白（半透明）
        ];
        let original = RgbaImage::from_fn(4, 1, |x, _| colors[x as usize]);
        let mut bands = [HslAdjust::default(); HSL_BANDS];
        bands[5] = HslAdjust { hue: 0, saturation: -100, lightness: 0 }; // 青を白黒に
        let mut image = original.clone();
        apply_hsl(&mut image, &bands);
        assert_eq!(image.get_pixel(0, 0), original.get_pixel(0, 0)); // 赤はそのまま
        let blue = image.get_pixel(1, 0);
        assert!(blue[0].abs_diff(blue[2]) <= 2, "{blue:?}"); // 青は灰色に
        assert_eq!(image.get_pixel(2, 0), original.get_pixel(2, 0)); // 灰色・白は変わらない
        assert_eq!(image.get_pixel(3, 0), original.get_pixel(3, 0));
        // すべて 0 なら何もしない
        let mut untouched = original.clone();
        apply_hsl(&mut untouched, &[HslAdjust::default(); HSL_BANDS]);
        assert_eq!(untouched, original);
    }

    #[test]
    fn hsl_hue_and_lightness_and_soft_borders() {
        let mut bands = [HslAdjust::default(); HSL_BANDS];
        bands[3] = HslAdjust { hue: 30, saturation: 0, lightness: -50 }; // 緑を水色寄りに・暗く
        let mut image = RgbaImage::from_pixel(1, 1, Rgba([60, 200, 60, 255]));
        apply_hsl(&mut image, &bands);
        let (h, _, l) =
            rgb_to_hsl(image.get_pixel(0, 0)[0], image.get_pixel(0, 0)[1], image.get_pixel(0, 0)[2]);
        assert!((h - 150.0).abs() < 3.0, "{h}");
        assert!(l < rgb_to_hsl(60, 200, 60).2);
        // 黄と緑のあいだ（90°）は、どちらにも半分ずつ
        let weights = band_weights(90.0);
        assert_eq!(weights, [(2, 0.5), (3, 0.5)]);
        // マゼンタと赤のあいだ（330°）も
        assert_eq!(band_weights(330.0), [(7, 0.5), (0, 0.5)]);
    }

    #[test]
    fn hsl_round_trip() {
        for c in [[220u8, 40, 40], [40, 80, 220], [10, 200, 120], [250, 250, 10], [90, 10, 140]] {
            let (h, s, l) = rgb_to_hsl(c[0], c[1], c[2]);
            let back = hsl_to_rgb(h, s, l);
            assert!(back.iter().zip(c).all(|(a, b)| a.abs_diff(b) <= 1), "{c:?} {back:?}");
        }
    }
}
