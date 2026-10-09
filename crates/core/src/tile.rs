//! 透かしを写真全体に繰り返して敷く（文字・ロゴの位置「全体に繰り返す（斜め）」。旧版にはない）。
//!
//! 文字・ロゴを透明な板の中央に描いて切り出し（スタンプ）、斜めに回してから、レンガ積みのようにずらしながら
//! 写真全体に重ねる。大きさ・不透明度・飾りは、ほかの位置に置くときと同じ設定を使う。

use image::{imageops, Pixel, Rgba, RgbaImage};
use rayon::prelude::*;

use crate::logo::{self, LogoSettings};
use crate::text::{self, TextPosition, TextSettings};

/// スタンプを傾ける角度（度。左下から右上へ上がる向き）。
pub const TILE_ANGLE: f64 = 30.0;
/// スタンプどうしの間隔（写真の短辺に対する比率）。
const GAP_RATIO: f64 = 0.06;

/// 文字を写真全体に繰り返して描く。
pub fn draw_tiled_text(image: &mut RgbaImage, settings: &TextSettings) {
    if let Some(stamp) = text_stamp(image.dimensions(), settings) {
        tile(image, &stamp);
    }
}

/// ロゴを写真全体に繰り返して描く。
pub fn draw_tiled_logo(image: &mut RgbaImage, settings: &LogoSettings) {
    if let Some(stamp) = logo_stamp(image.dimensions(), settings) {
        tile(image, &stamp);
    }
}

/// 文字のスタンプ: 写真（size）の中央に描いたときと同じ形・大きさ・飾りで、文字の入る板に描いて切り出す。
fn text_stamp((width, height): (u32, u32), settings: &TextSettings) -> Option<RgbaImage> {
    if settings.is_empty() {
        return None;
    }
    let short = f64::from(width.min(height));
    let size = f64::from(text::text_size_px(settings, short));
    let lines = settings.text.replace("\r\n", "\n").split('\n').count() as f64;
    // 行の高さ（文字の大きさの 1.3 倍ほど）と飾り（文字の大きさの 2 割ほど）が十分入る高さ
    let needed = (lines + 1.0) * size * 2.0 + short * text::TEXT_MARGIN_RATIO * 2.0;
    let mut canvas = RgbaImage::new(width, canvas_height(height, needed));
    let centered = TextSettings { position: TextPosition::Center, ..settings.clone() };
    text::draw_text(&mut canvas, &centered, None, Some(short));
    cut_stamp(&canvas)
}

/// ロゴのスタンプ（文字と同じく、ロゴの入る板に描いて切り出す）。
fn logo_stamp((width, height): (u32, u32), settings: &LogoSettings) -> Option<RgbaImage> {
    if settings.is_empty() {
        return None;
    }
    let short = f64::from(width.min(height));
    let wanted = short * f64::from(settings.size.clamp(logo::LOGO_SIZE_MIN, logo::LOGO_SIZE_MAX)) / 100.0;
    let needed = wanted + short * text::TEXT_MARGIN_RATIO * 2.0 + 4.0;
    let mut canvas = RgbaImage::new(width, canvas_height(height, needed));
    let centered = LogoSettings { position: TextPosition::Center, ..settings.clone() };
    logo::draw_logo(&mut canvas, &centered, None, Some(short));
    cut_stamp(&canvas)
}

/// スタンプを描く板の高さ: needed 以上で写真の高さ以下。写真の高さとの差を偶数にして、中央に置いたときの位置の
/// 端数（文字の形のなめらかさに効く）が写真の上に描くときと同じになるようにする。
fn canvas_height(height: u32, needed: f64) -> u32 {
    let needed = needed.ceil() as u32;
    if needed >= height {
        return height;
    }
    let spare = height - needed;
    height - (spare - spare % 2)
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
    let mut places = Vec::new();
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
            places.push(((cx - half_w).round() as i64, (cy - half_h).round() as i64));
        }
    }
    overlay_all(image, &rotated, &places);
}

/// stamp を image の places（左上）すべてに、並んでいる順に「上に重ねる」（文字を描くときと同じ合成。
/// はみ出す部分は描かない）。行ごとに並列に描く（どの画素も、重ねる順は places の順のまま）。
fn overlay_all(image: &mut RgbaImage, stamp: &RgbaImage, places: &[(i64, i64)]) {
    let width = image.width() as usize;
    let (stamp_width, stamp_height) = (i64::from(stamp.width()), i64::from(stamp.height()));
    image.as_mut().par_chunks_exact_mut(width * 4).enumerate().for_each(|(y, row)| {
        let y = y as i64;
        for &(left, top) in places.iter().filter(|&&(_, top)| (top..top + stamp_height).contains(&y)) {
            let from = left.max(0);
            let to = (left + stamp_width).min(width as i64);
            for x in from..to {
                let p = stamp.get_pixel((x - left) as u32, (y - top) as u32);
                if p[3] == 0 {
                    continue;
                }
                let at = x as usize * 4;
                let pixel = Rgba::from_slice_mut(&mut row[at..at + 4]);
                text::source_over(pixel, [p[0], p[1], p[2]].map(f32::from), f32::from(p[3]) / 255.0);
            }
        }
    });
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
    fn logo_is_repeated_over_the_whole_photo() {
        // 赤い四角のロゴ（まわりは透明）を書き出して、白い写真に敷き詰める
        let dir = std::env::temp_dir().join(format!("imageeditorrt-tile-logo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("logo.png");
        RgbaImage::from_fn(40, 20, |x, _| {
            if (10..30).contains(&x) {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        })
        .save(&path)
        .unwrap();
        let mut image = RgbaImage::from_pixel(600, 400, Rgba([255, 255, 255, 255]));
        let logo = LogoSettings {
            path: path.to_string_lossy().into_owned(),
            position: TextPosition::Tiled,
            size: 10.0,
            opacity: 100,
        };
        draw_tiled_logo(&mut image, &logo);
        let red = |x: u32, y: u32| {
            (x..x + 120).flat_map(|x| (y..y + 120).map(move |y| (x, y))).any(|(x, y)| {
                let p = image.get_pixel(x, y);
                p[0] > 200 && p[1] < 80
            })
        };
        for (x, y) in [(0, 0), (480, 0), (0, 280), (480, 280), (240, 140)] {
            assert!(red(x, y), "({x}, {y}) の近くにロゴがない");
        }
        // ロゴの透明な部分は写真をそのまま残す（全体を塗りつぶさない）
        let white = image.pixels().filter(|p| p.0 == [255, 255, 255, 255]).count();
        assert!(white > 600 * 400 / 2, "{white}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 速くする前のスタンプ（写真と同じ大きさの板に描いて切り出す）。
    fn full_text_stamp(size: (u32, u32), settings: &TextSettings) -> Option<RgbaImage> {
        let mut canvas = RgbaImage::new(size.0, size.1);
        text::draw_text(
            &mut canvas,
            &TextSettings { position: TextPosition::Center, ..settings.clone() },
            None,
            None,
        );
        cut_stamp(&canvas)
    }

    fn full_logo_stamp(size: (u32, u32), settings: &LogoSettings) -> Option<RgbaImage> {
        let mut canvas = RgbaImage::new(size.0, size.1);
        logo::draw_logo(
            &mut canvas,
            &LogoSettings { position: TextPosition::Center, ..settings.clone() },
            None,
            None,
        );
        cut_stamp(&canvas)
    }

    #[test]
    fn small_canvas_gives_the_same_stamp() {
        // 写真と同じ大きさの板に描いたときと同じスタンプになる
        use crate::text::{TextEffect, TextFont};
        let sizes = [(600, 400), (601, 401), (400, 900), (1234, 777), (300, 300)];
        let texts = [
            white_text(),
            TextSettings {
                text: "二行の\n透かし".into(),
                size: 12.0,
                effect: TextEffect::Shadow,
                ..white_text()
            },
            TextSettings {
                text: "Outline".into(),
                font: TextFont::GothicBold,
                size: 30.0,
                effect: TextEffect::Outline,
                ..white_text()
            },
        ];
        for size in sizes {
            for settings in &texts {
                let (a, b) = (text_stamp(size, settings).unwrap(), full_text_stamp(size, settings).unwrap());
                assert_eq!(a.dimensions(), b.dimensions(), "{size:?} {:?}", settings.text);
                // 置く位置の小数の丸めで、縁の不透明度が 1/255 違うことがある（見える色は同じ）
                for (p, q) in a.pixels().zip(b.pixels()) {
                    assert!(p[3].abs_diff(q[3]) <= 1, "{size:?} {:?}: {p:?} / {q:?}", settings.text);
                    if p[3] > 0 && q[3] > 0 {
                        assert_eq!([p[0], p[1], p[2]], [q[0], q[1], q[2]], "{size:?} {:?}", settings.text);
                    }
                }
            }
        }
        let dir = std::env::temp_dir().join(format!("imageeditorrt-tile-stamp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("logo.png");
        RgbaImage::from_fn(37, 23, |x, y| {
            Rgba([(x * 6) as u8, (y * 9) as u8, 200, if x < 30 { 255 } else { 90 }])
        })
        .save(&path)
        .unwrap();
        for size in sizes {
            for logo_size in [5.0, 25.0, 60.0] {
                let logo = LogoSettings {
                    path: path.to_string_lossy().into_owned(),
                    position: TextPosition::Tiled,
                    size: logo_size,
                    opacity: 70,
                };
                assert_eq!(logo_stamp(size, &logo), full_logo_stamp(size, &logo), "{size:?} {logo_size}");
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(canvas_height(1000, 301.2), 302);
        assert_eq!(canvas_height(1001, 301.2), 303);
        assert_eq!(canvas_height(200, 301.2), 200);
    }

    #[test]
    fn parallel_overlay_matches_one_by_one() {
        // 半透明で重なり合うスタンプを、1 つずつ順に重ねたときと同じになる（はみ出しも含む）
        let stamp =
            RgbaImage::from_fn(30, 20, |x, y| Rgba([(x * 8) as u8, (y * 12) as u8, 90, ((x + y) * 6) as u8]));
        let places = [(-10, -5), (5, 3), (20, 10), (90, 50), (15, 4), (100, 70)];
        let base = RgbaImage::from_fn(110, 80, |x, y| Rgba([(x * 2) as u8, (y * 3) as u8, 40, 255]));
        let mut one_by_one = base.clone();
        for &(left, top) in &places {
            for (sx, sy, p) in stamp.enumerate_pixels() {
                let (x, y) = (left + i64::from(sx), top + i64::from(sy));
                if p[3] == 0 || x < 0 || y < 0 || x >= 110 || y >= 80 {
                    continue;
                }
                text::source_over(
                    one_by_one.get_pixel_mut(x as u32, y as u32),
                    [p[0], p[1], p[2]].map(f32::from),
                    f32::from(p[3]) / 255.0,
                );
            }
        }
        let mut parallel = base;
        overlay_all(&mut parallel, &stamp, &places);
        assert_eq!(parallel, one_by_one);
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
