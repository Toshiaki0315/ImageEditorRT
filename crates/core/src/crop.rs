//! トリミング範囲の編集（プレビュー上のドラッグ・数値の入力・比・回転・反転）。
//! 旧版の ui/crop_overlay.py と ui/panel_crop.py の計算を、画面から切り離して移したもの。
//!
//! 座標はどれも、回転・反転した後の原寸画像の座標（px）。

use serde::{Deserialize, Serialize};

use crate::frames::{window_aspect, FrameType};
use crate::privacy::Region;
use crate::shapes::ShapeType;
use crate::transform::{
    aspect_drag_rect, clamp_crop, constrain_rect, fit_aspect, oriented, round_half_even, transform_rect,
    AspectRatio, CropRect, OrientOp, Orientation, MIN_SIZE,
};

/// ドラッグの種類。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DragMode {
    /// 範囲の外から新しく選ぶ（anchor は押した点）
    New,
    /// 範囲の中をつかんで動かす（anchor は押した点）
    Move,
    /// 四隅のハンドルで大きさを変える（anchor は反対側の角）
    Resize,
}

/// 範囲に保たせる縦横比の指定（比のプルダウン・縦向き・フレーム・形）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AspectChoice {
    pub ratio: AspectRatio,
    /// 縦向き（4:3 などを 3:4 にする）
    pub portrait: bool,
    #[serde(default)]
    pub frame: FrameType,
    #[serde(default)]
    pub shape: ShapeType,
}

impl AspectChoice {
    /// フレーム・円に合わせて比を固定しているか（比のプルダウンは選べない。旧版 FR-UI-36）。
    pub fn locked(self) -> bool {
        self.frame != FrameType::None || self.shape == ShapeType::Circle
    }

    /// size の範囲に保たせる縦横比 (幅, 高さ)。フレームがあれば写真部分の比（チェキは範囲の形に合わせて
    /// 縦横どちらにもなる）、フレームなしの円は 1:1、それ以外は比のプルダウンの選択（自由なら None）。
    pub fn aspect(self, size: (u32, u32)) -> Option<(f64, f64)> {
        if self.frame != FrameType::None {
            return window_aspect(self.frame, size);
        }
        if self.shape == ShapeType::Circle {
            return Some((1.0, 1.0));
        }
        self.ratio.ratio(self.portrait)
    }

    /// 向き（横長・縦長）をドラッグの形に合わせるか（写真部分が正方形でないフレームのとき）。
    fn free_orientation(self, size: (u32, u32)) -> bool {
        self.frame != FrameType::None && self.aspect(size).is_some_and(|(w, h)| w != h)
    }
}

fn size_of(rect: CropRect) -> (u32, u32) {
    (rect.width.max(0) as u32, rect.height.max(0) as u32)
}

/// 2 点を対角とする範囲。
pub fn rect_from_points(a: (i64, i64), b: (i64, i64)) -> CropRect {
    let (left, right) = (a.0.min(b.0), a.0.max(b.0));
    let (top, bottom) = (a.1.min(b.1), a.1.max(b.1));
    CropRect::new(left, top, right - left, bottom - top)
}

/// 範囲を大きさを保ったまま動かす。画像からはみ出さないよう位置を直す。
pub fn move_rect(rect: CropRect, dx: i64, dy: i64, (width, height): (u32, u32)) -> CropRect {
    let x = (rect.x + dx).max(0).min((i64::from(width) - rect.width).max(0));
    let y = (rect.y + dy).max(0).min((i64::from(height) - rect.height).max(0));
    CropRect::new(x, y, rect.width, rect.height)
}

/// 角の番号（左上から時計回りに 0〜3）の反対側の角。
pub fn opposite_corner(rect: CropRect, corner: usize) -> (i64, i64) {
    let (left, top, right, bottom) = (rect.x, rect.y, rect.right(), rect.bottom());
    [(right, bottom), (left, bottom), (left, top), (right, top)][corner % 4]
}

/// ドラッグ中の範囲。start は動かす・大きさを変える前の範囲。
pub fn drag(
    mode: DragMode,
    anchor: (i64, i64),
    point: (i64, i64),
    start: Option<CropRect>,
    aspect: AspectChoice,
    size: (u32, u32),
) -> Option<CropRect> {
    if mode == DragMode::Move {
        let start = start?;
        return Some(move_rect(start, point.0 - anchor.0, point.1 - anchor.1, size));
    }
    // 比は今の範囲（新規なら画像全体）の形から決める
    let range = start.map_or(size, size_of);
    let Some(ratio) = aspect.aspect(range) else { return Some(rect_from_points(anchor, point)) };
    // 向きを自由にするときは、大きさの変更は元の範囲の向き、新規はドラッグの方向に合わせる
    let ratio = if aspect.free_orientation(range) {
        let landscape = match (mode, start) {
            (DragMode::Resize, Some(start)) => start.width >= start.height,
            _ => (point.0 - anchor.0).abs() >= (point.1 - anchor.1).abs(),
        };
        oriented(ratio, landscape)
    } else {
        ratio
    };
    Some(aspect_drag_rect(anchor, point, ratio, size))
}

/// ドラッグを終えたとき: 幅・高さが 0（クリックしただけ）ならトリミングなし。
pub fn finish(rect: Option<CropRect>) -> Option<CropRect> {
    rect.filter(|r| r.width > 0 && r.height > 0)
}

/// 数値の欄のどれを変えたか。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpinField {
    X,
    Y,
    Width,
    Height,
}

/// 数値の欄の変更。比を指定しているときは、もう一方の辺や位置を直して比を保つ（旧版 FR-UI-35）。
///
/// values は欄の値 (X, Y, 幅, 高さ)、previous は変更前の範囲。幅・高さが 0 ならトリミングなし。
pub fn spin_edit(
    field: SpinField,
    values: CropRect,
    previous: Option<CropRect>,
    aspect: AspectChoice,
    size: (u32, u32),
) -> Option<CropRect> {
    let CropRect { mut x, mut y, mut width, mut height } = values;
    let range = previous.and_then(|p| clamp_crop(p, size)).map_or(size, size_of);
    if let Some((aw, ah)) = aspect.aspect(range) {
        match field {
            SpinField::Width if width > 0 => {
                height = round_half_even(width as f64 * ah / aw).max(i64::from(MIN_SIZE))
            }
            SpinField::Height if height > 0 => {
                width = round_half_even(height as f64 * aw / ah).max(i64::from(MIN_SIZE))
            }
            SpinField::X | SpinField::Y if previous.is_some() => {
                // 位置を変えたときは大きさを保ち、画像からはみ出す分だけ戻す
                x = x.min((i64::from(size.0) - width).max(0));
                y = y.min((i64::from(size.1) - height).max(0));
            }
            _ => {}
        }
        if width > 0 && height > 0 {
            if let Some(fitted) = constrain_rect(CropRect::new(x, y, width, height), (aw, ah), size) {
                return Some(fitted);
            }
        }
    }
    (width > 0 && height > 0).then_some(CropRect::new(x, y, width, height))
}

/// 比（または縦向き）を変えたとき: 範囲があれば、その中央を新しい比に直す。なければ何もしない。
pub fn fit_to_aspect(rect: Option<CropRect>, aspect: AspectChoice, size: (u32, u32)) -> Option<CropRect> {
    let rect = clamp_crop(rect?, size)?;
    Some(match aspect.aspect(size_of(rect)) {
        Some(ratio) => fit_aspect(rect, ratio),
        None => rect,
    })
}

/// 回転・反転の結果。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Oriented {
    pub orientation: Orientation,
    /// 同じ写真の部分を指すよう、一緒に回した範囲
    pub crop: Option<CropRect>,
    /// 一緒に回した、投稿加工の範囲
    pub regions: Vec<Region>,
    /// 回転・反転した後の原寸画像の大きさ
    pub size: (u32, u32),
}

/// 表示中の向きに対して回転・反転する（旧版 FR-UI-55）。size は今の向きの原寸画像の大きさ。
/// トリミング範囲と投稿加工の範囲も、同じ写真の部分を指すよう一緒に回す。
pub fn orient(
    orientation: Orientation,
    op: OrientOp,
    crop: Option<CropRect>,
    regions: &[Region],
    size: (u32, u32),
) -> Oriented {
    let rect = crop.and_then(|r| clamp_crop(r, size));
    let regions = regions
        .iter()
        .filter_map(|region| {
            let rect = clamp_crop(region.rect, size)?;
            Some(Region { rect: transform_rect(rect, size, op), ..region.clone() })
        })
        .collect();
    Oriented {
        orientation: orientation.apply(op),
        crop: rect.map(|r| transform_rect(r, size, op)),
        regions,
        size: if op.swaps_sides() { (size.1, size.0) } else { size },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::RegionKind;

    const fn choice(ratio: AspectRatio) -> AspectChoice {
        AspectChoice { ratio, portrait: false, frame: FrameType::None, shape: ShapeType::Rectangle }
    }
    const FREE: AspectChoice = choice(AspectRatio::Free);
    const SQUARE: AspectChoice = choice(AspectRatio::Square);
    const R4X3: AspectChoice = choice(AspectRatio::Ratio4x3);
    const INSTAX: AspectChoice = AspectChoice { frame: FrameType::InstaxMini, ..FREE };
    const CIRCLE: AspectChoice = AspectChoice { shape: ShapeType::Circle, ..FREE };

    #[test]
    fn new_drag_in_any_direction() {
        let rect = drag(DragMode::New, (50, 40), (10, 70), None, FREE, (100, 100));
        assert_eq!(rect, Some(CropRect::new(10, 40, 40, 30)));
        // 比を保つ（4:3、左上に向かって）
        let rect = drag(DragMode::New, (50, 50), (10, 40), None, R4X3, (100, 100)).unwrap();
        assert_eq!((rect.right(), rect.bottom()), (50, 50));
        assert_eq!(rect.width * 3, rect.height * 4);
    }

    #[test]
    fn move_stays_inside() {
        let start = CropRect::new(10, 10, 30, 20);
        let moved = drag(DragMode::Move, (20, 20), (200, -50), Some(start), FREE, (100, 80));
        assert_eq!(moved, Some(CropRect::new(70, 0, 30, 20)));
    }

    #[test]
    fn resize_from_opposite_corner() {
        let start = CropRect::new(10, 10, 30, 20);
        // 右下の角（2）をつかむと、左上（10, 10）が固定される
        let anchor = opposite_corner(start, 2);
        assert_eq!(anchor, (10, 10));
        let rect = drag(DragMode::Resize, anchor, (60, 70), Some(start), SQUARE, (100, 100)).unwrap();
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (10, 10, 60, 60));
    }

    #[test]
    fn click_without_drag_clears() {
        assert_eq!(finish(drag(DragMode::New, (5, 5), (5, 5), None, FREE, (10, 10))), None);
    }

    #[test]
    fn spins_keep_aspect() {
        let size = (200, 100);
        let previous = Some(CropRect::new(0, 0, 40, 30));
        // 幅を変えると高さを比から計算する
        let r = spin_edit(SpinField::Width, CropRect::new(0, 0, 80, 30), previous, R4X3, size);
        assert_eq!(r, Some(CropRect::new(0, 0, 80, 60)));
        // 高さを変えると幅を計算する
        let r = spin_edit(SpinField::Height, CropRect::new(0, 0, 40, 90), previous, R4X3, size);
        assert_eq!(r, Some(CropRect::new(0, 0, 120, 90)));
        // 位置を変えたら大きさを保ってはみ出す分だけ戻す
        let r = spin_edit(SpinField::X, CropRect::new(190, 0, 40, 30), previous, R4X3, size);
        assert_eq!(r, Some(CropRect::new(160, 0, 40, 30)));
        // 画像に収まらない大きさは、左上を固定して比を保ったまま縮める
        let r = spin_edit(SpinField::Width, CropRect::new(100, 0, 400, 0), previous, R4X3, size).unwrap();
        assert_eq!((r.x, r.y), (100, 0));
        assert!(r.right() <= 200 && r.bottom() <= 100);
        // 自由ならそのまま、0 ならなし
        assert_eq!(
            spin_edit(SpinField::Width, CropRect::new(1, 2, 3, 4), None, FREE, size),
            Some(CropRect::new(1, 2, 3, 4))
        );
        assert_eq!(spin_edit(SpinField::Width, CropRect::new(1, 2, 0, 4), None, FREE, size), None);
    }

    #[test]
    fn changing_aspect_fits_the_center() {
        let rect = fit_to_aspect(Some(CropRect::new(0, 0, 100, 50)), SQUARE, (200, 200));
        assert_eq!(rect, Some(CropRect::new(25, 0, 50, 50)));
        assert_eq!(fit_to_aspect(None, SQUARE, (200, 200)), None);
    }

    #[test]
    fn frame_and_circle_lock_the_aspect() {
        assert!(INSTAX.locked() && CIRCLE.locked() && !R4X3.locked());
        assert_eq!(CIRCLE.aspect((300, 100)), Some((1.0, 1.0)));
        // チェキの写真部分は 46:62。横長の範囲なら 62:46
        assert_eq!(INSTAX.aspect((100, 300)), Some((46.0, 62.0)));
        assert_eq!(INSTAX.aspect((300, 100)), Some((62.0, 46.0)));
        // 新規のドラッグは、ドラッグの方向で向きを決める
        let wide = drag(DragMode::New, (0, 0), (200, 50), None, INSTAX, (400, 400)).unwrap();
        assert!(wide.width > wide.height, "{wide:?}");
        let tall = drag(DragMode::New, (0, 0), (50, 200), None, INSTAX, (400, 400)).unwrap();
        assert!(tall.height > tall.width, "{tall:?}");
        // 比を変えたとき（フレームを選んだとき）は範囲の中央を写真部分の比に直す
        let fitted = fit_to_aspect(Some(CropRect::new(0, 0, 100, 300)), CIRCLE, (400, 400));
        assert_eq!(fitted, Some(CropRect::new(0, 100, 100, 100)));
    }

    #[test]
    fn rotating_turns_the_crop_with_the_image() {
        let result = orient(
            Orientation::default(),
            OrientOp::RotateRight,
            Some(CropRect::new(10, 20, 30, 40)),
            &[
                Region { kind: RegionKind::Mosaic, rect: CropRect::new(10, 20, 30, 40), strength: 70 },
                // 画像の外の範囲は捨てる
                Region { rect: CropRect::new(500, 500, 10, 10), ..Region::default() },
            ],
            (200, 100),
        );
        assert_eq!(result.orientation, Orientation::new(90, false));
        assert_eq!(result.size, (100, 200));
        assert_eq!(result.crop, Some(CropRect::new(40, 10, 40, 30)));
        assert_eq!(
            result.regions,
            vec![Region { kind: RegionKind::Mosaic, rect: CropRect::new(40, 10, 40, 30), strength: 70 }]
        );
        let flipped = orient(Orientation::default(), OrientOp::FlipHorizontal, None, &[], (200, 100));
        assert_eq!((flipped.size, flipped.crop), ((200, 100), None));
    }
}
