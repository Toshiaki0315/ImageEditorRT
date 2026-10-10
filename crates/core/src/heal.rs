//! スポット修復（旧版にはない）: 範囲に内接する楕円の中を、まわりの画素からなめらかに埋めて消す
//! （センサーのゴミ・ほくろ・小さな汚れなど）。
//!
//! 埋め方は押し引き（push-pull）: 楕円の外の画素だけで半分の大きさの画像を作ることを、穴がなくなるまで
//! 繰り返し、粗い画像から細かい画像へ戻りながら穴を埋める。まわりの色が穴の中へなだらかにつながる。
//! 埋めたままでは平らで目立つので、まわりの細かいざらつき（ノイズの強さ）を同じくらい足す。

use image::{Rgba, RgbaImage};

use crate::transform::{clamp_crop, CropRect};

/// 範囲のまわりに、埋める色を取る幅（範囲の長辺に対する割合）。
const MARGIN_RATIO: f64 = 0.5;
/// ざらつきを測る・足すときの、乱数の種。
const NOISE_SEED: u32 = 0x9E37_79B9;

/// image の rect（画像の座標）に内接する楕円の中を、まわりから埋める。画像の外にはみ出す部分は使わない。
pub fn heal(image: &mut RgbaImage, rect: CropRect) {
    if clamp_crop(rect, image.dimensions()).is_none() || rect.width < 2 || rect.height < 2 {
        return;
    }
    let margin = ((rect.width.max(rect.height) as f64 * MARGIN_RATIO).ceil() as i64).max(3);
    let outer =
        CropRect::new(rect.x - margin, rect.y - margin, rect.width + margin * 2, rect.height + margin * 2);
    let Some(window) = clamp_crop(outer, image.dimensions()) else { return };
    let (w, h) = (window.width as usize, window.height as usize);
    // 楕円の中心と半径（画像の座標）
    let (cx, cy) = (rect.x as f64 + rect.width as f64 / 2.0, rect.y as f64 + rect.height as f64 / 2.0);
    let (rx, ry) = (rect.width as f64 / 2.0, rect.height as f64 / 2.0);
    let inside = |x: usize, y: usize| {
        let dx = (window.x as f64 + x as f64 + 0.5 - cx) / rx;
        let dy = (window.y as f64 + y as f64 + 0.5 - cy) / ry;
        dx * dx + dy * dy <= 1.0
    };
    let mut colors = Vec::with_capacity(w * h);
    let mut known = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let p = image.get_pixel(window.x as u32 + x as u32, window.y as u32 + y as u32);
            colors.push([0, 1, 2, 3].map(|c| f32::from(p[c])));
            known.push(!inside(x, y));
        }
    }
    if known.iter().all(|&k| !k) || known.iter().all(|&k| k) {
        return; // まわりがない（範囲が画像全体を覆う）・埋める画素がない
    }
    let filled = fill(&colors, &known, w, h);
    let grain = surrounding_grain(&colors, &known);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if known[i] {
                continue;
            }
            let noise =
                grain * (hash_noise(window.x as u32 + x as u32, window.y as u32 + y as u32) * 2.0 - 1.0);
            let v = filled[i];
            let px = Rgba([0, 1, 2, 3].map(|c| {
                let extra = if c < 3 { noise } else { 0.0 };
                (v[c] + extra).round().clamp(0.0, 255.0) as u8
            }));
            image.put_pixel(window.x as u32 + x as u32, window.y as u32 + y as u32, px);
        }
    }
}

/// 押し引きで、known でない画素を埋めた色を返す。
fn fill(colors: &[[f32; 4]], known: &[bool], w: usize, h: usize) -> Vec<[f32; 4]> {
    if known.iter().all(|&k| k) || w <= 1 && h <= 1 {
        return colors.to_vec();
    }
    // 半分の大きさ（端数は切り上げ）: 2 × 2 の中の分かっている画素の平均
    let (hw, hh) = (w.div_ceil(2), h.div_ceil(2));
    let mut half = vec![[0.0f32; 4]; hw * hh];
    let mut half_known = vec![false; hw * hh];
    for y in 0..hh {
        for x in 0..hw {
            let mut sum = [0.0f32; 4];
            let mut count = 0.0;
            for (sx, sy) in [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x, 2 * y + 1), (2 * x + 1, 2 * y + 1)] {
                if sx < w && sy < h && known[sy * w + sx] {
                    for c in 0..4 {
                        sum[c] += colors[sy * w + sx][c];
                    }
                    count += 1.0;
                }
            }
            if count > 0.0 {
                half[y * hw + x] = sum.map(|s| s / count);
                half_known[y * hw + x] = true;
            }
        }
    }
    if half_known.iter().all(|&k| !k) {
        return colors.to_vec();
    }
    let coarse = fill(&half, &half_known, hw, hh);
    // 粗い画像を双線形に広げて、穴だけに入れる
    let sample = |fx: f32, fy: f32| -> [f32; 4] {
        let x0 = fx.floor().clamp(0.0, (hw - 1) as f32) as usize;
        let y0 = fy.floor().clamp(0.0, (hh - 1) as f32) as usize;
        let (x1, y1) = ((x0 + 1).min(hw - 1), (y0 + 1).min(hh - 1));
        let (tx, ty) = ((fx - x0 as f32).clamp(0.0, 1.0), (fy - y0 as f32).clamp(0.0, 1.0));
        std::array::from_fn(|c| {
            let top = coarse[y0 * hw + x0][c] * (1.0 - tx) + coarse[y0 * hw + x1][c] * tx;
            let bottom = coarse[y1 * hw + x0][c] * (1.0 - tx) + coarse[y1 * hw + x1][c] * tx;
            top * (1.0 - ty) + bottom * ty
        })
    };
    let mut out = colors.to_vec();
    for y in 0..h {
        for x in 0..w {
            if !known[y * w + x] {
                out[y * w + x] = sample((x as f32 + 0.5) / 2.0 - 0.5, (y as f32 + 0.5) / 2.0 - 0.5);
            }
        }
    }
    out
}

/// まわりの細かいざらつきの強さ（明るさの標準偏差の見積もり）。
fn surrounding_grain(colors: &[[f32; 4]], known: &[bool]) -> f32 {
    // 横に隣り合う分かっている画素どうしの差で見積もる
    let mut sum = 0.0f64;
    let mut count = 0.0f64;
    for i in 1..colors.len() {
        if known[i] && known[i - 1] {
            let luma = |p: &[f32; 4]| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
            let d = f64::from(luma(&colors[i]) - luma(&colors[i - 1]));
            sum += d * d;
            count += 1.0;
        }
    }
    if count == 0.0 {
        return 0.0;
    }
    // 隣どうしの差の分散は、ざらつきの分散の 2 倍。大きな模様の差まで足さないよう、上限を付ける
    ((sum / count / 2.0).sqrt() as f32).min(12.0)
}

/// 画素の位置から決まる 0〜1 の乱数（プレビューと保存で同じ模様にはならないが、強さは同じ）。
fn hash_noise(x: u32, y: u32) -> f32 {
    let mut v = x.wrapping_mul(0x85EB_CA6B) ^ y.wrapping_mul(0xC2B2_AE35) ^ NOISE_SEED;
    v ^= v >> 16;
    v = v.wrapping_mul(0x7FEB_352D);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846C_A68B);
    v ^= v >> 16;
    v as f32 / u32::MAX as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// なめらかな明るさの変化に、黒い点を置いた写真。
    fn spotted() -> RgbaImage {
        let mut image =
            RgbaImage::from_fn(80, 60, |x, y| Rgba([100 + x as u8, 120 + (y / 2) as u8, 140, 255]));
        for y in 26..34 {
            for x in 36..44 {
                image.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        image
    }

    #[test]
    fn spot_is_filled_from_the_surroundings() {
        let mut image = spotted();
        heal(&mut image, CropRect::new(33, 23, 14, 14));
        let p = image.get_pixel(40, 30).0;
        // まわりの色（R は x に、G は y に沿って変わる）に近い
        assert!(p[0].abs_diff(140) <= 6 && p[1].abs_diff(135) <= 6 && p[2].abs_diff(140) <= 6, "{p:?}");
        // 範囲の外はそのまま
        assert_eq!(image.get_pixel(5, 5).0, spotted().get_pixel(5, 5).0);
        assert_eq!(image.get_pixel(33, 23).0, spotted().get_pixel(33, 23).0, "楕円の外の角もそのまま");
    }

    #[test]
    fn edges_of_the_image_and_tiny_ranges() {
        // 画像の端にかかる範囲でも、内側の分かっている画素で埋める
        let mut image = spotted();
        for y in 0..6 {
            for x in 0..6 {
                image.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        heal(&mut image, CropRect::new(-4, -4, 12, 12));
        let p = image.get_pixel(1, 1).0;
        assert!(p[0] > 80 && p[1] > 100, "{p:?}");
        // 小さすぎる・画像の外の範囲では何もしない
        let mut same = spotted();
        heal(&mut same, CropRect::new(10, 10, 1, 1));
        heal(&mut same, CropRect::new(200, 200, 10, 10));
        assert_eq!(same, spotted());
    }

    #[test]
    fn grain_follows_the_surroundings() {
        // ざらついた写真では、埋めた所にも同じくらいのざらつきを足す
        let mut noisy = RgbaImage::from_fn(60, 60, |x, y| {
            let n = (hash_noise(x * 7, y * 3) * 30.0) as u8;
            Rgba([110 + n, 110 + n, 110 + n, 255])
        });
        heal(&mut noisy, CropRect::new(20, 20, 20, 20));
        let inside: Vec<f32> = (25..35)
            .flat_map(|y| (25..35).map(move |x| (x, y)))
            .map(|(x, y)| f32::from(noisy.get_pixel(x, y)[0]))
            .collect();
        let mean = inside.iter().sum::<f32>() / inside.len() as f32;
        let std = (inside.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / inside.len() as f32).sqrt();
        assert!(std > 3.0, "ざらつきがある: {std}");
        assert!((mean - 125.0).abs() < 6.0, "{mean}");
    }
}
