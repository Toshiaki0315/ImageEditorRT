//! macOS の ImageIO（OS の機能）で画像を読む。
//!
//! HEIC を含め、macOS が読める形式はすべて同じ方法で読む。EXIF の向き (Orientation) は
//! 読み込み時に直し、色は sRGB にそろえて RGBA（8bit）の画像にする。

use std::ffi::c_void;
use std::ptr::NonNull;

use image::RgbaImage;
use objc2_core_foundation::{
    CFBoolean, CFData, CFDictionary, CFNumber, CFRetained, CFString, CFType, CGPoint, CGRect,
    CGSize,
};
use objc2_core_graphics::{
    kCGColorSpaceSRGB, CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
};
use objc2_image_io::{
    kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
    kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
    kCGImageSourceThumbnailMaxPixelSize, CGImageSource,
};

/// 画像を読めなかった理由。
#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// 画像の形式として読めない（壊れている・対応していない）
    Unsupported,
    /// 読めたが、画素を取り出せなかった
    Render,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Unsupported => write!(f, "画像を読み込めません（対応していない形式か、壊れています）"),
            DecodeError::Render => write!(f, "画像の画素を取り出せません"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// ファイルの中身（バイト列）から画像を読み、向きを直した sRGB の RGBA 画像を返す。
pub fn decode(bytes: &[u8]) -> Result<RgbaImage, DecodeError> {
    let data = CFData::from_bytes(bytes);
    // SAFETY: data は有効な CFData。オプションは渡さない
    let source =
        unsafe { CGImageSource::with_data(&data, None) }.ok_or(DecodeError::Unsupported)?;
    let (width, height) = pixel_size(&source).ok_or(DecodeError::Unsupported)?;
    // 縮小しない（最大辺 = 元の長辺）サムネイルを、向きを直して作らせると、向きを直した原寸の画像になる
    let image = oriented_image(&source, width.max(height)).ok_or(DecodeError::Unsupported)?;
    render_rgba(&image).ok_or(DecodeError::Render)
}

/// 画像の大きさ（EXIF の向きを直す前）を返す。
fn pixel_size(source: &CGImageSource) -> Option<(usize, usize)> {
    // SAFETY: 0 番目の画像のプロパティを読むだけ
    let properties = unsafe { source.properties_at_index(0, None) }?;
    // SAFETY: キーは ImageIO の定数。値の型は CFNumber
    let width = unsafe { number(&properties, kCGImagePropertyPixelWidth) }?;
    let height = unsafe { number(&properties, kCGImagePropertyPixelHeight) }?;
    Some((width, height))
}

/// # Safety
/// properties は ImageIO のプロパティの辞書であること。
unsafe fn number(properties: &CFDictionary, key: &CFString) -> Option<usize> {
    let key_ptr: *const CFString = key;
    // SAFETY: キーは CFString、値があれば CFNumber
    let value = unsafe { properties.value(key_ptr.cast()) };
    let value = NonNull::new(value as *mut CFNumber)?;
    // SAFETY: 辞書が持っている値を借りるだけ（解放しない）
    let number = unsafe { value.as_ref() };
    number.as_i64().and_then(|v| usize::try_from(v).ok())
}

fn oriented_image(source: &CGImageSource, max_side: usize) -> Option<CFRetained<CGImage>> {
    let yes = CFBoolean::new(true);
    let size = CFNumber::new_i64(max_side as i64);
    // SAFETY: キーは ImageIO の定数
    let keys: [&CFString; 3] = unsafe {
        [
            kCGImageSourceCreateThumbnailWithTransform,
            kCGImageSourceCreateThumbnailFromImageAlways,
            kCGImageSourceThumbnailMaxPixelSize,
        ]
    };
    let values: [&CFType; 3] = [&yes, &yes, &size];
    let options = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);
    // SAFETY: オプションの辞書は CFString → CFType
    unsafe { source.thumbnail_at_index(0, Some(options.as_opaque())) }
}

/// CGImage を sRGB の RGBA（ストレートアルファ）の画素にする。
fn render_rgba(image: &CGImage) -> Option<RgbaImage> {
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    if width == 0 || height == 0 {
        return None;
    }
    let mut pixels = vec![0u8; width * height * 4];
    // SAFETY: 定数の名前から色空間を作るだけ
    let srgb = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    // SAFETY: pixels は width * height * 4 バイトあり、描き終わるまで生きている
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
    let rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width as f64, height as f64));
    CGContext::draw_image(Some(&context), rect, Some(image));
    drop(context);
    unpremultiply(&mut pixels);
    RgbaImage::from_raw(width as u32, height as u32, pixels)
}

/// 乗算済みアルファ（CoreGraphics の描き方）を、ふつうのアルファに戻す。
fn unpremultiply(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3];
        if alpha == 255 || alpha == 0 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * 255 + u32::from(alpha) / 2) / u32::from(alpha))
                .min(255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgba};
    use std::io::Cursor;

    fn encode(image: &RgbaImage, format: ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    #[test]
    fn decodes_png_with_alpha() {
        let mut source = RgbaImage::from_pixel(5, 3, Rgba([200, 30, 10, 255]));
        source.put_pixel(4, 2, Rgba([0, 0, 255, 128]));

        let decoded = decode(&encode(&source, ImageFormat::Png)).unwrap();

        assert_eq!(decoded.dimensions(), (5, 3));
        assert_eq!(decoded.get_pixel(0, 0), &Rgba([200, 30, 10, 255]));
        let p = decoded.get_pixel(4, 2);
        assert_eq!(p[3], 128);
        assert!(p[2] >= 250 && p[0] <= 2, "{p:?}");
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(decode(b"not an image").unwrap_err(), DecodeError::Unsupported);
    }

    #[test]
    fn unpremultiply_restores_color() {
        let mut pixels = [64, 32, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0];
        unpremultiply(&mut pixels);
        assert_eq!(pixels, [128, 64, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0]);
    }
}
