//! 遠近（台形）の補正（旧版にはない）。見上げて撮った建物のすぼまった縦の線（横の線）をまっすぐにする。
//!
//! 出力の長方形に、元の画像の中の台形（片側の辺を内側へ寄せた四角）を射影変換で写す。台形は画像の内側に取るので、
//! 余白は出ず、大きさも変わらない（はみ出す部分を切り取ったのと同じになる）。

use image::{Rgba, RgbaImage};
use rayon::prelude::*;

/// スライダーの範囲（-100〜100）。
pub const PERSPECTIVE_MAX: i32 = 100;
/// いちばん強く直すときに、辺を内側へ寄せる量（その向きの長さに対する比率、片側）。
const MAX_INSET: f64 = 0.25;

/// 遠近の補正をかける。vertical が正なら上をすぼめた台形を（見上げた建物の上が細くなったのを）広げ、負なら下を。
/// horizontal が正なら左を、負なら右をすぼめた台形を広げる。どちらも 0 なら複製を返す。
pub fn correct(image: &RgbaImage, vertical: i32, horizontal: i32) -> RgbaImage {
    if vertical == 0 && horizontal == 0 {
        return image.clone();
    }
    let (width, height) = (f64::from(image.width()), f64::from(image.height()));
    let quad = source_quad((width, height), vertical, horizontal);
    let map = Homography::square_to_quad(quad);
    let mut out = RgbaImage::new(image.width(), image.height());
    let row_len = image.width() as usize * 4;
    out.as_mut().par_chunks_exact_mut(row_len).enumerate().for_each(|(y, row)| {
        let v = (y as f64 + 0.5) / height;
        for (x, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let u = (x as f64 + 0.5) / width;
            let (sx, sy) = map.apply(u, v);
            *pixel = sample(image, sx - 0.5, sy - 0.5).0;
        }
    });
    out
}

/// 元の画像の中の台形の四隅（左上・右上・右下・左下。画素の端の座標）。
fn source_quad((width, height): (f64, f64), vertical: i32, horizontal: i32) -> [(f64, f64); 4] {
    let amount = |v: i32| {
        f64::from(v.clamp(-PERSPECTIVE_MAX, PERSPECTIVE_MAX)) / f64::from(PERSPECTIVE_MAX) * MAX_INSET
    };
    let mut quad = [(0.0, 0.0), (width, 0.0), (width, height), (0.0, height)];
    let k = amount(vertical).abs() * width;
    if vertical > 0 {
        quad[0].0 += k;
        quad[1].0 -= k;
    } else if vertical < 0 {
        quad[3].0 += k;
        quad[2].0 -= k;
    }
    let m = amount(horizontal).abs() * height;
    if horizontal > 0 {
        quad[0].1 += m;
        quad[3].1 -= m;
    } else if horizontal < 0 {
        quad[1].1 += m;
        quad[2].1 -= m;
    }
    quad
}

/// 単位正方形 (u, v) を四角形に写す射影変換（Heckbert の square-to-quad）。
struct Homography {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
    g: f64,
    h: f64,
}

impl Homography {
    /// (0,0)→p0、(1,0)→p1、(1,1)→p2、(0,1)→p3 に写す変換。
    fn square_to_quad([p0, p1, p2, p3]: [(f64, f64); 4]) -> Self {
        let (dx1, dy1) = (p1.0 - p2.0, p1.1 - p2.1);
        let (dx2, dy2) = (p3.0 - p2.0, p3.1 - p2.1);
        let (dx3, dy3) = (p0.0 - p1.0 + p2.0 - p3.0, p0.1 - p1.1 + p2.1 - p3.1);
        let (g, h) = if dx3.abs() < 1e-12 && dy3.abs() < 1e-12 {
            (0.0, 0.0)
        } else {
            let det = dx1 * dy2 - dx2 * dy1;
            ((dx3 * dy2 - dx2 * dy3) / det, (dx1 * dy3 - dx3 * dy1) / det)
        };
        Self {
            a: p1.0 - p0.0 + g * p1.0,
            b: p3.0 - p0.0 + h * p3.0,
            c: p0.0,
            d: p1.1 - p0.1 + g * p1.1,
            e: p3.1 - p0.1 + h * p3.1,
            f: p0.1,
            g,
            h,
        }
    }

    fn apply(&self, u: f64, v: f64) -> (f64, f64) {
        let w = self.g * u + self.h * v + 1.0;
        ((self.a * u + self.b * v + self.c) / w, (self.d * u + self.e * v + self.f) / w)
    }
}

/// (x, y) の色をまわりの 4 画素から求める（不透明度で重み付けし、透明な画素の色を混ぜない。外は端の画素）。
fn sample(image: &RgbaImage, x: f64, y: f64) -> Rgba<u8> {
    let (w, h) = (i64::from(image.width()), i64::from(image.height()));
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let mut sum = [0.0f64; 4];
    for (dx, dy, weight) in
        [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)]
    {
        let px = (x0 as i64 + dx).clamp(0, w - 1);
        let py = (y0 as i64 + dy).clamp(0, h - 1);
        let p = image.get_pixel(px as u32, py as u32);
        let alpha = f64::from(p[3]) * weight;
        for c in 0..3 {
            sum[c] += f64::from(p[c]) * alpha;
        }
        sum[3] += alpha;
    }
    if sum[3] <= 0.0 {
        return Rgba([0, 0, 0, 0]);
    }
    let color = |c: usize| (sum[c] / sum[3]).round().clamp(0.0, 255.0) as u8;
    Rgba([color(0), color(1), color(2), sum[3].round().clamp(0.0, 255.0) as u8])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 白地に、下で x = 20・80、上で x = 26・74 にすぼまる黒い 2 本の線（見上げた建物の縦の線）。
    /// 上を片側 10px 寄せた台形の辺と同じ点（x = 50、y = -400）に向かってすぼまる。
    fn converging() -> RgbaImage {
        RgbaImage::from_fn(100, 100, |x, y| {
            let t = f64::from(y) / 99.0; // 上 0 → 下 1
            let left = 26.0 - 6.0 * t;
            let right = 74.0 + 6.0 * t;
            let x = f64::from(x);
            if (x - left).abs() < 1.5 || (x - right).abs() < 1.5 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        })
    }

    /// 行 y で、黒い画素の x の平均（左半分・右半分）。
    fn lines_at(image: &RgbaImage, y: u32) -> (f64, f64) {
        let dark = |range: std::ops::Range<u32>| {
            let xs: Vec<f64> = range.filter(|&x| image.get_pixel(x, y)[0] < 128).map(f64::from).collect();
            xs.iter().sum::<f64>() / xs.len() as f64
        };
        (dark(0..50), dark(50..100))
    }

    #[test]
    fn zero_changes_nothing() {
        let image = converging();
        assert_eq!(correct(&image, 0, 0), image);
    }

    #[test]
    fn converging_verticals_become_parallel() {
        let image = converging();
        let (top, bottom) = (lines_at(&image, 2), lines_at(&image, 97));
        assert!((top.0 - bottom.0).abs() > 4.0, "直す前はすぼまっている");
        // 上を片側 10px（幅の 10%）寄せた台形を広げる: 10% = 25% × 40
        let fixed = correct(&image, 40, 0);
        let (top, bottom) = (lines_at(&fixed, 2), lines_at(&fixed, 97));
        assert!((top.0 - bottom.0).abs() < 1.5 && (top.1 - bottom.1).abs() < 1.5, "{top:?} / {bottom:?}");
        assert_eq!(fixed.dimensions(), (100, 100));
        // 余白（透明）は出ない
        assert!(fixed.pixels().all(|p| p[3] == 255));
    }

    #[test]
    fn signs_choose_the_edge() {
        let quad = source_quad((100.0, 80.0), -100, 0);
        assert_eq!(quad, [(0.0, 0.0), (100.0, 0.0), (75.0, 80.0), (25.0, 80.0)], "負なら下をすぼめた台形");
        let quad = source_quad((100.0, 80.0), 0, 100);
        assert_eq!(quad, [(0.0, 20.0), (100.0, 0.0), (100.0, 80.0), (0.0, 60.0)], "横は左の辺");
        // 単位正方形の四隅は四角形の四隅へ
        let map = Homography::square_to_quad(quad);
        let (x, y) = map.apply(1.0, 1.0);
        assert!((x - 100.0).abs() < 1e-9 && (y - 80.0).abs() < 1e-9);
    }
}
