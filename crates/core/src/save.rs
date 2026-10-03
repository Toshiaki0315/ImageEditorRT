//! 保存（形式・名前の決め方・EXIF を残す）。旧版の core/io.py の保存の部分を移したもの。

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::codecs::bmp::BmpEncoder;
use image::codecs::gif::GifEncoder;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, Frame, ImageEncoder, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::formats::has_transparency;
use crate::tiff::{tiff_block, ExifBlock, Order, TAG_ORIENTATION, TAG_PIXEL_X, TAG_PIXEL_Y};

/// JPEG の品質の既定値と範囲。
pub const DEFAULT_JPEG_QUALITY: u8 = 90;
pub const JPEG_QUALITY_MIN: u8 = 1;
pub const JPEG_QUALITY_MAX: u8 = 100;
/// 読み込みだけできる形式（HEIC など）の画像を保存するときの拡張子。
pub const FALLBACK_SAVE_SUFFIX: &str = "jpg";
/// JPEG の APP1 に入る EXIF の大きさの上限（これを超えると MakerNote を外す）。
pub const MAX_EXIF_BYTES: usize = 65533;
/// 保存できる拡張子（小文字・ドットなし）。
pub const SAVABLE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "tif", "tiff", "bmp"];
/// 元の画像と同じファイルを選んだときの説明。
pub const SAME_FILE_MESSAGE: &str =
    "元の画像と同じファイルには保存できません。別のファイル名を指定してください。";

/// 保存の設定（「出力」タブの「保存の設定」）。JSON では camelCase。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SaveOptions {
    /// JPEG の品質 1〜100
    pub quality: u8,
    /// 撮影日時などの EXIF を残す（JPEG・PNG・TIFF）
    pub keep_exif: bool,
    /// EXIF を残すとき、位置情報 (GPS) も残す
    pub keep_gps: bool,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self { quality: DEFAULT_JPEG_QUALITY, keep_exif: true, keep_gps: false }
    }
}

/// 保存できる形式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFormat {
    Png,
    Jpeg,
    Gif,
    Tiff,
    Bmp,
}

impl SaveFormat {
    /// 拡張子から形式を決める（大文字・小文字は区別しない）。保存できない拡張子なら None。
    pub fn from_path(path: &Path) -> Option<Self> {
        match extension(path)?.as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "gif" => Some(Self::Gif),
            "tif" | "tiff" => Some(Self::Tiff),
            "bmp" => Some(Self::Bmp),
            _ => None,
        }
    }

    /// 透過を持てない形式か（白い背景に合成する）。
    fn is_opaque(self) -> bool {
        matches!(self, Self::Jpeg | Self::Bmp)
    }

    /// EXIF を書き込める形式か。
    fn has_exif(self) -> bool {
        matches!(self, Self::Jpeg | Self::Png | Self::Tiff)
    }
}

/// 保存できなかった理由。
#[derive(Debug)]
pub enum SaveError {
    /// 保存できない拡張子（ドットなし。拡張子がなければ空）
    UnsupportedExtension(String),
    /// JPEG の品質が範囲外
    Quality(u8),
    /// 書き出し・書き込みに失敗した
    Write(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedExtension(e) if e.is_empty() => write!(f, "対応していない拡張子です: (なし)"),
            Self::UnsupportedExtension(e) => write!(f, "対応していない拡張子です: .{e}"),
            Self::Quality(q) => {
                write!(f, "quality は {JPEG_QUALITY_MIN}〜{JPEG_QUALITY_MAX} で指定してください: {q}")
            }
            Self::Write(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for SaveError {}

fn extension(path: &Path) -> Option<String> {
    Some(path.extension()?.to_str()?.to_ascii_lowercase())
}

/// 保存できる拡張子のファイルか（大文字・小文字は区別しない）。
pub fn is_savable(path: &Path) -> bool {
    SaveFormat::from_path(path).is_some()
}

/// 元の画像を保存するときの拡張子（元のつづりのまま。保存できない形式なら jpg）。
pub fn save_suffix(source: &Path) -> String {
    match source.extension().and_then(|e| e.to_str()) {
        Some(ext) if is_savable(source) => ext.to_string(),
        _ => FALLBACK_SAVE_SUFFIX.to_string(),
    }
}

/// folder に保存するときの名前の候補 `<元の名前>_edited`、`_edited_2` … を順に返す（終わりなく続く）。
pub fn edited_names(source: &Path, folder: &Path) -> impl Iterator<Item = PathBuf> {
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let suffix = save_suffix(source);
    let folder = folder.to_path_buf();
    (1..).map(move |n| {
        let number = if n == 1 { String::new() } else { format!("_{n}") };
        folder.join(format!("{stem}_edited{number}.{suffix}"))
    })
}

/// 保存ダイアログの初期のパス。すでにあれば `_edited_2`、`_edited_3` … と重ならない名前にする。
pub fn default_save_path(source: &Path) -> PathBuf {
    let folder = source.parent().unwrap_or(Path::new("."));
    edited_names(source, folder).find(|p| !p.exists()).expect("候補は終わりなく続く")
}

/// クリップボードから貼り付けた画像の表示名。
pub const PASTED_NAME: &str = "クリップボードの画像";

/// 貼り付けた画像の保存ダイアログの初期のパス `<folder>/クリップボード_<stamp>.png`。
///
/// stamp は日時（"20261002-064500" の形。ダイアログを開いたとき）。同じ名前があれば `_2` … を付ける。
pub fn pasted_save_path(stamp: &str, folder: &Path) -> PathBuf {
    (1..)
        .map(|n| {
            let number = if n == 1 { String::new() } else { format!("_{n}") };
            folder.join(format!("クリップボード_{stamp}{number}.png"))
        })
        .find(|p| !p.exists())
        .expect("候補は終わりなく続く")
}

/// 2 つのパスが同じファイルを指すか。
///
/// macOS のファイルシステムは大文字・小文字を区別しないので、実在するファイルは中身の場所
/// （デバイスと inode）で判定し、まだないファイルは絶対パスを大文字・小文字を無視して比べる。
pub fn is_same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(x), Ok(y)) = (std::fs::metadata(a), std::fs::metadata(b)) {
            return x.dev() == y.dev() && x.ino() == y.ino();
        }
    }
    let key = |p: &Path| {
        let absolute = match p.parent().and_then(|d| d.canonicalize().ok()) {
            Some(dir) => dir.join(p.file_name().unwrap_or_default()),
            None => p.to_path_buf(),
        };
        absolute.to_string_lossy().to_lowercase()
    };
    key(a) == key(b)
}

/// 元画像の EXIF を、編集後の画像に書き込める形（"Exif\0\0" 付き）に整える。
///
/// - 向き (Orientation) は読み込み時に補正済みなので 1（そのまま）にする
/// - Exif IFD の画像の幅・高さを size に合わせる
/// - keep_gps が false なら位置情報 (GPS) を取り除く
/// - MakerNote は元と同じ位置に置き、中の値の位置がずれないようにする。keep_maker_note が
///   false のとき・EXIF が JPEG に入らないほど大きいときは残さない
/// - サムネイルは編集前の画像なので残さない
///
/// 元の EXIF の形を読めなければ None（EXIF なしで保存する）。
pub fn prepare_exif(exif: &[u8], size: (u32, u32), keep_gps: bool, keep_maker_note: bool) -> Option<Vec<u8>> {
    prepare(exif, size, keep_gps, keep_maker_note, false)
}

/// prepare_exif と同じ。from_tiff なら、TIFF の画像の構造のタグも外す。
fn prepare(
    exif: &[u8],
    (width, height): (u32, u32),
    keep_gps: bool,
    keep_maker_note: bool,
    from_tiff: bool,
) -> Option<Vec<u8>> {
    let mut block = ExifBlock::parse(exif)?;
    if from_tiff {
        block.remove_image_structure();
    }
    block.set_short(false, TAG_ORIENTATION, 1);
    if !keep_gps {
        block.gps = None;
    }
    for (tag, length) in [(TAG_PIXEL_X, width), (TAG_PIXEL_Y, height)] {
        if block.exif.contains_key(&tag) {
            block.set_long(true, tag, length);
        }
    }
    let data = block.to_bytes(keep_maker_note);
    Some(if keep_maker_note && data.len() > MAX_EXIF_BYTES { block.to_bytes(false) } else { data })
}

/// 透過を白い背景に合成した RGB の画素を返す。
fn flatten_alpha(image: &RgbaImage) -> Vec<u8> {
    image
        .as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = u32::from(p[3]);
            std::array::from_fn::<u8, 3, _>(|c| ((u32::from(p[c]) * a + 255 * (255 - a) + 127) / 255) as u8)
        })
        .collect()
}

/// アルファを捨てた RGB の画素を返す（不透明な画像を RGB で保存するとき）。
fn rgb(image: &RgbaImage) -> Vec<u8> {
    image.as_raw().as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect()
}

/// 画像をその形式のバイト列にする。exif は prepare_exif で整えたもの（"Exif\0\0" 付き）。
///
/// JPEG・BMP は透過を白い背景に合成して RGB にする。PNG・TIFF は透過がなければ RGB にする。
pub fn encode(
    image: &RgbaImage,
    format: SaveFormat,
    quality: u8,
    exif: Option<&[u8]>,
) -> Result<Vec<u8>, SaveError> {
    if !(JPEG_QUALITY_MIN..=JPEG_QUALITY_MAX).contains(&quality) {
        return Err(SaveError::Quality(quality));
    }
    let (width, height) = image.dimensions();
    let exif = exif.filter(|_| format.has_exif());
    let alpha = !format.is_opaque() && has_transparency(image);
    let write_error = |e: &dyn std::fmt::Display| SaveError::Write(e.to_string());
    let mut out = Vec::new();
    match format {
        SaveFormat::Jpeg => {
            if width > u32::from(u16::MAX) || height > u32::from(u16::MAX) {
                return Err(SaveError::Write("JPEG にできる大きさ（65535px）を超えています".into()));
            }
            let mut encoder = jpeg_encoder::Encoder::new(&mut out, quality);
            // Pillow（旧版）と同じく、色の間引きは 4:2:0
            encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
            encoder
                .encode(&flatten_alpha(image), width as u16, height as u16, jpeg_encoder::ColorType::Rgb)
                .map_err(|e| write_error(&e))?;
            if let Some(exif) = exif {
                out = crate::tiff::insert_exif_into_jpeg(&out, exif).unwrap_or(out);
            }
        }
        SaveFormat::Png => {
            let mut encoder = PngEncoder::new(&mut out);
            if let Some(tiff) = exif.and_then(tiff_block) {
                // PNG の eXIf には先頭の "Exif\0\0" を付けない
                encoder.set_exif_metadata(tiff.to_vec()).map_err(|e| write_error(&e))?;
            }
            let (pixels, color) = if alpha {
                (image.as_raw().clone(), ExtendedColorType::Rgba8)
            } else {
                (rgb(image), ExtendedColorType::Rgb8)
            };
            encoder.write_image(&pixels, width, height, color).map_err(|e| write_error(&e))?;
        }
        SaveFormat::Gif => {
            let mut encoder = GifEncoder::new(&mut out);
            encoder.encode_frame(Frame::new(image.clone())).map_err(|e| write_error(&e))?;
            drop(encoder);
        }
        SaveFormat::Bmp => {
            BmpEncoder::new(&mut Cursor::new(&mut out))
                .write_image(&flatten_alpha(image), width, height, ExtendedColorType::Rgb8)
                .map_err(|e| write_error(&e))?;
        }
        SaveFormat::Tiff => {
            let block = exif.and_then(ExifBlock::parse).unwrap_or_else(|| ExifBlock::empty(Order::Little));
            out = if alpha {
                block.to_tiff_image(width, height, 4, image.as_raw())
            } else {
                block.to_tiff_image(width, height, 3, &rgb(image))
            };
        }
    }
    Ok(out)
}

/// 編集した画像を「保存の設定」に従ってファイルに保存する。
///
/// source_exif は元画像の EXIF（TIFF の部分。"Exif\0\0" 付きでもよい）。keep_exif のときだけ書き込む。
/// source_is_tiff なら、EXIF に入っている TIFF の画像の構造のタグを外す。
/// TIFF で保存するときは MakerNote を残さない（旧版と同じ）。
pub fn save_edited(
    image: &RgbaImage,
    path: &Path,
    options: SaveOptions,
    source_exif: Option<&[u8]>,
    source_is_tiff: bool,
) -> Result<(), SaveError> {
    let format = SaveFormat::from_path(path)
        .ok_or_else(|| SaveError::UnsupportedExtension(extension(path).unwrap_or_default()))?;
    // TIFF で保存するとき・TIFF から読んだ EXIF は、MakerNote を残さない（旧版と同じ）
    let keep_maker_note = format != SaveFormat::Tiff && !source_is_tiff;
    let exif = source_exif
        .filter(|_| options.keep_exif)
        .and_then(|raw| prepare(raw, image.dimensions(), options.keep_gps, keep_maker_note, source_is_tiff));
    let bytes = encode(image, format, options.quality, exif.as_deref())?;
    std::fs::write(path, bytes).map_err(|e| SaveError::Write(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn pasted_names() {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-pasted-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let first = pasted_save_path("20261002-064500", &dir);
        assert_eq!(first, dir.join("クリップボード_20261002-064500.png"));
        std::fs::write(&first, b"x").unwrap();
        assert_eq!(
            pasted_save_path("20261002-064500", &dir),
            dir.join("クリップボード_20261002-064500_2.png")
        );
        std::fs::write(dir.join("クリップボード_20261002-064500_2.png"), b"x").unwrap();
        assert_eq!(
            pasted_save_path("20261002-064500", &dir),
            dir.join("クリップボード_20261002-064500_3.png")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn suffixes_and_names() {
        assert_eq!(save_suffix(Path::new("/a/photo.JPG")), "JPG");
        assert_eq!(save_suffix(Path::new("/a/photo.heic")), "jpg");
        let names: Vec<_> = edited_names(Path::new("/a/photo.png"), Path::new("/b")).take(3).collect();
        assert_eq!(
            names,
            [
                PathBuf::from("/b/photo_edited.png"),
                "/b/photo_edited_2.png".into(),
                "/b/photo_edited_3.png".into()
            ]
        );
        assert!(
            is_savable(Path::new("x.TIFF"))
                && !is_savable(Path::new("x.heic"))
                && !is_savable(Path::new("x"))
        );
    }

    #[test]
    fn default_path_skips_existing() {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("photo.heic");
        assert_eq!(default_save_path(&source), dir.join("photo_edited.jpg"));
        std::fs::write(dir.join("photo_edited.jpg"), b"x").unwrap();
        assert_eq!(default_save_path(&source), dir.join("photo_edited_2.jpg"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn same_file_ignores_case() {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-same-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Photo.jpg");
        std::fs::write(&file, b"x").unwrap();
        assert!(is_same_file(&file, &dir.join("photo.JPG")));
        assert!(!is_same_file(&file, &dir.join("photo.png")));
        // まだないファイルどうし
        assert!(is_same_file(&dir.join("New.png"), &dir.join("new.PNG")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flatten_on_white() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 0]));
        assert_eq!(flatten_alpha(&image), [255, 255, 255]);
        let image = RgbaImage::from_pixel(1, 1, Rgba([0, 100, 200, 128]));
        assert_eq!(flatten_alpha(&image), [127, 177, 227]);
    }

    #[test]
    fn quality_out_of_range() {
        let image = RgbaImage::new(2, 2);
        assert!(matches!(encode(&image, SaveFormat::Jpeg, 0, None), Err(SaveError::Quality(0))));
        assert!(matches!(encode(&image, SaveFormat::Jpeg, 101, None), Err(SaveError::Quality(101))));
    }

    #[test]
    fn unsupported_extension() {
        let err =
            save_edited(&RgbaImage::new(1, 1), Path::new("/tmp/x.heic"), SaveOptions::default(), None, false)
                .unwrap_err();
        assert_eq!(err.to_string(), "対応していない拡張子です: .heic");
    }
}
