//! 自動補正（旧版にはない）: 写真の色の分布から、露出・コントラスト・色温度のちょうどよい値を求める。
//!
//! 求めた値は「加工」タブのスライダーに入れるだけなので、そのあと手で調整できる。値は元の写真（色の調整を
//! かける前）に対して求め、今のスライダーの値は置き換える。

use image::RgbaImage;
use serde::Serialize;

use crate::adjust::{self, Lut};

/// 露出を合わせる、明るさ（輝度）の平均の目標（0〜255。sRGB の中間の明るさ）。
const TARGET_MEAN: f64 = 118.0;
/// 露出の範囲と刻み（EV）。
const EXPOSURE_MAX: f64 = 3.0;
const EXPOSURE_STEP: f64 = 0.1;
/// コントラストの上限と刻み。
const CONTRAST_MAX: i32 = 40;
const CONTRAST_STEP: i32 = 5;
/// 暗い端・明るい端とみなす割合（それぞれ 1%）と、広げる目標の幅（0〜255 のうち）。
const TAIL: f64 = 0.01;
const TARGET_SPREAD: f64 = 235.0;
/// コントラストで増やしてよい、白飛び・黒つぶれの画素の割合。
const CLIP_ALLOWANCE: f64 = 0.005;
/// 色温度の範囲と刻み（K）。
const TEMPERATURE_MIN: u32 = 2000;
const TEMPERATURE_MAX: u32 = 10000;
const TEMPERATURE_STEP: u32 = 100;
/// 色かぶりをどれだけ打ち消すか（全部打ち消すと、夕焼けや草原の色まで消えるので控えめに）。
const TEMPERATURE_AMOUNT: f64 = 0.7;
/// 赤と青の平均の差がこの割合より小さければ、色かぶりはないとする。
const CAST_TOLERANCE: f64 = 0.03;
/// 色かぶりを調べる、灰色に近い画素（いちばん強い色に対する、強い色と弱い色の差の割合がこれ以下）。
/// 赤い絨毯・青空・草など、もともと色のついたものに引っぱられないため。
const NEUTRAL_SATURATION: f64 = 0.32;
/// 灰色に近い画素の明るさの範囲（暗すぎ・明るすぎは色が当てにならない）。
const NEUTRAL_LUMA: std::ops::RangeInclusive<u8> = 40..=235;
/// 灰色に近い画素がこの割合より少なければ、色かぶりは分からないとして色温度は変えない。
const NEUTRAL_MIN_FRACTION: f64 = 0.02;
/// 露出で、明るい端（上位 1%）をこれより明るくしない・暗い端（下位 1%）をこれより暗くしない。
const HIGHLIGHT_LIMIT: u8 = 250;
const SHADOW_LIMIT: u8 = 5;

/// 自動補正で求めた値（「加工」タブのスライダーの値）。
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct AutoAdjust {
    /// 露出（-3〜+3 EV、0.1 刻み）
    pub exposure: f64,
    /// コントラスト（0〜40、5 刻み）
    pub contrast: i32,
    /// 色温度（K、100 刻み。6500 = 変化なし）
    pub temperature: u32,
}

impl Default for AutoAdjust {
    fn default() -> Self {
        Self { exposure: 0.0, contrast: 0, temperature: adjust::TEMPERATURE_NEUTRAL }
    }
}

/// 画像（透明な画素は数えない）から、露出 → コントラスト → 色温度の順に値を求める。
pub fn auto_adjust(image: &RgbaImage) -> AutoAdjust {
    let mut luma = [0u64; 256];
    // 灰色に近い画素の赤・青の合計と数
    let (mut red, mut blue, mut neutral) = (0u64, 0u64, 0u64);
    for p in image.pixels().filter(|p| p[3] > 0) {
        let y = crate::pillow::luma(p[0], p[1], p[2]);
        luma[y as usize] += 1;
        let (high, low) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2]));
        if NEUTRAL_LUMA.contains(&y) && f64::from(high - low) <= NEUTRAL_SATURATION * f64::from(high) {
            red += u64::from(p[0]);
            blue += u64::from(p[2]);
            neutral += 1;
        }
    }
    let total: u64 = luma.iter().sum();
    if total == 0 {
        return AutoAdjust::default();
    }
    let exposure = find_exposure(&luma, total);
    let after_exposure = adjust::exposure_lut(exposure);
    let contrast = find_contrast(&luma, total, &after_exposure);
    let temperature = if (neutral as f64) < NEUTRAL_MIN_FRACTION * total as f64 {
        adjust::TEMPERATURE_NEUTRAL
    } else {
        find_temperature(red as f64 / neutral as f64, blue as f64 / neutral as f64)
    };
    AutoAdjust { exposure, contrast, temperature }
}

/// 輝度の分布を表に通したときの平均。
fn mean_after(histogram: &[u64; 256], total: u64, row: &[u8; 256]) -> f64 {
    histogram.iter().enumerate().map(|(v, &n)| f64::from(row[v]) * n as f64).sum::<f64>() / total as f64
}

/// 輝度の平均が目標にいちばん近くなる露出（-3〜+3 EV、0.1 刻み。同じなら 0 に近いほう）。
/// ただし、明るい端（上位 1%）が白飛びするほど上げず、暗い端（下位 1%）が黒つぶれするほど下げない
/// （黒い服・白い壁が多いだけの、きちんと写った写真を変えないため）。
fn find_exposure(luma: &[u64; 256], total: u64) -> f64 {
    let steps = (EXPOSURE_MAX / EXPOSURE_STEP).round() as i32;
    let (low, high) = (percentile(luma, total, TAIL), percentile(luma, total, 1.0 - TAIL));
    let allowed = |ev: f64| {
        let row = adjust::exposure_lut(ev)[1];
        (ev <= 0.0 || row[high] <= HIGHLIGHT_LIMIT.max(high as u8))
            && (ev >= 0.0 || row[low] >= SHADOW_LIMIT.min(low as u8))
    };
    let candidates = (-steps..=steps).map(|i| f64::from(i) * EXPOSURE_STEP).filter(|&ev| allowed(ev));
    let error = |ev: f64| (mean_after(luma, total, &adjust::exposure_lut(ev)[1]) - TARGET_MEAN).abs();
    let best = candidates.min_by(|&a, &b| error(a).total_cmp(&error(b)).then(a.abs().total_cmp(&b.abs())));
    (best.unwrap_or(0.0) * 10.0).round() / 10.0
}

/// 輝度の分布の、下から fraction の位置の値。
fn percentile(histogram: &[u64; 256], total: u64, fraction: f64) -> usize {
    let target = (total as f64 * fraction).ceil() as u64;
    let mut sum = 0;
    histogram
        .iter()
        .position(|&n| {
            sum += n;
            sum >= target.max(1)
        })
        .unwrap_or(255)
}

/// 白飛び・黒つぶれ（0・255 に張りつく）画素の割合。
fn clipped(histogram: &[u64; 256], total: u64, row: &[u8; 256]) -> f64 {
    histogram.iter().enumerate().filter(|(v, _)| row[*v] <= 1 || row[*v] >= 254).map(|(_, &n)| n).sum::<u64>()
        as f64
        / total as f64
}

/// 露出をかけた後の、暗い端（1%）と明るい端（99%）の幅が目標に届くまでのコントラスト（0〜40、5 刻み）。
/// 白飛び・黒つぶれを 0.5% より増やす値は使わない。
fn find_contrast(luma: &[u64; 256], total: u64, exposure: &Lut) -> i32 {
    let (low, high) = (percentile(luma, total, TAIL), percentile(luma, total, 1.0 - TAIL));
    let base = clipped(luma, total, &exposure[1]);
    let mut best = 0;
    for amount in (CONTRAST_STEP..=CONTRAST_MAX).step_by(CONTRAST_STEP as usize) {
        let row = adjust::compose(exposure, &adjust::contrast_lut(amount))[1];
        let spread = f64::from(row[high]) - f64::from(row[low]);
        if spread > TARGET_SPREAD || clipped(luma, total, &row) > base + CLIP_ALLOWANCE {
            break;
        }
        best = amount;
    }
    best
}

/// 灰色に近い部分の、赤と青の平均（red・blue）がそろう色温度を求め、その 7 割だけ打ち消す値（100 K 刻み）。
/// かぶりが小さければ 6500 K。
fn find_temperature(red: f64, blue: f64) -> u32 {
    if red <= 0.0 || blue <= 0.0 || ((red - blue) / red.max(blue)).abs() < CAST_TOLERANCE {
        return adjust::TEMPERATURE_NEUTRAL;
    }
    let balance = |kelvin: u32| {
        let m = adjust::temperature_multipliers(kelvin);
        (red * m[0] - blue * m[2]).abs()
    };
    let full = (TEMPERATURE_MIN..=TEMPERATURE_MAX)
        .step_by(TEMPERATURE_STEP as usize)
        .min_by(|&a, &b| balance(a).total_cmp(&balance(b)))
        .unwrap_or(adjust::TEMPERATURE_NEUTRAL);
    let neutral = f64::from(adjust::TEMPERATURE_NEUTRAL);
    let kelvin = neutral + (f64::from(full) - neutral) * TEMPERATURE_AMOUNT;
    ((kelvin / f64::from(TEMPERATURE_STEP)).round() as u32 * TEMPERATURE_STEP)
        .clamp(TEMPERATURE_MIN, TEMPERATURE_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// 左から右へ明るくなる灰色のグラデーション（low〜high）に、色の倍率 tint をかけた画像。
    fn gradient(low: f64, high: f64, tint: [f64; 3]) -> RgbaImage {
        RgbaImage::from_fn(256, 64, |x, _| {
            let v = low + (high - low) * f64::from(x) / 255.0;
            Rgba(
                [0, 1, 2]
                    .map(|c| (v * tint[c]).round().clamp(0.0, 255.0) as u8)
                    .into_iter()
                    .chain([255])
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
            )
        })
    }

    #[test]
    fn balanced_photo_is_left_almost_alone() {
        let result = auto_adjust(&gradient(0.0, 255.0, [1.0, 1.0, 1.0]));
        assert!(result.exposure.abs() <= 0.3, "{result:?}");
        assert_eq!((result.contrast, result.temperature), (0, 6500), "{result:?}");
    }

    #[test]
    fn dark_photo_gets_brighter() {
        let result = auto_adjust(&gradient(5.0, 90.0, [1.0, 1.0, 1.0]));
        assert!(result.exposure >= 0.8, "{result:?}");
        // 露出をかけた後の平均は目標に近い
        let after = adjust::exposure_lut(result.exposure);
        let image = gradient(5.0, 90.0, [1.0, 1.0, 1.0]);
        let mean = image.pixels().map(|p| f64::from(after[1][p[1] as usize])).sum::<f64>()
            / f64::from(image.width() * image.height());
        assert!((mean - TARGET_MEAN).abs() < 12.0, "{mean}");
    }

    #[test]
    fn flat_photo_gets_more_contrast() {
        let result = auto_adjust(&gradient(90.0, 160.0, [1.0, 1.0, 1.0]));
        assert!(result.contrast >= 20, "{result:?}");
        // 端まで広がっている写真には、コントラストを足さない
        assert_eq!(auto_adjust(&gradient(0.0, 255.0, [1.0, 1.0, 1.0])).contrast, 0);
    }

    #[test]
    fn blue_cast_is_warmed_and_orange_cast_is_cooled() {
        let blue = auto_adjust(&gradient(30.0, 200.0, [0.85, 0.95, 1.15]));
        assert!(blue.temperature != 6500, "{blue:?}");
        let warm = auto_adjust(&gradient(30.0, 200.0, [1.15, 0.95, 0.8]));
        assert!(warm.temperature != 6500, "{warm:?}");
        // 青かぶりと赤かぶりでは、逆の向きに直す
        assert!((f64::from(blue.temperature) - 6500.0) * (f64::from(warm.temperature) - 6500.0) < 0.0);
        // 直した後は、赤と青の差が小さくなる
        let gap = |tint: [f64; 3], kelvin: u32| {
            let m = adjust::temperature_multipliers(kelvin);
            (tint[0] * m[0] - tint[2] * m[2]).abs()
        };
        assert!(gap([0.85, 0.95, 1.15], blue.temperature) < gap([0.85, 0.95, 1.15], 6500));
        assert!(gap([1.15, 0.95, 0.8], warm.temperature) < gap([1.15, 0.95, 0.8], 6500));
    }

    #[test]
    fn transparent_image_changes_nothing() {
        let image = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 255, 0]));
        assert_eq!(auto_adjust(&image), AutoAdjust::default());
    }
    #[test]
    fn a_well_exposed_photo_with_dark_clothes_is_not_brightened() {
        // 8 割が黒い服（20）、1 割が白いシャツ（245）、1 割が赤い絨毯。平均は暗いが、明るい端はもう白に近い
        let image = RgbaImage::from_fn(100, 100, |x, _| match x {
            0..=79 => Rgba([20, 20, 22, 255]),
            80..=89 => Rgba([245, 245, 242, 255]),
            _ => Rgba([200, 30, 35, 255]),
        });
        let result = auto_adjust(&image);
        assert!(result.exposure <= 0.1, "{result:?}");
        // 赤い絨毯は色かぶりとみなさない（灰色に近い服・シャツはかぶっていない）
        assert_eq!(result.temperature, 6500, "{result:?}");
    }
}
