//! 写真の形（矩形・角丸・円）の切り抜き。旧版の core/shapes.py を移したもの（縁のアンチエイリアスも同じ）。

use image::{imageops, GrayImage, Luma, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::diorama::composite;

/// 角丸の半径（短辺に対する %）の範囲と既定値。50 で両端が半円（カプセル形）。
pub const CORNER_RADIUS_MIN: u32 = 0;
pub const CORNER_RADIUS_MAX: u32 = 50;
pub const CORNER_RADIUS_DEFAULT: u32 = 10;

/// 写真の形。JSON では旧版と同じ名前。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeType {
    #[default]
    Rectangle,
    Rounded,
    Circle,
}

impl ShapeType {
    /// すべての形（画面のプルダウンの順）。
    pub const ALL: [ShapeType; 3] = [Self::Rectangle, Self::Rounded, Self::Circle];

    /// 画面に出す名前。
    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "矩形",
            Self::Rounded => "角丸",
            Self::Circle => "円",
        }
    }
}

/// 形に合わせて写真を切り抜くときの縦横比（円は正方形）。切り抜かないなら None。
pub fn shape_aspect(shape: ShapeType) -> Option<(f64, f64)> {
    (shape == ShapeType::Circle).then_some((1.0, 1.0))
}

/// 形の内側を 255、外側を 0 とするマスク（縁はアンチエイリアス）。矩形・半径 0 の角丸なら None。
///
/// - 角丸: 角の半径 = 短辺 × corner_radius%（0〜50）
/// - 円: 中央の、短辺を直径とする正円
pub fn shape_mask((width, height): (u32, u32), shape: ShapeType, corner_radius: u32) -> Option<GrayImage> {
    let short = width.min(height);
    match shape {
        ShapeType::Circle => {
            let circle = rounded_rect_mask((short, short), f64::from(short) / 2.0);
            let mut mask = GrayImage::new(width, height);
            imageops::replace(
                &mut mask,
                &circle,
                i64::from((width - short) / 2),
                i64::from((height - short) / 2),
            );
            Some(mask)
        }
        ShapeType::Rounded => {
            let percent = corner_radius.clamp(CORNER_RADIUS_MIN, CORNER_RADIUS_MAX);
            let radius = f64::from(short) * f64::from(percent) / 100.0;
            (radius > 0.0).then(|| rounded_rect_mask((width, height), radius))
        }
        ShapeType::Rectangle => None,
    }
}

/// 画像を形で切り抜いた新しい画像を返す（矩形なら複製）。
///
/// fill が None なら形の外側を透明にする（元の透過も残す）。fill を指定すると形の外側を
/// その色で不透明に塗る（フレームと組み合わせるとき。写真の透過は残す）。
pub fn apply_shape(
    image: &RgbaImage,
    shape: ShapeType,
    corner_radius: u32,
    fill: Option<[u8; 3]>,
) -> RgbaImage {
    let Some(mask) = shape_mask(image.dimensions(), shape, corner_radius) else { return image.clone() };
    let mut out = image.clone();
    for (p, m) in out.pixels_mut().zip(mask.pixels()) {
        let m = m[0];
        match fill {
            // Image.paste(image, mask) で、塗った背景の上に写真を重ねる（アルファも混ぜる）
            Some([r, g, b]) => {
                for (v, background) in p.0.iter_mut().zip([r, g, b, 255]) {
                    *v = composite(*v, background, m);
                }
            }
            // ImageChops.multiply（切り捨て）でアルファにマスクを掛ける
            None => p[3] = (u32::from(p[3]) * u32::from(m) / 255) as u8,
        }
    }
    out
}

/// 角を半径 radius の円弧にした長方形のマスク（radius は短辺の半分まで）。
/// アンチエイリアスが必要なのは角だけなので、角の部分だけを計算して四隅に反転して貼る。
fn rounded_rect_mask((width, height): (u32, u32), radius: f64) -> GrayImage {
    let radius = radius.min(f64::from(width.min(height)) / 2.0);
    let corner = corner_mask(radius);
    let n = i64::from(corner.width());
    let mut mask = GrayImage::from_pixel(width, height, Luma([255]));
    let (w, h) = (i64::from(width), i64::from(height));
    imageops::replace(&mut mask, &corner, 0, 0);
    imageops::replace(&mut mask, &imageops::flip_horizontal(&corner), w - n, 0);
    imageops::replace(&mut mask, &imageops::flip_vertical(&corner), 0, h - n);
    imageops::replace(&mut mask, &imageops::rotate180(&corner), w - n, h - n);
    mask
}

/// 左上の角（ceil(radius) 四方）のマスク。円弧の中心は (radius, radius)。
///
/// 旧版と同じく、画素の中心から円弧までの距離を (r² − d²) / 2r で近似し、0.5 を足して 1px 幅で
/// なめらかに 0〜255 にする（ImageMath の float32 の計算と、F → L の切り捨てに合わせる）。
fn corner_mask(radius: f64) -> GrayImage {
    let n = (radius.ceil() as u32).max(1);
    // 円弧の中心までの横方向の距離の 2 乗（中心より右の画素は 0 = まっすぐな辺）。float32 で持つ
    let offsets: Vec<f32> =
        (0..n).map(|i| ((radius - (f64::from(i) + 0.5)).max(0.0).powi(2)) as f32).collect();
    let scale = (255.0 / (2.0 * radius)) as f32;
    let offset = (127.5 + radius * radius * (255.0 / (2.0 * radius))) as f32;
    GrayImage::from_fn(n, n, |x, y| {
        let sum = offsets[x as usize] + offsets[y as usize];
        let value = (offset - sum * scale).clamp(0.0, 255.0);
        Luma([value as u8])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_and_zero_radius_have_no_mask() {
        assert!(shape_mask((10, 10), ShapeType::Rectangle, 10).is_none());
        assert!(shape_mask((10, 10), ShapeType::Rounded, 0).is_none());
        assert_eq!(shape_aspect(ShapeType::Circle), Some((1.0, 1.0)));
        assert_eq!(shape_aspect(ShapeType::Rounded), None);
    }

    #[test]
    fn circle_is_centered() {
        let mask = shape_mask((100, 40), ShapeType::Circle, 0).unwrap();
        assert_eq!(mask.get_pixel(50, 20)[0], 255);
        assert_eq!(mask.get_pixel(5, 20)[0], 0);
        assert_eq!(mask.get_pixel(95, 20)[0], 0);
    }
}
