//! 顔の自動認識（投稿加工のスタンプ。旧版にはない）。macOS の Vision で顔を見つける（端末の中だけで処理する）。

use std::ffi::c_void;

use image::RgbaImage;
use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_core_foundation::{CFRetained, CGRect};
use objc2_core_graphics::{
    kCGColorSpaceSRGB, CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGImage,
    CGImageAlphaInfo,
};
use objc2_foundation::{NSArray, NSDictionary};
use objc2_vision::{VNDetectFaceRectanglesRequest, VNImageRequestHandler, VNRequest};

use crate::transform::CropRect;

/// 見つけた顔の枠を広げる倍率（スタンプが髪・あごまで覆うように）。
const COVER_SCALE: f64 = 1.4;

/// 全体のほかに認識する分け方（2 × 2 と 3 × 3）。
const TILE_SPLITS: [u32; 2] = [2, 3];
/// 分けた部分の一辺を、ちょうど分けた長さの何倍にするか（隣どうしを重ねて、境目の顔も見つける）。
const TILE_OVERLAP: f64 = 1.5;

/// 画像の中の顔を見つけ、顔ごとの枠（画像の座標、px）を上から順（同じ高さなら左から）に返す。見つからなければ空。
///
/// Vision は画像全体に対して顔が小さいと見落としやすい（集合写真で半分ほどしか見つからなかった）ので、
/// 全体に加えて、重なりを持たせて 2 × 2・3 × 3 に分けた部分ごとにも認識し、同じ顔をまとめる。
/// GPU・Neural Engine を使えない環境（CI の仮想マシンなど）では Vision が「Could not create inference
/// context」で失敗するので、そのときは CPU だけで認識し直す。
pub fn detect_faces(image: &RgbaImage) -> Result<Vec<CropRect>, String> {
    let (width, height) = image.dimensions();
    let mut found = detect_image(image)?;
    for split in TILE_SPLITS {
        // split × split に分け、隣どうしを重ねる（一辺は 1.5 ÷ split）
        let (tile_w, tile_h) = (tile_side(width, split), tile_side(height, split));
        if tile_w == 0 || tile_h == 0 {
            continue;
        }
        for j in 0..split {
            for i in 0..split {
                let x = (width - tile_w) * i / (split - 1);
                let y = (height - tile_h) * j / (split - 1);
                let tile = image::imageops::crop_imm(image, x, y, tile_w, tile_h).to_image();
                let shift =
                    |r: CropRect| CropRect::new(r.x + i64::from(x), r.y + i64::from(y), r.width, r.height);
                found.extend(detect_image(&tile)?.into_iter().map(shift));
            }
        }
    }
    Ok(merge(found))
}

/// split × split に分けるときの一辺（隣と重なるよう、ちょうど分けた長さの 1.5 倍）。
fn tile_side(length: u32, split: u32) -> u32 {
    ((f64::from(length) * TILE_OVERLAP / f64::from(split)) as u32).min(length)
}

/// 1 枚の画像（全体または分けた部分）の顔を認識する。
fn detect_image(image: &RgbaImage) -> Result<Vec<CropRect>, String> {
    let cg = cg_image(image).ok_or("画像を顔の認識に渡せません")?;
    let size = image.dimensions();
    detect(&cg, size, false).or_else(|_| detect(&cg, size, true))
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

/// 顔を認識する。cpu_only なら CPU だけで認識する。
fn detect(cg: &CGImage, size: (u32, u32), cpu_only: bool) -> Result<Vec<CropRect>, String> {
    // SAFETY: CGImage は認識が終わるまで生きている。オプションは空
    let handler = unsafe {
        VNImageRequestHandler::initWithCGImage_options(
            VNImageRequestHandler::alloc(),
            cg,
            &NSDictionary::new(),
        )
    };
    // SAFETY: 引数のない初期化
    let request = unsafe { VNDetectFaceRectanglesRequest::new() };
    if cpu_only {
        // SAFETY: 認識の前に設定を変えるだけ（新しい計算資源の指定は macOS 14 以降なので、古い設定を使う）
        #[allow(deprecated)]
        unsafe {
            request.setUsesCPUOnly(true)
        };
    }
    // 顔の認識 → 画像の認識 → 認識の順に親のクラスにして、認識の一覧に入れる
    let requests: Retained<NSArray<VNRequest>> =
        NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(request.clone()))]);
    handler.performRequests_error(&requests).map_err(|e| format!("顔を認識できません（{e}）"))?;
    // SAFETY: 認識が終わった後に結果を読む
    let results = unsafe { request.results() };
    Ok(results.map_or_else(Vec::new, |faces| {
        // SAFETY: 結果の枠を読むだけ
        (0..faces.count())
            .map(|i| to_image_rect(unsafe { faces.objectAtIndex(i).boundingBox() }, size))
            .collect()
    }))
}

/// Vision の枠（0〜1 に正規化、原点は左下）を、画像の座標（px、原点は左上）にする。
fn to_image_rect(rect: CGRect, (width, height): (u32, u32)) -> CropRect {
    let (w, h) = (f64::from(width), f64::from(height));
    let x = rect.origin.x * w;
    let y = (1.0 - rect.origin.y - rect.size.height) * h;
    CropRect::new(
        x.round() as i64,
        y.round() as i64,
        (rect.size.width * w).round() as i64,
        (rect.size.height * h).round() as i64,
    )
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

/// RGBA の画像を CGImage にする（sRGB）。
fn cg_image(image: &RgbaImage) -> Option<CFRetained<CGImage>> {
    let (width, height) = (image.width() as usize, image.height() as usize);
    // CoreGraphics は乗算済みのアルファで持つので、色にアルファを掛けて渡す
    let mut pixels: Vec<u8> = image
        .as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = u16::from(p[3]);
            let m = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
            [m(p[0]), m(p[1]), m(p[2]), p[3]]
        })
        .collect();
    // SAFETY: 定数の名前から色空間を作るだけ
    let srgb = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    // SAFETY: pixels は width * height * 4 バイトあり、画像を作り終えるまで生きている（画像は複製を持つ）
    let context: CFRetained<CGContext> = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast::<c_void>(),
            width,
            height,
            8,
            width * 4,
            Some(&srgb),
            CGImageAlphaInfo::PremultipliedLast.0,
        )
    }?;
    CGBitmapContextCreateImage(Some(&context))
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_core_foundation::{CGPoint, CGSize};

    #[test]
    fn vision_rect_is_flipped_to_top_left_origin() {
        // 左下 (0.1, 0.2) から 0.3 × 0.4 → 上から 1 − 0.2 − 0.4 = 0.4
        let rect = CGRect::new(CGPoint::new(0.1, 0.2), CGSize::new(0.3, 0.4));
        assert_eq!(to_image_rect(rect, (200, 100)), CropRect::new(20, 40, 60, 40));
    }

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
        // CPU だけで認識しても同じ顔を見つける（GPU を使えない環境で使う道）
        let cpu = detect(&cg_image(&image).unwrap(), image.dimensions(), true).unwrap();
        assert_eq!(cpu.len(), 1, "{cpu:?}");
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
