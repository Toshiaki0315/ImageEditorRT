//! HEIC で保存する（macOS の ImageIO で書き出す。旧版にはない）。
//!
//! 品質は JPEG と同じ 1〜100 を ImageIO の非可逆圧縮の品質（0〜1）にする。EXIF は、整えた EXIF を入れた
//! 小さな JPEG を ImageIO に読ませて取り出したプロパティ（Exif・GPS・TIFF の辞書）を、そのまま書き出しに渡す
//! （ImageIO は EXIF のバイト列を直接は受け取らないため）。MakerNote は ImageIO が書かないので残らない。

use std::ffi::c_void;

use image::{Rgba, RgbaImage};
use objc2_core_foundation::{
    CFData, CFDictionary, CFMutableData, CFMutableDictionary, CFNumber, CFRetained, CFString, CFType,
};
use objc2_image_io::{kCGImageDestinationLossyCompressionQuality, CGImageDestination, CGImageSource};

use crate::formats::has_transparency;

/// HEIC の UTI。
const HEIC_UTI: &str = "public.heic";

/// 画像を HEIC のバイト列にする（quality は 1〜100）。exif は prepare_exif で整えたもの（"Exif\0\0" 付き）。
pub fn encode_heic(image: &RgbaImage, quality: u8, exif: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let failed = || "HEIC に書き出せません".to_string();
    let cg = crate::vision::cg_image_with_alpha(image, has_transparency(image)).ok_or_else(failed)?;
    let properties = properties(quality, exif).ok_or_else(failed)?;
    let data = CFMutableData::new(None, 0).ok_or_else(failed)?;
    let uti = CFString::from_static_str(HEIC_UTI);
    // SAFETY: data・uti は有効。オプションは渡さない
    let destination = unsafe { CGImageDestination::with_data(&data, &uti, 1, None) }.ok_or_else(failed)?;
    // SAFETY: 辞書のキーは ImageIO のプロパティの名前、値は CF の型
    unsafe { destination.add_image(&cg, Some(&properties)) };
    // SAFETY: 画像を 1 枚加えた後に書き出す
    if !unsafe { destination.finalize() } {
        return Err(failed());
    }
    Ok(data.to_vec())
}

/// 書き出しのプロパティ（EXIF の辞書と、圧縮の品質）。
fn properties(quality: u8, exif: Option<&[u8]>) -> Option<CFRetained<CFMutableDictionary>> {
    // EXIF がなければ空の辞書から（複製の元に「なし」は渡せない）
    let base = exif.and_then(exif_properties).unwrap_or_else(|| {
        let empty = CFDictionary::<CFString, CFType>::empty();
        // SAFETY: 型付きの辞書を、型のない辞書として見るだけ
        unsafe { CFRetained::cast_unchecked(empty) }
    });
    // SAFETY: 有効な辞書を、書き換えられる辞書に複製するだけ
    let properties = unsafe { CFMutableDictionary::new_copy(None, 0, Some(&base)) }?;
    let key = unsafe { kCGImageDestinationLossyCompressionQuality };
    let value = CFNumber::new_f64(f64::from(quality.clamp(1, 100)) / 100.0);
    // SAFETY: キーは CFString、値は CFNumber（どちらも辞書が保持する）
    unsafe {
        CFMutableDictionary::set_value(
            Some(&properties),
            (key as *const CFString).cast::<c_void>(),
            (&*value as *const CFNumber).cast::<c_void>(),
        );
    }
    Some(properties)
}

/// EXIF を ImageIO のプロパティの辞書にする（EXIF を入れた 1 × 1 の JPEG を読ませて取り出す）。
fn exif_properties(exif: &[u8]) -> Option<CFRetained<CFDictionary>> {
    let pixel = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
    let jpeg = crate::tiff::insert_exif_into_jpeg(&crate::encode::to_jpeg(&pixel, 50), exif)?;
    let data = CFData::from_bytes(&jpeg);
    // SAFETY: data は有効な CFData。オプションは渡さない
    let source = unsafe { CGImageSource::with_data(&data, None) }?;
    // SAFETY: 先頭の画像のプロパティを読むだけ
    unsafe { source.properties_at_index(0, None) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo() -> RgbaImage {
        RgbaImage::from_fn(160, 120, |x, y| {
            let noise = ((x * 31 + y * 17) % 23) as u8;
            Rgba([(x * 255 / 160) as u8, (y * 255 / 120) as u8, 120 + noise, 255])
        })
    }

    #[test]
    fn heic_can_be_read_back_and_quality_changes_the_size() {
        let image = photo();
        let high = encode_heic(&image, 95, None).unwrap();
        let low = encode_heic(&image, 20, None).unwrap();
        assert!(low.len() < high.len(), "低い品質のほうが小さい: {} / {}", low.len(), high.len());
        let back = crate::decode::decode(&high).unwrap();
        assert_eq!(back.dimensions(), (160, 120));
        let (a, b) = (image.get_pixel(80, 60), back.get_pixel(80, 60));
        assert!((0..3).all(|c| a[c].abs_diff(b[c]) < 12), "{a:?} / {b:?}");
    }

    #[test]
    fn transparency_is_kept() {
        let mut image = photo();
        for x in 0..40 {
            for y in 0..120 {
                image.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
        let back = crate::decode::decode(&encode_heic(&image, 90, None).unwrap()).unwrap();
        assert!(back.get_pixel(10, 60)[3] < 16 && back.get_pixel(120, 60)[3] > 240);
    }
}
