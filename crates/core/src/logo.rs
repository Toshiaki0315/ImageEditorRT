//! ロゴの透かし（旧版にはない）: 画像ファイル（PNG など。透過はそのまま）を、文字と同じ位置の決め方で写真に重ねる。
//!
//! 画像はファイルの場所で覚える（プリセット・まとめて処理でも使う）。読めなければ描かない。一度読んだ画像は、
//! ファイルの更新日時が変わるまで覚えておく。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use image::{imageops, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::text::{TextPosition, TEXT_MARGIN_RATIO};

/// ロゴの大きさの範囲（写真の短辺に対する %。ロゴの長辺をこの大きさにする）。
pub const LOGO_SIZE_MIN: f32 = 2.0;
pub const LOGO_SIZE_MAX: f32 = 60.0;

/// ロゴの設定。path が空ならロゴなし。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LogoSettings {
    /// 画像ファイルの場所
    pub path: String,
    pub position: TextPosition,
    /// ロゴの長辺（写真の短辺に対する %）
    pub size: f32,
    /// 不透明度（%）
    pub opacity: u32,
    /// 位置が「自由」のときの、ロゴの中心（写真の幅・高さに対する割合。旧版にはない）
    #[serde(skip_serializing_if = "crate::text::is_default_point")]
    pub point: [f32; 2],
}

impl Default for LogoSettings {
    fn default() -> Self {
        Self {
            path: String::new(),
            position: TextPosition::BottomRight,
            size: 15.0,
            opacity: 80,
            point: crate::text::FREE_POINT_DEFAULT,
        }
    }
}

impl LogoSettings {
    pub fn is_empty(&self) -> bool {
        self.path.trim().is_empty()
    }
}

/// ロゴの画像を読む（向きは直す）。読めなければ None。同じファイル（場所と更新日時）は読み直さない。
pub fn load_logo(path: &str) -> Option<Arc<RgbaImage>> {
    type Cache = HashMap<(String, Option<SystemTime>), Option<Arc<RgbaImage>>>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let modified = std::fs::metadata(path).ok()?.modified().ok();
    let key = (path.to_string(), modified);
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    cache
        .entry(key)
        .or_insert_with(|| {
            let bytes = std::fs::read(Path::new(path)).ok()?;
            #[cfg(target_os = "macos")]
            let image = crate::decode::decode(&bytes).ok()?;
            #[cfg(not(target_os = "macos"))]
            let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
            Some(Arc::new(image))
        })
        .clone()
}

/// ロゴを描く。area（左, 上, 右, 下）の中に、position にそろえて置く（なければ画像全体）。
/// reference は大きさ・余白の基準にする写真の短辺（なければ area の短辺）。フレームの余白に置くときは余白を空けない。
pub fn draw_logo(
    image: &mut RgbaImage,
    settings: &LogoSettings,
    area: Option<(i64, i64, i64, i64)>,
    reference: Option<f64>,
) {
    if settings.is_empty() || settings.opacity == 0 {
        return;
    }
    let Some(logo) = load_logo(&settings.path) else { return };
    let (left, top, right, bottom) =
        area.unwrap_or((0, 0, i64::from(image.width()), i64::from(image.height())));
    let (width, height) = ((right - left) as f64, (bottom - top) as f64);
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let reference = reference.unwrap_or(width.min(height));
    let margin =
        if settings.position == TextPosition::FrameMargin { 0.0 } else { reference * TEXT_MARGIN_RATIO };
    // ロゴの長辺を写真の短辺の size % にする（置ける場所より大きければ縮める）
    let (lw, lh) = (f64::from(logo.width()), f64::from(logo.height()));
    let wanted = reference * f64::from(settings.size.clamp(LOGO_SIZE_MIN, LOGO_SIZE_MAX)) / 100.0;
    let scale = (wanted / lw.max(lh)).min((width - 2.0 * margin) / lw).min((height - 2.0 * margin) / lh);
    if scale <= 0.0 {
        return;
    }
    let size = (((lw * scale).round() as u32).max(1), ((lh * scale).round() as u32).max(1));
    let mut scaled = crate::transform::resize_to(&logo, size);
    let opacity = settings.opacity.min(100);
    if opacity < 100 {
        for p in scaled.pixels_mut() {
            p[3] = ((u32::from(p[3]) * opacity + 50) / 100) as u8;
        }
    }
    let (ax, ay) = if settings.position == TextPosition::Free {
        let free =
            |point, length, content: u32| crate::text::free_anchor(point, length, margin, f64::from(content));
        (free(settings.point[0], width, size.0), free(settings.point[1], height, size.1))
    } else {
        settings.position.anchor()
    };
    let x = left as f64 + margin + (width - 2.0 * margin - f64::from(size.0)) * ax;
    let y = top as f64 + margin + (height - 2.0 * margin - f64::from(size.1)) * ay;
    imageops::overlay(image, &scaled, x.round() as i64, y.round() as i64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// 赤い四角（まわりは透明）のロゴを書き出して、その場所を返す。
    fn logo_file(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-logo-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let logo = RgbaImage::from_fn(40, 20, |x, _| {
            if (10..30).contains(&x) {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        let path = dir.join("logo.png");
        logo.save(&path).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn red_box(image: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
        let red: Vec<(u32, u32)> = image
            .enumerate_pixels()
            .filter(|(_, _, p)| p[0] > 200 && p[1] < 60)
            .map(|(x, y, _)| (x, y))
            .collect();
        let (xs, ys): (Vec<u32>, Vec<u32>) = red.into_iter().unzip();
        Some((*xs.iter().min()?, *ys.iter().min()?, *xs.iter().max()?, *ys.iter().max()?))
    }

    #[test]
    fn free_position_puts_the_logo_center_on_the_point() {
        let path = logo_file("free");
        let mut image = RgbaImage::from_pixel(400, 200, Rgba([0, 0, 0, 255]));
        let logo = LogoSettings {
            path,
            position: TextPosition::Free,
            size: 20.0,
            opacity: 100,
            point: [0.75, 0.25],
        };
        draw_logo(&mut image, &logo, None, None);
        // ロゴ（40×20 → 長辺 40px、赤い部分は真ん中の 20px）の中心が (300, 50)
        let (l, t, r, b) = red_box(&image).unwrap();
        assert!(((l + r) / 2).abs_diff(300) <= 2 && ((t + b) / 2).abs_diff(50) <= 2, "{:?}", (l, t, r, b));
    }

    #[test]
    fn logo_is_placed_by_position_and_size() {
        let path = logo_file("place");
        let settings = LogoSettings {
            path,
            position: TextPosition::BottomRight,
            size: 20.0,
            opacity: 100,
            ..LogoSettings::default()
        };
        let mut image = RgbaImage::from_pixel(400, 200, Rgba([0, 0, 255, 255]));
        draw_logo(&mut image, &settings, None, None);
        // 短辺 200 の 20% = 長辺 40px（40 × 20）、余白は 3% = 6px → 右下 (354, 174) から。赤い部分はその中央の 20px
        let (x0, y0, x1, y1) = red_box(&image).unwrap();
        assert!((x0 as i64 - 364).abs() <= 1 && (x1 as i64 - 383).abs() <= 1, "{x0} {x1}");
        assert!((y0 as i64 - 174).abs() <= 1 && (y1 as i64 - 193).abs() <= 1, "{y0} {y1}");
        // 透明な部分は写真のまま
        assert_eq!(image.get_pixel(356, 184).0, [0, 0, 255, 255]);
        // 左上
        let mut top_left = RgbaImage::from_pixel(400, 200, Rgba([0, 0, 255, 255]));
        draw_logo(
            &mut top_left,
            &LogoSettings { position: TextPosition::TopLeft, ..settings.clone() },
            None,
            None,
        );
        let (x0, y0, _, _) = red_box(&top_left).unwrap();
        assert!((x0 as i64 - 16).abs() <= 1 && (y0 as i64 - 6).abs() <= 1, "{x0} {y0}");
    }

    #[test]
    fn opacity_and_missing_files() {
        let path = logo_file("opacity");
        let mut image = RgbaImage::from_pixel(400, 200, Rgba([0, 0, 0, 255]));
        draw_logo(
            &mut image,
            &LogoSettings {
                path,
                position: TextPosition::Center,
                size: 20.0,
                opacity: 50,
                ..LogoSettings::default()
            },
            None,
            None,
        );
        // 黒の上に 50% の赤 → 赤は半分ほど
        let p = image.get_pixel(200, 100);
        assert!((120..=135).contains(&p[0]) && p[2] == 0, "{p:?}");
        // ファイルがない・空ならなにもしない
        let original = RgbaImage::from_pixel(50, 50, Rgba([1, 2, 3, 255]));
        let mut untouched = original.clone();
        draw_logo(
            &mut untouched,
            &LogoSettings { path: "/no/such/logo.png".into(), ..LogoSettings::default() },
            None,
            None,
        );
        draw_logo(&mut untouched, &LogoSettings::default(), None, None);
        assert_eq!(untouched, original);
    }
}
