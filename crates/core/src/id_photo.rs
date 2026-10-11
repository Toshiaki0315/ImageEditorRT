//! 証明写真（旧版にはない）: 見つけた顔に合わせて規定の大きさに切り抜き（足りない所は白で埋める）、
//! 背景を白にして、印刷用に L 判（89 × 127mm）へ並べた画像も作る。
//!
//! 大きさは 300dpi の画素にする。顔の枠（Vision。眉のあたり〜あご）から頭の高さ（髪の上〜あご）を見積もり、
//! 写真の高さに対する頭の高さの割合・上の余白を、大きさごとの決まりに合わせる。

use image::{imageops, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::transform::CropRect;

/// 作った証明写真の表示名。
pub const ID_PHOTO_NAME: &str = "証明写真";
/// 印刷の解像度（dpi）。
pub const DPI: f64 = 300.0;
/// 顔の枠の高さに対する、頭全体（髪の上〜あご）の高さ。
const HEAD_PER_FACE: f64 = 1.45;
/// 顔の枠の上から、頭の上（髪の上）までの高さ（顔の枠の高さに対する割合）。
const HAIR_PER_FACE: f64 = 0.45;
/// L 判（mm）と、並べるときの間の幅（mm）。
const SHEET_MM: (f64, f64) = (89.0, 127.0);
const GAP_MM: f64 = 3.0;
/// 切り取り線の色。
const CUT_LINE: Rgba<u8> = Rgba([200, 200, 200, 255]);
const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);

/// 証明写真の大きさ。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdSize {
    /// 30 × 40mm（履歴書・各種申請）
    Resume,
    /// 35 × 45mm（パスポート・マイナンバーカード）
    Passport,
    /// 24 × 30mm（運転免許証）
    License,
}

impl IdSize {
    pub const ALL: [IdSize; 3] = [IdSize::Resume, IdSize::Passport, IdSize::License];

    /// 画面に出す名前。
    pub fn label(self) -> &'static str {
        match self {
            IdSize::Resume => "30 × 40mm（履歴書など）",
            IdSize::Passport => "35 × 45mm（パスポート・マイナンバーカード）",
            IdSize::License => "24 × 30mm（運転免許証）",
        }
    }

    /// 幅・高さ（mm）。
    pub fn millimeters(self) -> (f64, f64) {
        match self {
            IdSize::Resume => (30.0, 40.0),
            IdSize::Passport => (35.0, 45.0),
            IdSize::License => (24.0, 30.0),
        }
    }

    /// 写真の高さに対する、頭（髪の上〜あご）の高さと、上の余白の割合。
    fn layout(self) -> (f64, f64) {
        match self {
            // パスポートは 45mm の中で頭が 32〜36mm、上の余白が 2〜6mm
            IdSize::Passport => (34.0 / 45.0, 4.0 / 45.0),
            IdSize::Resume => (0.70, 0.10),
            IdSize::License => (0.70, 0.12),
        }
    }

    /// 画素の幅・高さ（300dpi）。
    pub fn pixels(self) -> (u32, u32) {
        let (w, h) = self.millimeters();
        (mm_to_px(w), mm_to_px(h))
    }
}

fn mm_to_px(mm: f64) -> u32 {
    (mm / 25.4 * DPI).round() as u32
}

/// 顔の枠（画像の座標）に合わせた、切り抜く範囲（画像の外にはみ出すこともある。左・上・幅・高さ）。
pub fn crop_for_face(face: CropRect, size: IdSize) -> (f64, f64, f64, f64) {
    let (head_ratio, top_ratio) = size.layout();
    let (w_mm, h_mm) = size.millimeters();
    let head = face.height as f64 * HEAD_PER_FACE;
    let height = head / head_ratio;
    let width = height * w_mm / h_mm;
    let head_top = face.y as f64 - face.height as f64 * HAIR_PER_FACE;
    let top = head_top - height * top_ratio;
    let center = face.x as f64 + face.width as f64 / 2.0;
    (center - width / 2.0, top, width, height)
}

/// image から、顔に合わせて切り抜いた証明写真（300dpi の大きさ）。画像の外は白で埋める。
pub fn render(image: &RgbaImage, face: CropRect, size: IdSize) -> RgbaImage {
    let (left, top, width, height) = crop_for_face(face, size);
    let (out_w, out_h) = size.pixels();
    // 切り抜く範囲を白い台紙に写してから縮める（はみ出した所は白）
    let (cw, ch) = (width.round().max(1.0) as u32, height.round().max(1.0) as u32);
    let mut canvas = RgbaImage::from_pixel(cw, ch, WHITE);
    let mut source = image.clone();
    // 透明な所（背景を消した後など）も白にする
    for p in source.pixels_mut() {
        let a = u32::from(p[3]);
        for c in 0..3 {
            p[c] = ((u32::from(p[c]) * a + 255 * (255 - a) + 127) / 255) as u8;
        }
        p[3] = 255;
    }
    imageops::replace(&mut canvas, &source, -(left.round() as i64), -(top.round() as i64));
    crate::resize::resize(&canvas, out_w, out_h)
}

/// 証明写真を L 判に並べた印刷用の画像（300dpi。まわりに切り取り線）。
pub fn sheet(photo: &RgbaImage) -> RgbaImage {
    let (sw, sh) = (mm_to_px(SHEET_MM.0), mm_to_px(SHEET_MM.1));
    let gap = mm_to_px(GAP_MM);
    let (pw, ph) = photo.dimensions();
    let cols = ((sw - gap) / (pw + gap)).max(1);
    let rows = ((sh - gap) / (ph + gap)).max(1);
    let used = (cols * pw + (cols - 1) * gap, rows * ph + (rows - 1) * gap);
    let origin = ((sw.saturating_sub(used.0)) / 2, (sh.saturating_sub(used.1)) / 2);
    let mut out = RgbaImage::from_pixel(sw, sh, WHITE);
    for row in 0..rows {
        for col in 0..cols {
            let x = origin.0 + col * (pw + gap);
            let y = origin.1 + row * (ph + gap);
            // 切り取り線（写真のまわり 1px 外側）
            for (lx, ly, lw, lh) in [
                (x.saturating_sub(1), y.saturating_sub(1), pw + 2, 1),
                (x.saturating_sub(1), y + ph, pw + 2, 1),
                (x.saturating_sub(1), y, 1, ph),
                (x + pw, y, 1, ph),
            ] {
                for yy in ly..(ly + lh).min(sh) {
                    for xx in lx..(lx + lw).min(sw) {
                        out.put_pixel(xx, yy, CUT_LINE);
                    }
                }
            }
            imageops::replace(&mut out, photo, i64::from(x), i64::from(y));
        }
    }
    out
}

/// いちばん大きい顔（証明写真の人）。
pub fn main_face(faces: &[CropRect]) -> Option<CropRect> {
    faces.iter().copied().max_by_key(|r| r.width * r.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_300dpi() {
        assert_eq!(IdSize::Passport.pixels(), (413, 531));
        assert_eq!(IdSize::Resume.pixels(), (354, 472));
        assert_eq!(IdSize::License.pixels(), (283, 354));
    }

    #[test]
    fn head_fills_the_right_share() {
        // 顔の枠 100 × 120px。頭は 174px、パスポートでは写真の高さの 34/45
        let face = CropRect::new(200, 300, 100, 120);
        let (left, top, width, height) = crop_for_face(face, IdSize::Passport);
        assert!((height - 174.0 / (34.0 / 45.0)).abs() < 0.01);
        assert!((width / height - 35.0 / 45.0).abs() < 1e-9);
        assert!((left + width / 2.0 - 250.0).abs() < 1e-9, "顔が左右の真ん中");
        let head_top = 300.0 - 120.0 * 0.45;
        assert!(((head_top - top) / height - 4.0 / 45.0).abs() < 1e-9, "上の余白");
    }

    #[test]
    fn render_fills_outside_with_white() {
        // 顔が画像の上の端に近く、切り抜く範囲が上にはみ出す
        let image = RgbaImage::from_pixel(400, 400, Rgba([100, 120, 140, 255]));
        let face = CropRect::new(150, 10, 100, 120);
        let photo = render(&image, face, IdSize::Resume);
        assert_eq!(photo.dimensions(), IdSize::Resume.pixels());
        assert_eq!(photo.get_pixel(photo.width() / 2, 1).0, [255, 255, 255, 255], "上のはみ出しは白");
        let mid = photo.get_pixel(photo.width() / 2, photo.height() / 2).0;
        assert!(mid[0].abs_diff(100) <= 2, "{mid:?}");
    }

    #[test]
    fn sheet_tiles_photos_on_l_size() {
        let photo = RgbaImage::from_pixel(
            IdSize::Passport.pixels().0,
            IdSize::Passport.pixels().1,
            Rgba([10, 20, 30, 255]),
        );
        let out = sheet(&photo);
        assert_eq!(out.dimensions(), (1051, 1500));
        // L 判には 35 × 45mm が 2 × 2 枚
        let count = out.pixels().filter(|p| p.0 == [10, 20, 30, 255]).count() as u32;
        assert_eq!(count, 4 * 413 * 531);
    }

    #[test]
    fn biggest_face_is_chosen() {
        let faces = [CropRect::new(0, 0, 10, 10), CropRect::new(50, 50, 40, 40), CropRect::new(5, 5, 20, 20)];
        assert_eq!(main_face(&faces), Some(CropRect::new(50, 50, 40, 40)));
        assert_eq!(main_face(&[]), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn id_photo_from_a_portrait() {
        // NASA のポートレート（tests/fixtures/face.jpg）: 顔を見つけ、背景を白にして証明写真にする
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/face.jpg");
        let image = crate::decode::decode(&std::fs::read(path).unwrap()).unwrap();
        let face = main_face(&crate::faces::detect_faces(&image).unwrap()).expect("顔がある");
        let mask = crate::foreground::foreground_mask(&image).unwrap().expect("被写体がある");
        let white = crate::background::apply_background(
            &image,
            &mask,
            crate::background::Background::White,
            crate::background::BLUR_DEFAULT,
        );
        let photo = render(&white, face, IdSize::Passport);
        assert_eq!(photo.dimensions(), IdSize::Passport.pixels());
        assert_eq!(photo.get_pixel(5, 5).0, [255, 255, 255, 255], "左上の背景は白");
        // 顔は真ん中あたり（上下の真ん中より少し上）にある: 真ん中の画素は白くない
        let center = photo.get_pixel(photo.width() / 2, photo.height() * 2 / 5).0;
        assert!(center[..3].iter().any(|&v| v < 230), "{center:?}");
    }
}
