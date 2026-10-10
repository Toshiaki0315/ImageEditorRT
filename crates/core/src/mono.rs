//! 白黒の色の混ぜ方（旧版にはない）: 写真を白黒にするとき、色ごと（赤・オレンジ・黄・緑・水色・青・紫・
//! マゼンタ）にどれだけ明るく写すかを決める（空を暗く・肌を明るく、など）。
//!
//! 灰色の明るさ（Rec. 709 の輝度）を、画素の色相に近い 2 色の値で明るく・暗くする。鮮やかな画素ほど強く効かせ、
//! 灰色に近い画素はほとんど変えない。色ごとの調整（`curve::apply_hsl`）の後にかける。

use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::curve::{band_weights, rgb_to_hsl, HSL_BANDS};
use crate::PIXELS_PER_TASK;

/// 白黒の設定。enabled でなければかけない。mix は色ごとの明るさ -100〜100。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MonoMix {
    pub enabled: bool,
    pub mix: [i32; HSL_BANDS],
}

impl MonoMix {
    /// 白黒にしないか。
    pub fn is_off(&self) -> bool {
        !self.enabled
    }
}

/// 画像を白黒にする（透過はそのまま）。
pub fn apply_mono(image: &mut RgbaImage, settings: &MonoMix) {
    if !settings.enabled {
        return;
    }
    let mix = settings.mix.map(|v| f64::from(v.clamp(-100, 100)) / 100.0);
    image.as_mut().par_chunks_exact_mut(4).with_min_len(PIXELS_PER_TASK).for_each(|p| {
        let gray = (0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])) / 255.0;
        let (h, s, _) = rgb_to_hsl(p[0], p[1], p[2]);
        let mut amount = 0.0;
        if s > 0.0 {
            for (band, weight) in band_weights(h) {
                amount += weight * mix[band];
            }
            amount *= s;
        }
        let value = if amount >= 0.0 { gray + (1.0 - gray) * amount } else { gray + gray * amount };
        let v = (value * 255.0).round().clamp(0.0, 255.0) as u8;
        p[0] = v;
        p[1] = v;
        p[2] = v;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn colors() -> RgbaImage {
        // 赤・緑・青・灰色
        let list = [[220, 40, 40], [40, 200, 60], [40, 70, 220], [128, 128, 128]];
        RgbaImage::from_fn(4, 1, |x, _| {
            let [r, g, b] = list[x as usize];
            Rgba([r, g, b, 255])
        })
    }

    #[test]
    fn off_changes_nothing_and_on_makes_gray() {
        let mut image = colors();
        apply_mono(&mut image, &MonoMix::default());
        assert_eq!(image, colors());
        apply_mono(&mut image, &MonoMix { enabled: true, ..MonoMix::default() });
        for p in image.pixels() {
            assert!(p[0] == p[1] && p[1] == p[2] && p[3] == 255, "{p:?}");
        }
        assert_eq!(image.get_pixel(3, 0)[0], 128, "灰色はそのまま");
    }

    #[test]
    fn each_color_gets_brighter_or_darker() {
        let plain = {
            let mut image = colors();
            apply_mono(&mut image, &MonoMix { enabled: true, ..MonoMix::default() });
            image
        };
        let mut mix = [0; HSL_BANDS];
        mix[0] = 80; // 赤を明るく
        mix[5] = -80; // 青を暗く
        let mut image = colors();
        apply_mono(&mut image, &MonoMix { enabled: true, mix });
        assert!(image.get_pixel(0, 0)[0] > plain.get_pixel(0, 0)[0] + 40, "赤は明るく");
        assert!(image.get_pixel(2, 0)[0] + 10 < plain.get_pixel(2, 0)[0], "青は暗く");
        assert_eq!(image.get_pixel(1, 0), plain.get_pixel(1, 0), "緑はそのまま");
        assert_eq!(image.get_pixel(3, 0), plain.get_pixel(3, 0), "灰色はそのまま");
    }

    #[test]
    fn json_round_trip() {
        let settings = MonoMix { enabled: true, mix: [10, 0, -20, 0, 0, 30, 0, 0] };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<MonoMix>(&json).unwrap(), settings);
    }
}
