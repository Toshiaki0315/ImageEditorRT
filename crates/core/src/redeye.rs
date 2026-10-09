//! 赤目の補正（旧版にはない）。フラッシュで赤く写った瞳を、暗い無彩色に戻す。
//!
//! 顔の枠（Vision で見つけたもの）の上の方、目のあるあたりの帯だけを調べ、赤が緑・青よりずっと強い画素
//! （瞳の赤）の赤を緑・青の平均に近づける。肌・唇の赤は赤と緑・青の差が小さいので変わらない。

use image::RgbaImage;

use crate::transform::CropRect;

/// 目を探す帯（顔の枠の高さに対する、上端からの比率）。
const EYE_BAND: (f64, f64) = (0.15, 0.6);
/// 赤が緑・青の強い方の何倍から直し始め、何倍で直しきるか。
const RATIO_START: f32 = 2.0;
const RATIO_FULL: f32 = 2.8;
/// これより暗い赤は直さない（影の中の色は赤みが不確か）。
const MIN_RED: f32 = 50.0;
/// 緑が青よりこれ以上（赤と青の差に対する比率）強ければ、赤ではなく茶・オレンジとして直さない（髪・眉など）。
const ORANGE_RATIO: f32 = 0.2;

/// image の顔ごとに、目のあたりの赤目を直す（顔の枠は image の座標）。
pub fn fix_red_eyes(image: &mut RgbaImage, faces: &[CropRect]) {
    let (width, height) = (i64::from(image.width()), i64::from(image.height()));
    for face in faces {
        let top = face.y + (face.height as f64 * EYE_BAND.0).round() as i64;
        let bottom = face.y + (face.height as f64 * EYE_BAND.1).round() as i64;
        for y in top.max(0)..bottom.min(height) {
            for x in face.x.max(0)..face.right().min(width) {
                let pixel = image.get_pixel_mut(x as u32, y as u32);
                let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(f32::from);
                let weight = red_eye_weight(r, g, b);
                if weight > 0.0 {
                    let neutral = (g + b) / 2.0;
                    pixel[0] = (r + (neutral - r) * weight).round() as u8;
                }
            }
        }
    }
}

/// 赤目らしさ（0〜1）。赤が緑・青の強い方よりずっと強いほど大きい（茶・オレンジは 0）。
fn red_eye_weight(r: f32, g: f32, b: f32) -> f32 {
    if r < MIN_RED || g - b > (r - b) * ORANGE_RATIO {
        return 0.0;
    }
    let ratio = r / (g.max(b) + 1.0);
    ((ratio - RATIO_START) / (RATIO_FULL - RATIO_START)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    const SKIN: Rgba<u8> = Rgba([215, 165, 140, 255]);
    const RED_EYE: Rgba<u8> = Rgba([200, 30, 35, 255]);

    /// 肌色の顔（枠 0,0,100,100）の、目の高さ（y=35）と口の高さ（y=80）に赤い丸。
    fn face() -> RgbaImage {
        RgbaImage::from_fn(100, 100, |x, y| {
            let near = |cx: i64, cy: i64| (i64::from(x) - cx).pow(2) + (i64::from(y) - cy).pow(2) <= 16;
            if near(30, 35) || near(70, 35) || near(50, 80) {
                RED_EYE
            } else {
                SKIN
            }
        })
    }

    #[test]
    fn red_pupils_become_dark_and_the_rest_stays() {
        let mut image = face();
        fix_red_eyes(&mut image, &[CropRect::new(0, 0, 100, 100)]);
        // 目の赤は暗い無彩色に（緑・青はそのまま）
        for (x, y) in [(30, 35), (70, 35)] {
            let p = image.get_pixel(x, y);
            assert_eq!([p[1], p[2], p[3]], [30, 35, 255]);
            assert!(p[0] <= 40, "瞳の赤が残っている: {p:?}");
        }
        // 目の帯の外（口）・肌は変わらない
        assert_eq!(*image.get_pixel(50, 80), RED_EYE);
        assert_eq!(*image.get_pixel(10, 35), SKIN);
        assert_eq!(*image.get_pixel(50, 10), SKIN);
    }

    #[test]
    fn no_faces_or_outside_the_image_changes_nothing() {
        let mut image = face();
        fix_red_eyes(&mut image, &[]);
        assert_eq!(image, face());
        // 画像からはみ出した枠でも落ちない（目の帯は y = -5〜40。目だけ直り、口はそのまま）
        fix_red_eyes(&mut image, &[CropRect::new(-50, -20, 200, 100)]);
        assert!(image.get_pixel(30, 35)[0] <= 40);
        assert_eq!(*image.get_pixel(50, 80), RED_EYE);
    }

    #[test]
    fn weight_grows_with_redness() {
        assert_eq!(red_eye_weight(215.0, 165.0, 140.0), 0.0); // 肌
        assert_eq!(red_eye_weight(40.0, 5.0, 5.0), 0.0); // 暗い
        assert_eq!(red_eye_weight(90.0, 42.0, 19.0), 0.0); // 茶色（髪・眉）
        assert!(red_eye_weight(200.0, 80.0, 75.0) > 0.0 && red_eye_weight(200.0, 80.0, 75.0) < 1.0);
        assert_eq!(red_eye_weight(200.0, 30.0, 35.0), 1.0);
    }
}
