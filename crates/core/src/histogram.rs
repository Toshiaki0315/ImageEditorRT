//! ヒストグラム（R・G・B・輝度の分布）の計算（Python 版の core/histogram.py と同じ数え方）。

use image::{GrayImage, RgbaImage};
use serde::Serialize;

use crate::pillow::luma;
use crate::transform::CropRect;

/// 段階の数。
pub const BINS: usize = 256;

/// R・G・B と輝度のヒストグラム（それぞれ 256 段階の画素数）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Histogram {
    pub red: Vec<u32>,
    pub green: Vec<u32>,
    pub blue: Vec<u32>,
    pub luma: Vec<u32>,
}

impl Histogram {
    /// (R, G, B, 輝度) の順。
    pub fn channels(&self) -> [&[u32]; 4] {
        [&self.red, &self.green, &self.blue, &self.luma]
    }

    /// 数えた画素の数。
    pub fn total(&self) -> u64 {
        self.luma.iter().map(|&c| u64::from(c)).sum()
    }

    /// 高さの基準にする値: すべてのチャンネルの、両端（0・255）を除いた最大の画素数（最小 1）。
    ///
    /// 白飛び・黒つぶれで両端だけ極端に多いとほかがつぶれて見えないので、両端は基準に入れず、
    /// グラフの上端で切る。
    pub fn peak(&self) -> u32 {
        self.channels().iter().flat_map(|c| c[1..BINS - 1].iter().copied()).max().unwrap_or(0).max(1)
    }

    /// 画面に渡すバイト列（R・G・B・輝度の順に 256 個ずつの u32 リトルエンディアン）。
    pub fn to_le_bytes(&self) -> Vec<u8> {
        self.channels().iter().flat_map(|c| c.iter().flat_map(|v| v.to_le_bytes())).collect()
    }
}

/// image の area（なければ全体）のヒストグラム。透明な画素（アルファ 0）は数えない。
///
/// mask（area と同じ大きさ）を渡すと、その範囲だけを数える。Pillow と同じく、アルファと
/// mask を掛けた値（ImageChops.multiply、切り捨て）が 0 でない画素を数える。
/// 輝度は ITU-R 601（Pillow の L 変換）で求める。
pub fn compute_histogram(image: &RgbaImage, area: Option<CropRect>, mask: Option<&GrayImage>) -> Histogram {
    let area = area.unwrap_or(CropRect::whole(image.dimensions()));
    // 画像の外にはみ出した分は数えない
    let (width, height) = (i64::from(image.width()), i64::from(image.height()));
    let (left, top) = (area.x.clamp(0, width), area.y.clamp(0, height));
    let (right, bottom) =
        ((area.x + area.width).clamp(left, width), (area.y + area.height).clamp(top, height));
    let mut counts = [[0u32; BINS]; 4];
    for y in top..bottom {
        for x in left..right {
            let p = image.get_pixel(x as u32, y as u32).0;
            let weight = match mask {
                Some(m) => {
                    u32::from(p[3]) * u32::from(m.get_pixel((x - area.x) as u32, (y - area.y) as u32)[0])
                        / 255
                }
                None => u32::from(p[3]),
            };
            if weight == 0 {
                continue;
            }
            counts[0][usize::from(p[0])] += 1;
            counts[1][usize::from(p[1])] += 1;
            counts[2][usize::from(p[2])] += 1;
            counts[3][usize::from(luma(p[0], p[1], p[2]))] += 1;
        }
    }
    let [red, green, blue, luma] = counts.map(|c| c.to_vec());
    Histogram { red, green, blue, luma }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgba};

    #[test]
    fn counts_each_channel_and_luma() {
        let mut image = RgbaImage::from_pixel(4, 2, Rgba([10, 20, 30, 255]));
        image.put_pixel(0, 0, Rgba([255, 0, 128, 255]));
        let h = compute_histogram(&image, None, None);
        assert_eq!(h.total(), 8);
        assert_eq!((h.red[10], h.red[255]), (7, 1));
        assert_eq!((h.green[20], h.green[0]), (7, 1));
        assert_eq!((h.blue[30], h.blue[128]), (7, 1));
        assert_eq!(h.luma[usize::from(luma(10, 20, 30))], 7);
        assert_eq!(h.luma[usize::from(luma(255, 0, 128))], 1);
    }

    #[test]
    fn skips_transparent_pixels_and_masked_out_area() {
        let mut image = RgbaImage::from_pixel(3, 1, Rgba([50, 50, 50, 255]));
        image.put_pixel(0, 0, Rgba([50, 50, 50, 0]));
        image.put_pixel(1, 0, Rgba([50, 50, 50, 1]));
        assert_eq!(compute_histogram(&image, None, None).total(), 2);
        // アルファ 1 × マスク 254 は 254 / 255 で 0 に切り捨てられる（Pillow の multiply と同じ）
        let mut mask = GrayImage::from_pixel(3, 1, Luma([255]));
        mask.put_pixel(1, 0, Luma([254]));
        mask.put_pixel(2, 0, Luma([0]));
        assert_eq!(compute_histogram(&image, None, Some(&mask)).total(), 0);
    }

    #[test]
    fn counts_only_the_area() {
        let mut image = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 255]));
        for y in 2..5 {
            for x in 3..7 {
                image.put_pixel(x, y, Rgba([200, 100, 50, 255]));
            }
        }
        let area = CropRect { x: 3, y: 2, width: 4, height: 3 };
        let h = compute_histogram(&image, Some(area), None);
        assert_eq!((h.total(), h.red[200], h.red[0]), (12, 12, 0));
    }

    #[test]
    fn peak_ignores_both_ends() {
        let mut image = RgbaImage::from_pixel(10, 1, Rgba([255, 255, 255, 255]));
        image.put_pixel(0, 0, Rgba([7, 7, 7, 255]));
        image.put_pixel(1, 0, Rgba([7, 9, 7, 255]));
        let h = compute_histogram(&image, None, None);
        assert_eq!(h.red[255], 8);
        assert_eq!(h.peak(), 2); // 両端（0・255）の 8 は数えない
        let flat = compute_histogram(&RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255])), None, None);
        assert_eq!(flat.peak(), 1);
        assert_eq!(h.to_le_bytes().len(), 4 * BINS * 4);
    }
}
