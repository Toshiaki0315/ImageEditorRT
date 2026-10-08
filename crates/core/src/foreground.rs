//! 被写体のマスク（背景を消す。旧版にはない）。macOS の Vision（VNGenerateForegroundInstanceMaskRequest、
//! macOS 14 以降）で、人・動物・ものなど目立つ被写体をすべて残すマスクを作る（端末の中だけで処理する）。

use image::{GrayImage, Luma, RgbaImage};
use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::AllocAnyThread;
use objc2_core_graphics::CGImage;
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary};
use objc2_vision::{VNGenerateForegroundInstanceMaskRequest, VNImageRequestHandler, VNRequest};

use crate::faces::cg_image;

/// 画像の被写体のマスク（画像と同じ大きさ。被写体 255・背景 0）。被写体が見つからなければ None。
pub fn foreground_mask(image: &RgbaImage) -> Result<Option<GrayImage>, String> {
    if AnyClass::get(c"VNGenerateForegroundInstanceMaskRequest").is_none() {
        return Err("背景を消すには macOS 14 以降が必要です".into());
    }
    let cg = cg_image(image).ok_or("画像を被写体の認識に渡せません")?;
    // GPU を使えない環境では CPU だけでやり直す（顔の認識と同じ）
    detect(&cg, false).or_else(|_| detect(&cg, true))
}

/// 被写体のマスクを作る。cpu_only なら CPU だけで認識する。
fn detect(cg: &CGImage, cpu_only: bool) -> Result<Option<GrayImage>, String> {
    // SAFETY: CGImage は認識が終わるまで生きている。オプションは空
    let handler = unsafe {
        VNImageRequestHandler::initWithCGImage_options(
            VNImageRequestHandler::alloc(),
            cg,
            &NSDictionary::new(),
        )
    };
    // SAFETY: 引数のない初期化（クラスがあることは確かめてある）
    let request = unsafe { VNGenerateForegroundInstanceMaskRequest::new() };
    if cpu_only {
        // SAFETY: 認識の前に設定を変えるだけ
        #[allow(deprecated)]
        unsafe {
            request.setUsesCPUOnly(true)
        };
    }
    let requests: Retained<NSArray<VNRequest>> =
        NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(request.clone()))]);
    handler.performRequests_error(&requests).map_err(|e| format!("被写体を認識できません（{e}）"))?;
    // SAFETY: 認識が終わった後に結果を読む
    let Some(results) = (unsafe { request.results() }) else { return Ok(None) };
    let Some(observation) = results.firstObject() else { return Ok(None) };
    // SAFETY: 見つけた被写体すべて（背景を除く）を、元の画像の大きさのマスクにする
    let buffer = unsafe {
        let instances = observation.allInstances();
        if instances.count() == 0 {
            return Ok(None);
        }
        observation.generateScaledMaskForImageForInstances_fromRequestHandler_error(&instances, &handler)
    }
    .map_err(|e| format!("被写体のマスクを作れません（{e}）"))?;
    read_mask(&buffer).map(Some)
}

/// 被写体の度合い（0〜1 の 32bit 浮動小数、1 チャンネル）の入れ物を、0〜255 のマスクにする。
fn read_mask(buffer: &CVPixelBuffer) -> Result<GrayImage, String> {
    let (width, height) = (CVPixelBufferGetWidth(buffer), CVPixelBufferGetHeight(buffer));
    // SAFETY: 読むあいだだけ固定する（読み終えたら外す）
    if unsafe { CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) } != 0 {
        return Err("被写体のマスクを読めません".into());
    }
    let base = CVPixelBufferGetBaseAddress(buffer).cast::<u8>();
    let stride = CVPixelBufferGetBytesPerRow(buffer);
    let mask = GrayImage::from_fn(width as u32, height as u32, |x, y| {
        // SAFETY: 固定した入れ物の中の、行の幅 stride・1 画素 4 バイトの位置を読む
        let value = unsafe { base.add(y as usize * stride + x as usize * 4).cast::<f32>().read_unaligned() };
        Luma([(value.clamp(0.0, 1.0) * 255.0).round() as u8])
    });
    // SAFETY: 上で固定したものを外す
    unsafe { CVPixelBufferUnlockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) };
    Ok(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_person_in_a_portrait() {
        // NASA のポートレート（tests/fixtures/face.jpg、256 × 320）。顔のあたりは被写体、左上の暗い背景は背景
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/face.jpg");
        let image = image::open(path).unwrap().to_rgba8();
        let mask = foreground_mask(&image).unwrap().expect("被写体が見つかる");
        assert_eq!(mask.dimensions(), image.dimensions());
        assert!(mask.get_pixel(160, 85)[0] > 200, "顔 {}", mask.get_pixel(160, 85)[0]);
        assert!(mask.get_pixel(10, 10)[0] < 50, "背景 {}", mask.get_pixel(10, 10)[0]);
    }

    #[test]
    fn plain_image_has_no_subject() {
        let image = RgbaImage::from_pixel(64, 48, image::Rgba([120, 160, 200, 255]));
        assert!(foreground_mask(&image).unwrap().is_none());
    }
}
