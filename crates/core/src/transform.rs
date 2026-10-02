//! 回転・反転・トリミング・リサイズ（旧版の core/transform.py を移したもの）。

use image::{imageops, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::resize;

/// 幅・高さの下限と上限（px）。
pub const MIN_SIZE: u32 = 1;
pub const MAX_SIZE: u32 = 20000;

/// トリミング範囲（回転・反転した後の原寸画像の座標、px）。x・y は画像の外（負）も表せる。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CropRect {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

impl CropRect {
    pub const fn new(x: i64, y: i64, width: i64, height: i64) -> Self {
        Self { x, y, width, height }
    }

    /// 画像全体の範囲。
    pub fn whole((width, height): (u32, u32)) -> Self {
        Self::new(0, 0, i64::from(width), i64::from(height))
    }

    /// 右端の x 座標（範囲に含まない）。
    pub fn right(&self) -> i64 {
        self.x + self.width
    }

    /// 下端の y 座標（範囲に含まない）。
    pub fn bottom(&self) -> i64 {
        self.y + self.height
    }

    /// 短辺（px）。
    pub fn short_side(&self) -> i64 {
        self.width.min(self.height)
    }
}

/// トリミングの縦横比の選択肢。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AspectRatio {
    Free,
    Square,
    Ratio4x3,
    Ratio3x2,
    Ratio16x9,
}

impl AspectRatio {
    /// すべての選択肢（画面のプルダウンの順）。
    pub const ALL: [AspectRatio; 5] =
        [Self::Free, Self::Square, Self::Ratio4x3, Self::Ratio3x2, Self::Ratio16x9];

    /// 横向きのときの (幅, 高さ)。自由なら None。
    fn value(self) -> Option<(u32, u32)> {
        match self {
            Self::Free => None,
            Self::Square => Some((1, 1)),
            Self::Ratio4x3 => Some((4, 3)),
            Self::Ratio3x2 => Some((3, 2)),
            Self::Ratio16x9 => Some((16, 9)),
        }
    }

    /// 画面に出す名前。
    pub fn label(self) -> String {
        self.value().map_or_else(|| "自由".into(), |(w, h)| format!("{w}:{h}"))
    }

    /// 縦横比 (幅, 高さ)。portrait なら縦向きにする。自由なら None。
    pub fn ratio(self, portrait: bool) -> Option<(f64, f64)> {
        let (w, h) = self.value()?;
        let (w, h) = (f64::from(w), f64::from(h));
        Some(if portrait { (h, w) } else { (w, h) })
    }
}

/// 縦横比を横向き（幅 ≥ 高さ）または縦向きにそろえて返す。
pub fn oriented(aspect: (f64, f64), landscape: bool) -> (f64, f64) {
    let (long, short) = (aspect.0.max(aspect.1), aspect.0.min(aspect.1));
    if landscape {
        (long, short)
    } else {
        (short, long)
    }
}

/// 範囲を画像内に収まるよう補正して返す。
///
/// 画像との重なり部分に切り詰める。幅・高さが 0 以下、または画像と重ならない場合は None。
pub fn clamp_crop(rect: CropRect, (width, height): (u32, u32)) -> Option<CropRect> {
    if rect.width <= 0 || rect.height <= 0 {
        return None;
    }
    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = rect.right().min(i64::from(width));
    let bottom = rect.bottom().min(i64::from(height));
    (right > left && bottom > top).then(|| CropRect::new(left, top, right - left, bottom - top))
}

/// 範囲を縦横比 aspect (幅, 高さ) になるよう中央で切り詰めた範囲を返す。
///
/// 長すぎる側だけを両端から均等に削る。端数は四捨五入し、最小 1px。
pub fn fit_aspect(rect: CropRect, aspect: (f64, f64)) -> CropRect {
    let (width, height) = fit_aspect_size(rect.width, rect.height, aspect);
    CropRect::new(
        rect.x + (rect.width - width).div_euclid(2),
        rect.y + (rect.height - height).div_euclid(2),
        width,
        height,
    )
}

/// 範囲を画像内に収め、縦横比 aspect になるよう左上を固定して縮めた範囲を返す。
///
/// 数値入力で比を保つとき用。画像と重ならなければ None。
pub fn constrain_rect(rect: CropRect, aspect: (f64, f64), size: (u32, u32)) -> Option<CropRect> {
    let clamped = clamp_crop(rect, size)?;
    let (width, height) = fit_aspect_size(clamped.width, clamped.height, aspect);
    Some(CropRect::new(clamped.x, clamped.y, width, height))
}

/// width × height に収まる、縦横比 aspect の大きさ（長すぎる側だけを縮める、最小 1px）。
fn fit_aspect_size(width: i64, height: i64, (aw, ah): (f64, f64)) -> (i64, i64) {
    let (w, h) = (width as f64, height as f64);
    if w * ah > h * aw {
        (width.min(round_half_even(h * aw / ah).max(1)), height)
    } else {
        (width, height.min(round_half_even(w * ah / aw).max(1)))
    }
}

/// anchor を固定した角として point の方向へ広げた、縦横比 aspect の範囲を返す。
///
/// ドラッグで比を保って範囲を選ぶとき用。マウスの位置まで届く大きさ（比に対して長い方の
/// 辺に合わせる）にし、画像の端を越えるときは比を保ったまま縮める。
pub fn aspect_drag_rect(
    anchor: (i64, i64),
    point: (i64, i64),
    (aw, ah): (f64, f64),
    size: (u32, u32),
) -> CropRect {
    let (ax, ay) = anchor;
    let (dx, dy) = (point.0 - ax, point.1 - ay);
    let (mut width, mut height) = ((dx.abs()) as f64, (dy.abs()) as f64);
    if width * ah >= height * aw {
        height = width * ah / aw;
    } else {
        width = height * aw / ah;
    }
    let max_width = if dx < 0 { ax } else { i64::from(size.0) - ax };
    let max_height = if dy < 0 { ay } else { i64::from(size.1) - ay };
    let mut scale = 1.0;
    if width > max_width as f64 {
        scale = max_width as f64 / width;
    }
    if height * scale > max_height as f64 {
        scale = max_height as f64 / height;
    }
    let width_px = (width * scale + 1e-9) as i64;
    let height_px = max_height.min(round_half_even(width_px as f64 * ah / aw));
    let x = if dx < 0 { ax - width_px } else { ax };
    let y = if dy < 0 { ay - height_px } else { ay };
    CropRect::new(x, y, width_px, height_px)
}

/// 画像を範囲で切り抜く。範囲は先に clamp_crop で画像の中に収めておくこと。
pub fn crop(image: &RgbaImage, rect: CropRect) -> RgbaImage {
    imageops::crop_imm(image, rect.x as u32, rect.y as u32, rect.width as u32, rect.height as u32).to_image()
}

/// 幅・高さの指定が範囲外。
#[derive(Debug, PartialEq, Eq)]
pub struct SizeError {
    pub name: &'static str,
    pub value: u32,
}

impl std::fmt::Display for SizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} は {MIN_SIZE}〜{MAX_SIZE} で指定してください: {}", self.name, self.value)
    }
}

impl std::error::Error for SizeError {}

/// 指定された幅・高さと縦横比保持の設定から出力サイズを計算する。
///
/// - 両方 None: 元のサイズ
/// - 片方だけ指定: keep_aspect なら縦横比から他方を計算、そうでなければ他方は元のまま
/// - 両方指定: keep_aspect なら指定範囲に収まる最大サイズ、そうでなければ指定どおり
///
/// 端数は四捨五入し、最小 1px。指定値が 1〜20000 の範囲外ならエラー。
pub fn fit_size(
    (orig_width, orig_height): (u32, u32),
    width: Option<u32>,
    height: Option<u32>,
    keep_aspect: bool,
) -> Result<(u32, u32), SizeError> {
    for (name, value) in [("width", width), ("height", height)] {
        if let Some(value) = value.filter(|v| !(MIN_SIZE..=MAX_SIZE).contains(v)) {
            return Err(SizeError { name, value });
        }
    }
    let (mut width, mut height) = (width, height);
    match (width, height) {
        (None, None) => return Ok((orig_width, orig_height)),
        _ if !keep_aspect => return Ok((width.unwrap_or(orig_width), height.unwrap_or(orig_height))),
        (Some(w), Some(h)) => {
            // 縦横比を保ったまま指定範囲に収める（縮小率が小さい方に合わせる）
            if u64::from(w) * u64::from(orig_height) <= u64::from(h) * u64::from(orig_width) {
                height = None;
            } else {
                width = None;
            }
        }
        _ => {}
    }
    Ok(match (width, height) {
        (Some(w), _) => (w, round_div(u64::from(orig_height) * u64::from(w), u64::from(orig_width))),
        (None, Some(h)) => (round_div(u64::from(orig_width) * u64::from(h), u64::from(orig_height)), h),
        (None, None) => unreachable!("どちらかは指定されている"),
    })
}

/// 回転・反転の操作（表示中の向きに対して行う）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrientOp {
    /// 反時計回りに 90°
    RotateLeft,
    /// 時計回りに 90°
    RotateRight,
    /// 左右反転
    FlipHorizontal,
    /// 上下反転
    FlipVertical,
}

impl OrientOp {
    /// 幅と高さが入れ替わる操作か。
    pub fn swaps_sides(self) -> bool {
        matches!(self, Self::RotateLeft | Self::RotateRight)
    }
}

/// 画像の向き。左右反転 (mirror) してから時計回りに rotation 度回した状態を表す。
///
/// 回転と反転の組み合わせはすべてこの 8 通りのどれかにまとまる。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Orientation {
    /// 0 / 90 / 180 / 270（時計回り）
    pub rotation: u32,
    pub mirror: bool,
}

impl Orientation {
    pub const fn new(rotation: u32, mirror: bool) -> Self {
        Self { rotation: rotation % 360, mirror }
    }

    /// 回転も反転もしていないか。
    pub fn is_identity(self) -> bool {
        self.rotation == 0 && !self.mirror
    }

    /// 今の向きに op を重ねた向きを返す。
    pub fn apply(self, op: OrientOp) -> Self {
        let rotation = self.rotation as i64;
        let (rotation, mirror) = match op {
            OrientOp::RotateRight => (rotation + 90, self.mirror),
            OrientOp::RotateLeft => (rotation - 90, self.mirror),
            // 左右反転 ∘ 回転(r) = 回転(-r) ∘ 左右反転
            OrientOp::FlipHorizontal => (-rotation, !self.mirror),
            // 上下反転 = 180° 回転 ∘ 左右反転
            OrientOp::FlipVertical => (180 - rotation, !self.mirror),
        };
        Self { rotation: rotation.rem_euclid(360) as u32, mirror }
    }

    /// size の画像をこの向きにしたときの大きさを返す。
    pub fn size(self, (width, height): (u32, u32)) -> (u32, u32) {
        if matches!(self.rotation, 90 | 270) {
            (height, width)
        } else {
            (width, height)
        }
    }

    /// 画像をこの向きにした新しい画像を返す（入力画像は変更しない）。
    pub fn transpose(self, image: &RgbaImage) -> RgbaImage {
        let mirrored = self.mirror.then(|| imageops::flip_horizontal(image));
        let source = mirrored.as_ref().unwrap_or(image);
        match self.rotation {
            90 => imageops::rotate90(source),
            180 => imageops::rotate180(source),
            270 => imageops::rotate270(source),
            _ => mirrored.unwrap_or_else(|| image.clone()),
        }
    }
}

/// size の画像上の範囲を、画像に op をかけた後の同じ部分を指す範囲に変換する。
pub fn transform_rect(rect: CropRect, (width, height): (u32, u32), op: OrientOp) -> CropRect {
    let (width, height) = (i64::from(width), i64::from(height));
    match op {
        OrientOp::RotateRight => CropRect::new(height - rect.bottom(), rect.x, rect.height, rect.width),
        OrientOp::RotateLeft => CropRect::new(rect.y, width - rect.right(), rect.height, rect.width),
        OrientOp::FlipHorizontal => CropRect::new(width - rect.right(), rect.y, rect.width, rect.height),
        OrientOp::FlipVertical => CropRect::new(rect.x, height - rect.bottom(), rect.width, rect.height),
    }
}

/// 画像を Lanczos で指定サイズにリサイズする（大きさが同じなら複製を返す）。
pub fn resize_to(image: &RgbaImage, (width, height): (u32, u32)) -> RgbaImage {
    if image.dimensions() == (width, height) {
        return image.clone();
    }
    resize::resize(image, width, height)
}

/// numerator / denominator を四捨五入し、最小 1 にする（浮動小数の誤差を避けて整数で計算）。
fn round_div(numerator: u64, denominator: u64) -> u32 {
    ((2 * numerator + denominator) / (2 * denominator)).max(u64::from(MIN_SIZE)) as u32
}

/// Python の round() と同じ、偶数への丸め（.5 は近い偶数へ）。
pub(crate) fn round_half_even(value: f64) -> i64 {
    let rounded = value.round();
    if (value - value.trunc()).abs() == 0.5 && rounded as i64 % 2 != 0 {
        (rounded - value.signum()) as i64
    } else {
        rounded as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_like_python() {
        assert_eq!(round_half_even(0.5), 0);
        assert_eq!(round_half_even(1.5), 2);
        assert_eq!(round_half_even(2.5), 2);
        assert_eq!(round_half_even(-1.5), -2);
        assert_eq!(round_half_even(2.4), 2);
        assert_eq!(round_half_even(2.6), 3);
    }

    #[test]
    fn orientation_sizes() {
        assert_eq!(Orientation::new(90, false).size((7, 5)), (5, 7));
        assert_eq!(Orientation::new(180, true).size((7, 5)), (7, 5));
        assert!(Orientation::default().is_identity());
        assert!(OrientOp::RotateLeft.swaps_sides() && !OrientOp::FlipVertical.swaps_sides());
    }

    #[test]
    fn four_rotations_return_to_start() {
        let mut o = Orientation::new(0, true);
        for _ in 0..4 {
            o = o.apply(OrientOp::RotateRight);
        }
        assert_eq!(o, Orientation::new(0, true));
        assert_eq!(
            Orientation::default().apply(OrientOp::FlipHorizontal).apply(OrientOp::FlipHorizontal),
            Orientation::default()
        );
    }

    #[test]
    fn transpose_keeps_alpha() {
        let mut image = RgbaImage::new(3, 2);
        image.put_pixel(0, 0, image::Rgba([1, 2, 3, 40]));
        let rotated = Orientation::new(90, true).transpose(&image);
        assert_eq!(rotated.dimensions(), (2, 3));
        assert!(rotated.pixels().any(|p| p.0 == [1, 2, 3, 40]));
    }
}
