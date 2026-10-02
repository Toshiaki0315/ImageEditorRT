//! 縮小（fast_image_resize の Lanczos3。SIMD と rayon で速く縮める）。

use fast_image_resize::{images::Image, images::ImageRef, PixelType, Resizer};
use image::RgbaImage;

/// 長辺が max_side px 以下になるように縦横比を保って縮める（もともと小さければそのまま）。
pub fn fit_long_side(image: &RgbaImage, max_side: u32) -> RgbaImage {
    let (width, height) = image.dimensions();
    let long = width.max(height);
    if long <= max_side || max_side == 0 {
        return image.clone();
    }
    let scale = max_side as f64 / long as f64;
    let new_width = ((width as f64 * scale).round() as u32).max(1);
    let new_height = ((height as f64 * scale).round() as u32).max(1);
    resize(image, new_width, new_height)
}

/// 指定の大きさに縮める・広げる（Lanczos3）。
pub fn resize(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let src = ImageRef::new(image.width(), image.height(), image.as_raw(), PixelType::U8x4)
        .expect("RgbaImage の大きさと画素の並びは正しい");
    let mut dst = Image::new(width, height, PixelType::U8x4);
    Resizer::new().resize(&src, &mut dst, None).expect("同じ画素の形式どうしなので縮められる");
    RgbaImage::from_raw(width, height, dst.into_vec()).expect("大きさは指定のとおり")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn fits_long_side_and_keeps_aspect() {
        let image = RgbaImage::from_pixel(600, 400, Rgba([10, 20, 30, 255]));
        let small = fit_long_side(&image, 300);
        assert_eq!(small.dimensions(), (300, 200));
        assert_eq!(small.get_pixel(150, 100), &Rgba([10, 20, 30, 255]));
        let tall = fit_long_side(&RgbaImage::new(400, 600), 300);
        assert_eq!(tall.dimensions(), (200, 300));
    }

    #[test]
    fn small_image_is_unchanged() {
        let image = RgbaImage::from_pixel(100, 50, Rgba([1, 2, 3, 4]));
        assert_eq!(fit_long_side(&image, 1600), image);
    }
}
