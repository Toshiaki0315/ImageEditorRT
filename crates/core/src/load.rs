//! 画像のファイルを読む: 拡張子を確かめ、中身を画像にし、元の EXIF と表示用の EXIF の情報を取り出す。
//!
//! 画像を開くとき・まとめて処理・起動確認で同じものを使う（macOS の ImageIO で読むので macOS のみ）。

use std::fmt;
use std::path::Path;

use crate::decode::{self, DecodeError, Decoded};
use crate::exif_info::{raw_exif, read_exif_info, ExifInfo};
use crate::formats;

/// 読み込んだファイル。
pub struct LoadedFile {
    /// 向きを直した画像・中身の形式・フレームの数
    pub decoded: Decoded,
    /// 元の EXIF（TIFF の部分）。保存のときに残すのに使う。なければ None
    pub raw_exif: Option<Vec<u8>>,
    /// 「EXIF」タブに出す情報（EXIF が壊れていても画像は開くので、そのときは空）
    pub exif: ExifInfo,
}

/// 読めなかった理由。
#[derive(Debug)]
pub enum LoadError {
    /// 対応していない拡張子（"(なし)" または ".txt" の形）
    UnsupportedExtension(String),
    /// ファイルを読めない
    Read(std::io::Error),
    /// 中身を画像にできない
    Decode(DecodeError),
}

impl LoadError {
    /// ダイアログに出す説明（読めないときはファイル名を添える。旧版 FR-IO-10）。
    pub fn message(&self, name: &str) -> String {
        match self {
            LoadError::UnsupportedExtension(_) | LoadError::Decode(DecodeError::UnsupportedFormat(_)) => {
                self.to_string()
            }
            LoadError::Read(e) => format!("画像を読み込めません: {name}\n({e})"),
            LoadError::Decode(e) => format!("画像を読み込めません: {name}\n({e})"),
        }
    }
}

/// 短い説明（まとめて処理の「処理できなかった画像」の一覧など、ファイル名を別に出すところで使う）。
impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::UnsupportedExtension(ext) => write!(f, "対応していない拡張子です: {ext}"),
            LoadError::Read(e) => write!(f, "読み込めません（{e}）"),
            LoadError::Decode(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// path の画像を読む。EXIF が読めなくても画像は返す（EXIF なしとして扱う）。
pub fn load_file(path: &Path) -> Result<LoadedFile, LoadError> {
    if !formats::is_supported(path) {
        let ext = path.extension().map_or("(なし)".into(), |e| format!(".{}", e.to_string_lossy()));
        return Err(LoadError::UnsupportedExtension(ext));
    }
    let bytes = std::fs::read(path).map_err(LoadError::Read)?;
    let decoded = decode::decode_file(&bytes).map_err(LoadError::Decode)?;
    Ok(LoadedFile { decoded, raw_exif: raw_exif(&bytes), exif: read_exif_info(&bytes) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::Format;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    #[test]
    fn reads_image_and_exif() {
        let loaded = load_file(&fixture("pentax.jpg")).unwrap();
        assert_eq!(loaded.decoded.format, Format::Jpeg);
        assert_eq!(loaded.decoded.image.dimensions(), (64, 48));
        assert!(loaded.raw_exif.is_some());
        assert!(!loaded.exif.empty);
        assert_eq!(loaded.exif.maker_note.as_deref(), Some("Pentax"));
    }

    #[test]
    fn rejects_unsupported_extensions() {
        let error = load_file(Path::new("/tmp/notes.txt")).err().unwrap();
        assert_eq!(error.to_string(), "対応していない拡張子です: .txt");
        let error = load_file(Path::new("/tmp/noextension")).err().unwrap();
        assert_eq!(error.message("noextension"), "対応していない拡張子です: (なし)");
    }

    #[test]
    fn reports_read_and_decode_errors_with_the_name() {
        let missing = load_file(Path::new("/no/such/photo.png")).err().unwrap();
        assert!(matches!(missing, LoadError::Read(_)));
        assert!(
            missing.message("photo.png").starts_with("画像を読み込めません: photo.png\n("),
            "{}",
            missing.message("photo.png")
        );
        let dir = std::env::temp_dir().join(format!("imageeditorrt-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("broken.png");
        std::fs::write(&broken, b"not a png").unwrap();
        let error = load_file(&broken).err().unwrap();
        assert!(matches!(error, LoadError::Decode(DecodeError::Unsupported)));
        assert_eq!(error.to_string(), DecodeError::Unsupported.to_string());
        assert!(error.message("broken.png").starts_with("画像を読み込めません: broken.png\n"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn image_without_exif() {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-load-noexif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plain.png");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255])).save(&path).unwrap();
        let loaded = load_file(&path).unwrap();
        assert_eq!(
            (loaded.decoded.format, loaded.raw_exif.is_none(), loaded.exif.empty),
            (Format::Png, true, true)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
