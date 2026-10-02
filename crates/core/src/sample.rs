//! 計測用の画像。

use image::RgbaImage;

/// なめらかな部分と細かい模様のある、写真に近い画像を作る（ベンチマーク・計測モードで使う）。
pub fn synthetic_photo(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_par_fn(width, height, |x, y| {
        let fx = x as f32 / width as f32;
        let fy = y as f32 / height as f32;
        let fine = if (x / 3 + y / 5) % 2 == 0 { 25 } else { 0 };
        image::Rgba([
            (fx * 200.0) as u8 + fine,
            (fy * 180.0) as u8 + 30,
            ((1.0 - fx) * 150.0) as u8 + fine,
            255,
        ])
    })
}
