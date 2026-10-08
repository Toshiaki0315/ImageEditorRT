//! macOS の Vision の呼び出し（顔・文字・水平線・被写体の認識で共通）。端末の中だけで処理する。
//!
//! RGBA の画像を CGImage にして渡し、認識を実行する。GPU・Neural Engine を使えない環境（CI の仮想マシンなど）
//! では「Could not create inference context」で失敗するので、そのときは CPU だけで認識し直す。結果の枠
//! （0〜1 に正規化、原点は左下）は画像の座標（px、原点は左上）にする。

use std::ffi::c_void;

use image::RgbaImage;
use objc2::rc::Retained;
use objc2::{AllocAnyThread, ClassType};
use objc2_core_foundation::{CFRetained, CGRect};
use objc2_core_graphics::{
    kCGColorSpaceSRGB, CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGImage,
    CGImageAlphaInfo,
};
use objc2_foundation::{NSArray, NSDictionary};
use objc2_vision::{VNImageBasedRequest, VNImageRequestHandler, VNRequest};

use crate::transform::CropRect;

/// 全体のほかに認識する分け方（2 × 2 と 3 × 3）。
const TILE_SPLITS: [u32; 2] = [2, 3];
/// 分けた部分の一辺を、ちょうど分けた長さの何倍にするか（隣どうしを重ねて、境目のものも見つける）。
const TILE_OVERLAP: f64 = 1.5;

/// 認識を終えた要求と、それを実行した画像の扱い手（被写体のマスクを作るときに使う）。
pub(crate) struct Performed<R: ClassType> {
    pub request: Retained<R>,
    pub handler: Retained<VNImageRequestHandler>,
}

/// image に対して、make で作った認識の要求を実行する（GPU を使えなければ CPU だけでやり直す）。
/// what は失敗したときの説明に使う名前（「顔」「文字」など）。
pub(crate) fn perform<R>(
    image: &RgbaImage,
    what: &str,
    make: impl Fn() -> Retained<R>,
) -> Result<Performed<R>, String>
where
    R: ClassType<Super = VNImageBasedRequest> + 'static,
{
    let cg = cg_image(image).ok_or_else(|| format!("画像を{what}の認識に渡せません"))?;
    let run = |cpu_only: bool| -> Result<Performed<R>, String> {
        // SAFETY: CGImage は認識が終わるまで生きている。オプションは空
        let handler = unsafe {
            VNImageRequestHandler::initWithCGImage_options(
                VNImageRequestHandler::alloc(),
                &cg,
                &NSDictionary::new(),
            )
        };
        let request = make();
        // 個々の認識 → 画像の認識 → 認識の順に親のクラスにする（設定と、認識の一覧に入れるため）
        let base: Retained<VNRequest> = Retained::into_super(Retained::into_super(request.clone()));
        if cpu_only {
            // SAFETY: 認識の前に設定を変えるだけ（新しい計算資源の指定は macOS 14 以降なので、古い設定を使う）
            #[allow(deprecated)]
            unsafe {
                base.setUsesCPUOnly(true)
            };
        }
        let requests = NSArray::from_retained_slice(&[base]);
        handler.performRequests_error(&requests).map_err(|e| format!("{what}を認識できません（{e}）"))?;
        Ok(Performed { request, handler })
    };
    run(false).or_else(|_| run(true))
}

/// 画像全体と、重なりを持たせて 2 × 2・3 × 3 に分けた部分ごとに detect をかけ、見つけた枠（画像の座標）を
/// すべて返す（同じものを何度も見つけるので、まとめるのは呼んだ側）。Vision は画像全体に対して小さいものを
/// 見落としやすいため（顔・文字の認識で使う）。
pub(crate) fn scan_tiles(
    image: &RgbaImage,
    detect: impl Fn(&RgbaImage) -> Result<Vec<CropRect>, String>,
) -> Result<Vec<CropRect>, String> {
    let (width, height) = image.dimensions();
    let mut found = detect(image)?;
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
                found.extend(detect(&tile)?.into_iter().map(shift));
            }
        }
    }
    Ok(found)
}

/// split × split に分けるときの一辺（隣と重なるよう、ちょうど分けた長さの 1.5 倍）。
fn tile_side(length: u32, split: u32) -> u32 {
    ((f64::from(length) * TILE_OVERLAP / f64::from(split)) as u32).min(length)
}

/// Vision の枠（0〜1 に正規化、原点は左下）を、画像の座標（px、原点は左上）にする。
pub(crate) fn to_image_rect(rect: CGRect, (width, height): (u32, u32)) -> CropRect {
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

/// RGBA の画像を CGImage にする（sRGB）。
pub(crate) fn cg_image(image: &RgbaImage) -> Option<CFRetained<CGImage>> {
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
    fn tiles_cover_the_image_with_overlap() {
        // 全体 1 回 + 2 × 2 + 3 × 3 の 14 回。分けた部分の位置は画像の座標に直して返す
        let image = RgbaImage::new(300, 200);
        let calls = std::cell::RefCell::new(Vec::new());
        let found = scan_tiles(&image, |tile| {
            calls.borrow_mut().push(tile.dimensions());
            Ok(vec![CropRect::new(0, 0, 1, 1)])
        })
        .unwrap();
        assert_eq!(calls.borrow().len(), 14);
        assert_eq!(calls.borrow()[1], (225, 150));
        assert_eq!(calls.borrow()[5], (150, 100));
        assert!(
            found.contains(&CropRect::new(75, 50, 1, 1)) && found.contains(&CropRect::new(150, 100, 1, 1))
        );
    }
}
