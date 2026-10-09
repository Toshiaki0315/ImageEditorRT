//! 部分補正（旧版にはない）: 円（楕円）・帯の範囲の中だけ、露出・コントラスト・色温度・彩度を変える。
//!
//! 範囲は回転・反転した後の原寸の画像の座標で持つ（トリミング範囲・投稿加工の範囲と同じ）。範囲ごとに、
//! 画像全体にその範囲の調整をかけた画像を作り、範囲の重み（中で 1、外で 0、境目はなめらか）で混ぜる。

use image::{imageops, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::adjust::TEMPERATURE_NEUTRAL;
use crate::transform::CropRect;

/// ぼかし幅の既定値（%）。
pub const FEATHER_DEFAULT: u32 = 50;

/// 部分補正の範囲の形。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalShape {
    /// rect に内接する楕円
    Ellipse { rect: CropRect },
    /// from の側で効き、to に向かって弱まり、to の先では効かない帯（空を暗くするなど）
    Band { from: [i64; 2], to: [i64; 2] },
}

/// 部分補正の 1 つ分。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LocalAdjust {
    pub shape: LocalShape,
    /// 露出（EV）
    pub exposure: f64,
    /// コントラスト -100〜100
    pub contrast: i32,
    /// 色温度（K。6500 で変えない）
    pub temperature: u32,
    /// 彩度 -100〜100
    pub saturation: i32,
    /// 楕円の境目のぼかし幅 0〜100（%。半径に対する、ぼかしが始まる内側までの幅）
    pub feather: u32,
}

impl Default for LocalAdjust {
    fn default() -> Self {
        Self {
            shape: LocalShape::Ellipse { rect: CropRect::new(0, 0, 0, 0) },
            exposure: 0.0,
            contrast: 0,
            temperature: TEMPERATURE_NEUTRAL,
            saturation: 0,
            feather: FEATHER_DEFAULT,
        }
    }
}

impl LocalAdjust {
    /// 調整がない（どこも変えない）か。
    pub fn is_neutral(&self) -> bool {
        self.exposure == 0.0
            && self.contrast == 0
            && self.temperature == TEMPERATURE_NEUTRAL
            && self.saturation == 0
    }

    /// (x, y)（原寸の座標）での効き方 0〜1。
    pub fn weight(&self, x: f64, y: f64) -> f32 {
        match self.shape {
            LocalShape::Ellipse { rect } => {
                let (rx, ry) = (rect.width as f64 / 2.0, rect.height as f64 / 2.0);
                if rx <= 0.0 || ry <= 0.0 {
                    return 0.0;
                }
                let (dx, dy) = ((x - rect.x as f64 - rx) / rx, (y - rect.y as f64 - ry) / ry);
                let distance = (dx * dx + dy * dy).sqrt();
                let inner = 1.0 - f64::from(self.feather.min(100)) / 100.0;
                if distance <= inner {
                    1.0
                } else if distance >= 1.0 {
                    0.0
                } else {
                    smoothstep((1.0 - distance) / (1.0 - inner)) as f32
                }
            }
            LocalShape::Band { from, to } => {
                let (vx, vy) = ((to[0] - from[0]) as f64, (to[1] - from[1]) as f64);
                let length = vx * vx + vy * vy;
                if length <= 0.0 {
                    return 0.0;
                }
                let t = ((x - from[0] as f64) * vx + (y - from[1] as f64) * vy) / length;
                (1.0 - smoothstep(t.clamp(0.0, 1.0))) as f32
            }
        }
    }
}

/// 0〜1 をなめらかにつなぐ（端で傾きが 0）。
fn smoothstep(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// image に部分補正をかける。原寸の座標 (x, y) は、この画像では (x * scale - offset) にある。
/// adjust は、渡した画像全体に 1 つ分の調整をかけた画像を返す（色の調整と同じかけ方。画素ごとに決まる処理であること）。
///
/// 速さのため、範囲が効くところ（円は外接する四角、帯は画像全体）だけを切り出して調整し、行ごとに並列に混ぜる。
pub fn apply_local(
    mut image: RgbaImage,
    adjusts: &[LocalAdjust],
    scale: (f64, f64),
    offset: (f64, f64),
    adjust: impl Fn(&RgbaImage, &LocalAdjust) -> RgbaImage,
) -> RgbaImage {
    let width = image.width() as usize;
    for local in adjusts.iter().filter(|a| !a.is_neutral()) {
        let Some((left, top, right, bottom)) = local.pixel_bounds(image.dimensions(), scale, offset) else {
            continue;
        };
        let part = imageops::crop_imm(&image, left, top, right - left, bottom - top).to_image();
        let adjusted = adjust(&part, local);
        image
            .as_mut()
            .par_chunks_exact_mut(width * 4)
            .enumerate()
            .skip(top as usize)
            .take((bottom - top) as usize)
            .for_each(|(y, row)| {
                // 画素の中心を原寸の座標に戻す
                let oy = (y as f64 + 0.5 + offset.1) / scale.1;
                for x in left..right {
                    let ox = (f64::from(x) + 0.5 + offset.0) / scale.0;
                    let weight = local.weight(ox, oy);
                    if weight <= 0.0 {
                        continue;
                    }
                    let target = adjusted.get_pixel(x - left, y as u32 - top);
                    let pixel = &mut row[x as usize * 4..x as usize * 4 + 3];
                    for c in 0..3 {
                        let (a, b) = (f32::from(pixel[c]), f32::from(target[c]));
                        pixel[c] = (a + (b - a) * weight).round().clamp(0.0, 255.0) as u8;
                    }
                }
            });
    }
    image
}

impl LocalAdjust {
    /// size の画像（原寸の (x, y) が (x * scale - offset) にある）で、範囲が効きうる画素の四角（左, 上, 右, 下。右・下は
    /// 含まない）。円は外接する四角、帯は画像全体。画像と重ならなければ None。
    fn pixel_bounds(
        &self,
        (width, height): (u32, u32),
        scale: (f64, f64),
        offset: (f64, f64),
    ) -> Option<(u32, u32, u32, u32)> {
        let (left, top, right, bottom) = match self.shape {
            LocalShape::Ellipse { rect } => (
                (rect.x as f64 * scale.0 - offset.0).floor(),
                (rect.y as f64 * scale.1 - offset.1).floor(),
                (rect.right() as f64 * scale.0 - offset.0).ceil(),
                (rect.bottom() as f64 * scale.1 - offset.1).ceil(),
            ),
            LocalShape::Band { .. } => (0.0, 0.0, f64::from(width), f64::from(height)),
        };
        let clamp = |v: f64, max: u32| v.clamp(0.0, f64::from(max)) as u32;
        let (left, right) = (clamp(left, width), clamp(right, width));
        let (top, bottom) = (clamp(top, height), clamp(bottom, height));
        (left < right && top < bottom).then_some((left, top, right, bottom))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// 調整の代わりに、赤を 255 にする（効き方を見やすく）。
    fn redden(image: &RgbaImage, _: &LocalAdjust) -> RgbaImage {
        let mut out = image.clone();
        for p in out.pixels_mut() {
            p[0] = 255;
        }
        out
    }

    fn ellipse(rect: CropRect, feather: u32) -> LocalAdjust {
        LocalAdjust { shape: LocalShape::Ellipse { rect }, exposure: 1.0, feather, ..LocalAdjust::default() }
    }

    #[test]
    fn ellipse_changes_only_inside_with_a_smooth_edge() {
        let image = RgbaImage::from_pixel(100, 100, Rgba([0, 50, 50, 255]));
        let out =
            apply_local(image, &[ellipse(CropRect::new(20, 20, 60, 60), 50)], (1.0, 1.0), (0.0, 0.0), redden);
        assert_eq!(out.get_pixel(50, 50)[0], 255, "中心は全部");
        assert_eq!(out.get_pixel(5, 5)[0], 0, "外は変えない");
        assert_eq!(out.get_pixel(50, 50)[1], 50, "ほかの色は調整のまま");
        // 中心から外へ、だんだん弱くなる（途中で増えない）
        let row: Vec<u8> = (50..85).map(|x| out.get_pixel(x, 50)[0]).collect();
        assert!(row.windows(2).all(|w| w[0] >= w[1]), "{row:?}");
        assert!(row.iter().any(|&v| v > 0 && v < 255), "境目はなめらか: {row:?}");
    }

    #[test]
    fn band_fades_from_start_to_end_and_follows_the_scale() {
        // 原寸 200×200 の上 (y=0) から y=100 にかけて弱まる帯を、半分の大きさの画像にかける
        let band = LocalAdjust {
            shape: LocalShape::Band { from: [0, 0], to: [0, 100] },
            exposure: -1.0,
            ..LocalAdjust::default()
        };
        let image = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 255]));
        let out = apply_local(image, &[band], (0.5, 0.5), (0.0, 0.0), redden);
        assert!(out.get_pixel(10, 0)[0] > 250);
        let middle = out.get_pixel(10, 25)[0];
        assert!(middle > 50 && middle < 200, "{middle}");
        assert_eq!(out.get_pixel(10, 60)[0], 0, "原寸の y=120 は帯の先");
    }

    #[test]
    fn neutral_or_empty_shapes_change_nothing() {
        let image = RgbaImage::from_pixel(10, 10, Rgba([1, 2, 3, 255]));
        let neutral = LocalAdjust { exposure: 0.0, ..ellipse(CropRect::new(0, 0, 10, 10), 0) };
        let empty = ellipse(CropRect::new(0, 0, 0, 5), 0);
        let point = LocalAdjust { shape: LocalShape::Band { from: [3, 3], to: [3, 3] }, ..empty };
        let out = apply_local(image.clone(), &[neutral, empty, point], (1.0, 1.0), (0.0, 0.0), redden);
        assert_eq!(out, image);
        // 境目のぼかし幅 0 なら、縁までくっきり
        assert_eq!(ellipse(CropRect::new(0, 0, 10, 10), 0).weight(5.0, 1.0), 1.0);
    }

    /// 速くする前のかけ方（画像全体を調整して、全部の画素で重みを求める）。
    fn reference(
        mut image: RgbaImage,
        adjusts: &[LocalAdjust],
        scale: (f64, f64),
        offset: (f64, f64),
        adjust: impl Fn(&RgbaImage, &LocalAdjust) -> RgbaImage,
    ) -> RgbaImage {
        for local in adjusts.iter().filter(|a| !a.is_neutral()) {
            let adjusted = adjust(&image, local);
            for (x, y, pixel) in image.enumerate_pixels_mut() {
                let ox = (f64::from(x) + 0.5 + offset.0) / scale.0;
                let oy = (f64::from(y) + 0.5 + offset.1) / scale.1;
                let weight = local.weight(ox, oy);
                if weight <= 0.0 {
                    continue;
                }
                let target = adjusted.get_pixel(x, y);
                for c in 0..3 {
                    let (a, b) = (f32::from(pixel[c]), f32::from(target[c]));
                    pixel[c] = (a + (b - a) * weight).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        image
    }

    #[test]
    fn same_result_as_adjusting_the_whole_image() {
        // 画素ごとに決まる調整（色を反転し、少し足す）で、はみ出す円・帯・重なる範囲を、倍率とずれ付きで比べる
        let invert = |image: &RgbaImage, a: &LocalAdjust| {
            let mut out = image.clone();
            for p in out.pixels_mut() {
                for c in 0..3 {
                    p[c] = (255 - p[c]).saturating_add(a.contrast as u8);
                }
            }
            out
        };
        let image = RgbaImage::from_fn(300, 200, |x, y| {
            Rgba([(x * 7 % 256) as u8, (y * 5 % 256) as u8, ((x + y) % 256) as u8, 255])
        });
        let adjusts = [
            LocalAdjust { contrast: 10, ..ellipse(CropRect::new(-100, 50, 400, 300), 40) },
            LocalAdjust { contrast: 20, ..ellipse(CropRect::new(500, 100, 200, 120), 0) },
            LocalAdjust {
                shape: LocalShape::Band { from: [0, 380], to: [600, 0] },
                contrast: 5,
                ..LocalAdjust::default()
            },
            // 画像の外の円は何もしない
            LocalAdjust { contrast: 30, ..ellipse(CropRect::new(2000, 2000, 50, 50), 50) },
        ];
        for (scale, offset) in
            [((1.0, 1.0), (0.0, 0.0)), ((0.5, 0.5), (0.0, 0.0)), ((0.37, 0.41), (12.5, 7.0))]
        {
            let fast = apply_local(image.clone(), &adjusts, scale, offset, invert);
            assert_eq!(
                fast,
                reference(image.clone(), &adjusts, scale, offset, invert),
                "{scale:?} {offset:?}"
            );
        }
    }

    #[test]
    fn json_shape() {
        let band =
            LocalAdjust { shape: LocalShape::Band { from: [1, 2], to: [3, 4] }, ..LocalAdjust::default() };
        let text = serde_json::to_string(&band).unwrap();
        assert!(text.contains(r#""shape":{"kind":"band","from":[1,2],"to":[3,4]}"#), "{text}");
        assert_eq!(serde_json::from_str::<LocalAdjust>(&text).unwrap(), band);
    }
}
