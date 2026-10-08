//! 顔の自動認識（投稿加工のスタンプ。旧版にはない）。macOS の Vision で顔を見つける（端末の中だけで処理する）。

use image::RgbaImage;
use objc2_vision::VNDetectFaceRectanglesRequest;

use crate::transform::CropRect;
use crate::vision::{perform, scan_tiles, to_image_rect};

/// 見つけた顔の枠を広げる倍率（スタンプが髪・あごまで覆うように）。
const COVER_SCALE: f64 = 1.4;

/// 画像の中の顔を見つけ、顔ごとの枠（画像の座標、px）を上から順（同じ高さなら左から）に返す。見つからなければ空。
///
/// Vision は画像全体に対して顔が小さいと見落としやすい（集合写真で半分ほどしか見つからなかった）ので、
/// 全体に加えて、重なりを持たせて 2 × 2・3 × 3 に分けた部分ごとにも認識し、同じ顔をまとめる。
/// GPU・Neural Engine を使えない環境（CI の仮想マシンなど）では Vision が「Could not create inference
/// context」で失敗するので、そのときは CPU だけで認識し直す。
pub fn detect_faces(image: &RgbaImage) -> Result<Vec<CropRect>, String> {
    Ok(merge(scan_tiles(image, detect_image)?))
}

/// 1 枚の画像（全体または分けた部分）の顔を認識する。
fn detect_image(image: &RgbaImage) -> Result<Vec<CropRect>, String> {
    // SAFETY: 引数のない初期化
    let done = perform(image, "顔", || unsafe { VNDetectFaceRectanglesRequest::new() })?;
    let size = image.dimensions();
    // SAFETY: 認識が終わった後に結果の枠を読むだけ
    Ok(unsafe { done.request.results() }.map_or_else(Vec::new, |faces| {
        (0..faces.count())
            .map(|i| to_image_rect(unsafe { faces.objectAtIndex(i).boundingBox() }, size))
            .collect()
    }))
}

/// 同じ顔の枠をまとめる: 大きい枠から順に残し、中心がもう残した枠の中にある枠は捨てる。
/// 上から順（同じ高さなら左から）に並べて返す。
fn merge(mut faces: Vec<CropRect>) -> Vec<CropRect> {
    faces.sort_by_key(|r| std::cmp::Reverse(r.width * r.height));
    let mut kept: Vec<CropRect> = Vec::new();
    for face in faces {
        let (cx, cy) = (face.x + face.width / 2, face.y + face.height / 2);
        if !kept.iter().any(|k| (k.x..=k.right()).contains(&cx) && (k.y..=k.bottom()).contains(&cy)) {
            kept.push(face);
        }
    }
    kept.sort_by_key(|r| (r.y, r.x));
    kept
}

/// 顔の枠を、中心を保って広げる（スタンプで覆う範囲。画像からはみ出す部分は切る）。
pub fn cover_rect(face: CropRect, (width, height): (u32, u32)) -> Option<CropRect> {
    let side = face.width.max(face.height) as f64 * COVER_SCALE;
    let (cx, cy) = (face.x as f64 + face.width as f64 / 2.0, face.y as f64 + face.height as f64 / 2.0);
    let rect = CropRect::new(
        (cx - side / 2.0).round() as i64,
        (cy - side / 2.0).round() as i64,
        side.round() as i64,
        side.round() as i64,
    );
    crate::transform::clamp_crop(rect, (width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_rect_grows_around_the_center_and_stays_inside() {
        assert_eq!(
            cover_rect(CropRect::new(40, 40, 20, 20), (100, 100)),
            Some(CropRect::new(36, 36, 28, 28))
        );
        assert_eq!(cover_rect(CropRect::new(0, 0, 20, 10), (100, 100)), Some(CropRect::new(0, 0, 24, 19)));
    }

    #[test]
    fn plain_image_has_no_faces() {
        let image = RgbaImage::from_pixel(64, 48, image::Rgba([120, 160, 200, 255]));
        assert_eq!(detect_faces(&image).unwrap(), vec![]);
    }

    #[test]
    fn finds_the_face_in_a_portrait() {
        // NASA のポートレート（パブリックドメイン。tests/fixtures/face.jpg、256×320）。顔は x ≒ 130〜185、y ≒ 50〜120
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/face.jpg");
        let image = image::open(path).unwrap().to_rgba8();
        let faces = detect_faces(&image).unwrap();
        assert_eq!(faces.len(), 1, "{faces:?}");

        let face = faces[0];
        let (cx, cy) = (face.x + face.width / 2, face.y + face.height / 2);
        assert!((135..185).contains(&cx) && (60..115).contains(&cy), "{face:?}");
        // スタンプで覆う範囲は顔を含む
        let cover = cover_rect(face, image.dimensions()).unwrap();
        assert!(
            cover.x <= face.x
                && cover.y <= face.y
                && cover.right() >= face.right()
                && cover.bottom() >= face.bottom()
        );
    }

    #[test]
    fn merge_keeps_one_box_per_face() {
        let faces = vec![
            CropRect::new(10, 10, 20, 20),
            CropRect::new(12, 11, 18, 18), // 同じ顔（中心が上の枠の中）
            CropRect::new(100, 5, 20, 20),
            CropRect::new(50, 60, 30, 30),
        ];
        assert_eq!(
            merge(faces),
            vec![CropRect::new(100, 5, 20, 20), CropRect::new(10, 10, 20, 20), CropRect::new(50, 60, 30, 30)]
        );
    }

    #[test]
    fn finds_every_small_face_in_a_group() {
        // ポートレートを 192 × 240 に縮めて 5 × 3 に並べた集合写真（顔の幅は画像の幅の 3% ほど）。
        // 全体だけを認識すると 1 つも見つからない大きさ
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/face.jpg");
        let portrait = crate::transform::resize_to(&image::open(path).unwrap().to_rgba8(), (192, 240));
        let mut group = RgbaImage::from_pixel(1600, 1000, image::Rgba([70, 70, 80, 255]));
        let mut centers = Vec::new();
        for row in 0..3 {
            for column in 0..5 {
                let (x, y) = (20 + column * 312 + (row % 2) * 20, 20 + row * 320);
                image::imageops::replace(&mut group, &portrait, x, y);
                // 顔の中心は、縮めたポートレートの (120, 64) あたり
                centers.push((x + 120, y + 64));
            }
        }
        let faces = detect_faces(&group).unwrap();
        assert_eq!(faces.len(), centers.len(), "{faces:?}");
        for (cx, cy) in centers {
            let hits = faces
                .iter()
                .filter(|f| (f.x..=f.right()).contains(&cx) && (f.y..=f.bottom()).contains(&cy))
                .count();
            assert_eq!(hits, 1, "({cx}, {cy}) {faces:?}");
        }
        // 上から順（同じ高さなら左から）に並ぶ
        assert!(faces.windows(2).all(|w| (w[0].y, w[0].x) <= (w[1].y, w[1].x)));
    }
}
