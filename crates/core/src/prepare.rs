//! 元の画像に前もってかける処理（赤目の補正 → 肌をなめらかに → 背景を消す・ぼかす。旧版にはない）。
//!
//! どちらも Vision で見つけた顔の枠・被写体のマスクが要る。顔の枠・マスクは画像ごとに 1 回、プレビュー用の
//! 画像（回転・反転する前）から作って覚えておき、ここでかける画像（プレビュー用の画像か原寸）の大きさに合わせる。
//! 処理の流れ（`pipeline`）より前に、回転・反転する前の画像にかける。

use image::{GrayImage, RgbaImage};

use crate::background::{self, Background};
use crate::pipeline::EditSettings;
use crate::transform::CropRect;
use crate::{redeye, skin};

/// 前もってかける処理の材料（プレビュー用の画像の大きさ・座標）。
#[derive(Clone, Copy, Debug)]
pub struct Sources<'a> {
    /// 顔の枠（まだ探していなければ None）
    pub faces: Option<&'a [CropRect]>,
    /// 被写体のマスク（まだ作っていない・被写体がなければ None）
    pub mask: Option<&'a GrayImage>,
    /// プレビュー用の画像の幅（顔の枠を、かける画像の大きさに合わせるため）
    pub preview_width: u32,
}

/// settings と材料で、かける処理があるか。
pub fn needed(settings: &EditSettings, sources: &Sources) -> bool {
    let skin = uses_faces(settings) && sources.faces.is_some_and(|f| !f.is_empty());
    let background = active_background(settings) && sources.mask.is_some();
    skin || background
}

/// 背景をかえるか（「画像に置き換える」で画像を選んでいなければかえない）。
fn active_background(settings: &EditSettings) -> bool {
    match settings.background {
        Background::Keep => false,
        Background::Image => !settings.background_image.trim().is_empty(),
        _ => true,
    }
}

/// 顔の枠を使う処理（肌をなめらかに・赤目の補正）があるか。
pub fn uses_faces(settings: &EditSettings) -> bool {
    settings.skin_smooth > 0 || settings.red_eye
}

/// image（回転・反転する前のプレビュー用の画像か原寸）に、赤目 → 肌をなめらかに → 背景の順でかけた画像を返す。
/// かける処理がなければ None（呼んだ側は元の画像をそのまま使う）。
pub fn prepare(image: &RgbaImage, settings: &EditSettings, sources: &Sources) -> Option<RgbaImage> {
    if !needed(settings, sources) {
        return None;
    }
    let mut out = image.clone();
    if let Some(faces) = sources.faces.filter(|_| uses_faces(settings)) {
        let scale = f64::from(image.width()) / f64::from(sources.preview_width.max(1));
        let fitted: Vec<CropRect> = faces.iter().map(|r| r.scaled(scale)).collect();
        if settings.red_eye {
            redeye::fix_red_eyes(&mut out, &fitted);
        }
        if settings.skin_smooth > 0 {
            skin::smooth_skin(&mut out, &fitted, settings.skin_smooth);
        }
    }
    if let Some(mask) = sources.mask.filter(|_| active_background(settings)) {
        if settings.background == Background::Image {
            // 読めない画像なら置き換えない（ロゴと同じく、同じファイルは読み直さない）
            if let Some(backdrop) = crate::logo::load_logo(&settings.background_image) {
                out = background::replace_background(&out, mask, &backdrop);
            }
            return Some(out);
        }
        out = background::apply_background_with(
            &out,
            mask,
            settings.background,
            settings.background_blur,
            settings.background_color,
        );
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgba};

    fn photo(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            let noise = if (x * 7 + y * 13) % 5 < 2 { 10 } else { 0 };
            Rgba([200 - noise, 150 - noise, 130 - noise, 255])
        })
    }

    #[test]
    fn background_image_needs_a_readable_file() {
        let image = photo(40, 40);
        let mask = GrayImage::from_fn(40, 40, |x, _| Luma([if x < 20 { 255 } else { 0 }]));
        let sources = Sources { faces: None, mask: Some(&mask), preview_width: 40 };
        // 画像を選んでいなければ何もしない
        let none = EditSettings { background: Background::Image, ..EditSettings::default() };
        assert!(prepare(&image, &none, &sources).is_none());
        // 選んだ画像で背景を置き換える
        let dir = std::env::temp_dir().join(format!("ier-backdrop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("backdrop.png");
        RgbaImage::from_pixel(10, 10, Rgba([0, 0, 255, 255])).save(&path).unwrap();
        let settings = EditSettings {
            background: Background::Image,
            background_image: path.to_string_lossy().into_owned(),
            ..EditSettings::default()
        };
        let out = prepare(&image, &settings, &sources).expect("置き換える");
        assert_eq!(out.get_pixel(35, 5).0, [0, 0, 255, 255]);
        assert_eq!(out.get_pixel(2, 5), image.get_pixel(2, 5));
        // 読めない画像なら置き換えない
        let missing = EditSettings {
            background_image: dir.join("none.png").to_string_lossy().into_owned(),
            ..settings
        };
        assert_eq!(prepare(&image, &missing, &sources), Some(image.clone()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn nothing_to_do_returns_none() {
        let image = photo(40, 40);
        let faces = [CropRect::new(10, 10, 20, 20)];
        let mask = GrayImage::from_pixel(40, 40, Luma([255]));
        let none = Sources { faces: None, mask: None, preview_width: 40 };
        // 設定があっても材料がない・材料があっても設定が 0／そのまま
        let skin = EditSettings { skin_smooth: 50, background: Background::White, ..EditSettings::default() };
        assert!(prepare(&image, &skin, &none).is_none());
        let sources = Sources { faces: Some(&faces), mask: Some(&mask), preview_width: 40 };
        assert!(prepare(&image, &EditSettings::default(), &sources).is_none());
        // 顔がひとつもなければ肌の処理はない
        let empty = Sources { faces: Some(&[]), mask: None, preview_width: 40 };
        assert!(!needed(&skin, &empty));
    }

    #[test]
    fn faces_are_fitted_to_the_image_size() {
        // プレビュー（幅 50）で見つけた顔の枠を、原寸（幅 100）では 2 倍にしてかける
        let original = photo(100, 100);
        let faces = [CropRect::new(10, 10, 30, 30)];
        let settings = EditSettings { skin_smooth: 80, ..EditSettings::default() };
        let out =
            prepare(&original, &settings, &Sources { faces: Some(&faces), mask: None, preview_width: 50 })
                .unwrap();
        let mut expected = original.clone();
        skin::smooth_skin(&mut expected, &[CropRect::new(20, 20, 60, 60)], 80);
        assert_eq!(out, expected);
    }

    #[test]
    fn skin_then_background() {
        let original = photo(60, 60);
        let faces = [CropRect::new(10, 10, 40, 40)];
        let mask = GrayImage::from_fn(60, 60, |x, _| if x < 30 { Luma([255]) } else { Luma([0]) });
        let settings =
            EditSettings { skin_smooth: 60, background: Background::White, ..EditSettings::default() };
        let out = prepare(
            &original,
            &settings,
            &Sources { faces: Some(&faces), mask: Some(&mask), preview_width: 60 },
        )
        .unwrap();
        let mut expected = original.clone();
        skin::smooth_skin(&mut expected, &faces, 60);
        let expected =
            background::apply_background(&expected, &mask, Background::White, settings.background_blur);
        assert_eq!(out, expected);
        assert_eq!(out.get_pixel(55, 30).0, [255, 255, 255, 255]);
    }
}
