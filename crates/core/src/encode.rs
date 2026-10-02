//! 書き出し（JPEG）。

use image::RgbaImage;

/// JPEG にする（quality は 1〜100）。アルファは捨てる。
pub fn to_jpeg(image: &RgbaImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let encoder = jpeg_encoder::Encoder::new(&mut out, quality.clamp(1, 100));
    encoder
        .encode(image.as_raw(), image.width() as u16, image.height() as u16, jpeg_encoder::ColorType::Rgba)
        .expect("RGBA の画素は JPEG にできる");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn jpeg_round_trip() {
        let image = RgbaImage::from_pixel(32, 16, Rgba([200, 100, 50, 255]));
        let jpeg = to_jpeg(&image, 90);
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        let back = image::load_from_memory(&jpeg).unwrap().to_rgba8();
        assert_eq!(back.dimensions(), (32, 16));
        let p = back.get_pixel(8, 8);
        assert!(p[0].abs_diff(200) < 6 && p[1].abs_diff(100) < 6 && p[2].abs_diff(50) < 6, "{p:?}");
    }
}
