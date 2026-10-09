//! 透かしを写真全体に繰り返して敷く（文字・ロゴの位置「全体に繰り返す（斜め）」。旧版にはない）。
//!
//! 文字・ロゴを透明な板の中央に描いて切り出し（スタンプ）、斜めに回してから、レンガ積みのようにずらしながら
//! 写真全体に重ねる。大きさ・不透明度・飾りは、ほかの位置に置くときと同じ設定を使う。

use image::{imageops, Rgba, RgbaImage};

use crate::logo::{self, LogoSettings};
use crate::text::{self, TextPosition, TextSettings};

/// スタンプを傾ける角度（度。左下から右上へ上がる向き）。
pub const TILE_ANGLE: f64 = 30.0;
/// スタンプどうしの間隔（写真の短辺に対する比率）。
const GAP_RATIO: f64 = 0.06;

/// 文字を写真全体に繰り返して描く。
pub fn draw_tiled_text(image: &mut RgbaImage, settings: &TextSettings) {
    if settings.is_empty() {
        return;
    }
    let mut canvas = RgbaImage::new(image.width(), image.height());
    let centered = TextSettings { position: TextPosition::Center, ..settings.clone() };
    text::draw_text(&mut canvas, &centered, None, None);
    if let Some(stamp) = cut_stamp(&canvas) {
        tile(image, &stamp);
    }
}

/// ロゴを写真全体に繰り返して描く。
pub fn draw_tiled_logo(image: &mut RgbaImage, settings: &LogoSettings) {
    if settings.is_empty() {
        return;
    }
    let mut canvas = RgbaImage::new(image.width(), image.height());
    let centered = LogoSettings { position: TextPosition::Center, ..settings.clone() };
    logo::draw_logo(&mut canvas, &centered, None, None);
    if let Some(stamp) = cut_stamp(&canvas) {
        tile(image, &stamp);
    }
}

/// 透明な板から、描いた部分（透明でない画素）を囲む範囲を切り出す。何も描いていなければ None。
fn cut_stamp(canvas: &RgbaImage) -> Option<RgbaImage> {
    let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in canvas.enumerate_pixels() {
        if p[3] > 0 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    (left <= right)
        .then(|| imageops::crop_imm(canvas, left, top, right - left + 1, bottom - top + 1).to_image())
}

/// スタンプを TILE_ANGLE だけ傾け、写真全体に重ねる（端で切れるものも描く）。
///
/// 文字の向きに沿って「スタンプの幅＋すき間 2 つ分」ごとに並べ、文字の向きと直角に「高さ＋すき間 2 つ分」ごとに
/// 行を重ねる。行ごとに文字の向きへ半分ずらす（レンガ積み）。
fn tile(image: &mut RgbaImage, stamp: &RgbaImage) {
    let angle = TILE_ANGLE.to_radians();
    let rotated = rotate(stamp, angle);
    let (width, height) = (f64::from(image.width()), f64::from(image.height()));
    let gap = (width.min(height) * GAP_RATIO).round().max(4.0);
    // 文字の向き（右上へ上がる）と、それに直角な向き（画面の y は下向き）
    let along = (angle.cos(), -angle.sin());
    let across = (angle.sin(), angle.cos());
    let step = f64::from(stamp.width()) + gap * 2.0;
    let line = f64::from(stamp.height()) + gap * 2.0;
    // 写真の中心から、対角線の半分＋スタンプ 1 つ分まで敷けば四隅まで届く
    let reach = (width.hypot(height) / 2.0) + f64::from(rotated.width().max(rotated.height()));
    let columns = (reach / step).ceil() as i64 + 1;
    let rows = (reach / line).ceil() as i64 + 1;
    let (half_w, half_h) = (f64::from(rotated.width()) / 2.0, f64::from(rotated.height()) / 2.0);
    for row in -rows..=rows {
        let shift = if row.rem_euclid(2) == 1 { step / 2.0 } else { 0.0 };
        for column in -columns..=columns {
            let a = column as f64 * step + shift;
            let b = row as f64 * line;
            let cx = width / 2.0 + along.0 * a + across.0 * b;
            let cy = height / 2.0 + along.1 * a + across.1 * b;
            if cx < -half_w || cy < -half_h || cx > width + half_w || cy > height + half_h {
                continue;
            }
            overlay(image, &rotated, (cx - half_w).round() as i64, (cy - half_h).round() as i64);
        }
    }
}

/// stamp を image の (x, y) に「上に重ねる」（文字を描くときと同じ合成。はみ出す部分は描かない）。
fn overlay(image: &mut RgbaImage, stamp: &RgbaImage, x: i64, y: i64) {
    let (width, height) = (i64::from(image.width()), i64::from(image.height()));
    for (sx, sy, p) in stamp.enumerate_pixels() {
        let (px, py) = (x + i64::from(sx), y + i64::from(sy));
        if p[3] == 0 || px < 0 || py < 0 || px >= width || py >= height {
            continue;
        }
        let color = [p[0], p[1], p[2]].map(f32::from);
        text::source_over(image.get_pixel_mut(px as u32, py as u32), color, f32::from(p[3]) / 255.0);
    }
}

/// 画像を angle（ラジアン。左回り）だけ回し、はみ出さない大きさの透明な板に描く（バイリニア、透過を考えて混ぜる）。
pub fn rotate(image: &RgbaImage, angle: f64) -> RgbaImage {
    let (w, h) = (f64::from(image.width()), f64::from(image.height()));
    let (sin, cos) = angle.sin_cos();
    let out_w = (w * cos.abs() + h * sin.abs()).ceil() as u32;
    let out_h = (w * sin.abs() + h * cos.abs()).ceil() as u32;
    let (cx, cy) = (w / 2.0, h / 2.0);
    let (ox, oy) = (f64::from(out_w) / 2.0, f64::from(out_h) / 2.0);
    RgbaImage::from_fn(out_w, out_h, |x, y| {
        // 出力の画素の中心を、元の画像の座標に戻す（画面の y は下向きなので、左回りは逆の回転で戻す）
        let (dx, dy) = (f64::from(x) + 0.5 - ox, f64::from(y) + 0.5 - oy);
        let sx = cos * dx - sin * dy + cx - 0.5;
        let sy = sin * dx + cos * dy + cy - 0.5;
        sample(image, sx, sy)
    })
}

/// (x, y) の色をまわりの 4 画素から求める（色は不透明度で重み付けし、透明な画素の色を混ぜない）。
fn sample(image: &RgbaImage, x: f64, y: f64) -> Rgba<u8> {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let mut sum = [0.0f64; 4];
    for (dx, dy, weight) in
        [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)]
    {
        let (px, py) = (x0 as i64 + dx, y0 as i64 + dy);
        if weight <= 0.0
            || px < 0
            || py < 0
            || px >= i64::from(image.width())
            || py >= i64::from(image.height())
        {
            continue;
        }
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

    fn white_text() -> TextSettings {
        TextSettings {
            text: "© SAMPLE".into(),
            size: 5.0,
            opacity: 100,
            color: [255, 255, 255],
            position: TextPosition::Tiled,
            ..TextSettings::default()
        }
    }

    /// 範囲の中に白い（文字の）画素があるか。
    fn has_text(image: &RgbaImage, x: std::ops::Range<u32>, y: std::ops::Range<u32>) -> bool {
        x.flat_map(|x| y.clone().map(move |y| (x, y))).any(|(x, y)| image.get_pixel(x, y)[0] > 200)
    }

    #[test]
    fn text_is_repeated_over_the_whole_photo() {
        let mut image = RgbaImage::from_pixel(600, 400, Rgba([20, 40, 60, 255]));
        draw_tiled_text(&mut image, &white_text());
        // 四隅の近くと中央に、どれも文字がある
        for (x, y) in [(0, 0), (500, 0), (0, 300), (500, 300), (250, 150)] {
            assert!(has_text(&image, x..x + 100, y..y + 100), "({x}, {y}) の近くに文字がない");
        }
        // 敷き詰めても、文字は全体の一部だけ（塗りつぶさない）
        let lit = image.pixels().filter(|p| p[0] > 200).count();
        assert!(lit > 2000 && lit < 600 * 400 / 3, "{lit}");
        assert!(image.pixels().all(|p| p[3] == 255));
    }

    #[test]
    fn empty_text_or_logo_draws_nothing() {
        let mut image = RgbaImage::from_pixel(100, 80, Rgba([1, 2, 3, 255]));
        draw_tiled_text(&mut image, &TextSettings { text: " ".into(), ..white_text() });
        draw_tiled_logo(
            &mut image,
            &LogoSettings { position: TextPosition::Tiled, ..LogoSettings::default() },
        );
        assert!(image.pixels().all(|p| p.0 == [1, 2, 3, 255]));
    }

    #[test]
    fn rotation_keeps_the_whole_stamp() {
        let stamp = RgbaImage::from_pixel(40, 10, Rgba([255, 0, 0, 255]));
        let rotated = rotate(&stamp, TILE_ANGLE.to_radians());
        // 40×10 を 30° 回すと 40cos30 + 10sin30 ≒ 39.6 → 40、40sin30 + 10cos30 ≒ 28.7 → 29
        assert_eq!(rotated.dimensions(), (40, 29));
        // 中央は赤、隅は透明。透明なところの色を混ぜないので、縁でも赤のまま
        assert_eq!(*rotated.get_pixel(20, 14), Rgba([255, 0, 0, 255]));
        assert_eq!(rotated.get_pixel(0, 0)[3], 0);
        assert!(rotated.pixels().filter(|p| p[3] > 0).all(|p| p[0] == 255 && p[1] == 0));
    }
}
