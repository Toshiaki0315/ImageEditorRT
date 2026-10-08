//! 文字の自動認識（投稿加工。旧版にはない）。macOS の Vision で文字の並んでいる範囲を見つける（端末の中だけで処理する）。
//!
//! 表札・ナンバープレート・名札・書類などを隠すため。Vision の文字の認識（日本語・英語）で、文字として読めた
//! 範囲だけを使う（文字の並びを探すだけの認識は、階段の縞や服の模様も文字と間違えたため）。顔と同じく、
//! 全体と分けた部分ごとに探す（macOS 27 では全体 1 回で小さなナンバープレートも見つけたが、macOS 14 では
//! 見つけなかったため）。ほぼ同じ範囲は 1 つにまとめる。

use image::RgbaImage;
use objc2_foundation::{NSArray, NSString};
use objc2_vision::{VNRecognizeTextRequest, VNRequestTextRecognitionLevel};

use crate::transform::{clamp_crop, CropRect};
use crate::vision::{perform, scan_tiles, to_image_rect};

/// 文字の範囲を広げる幅（文字の高さに対する割合。上下左右に）。文字の端まで確実に隠すため。
const PADDING: f64 = 0.3;
/// 文字として使う確からしさの下限（0〜1）。
const MIN_CONFIDENCE: f32 = 0.5;
/// 文字として使う、文字・数字の数の下限（記号や縞を 1 文字と読んだものを除く）。
const MIN_CHARACTERS: usize = 2;
/// 2 つの範囲の重なりが、小さいほうのこの割合を超えたら同じ文字とみなしてまとめる。
const SAME_OVERLAP: f64 = 0.5;

/// 画像の中の文字の範囲を見つけ、隠す範囲（少し広げて、重なるものをまとめた枠。画像の座標、px）を
/// 上から順（同じ高さなら左から）に返す。見つからなければ空。
pub fn detect_text(image: &RgbaImage) -> Result<Vec<CropRect>, String> {
    let size = image.dimensions();
    let found = scan_tiles(image, detect_image)?;
    let padded = merge(found).into_iter().filter_map(|r| pad(r, size)).collect();
    Ok(merge(padded))
}

/// 文字の範囲を、文字の高さ（短いほうの辺）の 30% ずつ広げる（画像からはみ出す部分は切る）。
fn pad(rect: CropRect, size: (u32, u32)) -> Option<CropRect> {
    let margin = (rect.short_side() as f64 * PADDING).round() as i64;
    clamp_crop(
        CropRect::new(rect.x - margin, rect.y - margin, rect.width + 2 * margin, rect.height + 2 * margin),
        size,
    )
}

/// ほぼ同じ範囲（重なりが小さいほうの半分を超える）を、それらを囲む 1 つの範囲にまとめる。
fn merge(mut rects: Vec<CropRect>) -> Vec<CropRect> {
    let same = |a: &CropRect, b: &CropRect| {
        let w = (a.right().min(b.right()) - a.x.max(b.x)).max(0);
        let h = (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0);
        let smaller = (a.width * a.height).min(b.width * b.height);
        smaller > 0 && (w * h) as f64 > SAME_OVERLAP * smaller as f64
    };
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                if same(&rects[i], &rects[j]) {
                    let (a, b) = (rects[i], rects.swap_remove(j));
                    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
                    rects[i] =
                        CropRect::new(x, y, a.right().max(b.right()) - x, a.bottom().max(b.bottom()) - y);
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
    rects.sort_by_key(|r| (r.y, r.x));
    rects
}

/// 1 枚の画像（全体または分けた部分）の文字の範囲を認識する。
fn detect_image(image: &RgbaImage) -> Result<Vec<CropRect>, String> {
    let done = perform(image, "文字", || {
        let request = VNRecognizeTextRequest::new();
        // 精度重視（速い認識は日本語に対応しておらず、日本語を指定すると Vision ごと止まる）
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        request.setRecognitionLanguages(&NSArray::from_retained_slice(&[
            NSString::from_str("ja-JP"),
            NSString::from_str("en-US"),
        ]));
        // 辞書で読み替えない（ナンバープレートなどの意味のない並びも読む）
        request.setUsesLanguageCorrection(false);
        request
    })?;
    let Some(texts) = done.request.results() else { return Ok(Vec::new()) };
    let size = image.dimensions();
    let mut found = Vec::new();
    for i in 0..texts.count() {
        let text = texts.objectAtIndex(i);
        let Some(best) = text.topCandidates(1).firstObject() else { continue };
        let characters = best.string().to_string().chars().filter(|c| c.is_alphanumeric()).count();
        if best.confidence() < MIN_CONFIDENCE || characters < MIN_CHARACTERS {
            continue;
        }
        // SAFETY: 結果の枠を読むだけ
        found.push(to_image_rect(unsafe { text.boundingBox() }, size));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{draw_text, TextFont, TextPosition, TextSettings};
    use image::Rgba;

    #[test]
    fn merge_joins_only_nearly_the_same_rects() {
        let rects = vec![
            CropRect::new(0, 0, 10, 10),
            CropRect::new(2, 1, 10, 10),  // ほぼ同じ
            CropRect::new(11, 0, 10, 10), // 少し重なるだけ（別の文字）
            CropRect::new(50, 50, 5, 5),
        ];
        assert_eq!(
            merge(rects),
            vec![CropRect::new(0, 0, 12, 11), CropRect::new(11, 0, 10, 10), CropRect::new(50, 50, 5, 5)]
        );
    }

    #[test]
    fn pad_grows_by_the_text_height() {
        assert_eq!(pad(CropRect::new(20, 20, 100, 10), (200, 200)), Some(CropRect::new(17, 17, 106, 16)));
        assert_eq!(pad(CropRect::new(0, 0, 100, 10), (200, 200)), Some(CropRect::new(0, 0, 103, 13)));
    }

    #[test]
    fn finds_the_text_in_an_image() {
        // 灰色の板に黒い文字（ナンバープレートのような）を、画像の右下に描く
        let mut image = RgbaImage::from_pixel(800, 600, Rgba([90, 120, 80, 255]));
        for (x, y, p) in image.enumerate_pixels_mut() {
            if (440..760).contains(&x) && (440..540).contains(&y) {
                *p = Rgba([235, 235, 225, 255]);
            }
        }
        let text = TextSettings {
            text: "ABC 1234".into(),
            font: TextFont::GothicBold,
            size: 8.0,
            color: [20, 20, 20],
            opacity: 100,
            position: TextPosition::BottomRight,
            ..TextSettings::default()
        };
        draw_text(&mut image, &text, Some((440, 440, 760, 540)), Some(600.0));
        let found = detect_text(&image).unwrap();
        assert!(!found.is_empty());
        // 見つけた範囲は板の中の文字にかかり、板の外の左上には何もない
        assert!(found.iter().all(|r| r.x >= 400 && r.y >= 400), "{found:?}");
        let dark: Vec<(u32, u32)> =
            image.enumerate_pixels().filter(|(_, _, p)| p[0] < 60).map(|(x, y, _)| (x, y)).collect();
        assert!(!dark.is_empty());
        let covered = dark
            .iter()
            .filter(|&&(x, y)| {
                found.iter().any(|r| {
                    (r.x..r.right()).contains(&i64::from(x)) && (r.y..r.bottom()).contains(&i64::from(y))
                })
            })
            .count();
        assert!(covered * 10 >= dark.len() * 9, "{covered} / {} {found:?}", dark.len());
    }

    #[test]
    fn plain_image_has_no_text() {
        let image = RgbaImage::from_pixel(64, 48, Rgba([120, 160, 200, 255]));
        assert_eq!(detect_text(&image).unwrap(), vec![]);
    }
    #[test]
    fn finds_a_small_number_plate() {
        // 1600 × 1200 の画像の右下の、160 × 60 の板に 2 行の文字（日本のナンバープレートのような）
        let mut image = RgbaImage::from_pixel(1600, 1200, Rgba([90, 120, 80, 255]));
        let plate = (1200, 900, 1360, 960);
        for (x, y, p) in image.enumerate_pixels_mut() {
            if (plate.0..plate.2).contains(&x) && (plate.1..plate.3).contains(&y) {
                *p = Rgba([235, 235, 225, 255]);
            }
        }
        let text = TextSettings {
            text: "品川 300\nさ 12-34".into(),
            font: TextFont::GothicBold,
            size: 30.0,
            color: [20, 20, 20],
            opacity: 100,
            position: TextPosition::Center,
            ..TextSettings::default()
        };
        let area = (i64::from(plate.0), i64::from(plate.1), i64::from(plate.2), i64::from(plate.3));
        draw_text(&mut image, &text, Some(area), Some(60.0));
        let found = detect_text(&image).unwrap();
        assert!(!found.is_empty());
        // 見つけた範囲は板のまわりだけ
        assert!(
            found.iter().all(|r| r.x >= 1150 && r.y >= 850 && r.right() <= 1410 && r.bottom() <= 1010),
            "{found:?}"
        );
    }
}
