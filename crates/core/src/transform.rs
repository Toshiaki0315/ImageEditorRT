//! 回転・反転・トリミング・リサイズ（旧版の core/transform.py を移したもの）。

use image::{imageops, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::resize;
use crate::PIXELS_PER_TASK;

/// 水平の補正の角度の上限（度）。
pub const STRAIGHTEN_MAX: f64 = 45.0;

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
    /// 5:4（縦向きで 4:5。Instagram の縦長）
    Ratio5x4,
    Ratio4x3,
    Ratio3x2,
    Ratio16x9,
}

impl AspectRatio {
    /// すべての選択肢（画面のプルダウンの順）。
    pub const ALL: [AspectRatio; 6] =
        [Self::Free, Self::Square, Self::Ratio5x4, Self::Ratio4x3, Self::Ratio3x2, Self::Ratio16x9];

    /// 横向きのときの (幅, 高さ)。自由なら None。
    fn value(self) -> Option<(u32, u32)> {
        match self {
            Self::Free => None,
            Self::Square => Some((1, 1)),
            Self::Ratio5x4 => Some((5, 4)),
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

/// 水平の補正: 画像を中心で degrees 度（正は時計回り、±45° まで）回し、四隅に余白が出ないよう
/// 拡大して、元と同じ大きさの新しい画像を返す（旧版にはない）。0° なら複製を返す。
///
/// 大きさが変わらないので、トリミング範囲・出力の大きさはそのまま使える。画素は双線形で補間する
/// （アルファを掛けた値で補間し、透明な画素の色がにじまないようにする）。
pub fn straighten(image: &RgbaImage, degrees: f64) -> RgbaImage {
    let degrees = degrees.clamp(-STRAIGHTEN_MAX, STRAIGHTEN_MAX);
    let (width, height) = image.dimensions();
    if degrees == 0.0 || width == 0 || height == 0 {
        return image.clone();
    }
    let (sin, cos) = degrees.to_radians().sin_cos();
    let scale = straighten_scale((width, height), degrees);
    // 出力の画素から元の画素への逆の変換: 中心からの位置を -θ 回して 1/scale 倍する
    let (a, b) = (cos / scale, sin / scale);
    let (cx, cy) = (f64::from(width) / 2.0, f64::from(height) / 2.0);
    let source = image.as_raw();
    let stride = width as usize * 4;
    let fetch = |x: i64, y: i64| -> [f64; 4] {
        let x = x.clamp(0, i64::from(width) - 1) as usize;
        let y = y.clamp(0, i64::from(height) - 1) as usize;
        let p = &source[y * stride + x * 4..y * stride + x * 4 + 4];
        let alpha = f64::from(p[3]) / 255.0;
        [f64::from(p[0]) * alpha, f64::from(p[1]) * alpha, f64::from(p[2]) * alpha, f64::from(p[3])]
    };
    let mut out = vec![0u8; source.len()];
    let rows_per_task = (PIXELS_PER_TASK / width as usize).max(1);
    out.par_chunks_mut(stride).with_min_len(rows_per_task).enumerate().for_each(|(y, row)| {
        let dy = y as f64 + 0.5 - cy;
        for (x, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let dx = x as f64 + 0.5 - cx;
            let sx = a * dx + b * dy + cx - 0.5;
            let sy = -b * dx + a * dy + cy - 0.5;
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let (p00, p10, p01, p11) =
                (fetch(x0, y0), fetch(x0 + 1, y0), fetch(x0, y0 + 1), fetch(x0 + 1, y0 + 1));
            let mix = |i: usize| {
                let top = p00[i] + (p10[i] - p00[i]) * fx;
                let bottom = p01[i] + (p11[i] - p01[i]) * fx;
                top + (bottom - top) * fy
            };
            let alpha = mix(3);
            pixel[3] = alpha.round().clamp(0.0, 255.0) as u8;
            if alpha > 0.0 {
                let unpremultiply = 255.0 / alpha;
                for (i, channel) in pixel.iter_mut().take(3).enumerate() {
                    *channel = (mix(i) * unpremultiply).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    });
    RgbaImage::from_raw(width, height, out).expect("大きさは元と同じ")
}

/// 水平の補正で、四隅に余白が出ないために拡大する倍率（size の画像を degrees 度回すとき）。
pub fn straighten_scale((width, height): (u32, u32), degrees: f64) -> f64 {
    let (sin, cos) = degrees.clamp(-STRAIGHTEN_MAX, STRAIGHTEN_MAX).to_radians().sin_cos();
    let (width, height) = (f64::from(width.max(1)), f64::from(height.max(1)));
    // 回して拡大した画像が、元の長方形の四隅を覆う条件: W·cos + H·sin ≤ sW かつ W·sin + H·cos ≤ sH
    cos + sin.abs() * (width / height).max(height / width)
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

/// 画像を Lanczos で指定サイズにリサイズする（Pillow の Image.resize(LANCZOS) と画素まで同じ。
/// 大きさが同じなら複製を返す）。
pub fn resize_to(image: &RgbaImage, size: (u32, u32)) -> RgbaImage {
    resize::pillow_resize(image, size, resize::PillowFilter::Lanczos)
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

    #[test]
    fn straighten_zero_is_a_copy() {
        let image = RgbaImage::from_fn(5, 4, |x, y| image::Rgba([x as u8 * 40, y as u8 * 50, 7, 255]));
        assert_eq!(straighten(&image, 0.0), image);
    }

    #[test]
    fn straighten_keeps_size_and_fills_the_corners() {
        let image = RgbaImage::from_pixel(120, 80, image::Rgba([10, 200, 30, 255]));
        for degrees in [-45.0, -10.0, 3.5, 45.0, 90.0] {
            let out = straighten(&image, degrees);
            assert_eq!(out.dimensions(), (120, 80));
            // 四隅にも元の画素が来る（透明な余白はできない）
            assert!(out.pixels().all(|p| p.0 == [10, 200, 30, 255]), "{degrees}");
        }
        assert!((straighten_scale((120, 80), 0.0) - 1.0).abs() < 1e-12);
        assert_eq!(straighten_scale((120, 80), 10.0), straighten_scale((80, 120), -10.0));
    }

    #[test]
    fn straighten_levels_a_tilted_line() {
        // 右上がりに 10° 傾いた線（左が低い）を、時計回りに 10° 回すと水平になる
        let (width, height) = (201u32, 201u32);
        let tilt = 10f64.to_radians();
        let image = RgbaImage::from_fn(width, height, |x, y| {
            let (dx, dy) = (f64::from(x) - 100.0, f64::from(y) - 100.0);
            // 中心を通り、x が増えると y が減る（画面で右上がり）線からの距離
            let distance = (dx * tilt.sin() + dy * tilt.cos()).abs();
            if distance < 2.0 {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([255, 255, 255, 255])
            }
        });
        let out = straighten(&image, 10.0);
        let dark_rows = |x: u32| (0..height).filter(|&y| out.get_pixel(x, y)[0] < 128).collect::<Vec<_>>();
        let (left, right) = (dark_rows(40), dark_rows(160));
        assert!(!left.is_empty() && !right.is_empty());
        let mean = |rows: &[u32]| rows.iter().map(|&y| f64::from(y)).sum::<f64>() / rows.len() as f64;
        assert!((mean(&left) - mean(&right)).abs() <= 1.0, "{left:?} {right:?}");
        // 逆向きに回すと、もっと傾く
        let worse = straighten(&image, -10.0);
        let rows =
            |x: u32| (0..height).filter(|&y| worse.get_pixel(x, y)[0] < 128).map(f64::from).sum::<f64>();
        assert!(rows(160) < rows(40));
    }

    #[test]
    fn straighten_does_not_bleed_transparent_colors() {
        // 透明な部分の色（赤）が、不透明な部分の境目ににじまない
        let image = RgbaImage::from_fn(60, 60, |x, _| {
            if x < 30 {
                image::Rgba([255, 0, 0, 0])
            } else {
                image::Rgba([0, 0, 255, 255])
            }
        });
        let out = straighten(&image, 7.0);
        assert!(out.pixels().filter(|p| p[3] > 0).all(|p| p[0] == 0 && p[2] == 255));
    }

    #[test]
    fn ratio_5x4_turns_into_4x5_for_portrait() {
        assert_eq!(AspectRatio::Ratio5x4.label(), "5:4");
        assert_eq!(AspectRatio::Ratio5x4.ratio(false), Some((5.0, 4.0)));
        assert_eq!(AspectRatio::Ratio5x4.ratio(true), Some((4.0, 5.0)));
        assert_eq!(serde_json::to_string(&AspectRatio::Ratio5x4).unwrap(), "\"ratio5x4\"");
        // 縦向きの 4:5 で、1000 × 1000 の範囲は 800 × 1000 になる
        let rect = fit_aspect(CropRect::new(0, 0, 1000, 1000), AspectRatio::Ratio5x4.ratio(true).unwrap());
        assert_eq!(rect, CropRect::new(100, 0, 800, 1000));
    }
}
