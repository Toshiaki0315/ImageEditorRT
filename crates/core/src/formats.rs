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
    /// HEIC / HEIF（保存は macOS の ImageIO で HEIC に）
    Heif,
    /// カメラの RAW（読み込みのみ。現像は macOS の ImageIO。旧版にはない）
    Raw,
}

/// カメラの RAW の拡張子（小文字・ドットなし）。DNG・キヤノン・ニコン・ソニー・富士フイルム・オリンパス／OM・
/// パナソニック・ペンタックス・サムスン・ライカ。
pub const RAW_EXTENSIONS: [&str; 14] =
    ["dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "raf", "orf", "rw2", "pef", "srw", "rwl"];

/// 読み込める拡張子（小文字・ドットなし）。
pub const SUPPORTED_EXTENSIONS: [&str; 23] = [
    "png", "jpg", "jpeg", "gif", "tif", "tiff", "bmp", "heic", "heif", "dng", "cr2", "cr3", "crw", "nef",
    "nrw", "arw", "srf", "raf", "orf", "rw2", "pef", "srw", "rwl",
];

/// エラーのダイアログに添える、対応形式の説明。
pub const FORMATS_TEXT: &str =
    "PNG / JPEG / GIF / TIFF / BMP / HEIC / HEIF（カメラの RAW（DNG・CR2・CR3・NEF・ARW・RAF など）は読み込みのみ）";

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
            raw if RAW_EXTENSIONS.contains(&raw) => Some(Self::Raw),
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
            // カメラの RAW は、どのメーカーのものも「…raw-image」（com.adobe.raw-image・com.canon.cr3-raw-image など）
            raw if raw.ends_with("raw-image") => Some(Self::Raw),
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
            Self::Raw => "RAW",
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
    fn camera_raw() {
        for name in ["a.DNG", "b.cr3", "c.NEF", "d.arw", "e.RAF", "f.orf", "g.rw2", "h.pef"] {
            assert_eq!(Format::from_path(Path::new(name)), Some(Format::Raw), "{name}");
        }
        for uti in
            ["com.adobe.raw-image", "com.canon.cr3-raw-image", "com.sony.arw-raw-image", "com.fuji.raw-image"]
        {
            assert_eq!(Format::from_uti(uti), Some(Format::Raw), "{uti}");
        }
        assert!(RAW_EXTENSIONS.iter().all(|ext| SUPPORTED_EXTENSIONS.contains(ext)));
        assert_eq!(Format::Raw.name(), "RAW");
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
