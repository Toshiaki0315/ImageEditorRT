//! 被写体だけ・背景だけに補正（旧版にはない）: 「背景を消す」と同じ被写体のマスクで、被写体と背景に別の
//! 露出・コントラスト・色温度・彩度をかける。
//!
//! マスクは元の画像（回転・反転する前）の大きさに合わせてかける（`prepare` から呼ぶ）。全体の色の調整は、
//! この後の処理の流れでかかる。

use image::{GrayImage, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::adjust::TEMPERATURE_NEUTRAL;
use crate::pipeline::EditSettings;
use crate::PIXELS_PER_TASK;

/// 被写体・背景の一方にかける補正。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MaskedAdjust {
    /// 露出（EV、-5〜5）
    pub exposure: f64,
    /// コントラスト -100〜100
    pub contrast: i32,
    /// 色温度（K。6500 で変えない）
    pub temperature: u32,
    /// 彩度 -100〜100
    pub saturation: i32,
}

impl Default for MaskedAdjust {
    fn default() -> Self {
        Self { exposure: 0.0, contrast: 0, temperature: TEMPERATURE_NEUTRAL, saturation: 0 }
    }
}

impl MaskedAdjust {
    /// 何も変えないか。
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    /// 同じ補正を全体にかける設定。
    fn as_settings(&self) -> EditSettings {
        EditSettings {
            exposure: self.exposure,
            contrast: self.contrast,
            temperature: self.temperature,
            saturation: self.saturation,
            ..EditSettings::default()
        }
    }
}

/// image に、mask（被写体 255・背景 0。大きさが違えば合わせる）で被写体と背景に別の補正をかけた画像を返す。
pub fn apply_masked(
    image: &RgbaImage,
    mask: &GrayImage,
    subject: &MaskedAdjust,
    background: &MaskedAdjust,
) -> RgbaImage {
    if subject.is_neutral() && background.is_neutral() {
        return image.clone();
    }
    let fitted = crate::background::fit_mask(mask, image.dimensions());
    let adjusted = |adjust: &MaskedAdjust| {
        (!adjust.is_neutral())
            .then(|| crate::pipeline::basic_adjustments(image.clone(), &adjust.as_settings()))
    };
    let (front, back) = (adjusted(subject), adjusted(background));
    let mut out = image.clone();
    out.as_mut().par_chunks_exact_mut(4).enumerate().with_min_len(PIXELS_PER_TASK).for_each(|(i, p)| {
        let m = u32::from(fitted[i]);
        let original = [p[0], p[1], p[2]];
        let pick = |side: &Option<RgbaImage>, c: usize| -> u32 {
            side.as_ref().map_or(u32::from(original[c]), |s| u32::from(s.as_raw()[i * 4 + c]))
        };
        for (c, value) in p[..3].iter_mut().enumerate() {
            let v = pick(&front, c) * m + pick(&back, c) * (255 - m);
            *value = ((v + 127) / 255) as u8;
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgba};

    #[test]
    fn subject_and_background_are_adjusted_separately() {
        let image = RgbaImage::from_pixel(20, 10, Rgba([100, 100, 100, 255]));
        // 左半分が被写体。マスクは小さくてもよい（画像の大きさに合わせる）
        let mask = GrayImage::from_fn(4, 2, |x, _| Luma([if x < 2 { 255 } else { 0 }]));
        let brighter = MaskedAdjust { exposure: 1.0, ..MaskedAdjust::default() };
        let out = apply_masked(&image, &mask, &brighter, &MaskedAdjust::default());
        assert!(out.get_pixel(1, 5)[0] > 120, "被写体は明るく: {:?}", out.get_pixel(1, 5));
        assert_eq!(out.get_pixel(18, 5).0, [100, 100, 100, 255], "背景はそのまま");
        // 背景だけ暗く・色を落とす
        let colorful = RgbaImage::from_pixel(20, 10, Rgba([200, 80, 40, 255]));
        let dull = MaskedAdjust { exposure: -1.0, saturation: -100, ..MaskedAdjust::default() };
        let out = apply_masked(&colorful, &mask, &MaskedAdjust::default(), &dull);
        assert_eq!(out.get_pixel(1, 5).0, [200, 80, 40, 255]);
        let p = out.get_pixel(18, 5).0;
        assert!(p[0].abs_diff(p[2]) <= 2 && p[0] < 120, "背景は灰色で暗く: {p:?}");
        // どちらも変えなければそのまま
        assert_eq!(apply_masked(&image, &mask, &MaskedAdjust::default(), &MaskedAdjust::default()), image);
    }

    #[test]
    fn transparency_is_kept() {
        let image = RgbaImage::from_pixel(4, 4, Rgba([100, 100, 100, 50]));
        let mask = GrayImage::from_pixel(4, 4, Luma([255]));
        let out = apply_masked(
            &image,
            &mask,
            &MaskedAdjust { contrast: 50, ..MaskedAdjust::default() },
            &MaskedAdjust::default(),
        );
        assert_eq!(out.get_pixel(2, 2)[3], 50);
    }
}
