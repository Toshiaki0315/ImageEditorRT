//! 試作用のプレビュー処理（Python 版の加工のうち、重いものをひととおり同じ順でかける）。
//!
//! 処理順（Python 版と同じ）: 露出 → 明るさ → コントラスト → 色温度 → 彩度 →
//! ディテール（ノイズ除去 → ぼかし → シャープ） → ジオラマ → フィルター（HDR 風） →
//! 周辺減光 → 経年劣化 → 文字

use std::path::Path;

use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::adjust::{self, Lut};
use crate::blur::{gaussian_blur, unsharp_mask};
use crate::text;

const SHARPEN_RADIUS_RATIO: f32 = 0.0012;
const SHARPEN_MAX_PERCENT: u32 = 250;
const BLUR_MAX_RADIUS_RATIO: f32 = 0.01;
const DENOISE_RADIUS_RATIO: f32 = 0.002;
const DENOISE_EDGE_THRESHOLD: f64 = 40.0;
const DIORAMA_MAX_RADIUS_RATIO: f32 = 0.02;
const DIORAMA_TRANSITION: f64 = 0.25;
const HDR_RADIUS_RATIO: f32 = 0.02;

/// 試作で使う設定（JSON では camelCase）。
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub exposure: f64,
    pub brightness: i32,
    pub contrast: i32,
    pub temperature: u32,
    pub saturation: i32,
    pub denoise: u32,
    pub blur: u32,
    pub sharpen: u32,
    pub diorama_blur: u32,
    pub diorama_position: u32,
    pub diorama_width: u32,
    pub diorama_vivid: u32,
    pub hdr: bool,
    pub vignette: u32,
    pub aging: u32,
    pub text: String,
    pub text_size: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            brightness: 0,
            contrast: 0,
            temperature: adjust::TEMPERATURE_NEUTRAL,
            saturation: 0,
            denoise: 0,
            blur: 0,
            sharpen: 0,
            diorama_blur: 0,
            diorama_position: 50,
            diorama_width: 20,
            diorama_vivid: 30,
            hdr: false,
            vignette: 0,
            aging: 0,
            text: String::new(),
            text_size: 5.0,
        }
    }
}

impl Settings {
    /// Python 版のベンチマークの「重い設定」に合わせたもの（ほぼすべての効果をかける）。
    pub fn heavy() -> Self {
        Self {
            exposure: 0.5,
            brightness: 10,
            contrast: 20,
            temperature: 5000,
            saturation: 20,
            denoise: 50,
            blur: 10,
            sharpen: 50,
            diorama_blur: 80,
            hdr: true,
            vignette: 50,
            aging: 30,
            text: "© 2026 写真".into(),
            ..Self::default()
        }
    }
}

/// 設定をかけた新しい画像を返す（入力画像は変更しない）。
pub fn render(image: &RgbaImage, settings: &Settings) -> RgbaImage {
    let mut out = image.clone();
    let reference = out.width().min(out.height()) as f32;

    // 露出〜色温度は 1 つの表にまとめて 1 回でかける
    let mut lut: Lut = adjust::identity_lut();
    if settings.exposure != 0.0 {
        lut = adjust::compose(&lut, &adjust::exposure_lut(settings.exposure));
    }
    if settings.brightness != 0 {
        lut = adjust::compose(&lut, &adjust::brightness_lut(settings.brightness));
    }
    if settings.contrast != 0 {
        lut = adjust::compose(&lut, &adjust::contrast_lut(settings.contrast));
    }
    if settings.temperature != adjust::TEMPERATURE_NEUTRAL {
        lut = adjust::compose(&lut, &adjust::temperature_lut(settings.temperature));
    }
    if lut != adjust::identity_lut() {
        adjust::apply_lut(&mut out, &lut);
    }
    if settings.saturation != 0 {
        adjust::saturation(&mut out, settings.saturation);
    }

    if settings.denoise > 0 {
        out = denoise(&out, settings.denoise, reference);
    }
    if settings.blur > 0 {
        let sigma = reference * BLUR_MAX_RADIUS_RATIO * settings.blur as f32 / 100.0;
        out = gaussian_blur(&out, sigma);
    }
    if settings.sharpen > 0 {
        let sigma = (reference * SHARPEN_RADIUS_RATIO).max(1.0);
        out = unsharp_mask(&out, sigma, SHARPEN_MAX_PERCENT * settings.sharpen / 100, 2);
    }
    if settings.diorama_blur > 0 {
        out = diorama(&out, settings, reference);
    }
    if settings.hdr {
        out = unsharp_mask(&out, (reference * HDR_RADIUS_RATIO).max(1.0), 90, 0);
        adjust::apply_lut(&mut out, &adjust::curve_lut(|x| x.powf(0.85)));
        adjust::enhance_color(&mut out, 1.15);
    }
    adjust::vignette(&mut out, settings.vignette, None);
    adjust::aging(&mut out, settings.aging);
    if !settings.text.trim().is_empty() {
        if let Some(font) = text::load_font(Path::new(text::HIRAGINO_W3), 0) {
            text::draw_text_bottom_right(
                &mut out,
                font,
                &settings.text,
                settings.text_size,
                [255, 255, 255],
                0.8,
            );
        }
    }
    out
}

/// 計測用の画像（なめらかな部分と細かい模様のある、写真に近い画像）を作る。
pub fn synthetic_photo(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_par_fn(width, height, |x, y| {
        let fx = x as f32 / width as f32;
        let fy = y as f32 / height as f32;
        let fine = if (x / 3 + y / 5) % 2 == 0 { 25 } else { 0 };
        image::Rgba([
            (fx * 200.0) as u8 + fine,
            (fy * 180.0) as u8 + 30,
            ((1.0 - fx) * 150.0) as u8 + fine,
            255,
        ])
    })
}

/// 輪郭を残してざらつきをなめらかにする（Python 版の effects.denoise と同じ考え方）。
fn denoise(image: &RgbaImage, amount: u32, reference: f32) -> RgbaImage {
    let smooth = gaussian_blur(image, reference * DENOISE_RADIUS_RATIO);
    let strength = f64::from(amount) / 100.0;
    let weights: [u16; 256] = std::array::from_fn(|v| {
        let t = (v as f64 / DENOISE_EDGE_THRESHOLD).clamp(0.0, 1.0);
        (255.0 * strength * (1.0 - adjust::smoothstep(t))).round() as u16
    });
    let mut out = image.clone();
    out.as_mut().par_chunks_exact_mut(4).zip(smooth.as_raw().par_chunks_exact(4)).for_each(|(p, s)| {
        let d: [u32; 3] = std::array::from_fn(|c| u32::from(p[c].abs_diff(s[c])));
        let luma = ((d[0] * 299 + d[1] * 587 + d[2] * 114) / 1000) as usize;
        let w = weights[luma];
        for c in 0..3 {
            let mixed = u16::from(s[c]) * w + u16::from(p[c]) * (255 - w);
            p[c] = ((mixed + 127) / 255) as u8;
        }
    });
    out
}

/// ジオラマ風（横の帯）。帯の中はそのまま、外に向かってなめらかにぼかし、色を少し鮮やかにする。
fn diorama(image: &RgbaImage, settings: &Settings, reference: f32) -> RgbaImage {
    let sigma = reference * DIORAMA_MAX_RADIUS_RATIO * settings.diorama_blur as f32 / 100.0;
    let blurred = gaussian_blur(image, sigma);
    let height = image.height() as f64;
    let center = f64::from(settings.diorama_position) / 100.0;
    let half = f64::from(settings.diorama_width) / 200.0;
    let stride = image.width() as usize * 4;
    let mut out = image.clone();
    out.as_mut()
        .par_chunks_exact_mut(stride)
        .zip(blurred.as_raw().par_chunks_exact(stride))
        .enumerate()
        .for_each(|(y, (row, soft))| {
            let u = (y as f64 + 0.5) / height;
            let outside = ((u - center).abs() - half).max(0.0);
            let blur = adjust::smoothstep((outside / DIORAMA_TRANSITION).min(1.0));
            let w = (blur * 256.0).round() as u16;
            for (p, s) in row.chunks_exact_mut(4).zip(soft.chunks_exact(4)) {
                for c in 0..3 {
                    p[c] = ((u16::from(s[c]) * w + u16::from(p[c]) * (256 - w)) >> 8) as u8;
                }
            }
        });
    let vivid = f64::from(settings.diorama_vivid) / 100.0;
    if vivid > 0.0 {
        adjust::enhance_color(&mut out, 1.0 + 0.5 * vivid);
        adjust::apply_lut(&mut out, &adjust::curve_lut(|x| adjust::s_curve(x, 0.4 * vivid)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn sample() -> RgbaImage {
        RgbaImage::from_fn(120, 80, |x, y| Rgba([(x * 2) as u8, (y * 3) as u8, ((x + y) % 256) as u8, 255]))
    }

    #[test]
    fn default_settings_change_nothing() {
        let image = sample();
        assert_eq!(render(&image, &Settings::default()), image);
    }

    #[test]
    fn heavy_settings_change_the_image_and_keep_size() {
        let image = sample();
        let out = render(&image, &Settings::heavy());
        assert_eq!(out.dimensions(), image.dimensions());
        assert_ne!(out, image);
    }

    #[test]
    fn settings_from_json_use_defaults() {
        let s: Settings = serde_json::from_str(r#"{"exposure": 1.5, "dioramaBlur": 40}"#).unwrap();
        assert_eq!(s.exposure, 1.5);
        assert_eq!(s.diorama_blur, 40);
        assert_eq!(s.temperature, 6500);
        assert_eq!(s.diorama_position, 50);
    }

    #[test]
    fn diorama_keeps_band_sharp() {
        let mut image = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 255]));
        for y in 0..100 {
            for x in (0..100).step_by(4) {
                image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let settings = Settings { diorama_blur: 100, diorama_vivid: 0, ..Settings::default() };
        let out = diorama(&image, &settings, 100.0);
        assert_eq!(out.get_pixel(0, 50), image.get_pixel(0, 50)); // 帯の中
        assert!(out.get_pixel(0, 0)[0] < 200); // 帯から遠い上端はぼける
    }
}
