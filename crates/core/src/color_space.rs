//! 画像の色空間（旧版にはない）: 広い色域（Display P3 など）の写真は Display P3 のまま扱い、保存するときに
//! Display P3 の色のプロファイル（ICC）を付ける。sRGB の写真は今まで通り sRGB。
//!
//! 画素の値は 8bit の RGBA のまま（色空間が違うだけ）。加工の計算は色空間によらず同じ式を使う。
//! 色のプロファイルを付けられない形式（GIF・BMP）で保存するときは、sRGB に直してから書き出す。

use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PIXELS_PER_TASK;

/// 画像の色空間。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorSpace {
    #[default]
    Srgb,
    /// Display P3（iPhone などの写真。sRGB より鮮やかな色まで表せる）
    DisplayP3,
}

impl ColorSpace {
    /// 画面に出す名前。
    pub fn label(self) -> &'static str {
        match self {
            ColorSpace::Srgb => "sRGB",
            ColorSpace::DisplayP3 => "Display P3",
        }
    }
}

/// Display P3 の色のプロファイル（ICC）。sRGB は付けない（付けなくても sRGB として扱われる）ので None。
pub fn icc_profile(space: ColorSpace) -> Option<&'static [u8]> {
    match space {
        ColorSpace::Srgb => None,
        ColorSpace::DisplayP3 => display_p3_icc(),
    }
}

#[cfg(target_os = "macos")]
fn display_p3_icc() -> Option<&'static [u8]> {
    use objc2_core_graphics::{kCGColorSpaceDisplayP3, CGColorSpace};
    static ICC: std::sync::OnceLock<Option<Vec<u8>>> = std::sync::OnceLock::new();
    ICC.get_or_init(|| {
        // SAFETY: 定数の名前から色空間を作るだけ
        let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceDisplayP3 }))?;
        CGColorSpace::icc_data(Some(&space)).map(|data| data.to_vec())
    })
    .as_deref()
}

#[cfg(not(target_os = "macos"))]
fn display_p3_icc() -> Option<&'static [u8]> {
    None
}

/// sRGB の値（0〜1）を線形の値にする。Display P3 も同じ曲線。
fn to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Display P3（線形）から sRGB（線形）への変換（どちらも白は D65）。
const P3_TO_SRGB: [[f32; 3]; 3] = [
    [1.224_940_2, -0.224_940_2, 0.0],
    [-0.042_056_955, 1.042_057, 0.0],
    [-0.019_637_555, -0.078_636_05, 1.098_273_6],
];

/// Display P3 の画像を sRGB に直す（sRGB の外の色は切る。透過はそのまま）。
pub fn p3_to_srgb(image: &mut RgbaImage) {
    let table: [f32; 256] = std::array::from_fn(|i| to_linear(i as f32 / 255.0));
    image.as_mut().par_chunks_exact_mut(4).with_min_len(PIXELS_PER_TASK).for_each(|p| {
        let rgb = [table[usize::from(p[0])], table[usize::from(p[1])], table[usize::from(p[2])]];
        for (c, row) in P3_TO_SRGB.iter().enumerate() {
            let v = row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2];
            p[c] = (from_linear(v) * 255.0).round() as u8;
        }
    });
}

/// JPEG に色のプロファイル（APP2 の ICC_PROFILE）を入れる。先頭の APP0〜APP15（JFIF・EXIF）の後ろに置く。
/// JPEG として読めない・プロファイルが大きすぎる（1 つの区切りに入らない）ときは None。
pub fn insert_icc_into_jpeg(jpeg: &[u8], icc: &[u8]) -> Option<Vec<u8>> {
    const HEADER: &[u8] = b"ICC_PROFILE\0";
    let length = 2 + HEADER.len() + 2 + icc.len();
    if jpeg.len() < 4 || jpeg[..2] != [0xFF, 0xD8] || length > usize::from(u16::MAX) {
        return None;
    }
    // APP の区切りを飛ばした位置に入れる
    let mut at = 2;
    while at + 4 <= jpeg.len() && jpeg[at] == 0xFF && (0xE0..=0xEF).contains(&jpeg[at + 1]) {
        let size = usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
        at += 2 + size;
    }
    if at > jpeg.len() {
        return None;
    }
    let mut out = Vec::with_capacity(jpeg.len() + length + 2);
    out.extend_from_slice(&jpeg[..at]);
    out.extend_from_slice(&[0xFF, 0xE2]);
    out.extend_from_slice(&(length as u16).to_be_bytes());
    out.extend_from_slice(HEADER);
    out.extend_from_slice(&[1, 1]); // 1 つ目（全部で 1 つ）
    out.extend_from_slice(icc);
    out.extend_from_slice(&jpeg[at..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn p3_colors_become_srgb() {
        // 白・黒・灰色はそのまま、P3 の純色の赤は sRGB では切れて赤いまま
        let mut image = RgbaImage::from_fn(4, 1, |x, _| {
            [
                Rgba([255, 255, 255, 255]),
                Rgba([0, 0, 0, 0]),
                Rgba([128, 128, 128, 255]),
                Rgba([255, 0, 0, 255]),
            ][x as usize]
        });
        p3_to_srgb(&mut image);
        assert_eq!(image.get_pixel(0, 0).0, [255, 255, 255, 255]);
        assert_eq!(image.get_pixel(1, 0).0, [0, 0, 0, 0]);
        let gray = image.get_pixel(2, 0).0;
        assert!(gray[..3].iter().all(|&v| v.abs_diff(128) <= 1), "{gray:?}");
        let red = image.get_pixel(3, 0).0;
        assert!(red[0] == 255 && red[1] == 0 && red[2] == 0, "{red:?}");
        // P3 のくすんだ赤は、sRGB ではより鮮やかな値になる
        let mut dull = RgbaImage::from_pixel(1, 1, Rgba([200, 100, 100, 255]));
        p3_to_srgb(&mut dull);
        let p = dull.get_pixel(0, 0).0;
        assert!(p[0] > 200 && p[1] < 100, "{p:?}");
    }

    #[test]
    fn icc_goes_after_the_app_segments() {
        let jpeg = crate::encode::to_jpeg(&RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255])), 80);
        let icc = vec![7u8; 100];
        let out = insert_icc_into_jpeg(&jpeg, &icc).unwrap();
        let at = out.windows(12).position(|w| w == b"ICC_PROFILE\0").unwrap();
        assert_eq!(&out[at - 4..at - 2], &[0xFF, 0xE2]);
        assert_eq!(&out[at + 14..at + 114], icc.as_slice());
        assert!(image::load_from_memory(&out).is_ok(), "JPEG として読める");
        assert!(insert_icc_into_jpeg(b"not a jpeg", &icc).is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn display_p3_profile_exists() {
        let icc = icc_profile(ColorSpace::DisplayP3).expect("Display P3 のプロファイル");
        assert!(icc.len() > 100 && &icc[36..40] == b"acsp", "ICC のファイルの印");
        assert!(icc_profile(ColorSpace::Srgb).is_none());
    }
}
