//! 読み込める画像の形式（拡張子と中身の形式）。旧版の core/io.py の拡張子の扱いと同じ。

use std::path::Path;

use image::RgbaImage;
use rayon::prelude::*;
use serde::Serialize;

/// 読み込める画像の形式。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Format {
    Png,
    Jpeg,
    Gif,
    Tiff,
    Bmp,
    /// HEIC / HEIF（読み込みのみ）
    Heif,
}

/// 読み込める拡張子（小文字・ドットなし）。
pub const SUPPORTED_EXTENSIONS: [&str; 9] =
    ["png", "jpg", "jpeg", "gif", "tif", "tiff", "bmp", "heic", "heif"];

/// エラーのダイアログに添える、対応形式の説明。
pub const FORMATS_TEXT: &str = "PNG / JPEG / GIF / TIFF / BMP（HEIC / HEIF は読み込みのみ）";

impl Format {
    /// 拡張子から形式を決める（大文字・小文字は区別しない）。読めない拡張子なら None。
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "gif" => Some(Self::Gif),
            "tif" | "tiff" => Some(Self::Tiff),
            "bmp" => Some(Self::Bmp),
            "heic" | "heif" => Some(Self::Heif),
            _ => None,
        }
    }

    /// macOS の UTI（ImageIO が返す中身の形式）から形式を決める。読めない形式なら None。
    pub fn from_uti(uti: &str) -> Option<Self> {
        match uti {
            "public.png" => Some(Self::Png),
            "public.jpeg" => Some(Self::Jpeg),
            "com.compuserve.gif" => Some(Self::Gif),
            "public.tiff" => Some(Self::Tiff),
            "com.microsoft.bmp" => Some(Self::Bmp),
            "public.heic" | "public.heif" => Some(Self::Heif),
            _ => None,
        }
    }

    /// 表示用の名前。
    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Gif => "GIF",
            Self::Tiff => "TIFF",
            Self::Bmp => "BMP",
            Self::Heif => "HEIC",
        }
    }
}

/// 読み込める拡張子のファイルか（大文字・小文字は区別しない）。
pub fn is_supported(path: &Path) -> bool {
    Format::from_path(path).is_some()
}

/// 透明・半透明の画素があるか（プレビューで市松模様を出すかに使う）。
pub fn has_transparency(image: &RgbaImage) -> bool {
    image.as_raw().par_chunks_exact(4).with_min_len(crate::PIXELS_PER_TASK).any(|p| p[3] != 255)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn extensions_ignore_case() {
        assert_eq!(Format::from_path(Path::new("a/B.JPG")), Some(Format::Jpeg));
        assert_eq!(Format::from_path(Path::new("x.HeIc")), Some(Format::Heif));
        assert_eq!(Format::from_path(Path::new("x.tiff")), Some(Format::Tiff));
        assert!(!is_supported(Path::new("x.webp")));
        assert!(!is_supported(Path::new("noext")));
        for ext in SUPPORTED_EXTENSIONS {
            assert!(is_supported(Path::new(&format!("a.{ext}"))), "{ext}");
        }
    }

    #[test]
    fn utis() {
        assert_eq!(Format::from_uti("public.jpeg"), Some(Format::Jpeg));
        assert_eq!(Format::from_uti("org.webmproject.webp"), None);
    }

    #[test]
    fn transparency() {
        let mut image = RgbaImage::from_pixel(4, 4, Rgba([1, 2, 3, 255]));
        assert!(!has_transparency(&image));
        image.put_pixel(3, 3, Rgba([1, 2, 3, 254]));
        assert!(has_transparency(&image));
    }
}
