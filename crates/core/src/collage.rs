//! 並べて 1 枚に（コラージュ。旧版にはない）: 2〜4 枚の写真を決まった配置で 1 枚の画像にまとめる。
//!
//! 写真はそれぞれの枠に合わせて、中央を枠の比で切り抜いてから縮める（Lanczos）。枠と枠のあいだ・まわりには
//! すき間を空け、背景の色で塗る。

use image::{imageops, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

/// 並べ方。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollageLayout {
    /// 横に 2 枚
    Row2,
    /// 縦に 2 枚
    Column2,
    /// 左に大きく 1 枚、右に縦に 2 枚
    BigLeft,
    /// 横に 3 枚
    Row3,
    /// 2 × 2
    Grid4,
}

impl CollageLayout {
    /// すべての並べ方（画面の選択肢の順）。
    pub const ALL: [CollageLayout; 5] = [Self::Row2, Self::Column2, Self::BigLeft, Self::Row3, Self::Grid4];

    /// 画面に出す名前。
    pub fn label(self) -> &'static str {
        match self {
            Self::Row2 => "横に 2 枚",
            Self::Column2 => "縦に 2 枚",
            Self::BigLeft => "左に大きく 1 枚＋右に 2 枚",
            Self::Row3 => "横に 3 枚",
            Self::Grid4 => "2 × 2（4 枚）",
        }
    }

    /// 並べる写真の枚数。
    pub fn count(self) -> usize {
        match self {
            Self::Row2 | Self::Column2 => 2,
            Self::BigLeft | Self::Row3 => 3,
            Self::Grid4 => 4,
        }
    }
}

/// 並べるときの設定。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollageOptions {
    pub layout: CollageLayout,
    /// できあがりの縦横比（幅, 高さ）
    pub aspect: (u32, u32),
    /// できあがりの長辺（px）
    pub long_side: u32,
    /// すき間（短辺に対する %、0〜5）
    pub gap: f64,
    /// 背景（すき間）の色
    pub background: [u8; 3],
}

/// 並べた画像の名前（ウィンドウのタイトル・ステータスバーに出す）。
pub const COLLAGE_NAME: &str = "並べた画像";

/// 縦横比の選択肢（幅, 高さ）。
pub const ASPECTS: [(u32, u32); 5] = [(1, 1), (4, 5), (3, 2), (16, 9), (9, 16)];
/// 長辺の上限・下限（px）。
pub const LONG_SIDE_MIN: u32 = 200;
pub const LONG_SIDE_MAX: u32 = 8000;
/// すき間の上限（%）。
pub const GAP_MAX: f64 = 5.0;

/// 枠（左上の x, y と幅・高さ、px）。
pub type Cell = (u32, u32, u32, u32);

/// できあがりの大きさ（幅, 高さ）。
pub fn canvas_size(options: &CollageOptions) -> (u32, u32) {
    let long = options.long_side.clamp(LONG_SIDE_MIN, LONG_SIDE_MAX);
    let (w, h) = (options.aspect.0.max(1), options.aspect.1.max(1));
    if w >= h {
        (long, ((f64::from(long) * f64::from(h) / f64::from(w)).round() as u32).max(1))
    } else {
        (((f64::from(long) * f64::from(w) / f64::from(h)).round() as u32).max(1), long)
    }
}

/// 並べ方ごとの枠。size はできあがりの大きさ、gap はすき間（px。まわりにも同じだけ空ける）。
pub fn cells(layout: CollageLayout, (width, height): (u32, u32), gap: u32) -> Vec<Cell> {
    // 0〜length を、すき間を挟んで n 個に分けた区間（端数は前の区間から順に 1px ずつ足す）
    let split = |length: u32, n: u32| -> Vec<(u32, u32)> {
        let inner = length.saturating_sub(gap * (n + 1));
        let mut start = gap;
        (0..n)
            .map(|i| {
                let size = inner / n + u32::from(i < inner % n);
                let span = (start, size.max(1));
                start += size + gap;
                span
            })
            .collect()
    };
    let cell = |(x, w): (u32, u32), (y, h): (u32, u32)| (x, y, w, h);
    match layout {
        CollageLayout::Row2 => split(width, 2).into_iter().map(|c| cell(c, split(height, 1)[0])).collect(),
        CollageLayout::Row3 => split(width, 3).into_iter().map(|c| cell(c, split(height, 1)[0])).collect(),
        CollageLayout::Column2 => split(height, 2).into_iter().map(|r| cell(split(width, 1)[0], r)).collect(),
        CollageLayout::BigLeft => {
            let columns = split(width, 2);
            let rows = split(height, 2);
            vec![cell(columns[0], split(height, 1)[0]), cell(columns[1], rows[0]), cell(columns[1], rows[1])]
        }
        CollageLayout::Grid4 => {
            let (columns, rows) = (split(width, 2), split(height, 2));
            rows.iter().flat_map(|&r| columns.iter().map(move |&c| cell(c, r))).collect()
        }
    }
}

/// 写真を枠の大きさ（width × height）に合わせる: 中央を枠の比で切り抜いてから縮める（大きくもする）。
pub fn cover(image: &RgbaImage, (width, height): (u32, u32)) -> RgbaImage {
    let (iw, ih) = image.dimensions();
    let scale = (f64::from(width) / f64::from(iw)).max(f64::from(height) / f64::from(ih));
    let (cw, ch) = (
        ((f64::from(width) / scale).round() as u32).clamp(1, iw),
        ((f64::from(height) / scale).round() as u32).clamp(1, ih),
    );
    let cropped = imageops::crop_imm(image, (iw - cw) / 2, (ih - ch) / 2, cw, ch).to_image();
    crate::transform::resize_to(&cropped, (width, height))
}

/// 写真を並べた 1 枚の画像を作る。写真が枠より少なければ空いた枠は背景のまま、多ければ余りは使わない。
pub fn make_collage(images: &[RgbaImage], options: &CollageOptions) -> RgbaImage {
    let size = canvas_size(options);
    let gap = (f64::from(size.0.min(size.1)) * options.gap.clamp(0.0, GAP_MAX) / 100.0).round() as u32;
    let [r, g, b] = options.background;
    let mut canvas = RgbaImage::from_pixel(size.0, size.1, Rgba([r, g, b, 255]));
    for (image, (x, y, w, h)) in images.iter().zip(cells(options.layout, size, gap)) {
        let fitted = cover(image, (w, h));
        // 透明な部分は背景の色の上に重ねる
        imageops::overlay(&mut canvas, &fitted, i64::from(x), i64::from(y));
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(layout: CollageLayout, aspect: (u32, u32), gap: f64) -> CollageOptions {
        CollageOptions { layout, aspect, long_side: 1000, gap, background: [255, 255, 255] }
    }

    #[test]
    fn canvas_follows_the_aspect() {
        assert_eq!(canvas_size(&options(CollageLayout::Row2, (3, 2), 0.0)), (1000, 667));
        assert_eq!(canvas_size(&options(CollageLayout::Row2, (4, 5), 0.0)), (800, 1000));
        assert_eq!(canvas_size(&options(CollageLayout::Row2, (1, 1), 0.0)), (1000, 1000));
    }

    #[test]
    fn cells_fill_the_canvas_with_gaps() {
        // 横に 2 枚・すき間 10: 10 | 485 | 10 | 485 | 10
        assert_eq!(
            cells(CollageLayout::Row2, (1000, 500), 10),
            vec![(10, 10, 485, 480), (505, 10, 485, 480)]
        );
        // 縦に 2 枚
        assert_eq!(cells(CollageLayout::Column2, (400, 1000), 0), vec![(0, 0, 400, 500), (0, 500, 400, 500)]);
        // 左に大きく: 左は全体の高さ、右は上下 2 つ
        assert_eq!(
            cells(CollageLayout::BigLeft, (1000, 1000), 10),
            vec![(10, 10, 485, 980), (505, 10, 485, 485), (505, 505, 485, 485)]
        );
        // 2 × 2 は左上・右上・左下・右下の順
        let grid = cells(CollageLayout::Grid4, (100, 100), 0);
        assert_eq!(grid, vec![(0, 0, 50, 50), (50, 0, 50, 50), (0, 50, 50, 50), (50, 50, 50, 50)]);
        // 割り切れない端数は前の枠に足し、右端までちょうど埋まる
        let row3 = cells(CollageLayout::Row3, (100, 50), 0);
        assert_eq!(row3.iter().map(|c| c.2).collect::<Vec<_>>(), vec![34, 33, 33]);
        assert_eq!(row3[2].0 + row3[2].2, 100);
        for layout in CollageLayout::ALL {
            assert_eq!(cells(layout, (300, 200), 4).len(), layout.count(), "{layout:?}");
        }
    }

    #[test]
    fn cover_crops_the_center() {
        // 左右が赤、中央が青の横長の写真を、正方形の枠に合わせると中央の青だけが残る
        let image = RgbaImage::from_fn(300, 100, |x, _| {
            if (100..200).contains(&x) {
                Rgba([0, 0, 255, 255])
            } else {
                Rgba([255, 0, 0, 255])
            }
        });
        let out = cover(&image, (50, 50));
        assert_eq!(out.dimensions(), (50, 50));
        assert!(out.pixels().all(|p| p[2] > 200 && p[0] < 50));
    }

    #[test]
    fn collage_places_each_photo_in_its_cell() {
        let red = RgbaImage::from_pixel(40, 30, Rgba([255, 0, 0, 255]));
        let blue = RgbaImage::from_pixel(30, 40, Rgba([0, 0, 255, 255]));
        let out = make_collage(
            &[red, blue],
            &CollageOptions { gap: 2.0, ..options(CollageLayout::Row2, (2, 1), 2.0) },
        );
        assert_eq!(out.dimensions(), (1000, 500));
        // すき間は 500 の 2% = 10px。まわりは背景（白）
        assert_eq!(out.get_pixel(5, 250).0, [255, 255, 255, 255]);
        assert_eq!(out.get_pixel(250, 250).0, [255, 0, 0, 255]);
        assert_eq!(out.get_pixel(750, 250).0, [0, 0, 255, 255]);
        assert_eq!(out.get_pixel(500, 250).0, [255, 255, 255, 255]); // 真ん中のすき間
                                                                     // 写真が足りなければ空いた枠は背景のまま
        let one = make_collage(
            &[RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 255]))],
            &options(CollageLayout::Row2, (2, 1), 0.0),
        );
        assert_eq!(one.get_pixel(750, 250).0, [255, 255, 255, 255]);
    }
}
