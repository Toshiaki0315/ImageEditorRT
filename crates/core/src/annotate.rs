//! 注釈（旧版にはない）: 矢印・四角・楕円・手書きの線・吹き出し（文字）を写真に描く。
//!
//! 座標・太さは、回転・反転した後の原寸の画像の座標（px）で持ち、スタンプと同じく切り抜き・リサイズの後の画像に
//! 描く（`scale`・`offset` で合わせる）ので、保存する大きさでもくっきり描ける。形は、画素ごとに線までの距離から
//! 濃さを決めて描く（縁はなめらか）。

use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::text::{self, TextPosition, TextSettings};

/// 注釈の種類。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    /// 矢印（始点 → 終点。終点に矢じり）
    #[default]
    Arrow,
    /// 四角（2 点を対角にした枠）
    Rect,
    /// 楕円（2 点を対角にした四角に内接する枠）
    Ellipse,
    /// 手書きの線（点を順につなぐ）
    Pen,
    /// 吹き出し（2 点を対角にした白い箱に文字）
    Bubble,
}

/// 注釈 1 つ。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Annotation {
    pub kind: AnnotationKind,
    /// 点（回転・反転した後の原寸の座標、px）
    pub points: Vec<[f64; 2]>,
    pub color: [u8; 3],
    /// 線の太さ（原寸の px）
    pub width: f64,
    /// 吹き出しの文字
    pub text: String,
}

impl Default for Annotation {
    fn default() -> Self {
        Self {
            kind: AnnotationKind::Arrow,
            points: Vec::new(),
            color: [230, 40, 40],
            width: 6.0,
            text: String::new(),
        }
    }
}

/// 矢じりの長さ（線の太さに対する倍率）と開き（度）。
const HEAD_PER_WIDTH: f64 = 4.0;
const HEAD_ANGLE: f64 = 28.0;
/// 吹き出しの角の丸み（箱の短辺に対する割合）。
const BUBBLE_RADIUS: f64 = 0.2;

/// 線分 a–b までの距離。
fn segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length2 = dx * dx + dy * dy;
    let t = if length2 == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length2).clamp(0.0, 1.0)
    };
    ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
}

/// 角を丸めた四角（中心 c・半分の大きさ h・丸み r）の内側を負とする距離。
fn rounded_rect_distance(p: [f64; 2], c: [f64; 2], h: [f64; 2], r: f64) -> f64 {
    let q = [(p[0] - c[0]).abs() - h[0] + r, (p[1] - c[1]).abs() - h[1] + r];
    let outside = (q[0].max(0.0).powi(2) + q[1].max(0.0).powi(2)).sqrt();
    outside + q[0].max(q[1]).min(0.0) - r
}

/// 楕円（中心 c・半径 r）の輪郭までのおおよその距離（内側を負）。
fn ellipse_distance(p: [f64; 2], c: [f64; 2], r: [f64; 2]) -> f64 {
    let (x, y) = ((p[0] - c[0]) / r[0].max(0.5), (p[1] - c[1]) / r[1].max(0.5));
    let f = x * x + y * y - 1.0;
    let gx = 2.0 * x / r[0].max(0.5);
    let gy = 2.0 * y / r[1].max(0.5);
    f / (gx * gx + gy * gy).sqrt().max(1e-9)
}

/// 画像の座標にした注釈の形（画素ごとの濃さを返す）。
enum Shape {
    /// 線分の集まり（太さ）
    Lines(Vec<([f64; 2], [f64; 2])>, f64),
    /// 輪郭（中心・大きさ・太さ）
    RectOutline([f64; 2], [f64; 2], f64),
    EllipseOutline([f64; 2], [f64; 2], f64),
}

impl Shape {
    /// 点 p の濃さ（0〜1）。
    fn coverage(&self, p: [f64; 2]) -> f64 {
        let edge = |distance: f64, half: f64| (half - distance + 0.5).clamp(0.0, 1.0);
        match self {
            Shape::Lines(lines, width) => {
                let d = lines.iter().map(|&(a, b)| segment_distance(p, a, b)).fold(f64::INFINITY, f64::min);
                edge(d, width / 2.0)
            }
            Shape::RectOutline(c, h, width) => edge(rounded_rect_distance(p, *c, *h, 0.0).abs(), width / 2.0),
            Shape::EllipseOutline(c, r, width) => edge(ellipse_distance(p, *c, *r).abs(), width / 2.0),
        }
    }
}

/// 2 点を対角にした四角の中心と半分の大きさ。
fn rect_of(a: [f64; 2], b: [f64; 2]) -> ([f64; 2], [f64; 2]) {
    ([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0], [(a[0] - b[0]).abs() / 2.0, (a[1] - b[1]).abs() / 2.0])
}

/// 矢印の線（軸と矢じりの 2 本）。
fn arrow_lines(a: [f64; 2], b: [f64; 2], width: f64) -> Vec<([f64; 2], [f64; 2])> {
    let angle = (b[1] - a[1]).atan2(b[0] - a[0]);
    let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let head = (width * HEAD_PER_WIDTH).min(length * 0.5);
    let wing = |side: f64| {
        let t = angle + std::f64::consts::PI - side * HEAD_ANGLE.to_radians();
        [b[0] + head * t.cos(), b[1] + head * t.sin()]
    };
    vec![(a, b), (b, wing(1.0)), (b, wing(-1.0))]
}

/// image の範囲 (x0, y0, x1, y1) に、濃さの関数で color を重ねる。
fn paint(
    image: &mut RgbaImage,
    bounds: (f64, f64, f64, f64),
    color: [u8; 3],
    coverage: impl Fn([f64; 2]) -> f64 + Sync,
) {
    let (w, h) = image.dimensions();
    let x0 = bounds.0.floor().max(0.0) as u32;
    let y0 = bounds.1.floor().max(0.0) as u32;
    let x1 = (bounds.2.ceil().max(0.0) as u32).min(w);
    let y1 = (bounds.3.ceil().max(0.0) as u32).min(h);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = w as usize;
    image
        .as_mut()
        .par_chunks_exact_mut(width * 4)
        .enumerate()
        .skip(y0 as usize)
        .take((y1 - y0) as usize)
        .for_each(|(y, row)| {
            for x in x0..x1 {
                let a = coverage([f64::from(x) + 0.5, y as f64 + 0.5]);
                if a <= 0.0 {
                    continue;
                }
                let p = &mut row[x as usize * 4..x as usize * 4 + 4];
                for c in 0..3 {
                    p[c] = (f64::from(p[c]) * (1.0 - a) + f64::from(color[c]) * a).round() as u8;
                }
                p[3] = (f64::from(p[3]) + (255.0 - f64::from(p[3])) * a).round() as u8;
            }
        });
}

/// 注釈を描く。原寸の座標 (x, y) は画像の (x × scale.0 − offset.0, y × scale.1 − offset.1) に当たる。
pub fn draw(image: &mut RgbaImage, annotations: &[Annotation], scale: (f64, f64), offset: (f64, f64)) {
    let unit = (scale.0 + scale.1) / 2.0;
    for note in annotations {
        let points: Vec<[f64; 2]> =
            note.points.iter().map(|p| [p[0] * scale.0 - offset.0, p[1] * scale.1 - offset.1]).collect();
        let Some(&first) = points.first() else { continue };
        let last = *points.last().unwrap_or(&first);
        let width = (note.width * unit).max(1.0);
        let margin = width * HEAD_PER_WIDTH + 2.0;
        let (min_x, min_y, max_x, max_y) = points
            .iter()
            .fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |b, p| {
                (b.0.min(p[0]), b.1.min(p[1]), b.2.max(p[0]), b.3.max(p[1]))
            });
        let bounds = (min_x - margin, min_y - margin, max_x + margin, max_y + margin);
        let shape = match note.kind {
            AnnotationKind::Arrow => Shape::Lines(arrow_lines(first, last, width), width),
            AnnotationKind::Pen => {
                let lines = if points.len() == 1 {
                    vec![(first, first)]
                } else {
                    points.windows(2).map(|w| (w[0], w[1])).collect()
                };
                Shape::Lines(lines, width)
            }
            AnnotationKind::Rect => {
                let (c, h) = rect_of(first, last);
                Shape::RectOutline(c, h, width)
            }
            AnnotationKind::Ellipse => {
                let (c, h) = rect_of(first, last);
                Shape::EllipseOutline(c, h, width)
            }
            AnnotationKind::Bubble => {
                draw_bubble(image, note, first, last, width);
                continue;
            }
        };
        paint(image, bounds, note.color, |p| shape.coverage(p));
    }
}

/// 吹き出し: 白い角丸の箱に色の枠、中に色の文字（箱に収まる大きさ）。
fn draw_bubble(image: &mut RgbaImage, note: &Annotation, a: [f64; 2], b: [f64; 2], width: f64) {
    let (c, h) = rect_of(a, b);
    if h[0] < 1.0 || h[1] < 1.0 {
        return;
    }
    let r = h[0].min(h[1]) * 2.0 * BUBBLE_RADIUS;
    let bounds = (c[0] - h[0] - width, c[1] - h[1] - width, c[0] + h[0] + width, c[1] + h[1] + width);
    paint(image, bounds, [255, 255, 255], |p| (0.5 - rounded_rect_distance(p, c, h, r)).clamp(0.0, 1.0));
    paint(image, bounds, note.color, |p| {
        (width / 2.0 - rounded_rect_distance(p, c, h, r).abs() + 0.5).clamp(0.0, 1.0)
    });
    if note.text.trim().is_empty() {
        return;
    }
    let inset = width + h[0].min(h[1]) * 0.15;
    let area = (
        (c[0] - h[0] + inset).round() as i64,
        (c[1] - h[1] + inset).round() as i64,
        (c[0] + h[0] - inset).round() as i64,
        (c[1] + h[1] - inset).round() as i64,
    );
    if area.2 <= area.0 || area.3 <= area.1 {
        return;
    }
    let settings = TextSettings {
        text: note.text.clone(),
        color: note.color,
        opacity: 100,
        position: TextPosition::Center,
        // 箱に収まるまで小さくするので、大きめに始める
        size: 60.0,
        ..TextSettings::default()
    };
    text::draw_text(image, &settings, Some(area), Some((h[0].min(h[1]) * 2.0).max(1.0)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn blank() -> RgbaImage {
        RgbaImage::from_pixel(100, 80, Rgba([255, 255, 255, 255]))
    }

    fn note(kind: AnnotationKind, points: &[[f64; 2]]) -> Annotation {
        Annotation { kind, points: points.to_vec(), color: [200, 0, 0], width: 4.0, text: String::new() }
    }

    fn is_red(image: &RgbaImage, x: u32, y: u32) -> bool {
        let p = image.get_pixel(x, y);
        p[0] > 150 && p[1] < 80
    }

    #[test]
    fn shapes_are_drawn_where_they_should_be() {
        let mut image = blank();
        draw(
            &mut image,
            &[note(AnnotationKind::Rect, &[[10.0, 10.0], [50.0, 40.0]])],
            (1.0, 1.0),
            (0.0, 0.0),
        );
        assert!(is_red(&image, 10, 25) && is_red(&image, 30, 10), "枠の上");
        assert!(!is_red(&image, 30, 25), "枠の中は塗らない");

        let mut image = blank();
        draw(
            &mut image,
            &[note(AnnotationKind::Ellipse, &[[10.0, 10.0], [90.0, 70.0]])],
            (1.0, 1.0),
            (0.0, 0.0),
        );
        assert!(is_red(&image, 50, 10) && is_red(&image, 10, 40), "輪郭の上");
        assert!(!is_red(&image, 50, 40) && !is_red(&image, 12, 12), "中心・四角の角は塗らない");

        let mut image = blank();
        draw(
            &mut image,
            &[note(AnnotationKind::Arrow, &[[10.0, 40.0], [80.0, 40.0]])],
            (1.0, 1.0),
            (0.0, 0.0),
        );
        assert!(is_red(&image, 45, 40), "軸");
        assert!(is_red(&image, 72, 36) || is_red(&image, 72, 44), "矢じり");
        assert!(!is_red(&image, 45, 60));

        let mut image = blank();
        draw(
            &mut image,
            &[note(AnnotationKind::Pen, &[[10.0, 10.0], [30.0, 30.0], [50.0, 10.0]])],
            (1.0, 1.0),
            (0.0, 0.0),
        );
        assert!(is_red(&image, 20, 20) && is_red(&image, 40, 20));
        assert!(!is_red(&image, 30, 10));
    }

    #[test]
    fn coordinates_follow_scale_and_offset() {
        // 原寸の (20, 20)〜(60, 20) の線を、半分に縮めて (10, 0) ずらした画像に描く → (0, 10)〜(20, 10)
        let mut image = blank();
        let mut line = note(AnnotationKind::Pen, &[[20.0, 20.0], [60.0, 20.0]]);
        line.width = 8.0;
        draw(&mut image, &[line], (0.5, 0.5), (10.0, 0.0));
        assert!(is_red(&image, 10, 10));
        assert!(!is_red(&image, 30, 10));
        let p = image.get_pixel(10, 13).0;
        assert!(p[1] > 200, "太さも半分（4px）: {p:?}");
    }

    #[test]
    fn bubble_is_a_white_box_with_border_and_text() {
        let mut image = RgbaImage::from_pixel(200, 120, Rgba([20, 20, 20, 255]));
        let mut bubble = note(AnnotationKind::Bubble, &[[20.0, 20.0], [180.0, 100.0]]);
        bubble.text = "ここ".into();
        draw(&mut image, &[bubble], (1.0, 1.0), (0.0, 0.0));
        assert_eq!(image.get_pixel(5, 5).0, [20, 20, 20, 255], "外はそのまま");
        assert!(is_red(&image, 100, 20), "上の枠");
        assert!(image.get_pixel(30, 60)[0] > 240 && image.get_pixel(30, 60)[1] > 240, "箱の中は白");
        let reds = (40..160)
            .flat_map(|x| (30..90).map(move |y| (x, y)))
            .filter(|&(x, y)| is_red(&image, x, y))
            .count();
        assert!(reds > 50, "文字が描かれている: {reds}");
    }

    #[test]
    fn json_round_trip_and_empty_points() {
        let a = note(AnnotationKind::Bubble, &[[1.0, 2.0], [3.0, 4.0]]);
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(serde_json::from_str::<Annotation>(&json).unwrap(), a);
        let mut image = blank();
        draw(&mut image, &[note(AnnotationKind::Arrow, &[])], (1.0, 1.0), (0.0, 0.0));
        assert_eq!(image, blank());
    }
}
