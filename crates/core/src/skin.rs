//! 肌をなめらかに（旧版にはない）: 顔のまわり（ぼかした楕円）だけに、輪郭を残すなめらかな処理をかける。
//!
//! 顔の枠は Vision で見つける（`faces`）。ここでは枠を受け取って処理する。ぼかした画像との差が小さい（肌の
//! ような）部分ほどぼかした画像を混ぜ、差が大きい輪郭（目・眉・口・髪の境目など）は元のまま残す。半径は顔の
//! 大きさに比例させるので、縮小したプレビューと原寸で同じ見え方になる。

use image::{imageops, RgbaImage};

use crate::blur::gaussian_blur;
use crate::pillow::luma;
use crate::transform::{clamp_crop, CropRect};

/// ぼかしの半径（顔の枠の短辺に対する割合。強さ 0 → 最小、100 → 最大）。
const RADIUS_MIN: f64 = 0.01;
const RADIUS_MAX: f64 = 0.035;
/// この差（輝度）より大きいところは輪郭とみなし、元のまま残す。
const EDGE_THRESHOLD: f64 = 28.0;
/// 楕円の大きさ（顔の枠の幅・高さに対する、半径の割合。額・あごまで覆う）と、縁をぼかす幅（半径に対する割合）。
const ELLIPSE_X: f64 = 0.62;
const ELLIPSE_Y: f64 = 0.78;
const FEATHER: f64 = 0.25;

/// image の faces（image の座標の顔の枠）のまわりを、強さ amount（0〜100）でなめらかにする。
pub fn smooth_skin(image: &mut RgbaImage, faces: &[CropRect], amount: u32) {
    if amount == 0 {
        return;
    }
    let strength = f64::from(amount.min(100)) / 100.0;
    for face in faces {
        let (cx, cy) = (face.x as f64 + face.width as f64 / 2.0, face.y as f64 + face.height as f64 / 2.0);
        let (rx, ry) = (face.width as f64 * ELLIPSE_X, face.height as f64 * ELLIPSE_Y);
        if rx < 1.0 || ry < 1.0 {
            continue;
        }
        // 楕円を囲む範囲だけを処理する
        let bounds = CropRect::new(
            (cx - rx).floor() as i64,
            (cy - ry).floor() as i64,
            (2.0 * rx).ceil() as i64 + 1,
            (2.0 * ry).ceil() as i64 + 1,
        );
        let Some(area) = clamp_crop(bounds, image.dimensions()) else { continue };
        let region =
            imageops::crop_imm(image, area.x as u32, area.y as u32, area.width as u32, area.height as u32)
                .to_image();
        let radius = face.width.min(face.height) as f64 * (RADIUS_MIN + (RADIUS_MAX - RADIUS_MIN) * strength);
        let smooth = gaussian_blur(&region, radius.max(0.5) as f32);
        let mut out = region.clone();
        for (x, y, p) in out.enumerate_pixels_mut() {
            // 楕円の中は 1、縁に向かって 0 へ
            let dx = (area.x as f64 + f64::from(x) + 0.5 - cx) / rx;
            let dy = (area.y as f64 + f64::from(y) + 0.5 - cy) / ry;
            let distance = dx.hypot(dy);
            let inside = ((1.0 - distance) / FEATHER).clamp(0.0, 1.0);
            if inside <= 0.0 {
                continue;
            }
            let s = smooth.get_pixel(x, y);
            let diff = f64::from(luma(p[0].abs_diff(s[0]), p[1].abs_diff(s[1]), p[2].abs_diff(s[2])));
            let t = (diff / EDGE_THRESHOLD).clamp(0.0, 1.0);
            let weight = strength * inside * (1.0 - t * t * (3.0 - 2.0 * t));
            for c in 0..3 {
                p[c] = (f64::from(p[c]) + (f64::from(s[c]) - f64::from(p[c])) * weight).round() as u8;
            }
        }
        imageops::replace(image, &out, area.x, area.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// 肌色に細かいざらつき（±8）、中央に黒い線（輪郭）のある画像。
    fn face_like() -> RgbaImage {
        RgbaImage::from_fn(200, 200, |x, y| {
            if (95..105).contains(&x) && (80..120).contains(&y) {
                return Rgba([20, 20, 20, 255]);
            }
            let noise = if (x * 7 + y * 13) % 5 < 2 { 8 } else { 0 };
            Rgba([210 - noise, 160 - noise, 140 - noise, 255])
        })
    }

    fn roughness(image: &RgbaImage, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>) -> u32 {
        ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
            .map(|(x, y)| u32::from(image.get_pixel(x, y)[0].abs_diff(image.get_pixel(x + 1, y)[0])))
            .sum()
    }

    #[test]
    fn smooths_only_around_the_face_and_keeps_edges() {
        let original = face_like();
        let mut image = original.clone();
        let face = CropRect::new(50, 50, 100, 100);
        smooth_skin(&mut image, &[face], 80);
        // 顔のあたり（線から離れたところ）のざらつきが減る
        assert!(roughness(&image, 60..90, 60..90) * 2 < roughness(&original, 60..90, 60..90));
        // 顔から離れたところ（角）は変わらない
        assert_eq!(image.get_pixel(2, 2), original.get_pixel(2, 2));
        assert_eq!(image.get_pixel(197, 197), original.get_pixel(197, 197));
        // 輪郭（黒い線）は残る
        assert!(image.get_pixel(100, 100)[0] < 40, "{:?}", image.get_pixel(100, 100));
        // 0 なら何もしない
        let mut untouched = original.clone();
        smooth_skin(&mut untouched, &[face], 0);
        assert_eq!(untouched, original);
    }

    #[test]
    fn stronger_is_smoother_and_alpha_is_kept() {
        let original = face_like();
        let at = |amount: u32| {
            let mut image = original.clone();
            smooth_skin(&mut image, &[CropRect::new(50, 50, 100, 100)], amount);
            roughness(&image, 60..90, 60..90)
        };
        assert!(at(100) <= at(20));
        let mut translucent = RgbaImage::from_pixel(40, 40, Rgba([200, 150, 130, 77]));
        smooth_skin(&mut translucent, &[CropRect::new(5, 5, 30, 30)], 100);
        assert!(translucent.pixels().all(|p| p[3] == 77));
    }
}
