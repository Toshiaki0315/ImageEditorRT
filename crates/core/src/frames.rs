//! フレーム（ポラロイド・チェキの白い台紙）。旧版の core/frames.py を移したもの。

use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

/// フレームの色（白・不透明）。
pub const FRAME_COLOR: [u8; 3] = [255, 255, 255];

/// フレームの種類。JSON では旧版と同じ名前。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameType {
    #[default]
    None,
    Polaroid,
    InstaxMini,
}

impl FrameType {
    /// すべての種類（画面のプルダウンの順）。
    pub const ALL: [FrameType; 3] = [Self::None, Self::Polaroid, Self::InstaxMini];

    /// 画面に出す名前。
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "なし",
            Self::Polaroid => "ポラロイド",
            Self::InstaxMini => "チェキ",
        }
    }
}

/// フレームの寸法（mm、縦向きのカード）。実物のおおよその値。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSpec {
    /// 写真部分 (幅, 高さ)
    pub window: (f64, f64),
    /// 余白 (左, 上, 右, 下)
    pub margins: (f64, f64, f64, f64),
}

fn spec(frame: FrameType) -> Option<FrameSpec> {
    match frame {
        FrameType::None => None,
        // ポラロイド 600 / i-Type: カード 88×107mm、写真部分 79×79mm
        FrameType::Polaroid => Some(FrameSpec { window: (79.0, 79.0), margins: (4.5, 6.0, 4.5, 22.0) }),
        // チェキ (instax mini): カード 54×86mm、写真部分 46×62mm
        FrameType::InstaxMini => Some(FrameSpec { window: (46.0, 62.0), margins: (4.0, 6.5, 4.0, 17.5) }),
    }
}

/// 写真の向きに合わせたフレームの寸法。フレームなしなら None。
///
/// 写真部分が縦長のフレームに横長の写真を入れるときは、カードを横向きにする
/// （反時計回りに 90° 回したときと同じく、下の広い余白が右に来る）。
pub fn frame_spec(frame: FrameType, (width, height): (u32, u32)) -> Option<FrameSpec> {
    let s = spec(frame)?;
    let (window_width, window_height) = s.window;
    if width > height && window_width < window_height {
        let (left, top, right, bottom) = s.margins;
        return Some(FrameSpec {
            window: (window_height, window_width),
            margins: (top, right, bottom, left),
        });
    }
    Some(s)
}

/// size の写真を入れるときの、写真部分の縦横比 (幅, 高さ)。フレームなしなら None。
pub fn window_aspect(frame: FrameType, size: (u32, u32)) -> Option<(f64, f64)> {
    frame_spec(frame, size).map(|s| s.window)
}

/// size の写真に付ける余白 (左, 上, 右, 下) を px で返す（四捨五入、最小 1px）。フレームなしなら 0。
///
/// 余白は写真部分の大きさから実物の比率で計算する。写真の縦横比が写真部分と違うときは、
/// 写真部分に収まる側の縮尺に合わせる。
pub fn frame_margins(frame: FrameType, size: (u32, u32)) -> (u32, u32, u32, u32) {
    let Some(s) = frame_spec(frame, size) else { return (0, 0, 0, 0) };
    let scale = (f64::from(size.0) / s.window.0).min(f64::from(size.1) / s.window.1);
    let px = |mm: f64| ((mm * scale + 0.5) as u32).max(1);
    (px(s.margins.0), px(s.margins.1), px(s.margins.2), px(s.margins.3))
}

/// フレームを付けた後の大きさ。
pub fn framed_size(size: (u32, u32), frame: FrameType) -> (u32, u32) {
    let (left, top, right, bottom) = frame_margins(frame, size);
    (size.0 + left + right, size.1 + top + bottom)
}

/// 画像の周囲にフレームを付けた新しい画像を返す。フレームは白で不透明。写真の透過はそのまま残す。
pub fn add_frame(image: &RgbaImage, frame: FrameType) -> RgbaImage {
    if frame == FrameType::None {
        return image.clone();
    }
    let (left, top, _, _) = frame_margins(frame, image.dimensions());
    let (width, height) = framed_size(image.dimensions(), frame);
    let [r, g, b] = FRAME_COLOR;
    let mut framed = RgbaImage::from_pixel(width, height, Rgba([r, g, b, 255]));
    image::imageops::replace(&mut framed, image, i64::from(left), i64::from(top));
    framed
}

/// フレームの、いちばん広い余白の範囲（左, 上, 右, 下。フレームを付けた後の画像の座標）。
/// 縦向きのカードは写真の下、横向きのチェキは写真の右の余白。フレームなしなら None。
pub fn margin_box(size: (u32, u32), frame: FrameType) -> Option<(u32, u32, u32, u32)> {
    if frame == FrameType::None {
        return None;
    }
    let (left, top, right, bottom) = frame_margins(frame, size);
    let (width, height) = size;
    Some(if right > bottom {
        (left + width, top, left + width + right, top + height)
    } else {
        (left, top + height, left + width, top + height + bottom)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    const FRAMES: [FrameType; 2] = [FrameType::Polaroid, FrameType::InstaxMini];

    #[test]
    fn no_frame_adds_nothing() {
        assert_eq!(frame_margins(FrameType::None, (300, 200)), (0, 0, 0, 0));
        assert_eq!(framed_size((300, 200), FrameType::None), (300, 200));
        assert_eq!(
            (window_aspect(FrameType::None, (1, 1)), margin_box((1, 1), FrameType::None)),
            (None, None)
        );
        let image = RgbaImage::from_pixel(3, 2, Rgba([1, 2, 3, 4]));
        assert_eq!(add_frame(&image, FrameType::None), image);
    }

    #[test]
    fn tiny_photos_still_get_a_frame() {
        for frame in FRAMES {
            let (l, t, r, b) = frame_margins(frame, (1, 1));
            assert!(l >= 1 && t >= 1 && r >= 1 && b >= 1, "{frame:?}");
            assert_eq!(framed_size((1, 1), frame), (1 + l + r, 1 + t + b));
        }
    }

    #[test]
    fn the_wide_margin_follows_the_orientation() {
        // ポラロイド（正方形の写真部分）はどの向きでも下が広い
        for size in [(400, 400), (300, 500), (500, 300)] {
            let (l, t, r, b) = frame_margins(FrameType::Polaroid, size);
            assert!(b > t && b > l && b > r, "{size:?}");
            assert_eq!(l, r);
        }
        // チェキは縦長の写真なら下、横長なら右（カードを横向きにする）が広い
        let (_, t, _, b) = frame_margins(FrameType::InstaxMini, (460, 620));
        assert!(b > t);
        let (l, _, r, _) = frame_margins(FrameType::InstaxMini, (620, 460));
        assert!(r > l);
        assert_eq!(window_aspect(FrameType::InstaxMini, (620, 460)), Some((62.0, 46.0)));
        assert_eq!(window_aspect(FrameType::InstaxMini, (460, 620)), Some((46.0, 62.0)));
    }

    #[test]
    fn margins_scale_with_the_photo() {
        // 写真部分にちょうど合う大きさなら、余白は実物の比率どおり（ポラロイド 79mm に 4.5mm）
        let (l, _, _, b) = frame_margins(FrameType::Polaroid, (790, 790));
        assert_eq!((l, b), (45, 220));
        // 写真部分と比が違うときは、収まる側の縮尺に合わせる
        assert_eq!(
            frame_margins(FrameType::Polaroid, (790, 1580)),
            frame_margins(FrameType::Polaroid, (790, 790))
        );
    }

    #[test]
    fn margin_box_is_the_widest_margin() {
        let size = (460, 620);
        let (l, t, _, b) = frame_margins(FrameType::InstaxMini, size);
        assert_eq!(margin_box(size, FrameType::InstaxMini), Some((l, t + 620, l + 460, t + 620 + b)));
        let wide = (620, 460);
        let (l, t, r, _) = frame_margins(FrameType::InstaxMini, wide);
        assert_eq!(margin_box(wide, FrameType::InstaxMini), Some((l + 620, t, l + 620 + r, t + 460)));
    }

    #[test]
    fn add_frame_puts_the_photo_inside_a_white_card() {
        let photo = RgbaImage::from_pixel(50, 40, Rgba([10, 20, 30, 128]));
        for frame in FRAMES {
            let framed = add_frame(&photo, frame);
            let (l, t, _, _) = frame_margins(frame, (50, 40));
            assert_eq!(framed.dimensions(), framed_size((50, 40), frame));
            assert_eq!(*framed.get_pixel(0, 0), Rgba([255, 255, 255, 255]));
            // 写真の透過はそのまま
            assert_eq!(*framed.get_pixel(l, t), Rgba([10, 20, 30, 128]));
            assert_eq!(*framed.get_pixel(l + 49, t + 39), Rgba([10, 20, 30, 128]));
            assert_eq!(*framed.get_pixel(l + 50, t + 40), Rgba([255, 255, 255, 255]));
        }
    }
}
