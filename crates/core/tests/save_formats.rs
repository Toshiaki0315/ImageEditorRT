//! 各形式で保存して読み直せるか、EXIF（MakerNote）が残るか（旧版 FR-IO-01〜05・12）。
#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};
use imageeditorrt_core::decode::decode_file;
use imageeditorrt_core::exif_info::{raw_exif, read_exif_info, Group};
use imageeditorrt_core::formats::Format;
use imageeditorrt_core::save::{save_edited, SaveOptions};
use imageeditorrt_core::tiff::{tiff_block, ExifBlock, TAG_MAKERNOTE};

/// テストごとの作業用のフォルダ。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

/// 左半分は不透明な赤、右半分は透明（右下の 1 マスだけ半透明の青）。
fn sample() -> RgbaImage {
    RgbaImage::from_fn(20, 10, |x, y| match (x, y) {
        (19, 9) => Rgba([0, 0, 255, 128]),
        (x, _) if x < 10 => Rgba([220, 30, 30, 255]),
        _ => Rgba([0, 0, 0, 0]),
    })
}

fn near(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
    a.iter().zip(b).all(|(&x, y)| x.abs_diff(y) <= tolerance)
}

fn save_and_read(
    dir: &TempDir,
    name: &str,
    image: &RgbaImage,
) -> (PathBuf, imageeditorrt_core::decode::Decoded) {
    let path = dir.join(name);
    save_edited(image, &path, SaveOptions::default(), None, false).unwrap();
    let decoded = decode_file(&std::fs::read(&path).unwrap()).unwrap();
    (path, decoded)
}

#[test]
fn every_format_round_trips() {
    let dir = TempDir::new("formats");
    let image = sample();
    for (name, format, keeps_alpha, tolerance) in [
        ("a.png", Format::Png, true, 0),
        ("a.tif", Format::Tiff, true, 0),
        ("a.TIFF", Format::Tiff, true, 0),
        ("a.gif", Format::Gif, true, 8),
        ("a.bmp", Format::Bmp, false, 0),
        ("a.jpg", Format::Jpeg, false, 12),
        ("a.JPEG", Format::Jpeg, false, 12),
    ] {
        let (_, d) = save_and_read(&dir, name, &image);
        assert_eq!(d.format, format, "{name}");
        assert_eq!(d.image.dimensions(), (20, 10), "{name}");
        let red = d.image.get_pixel(2, 5).0;
        assert!(near(red, [220, 30, 30, 255], tolerance), "{name}: {red:?}");
        let clear = d.image.get_pixel(14, 5).0;
        if keeps_alpha {
            assert_eq!(clear[3], 0, "{name}: 透明のまま");
        } else {
            // JPEG・BMP は透過を白い背景に合成する
            assert!(near(clear, [255, 255, 255, 255], tolerance), "{name}: {clear:?}");
        }
    }
}

#[test]
fn png_and_tiff_keep_semi_transparency() {
    let dir = TempDir::new("alpha");
    for name in ["b.png", "b.tif"] {
        let (_, d) = save_and_read(&dir, name, &sample());
        assert_eq!(d.image.get_pixel(19, 9).0, [0, 0, 255, 128], "{name}");
    }
}

#[test]
fn opaque_png_is_saved_as_rgb() {
    let dir = TempDir::new("rgb");
    let image = RgbaImage::from_pixel(8, 8, Rgba([10, 20, 30, 255]));
    let (path, d) = save_and_read(&dir, "c.png", &image);
    assert_eq!(d.image.get_pixel(3, 3).0, [10, 20, 30, 255]);
    let decoded = image::open(&path).unwrap();
    assert_eq!(decoded.color(), image::ColorType::Rgb8);
}

/// 保存した画像の EXIF（TIFF の部分）を ExifBlock で読む。
fn saved_block(path: &Path) -> ExifBlock {
    ExifBlock::parse(&raw_exif(&std::fs::read(path).unwrap()).expect("EXIF がある")).unwrap()
}

#[test]
fn jpeg_keeps_maker_note_at_original_offset() {
    let dir = TempDir::new("makernote");
    let source = raw_exif(&fixture("canon.jpg")).unwrap();
    let original = ExifBlock::parse(&source).unwrap();
    let (at, note) = original.maker_note.clone().expect("MakerNote がある");

    let path = dir.join("canon_edited.jpg");
    save_edited(&sample(), &path, SaveOptions::default(), Some(&source), false).unwrap();

    let saved = raw_exif(&std::fs::read(&path).unwrap()).unwrap();
    let tiff = tiff_block(&saved).unwrap();
    assert_eq!(&tiff[at..at + note.len()], &note[..], "MakerNote が元の位置に残る");
    let info = read_exif_info(&std::fs::read(&path).unwrap());
    assert!(info.entries.iter().any(|e| e.tag == "Make" && e.value == "Canon"));
}

#[test]
fn exif_is_tidied_for_the_edited_image() {
    let dir = TempDir::new("exif");
    let source = raw_exif(&fixture("pentax.jpg")).unwrap();
    let image = sample();

    // 既定: EXIF を残し、位置情報は外す。向きは 1
    let path = dir.join("pentax_edited.jpg");
    save_edited(&image, &path, SaveOptions::default(), Some(&source), false).unwrap();
    let info = read_exif_info(&std::fs::read(&path).unwrap());
    let value = |tag: &str| info.entries.iter().find(|e| e.tag == tag).map(|e| e.value.clone());
    assert_eq!(value("Model").as_deref(), Some("PENTAX K-1"));
    assert!(value("DateTimeOriginal").is_some());
    assert!(info.gps.is_none(), "位置情報は外す");
    assert!(info.entries.iter().all(|e| e.group != Group::Gps));
    assert_eq!(info.maker_note.as_deref(), Some("Pentax"), "MakerNote も読める");
    let block = saved_block(&path);
    assert_eq!(block.order.u16(&block.ifd0[&0x0112].data), 1, "向きは 1（読み込み時に直してある）");

    // 位置情報も残す
    let options = SaveOptions { keep_gps: true, ..SaveOptions::default() };
    save_edited(&image, &path, options, Some(&source), false).unwrap();
    assert!(read_exif_info(&std::fs::read(&path).unwrap()).gps.is_some());

    // EXIF を残さない
    let options = SaveOptions { keep_exif: false, ..SaveOptions::default() };
    save_edited(&image, &path, options, Some(&source), false).unwrap();
    assert!(raw_exif(&std::fs::read(&path).unwrap()).is_none());
}

#[test]
fn png_and_tiff_carry_exif() {
    let dir = TempDir::new("exif-formats");
    let source = raw_exif(&fixture("pentax.jpg")).unwrap();
    for name in ["d.png", "d.tif"] {
        let path = dir.join(name);
        save_edited(&sample(), &path, SaveOptions::default(), Some(&source), false).unwrap();
        let info = read_exif_info(&std::fs::read(&path).unwrap());
        assert!(info.entries.iter().any(|e| e.tag == "Model" && e.value == "PENTAX K-1"), "{name}");
        // 読み直しても画像は同じ
        let d = decode_file(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(d.image.get_pixel(2, 5).0, [220, 30, 30, 255], "{name}");
    }
    // TIFF には MakerNote を残さない（旧版と同じ）
    let block = saved_block(&dir.join("d.tif"));
    assert!(block.maker_note.is_none());
    assert!(!block.exif.contains_key(&TAG_MAKERNOTE));
}

#[test]
fn exif_from_tiff_drops_image_structure() {
    let dir = TempDir::new("from-tiff");
    let source = raw_exif(&fixture("pentax.jpg")).unwrap();
    let tiff = dir.join("source.tif");
    save_edited(&sample(), &tiff, SaveOptions::default(), Some(&source), false).unwrap();

    // TIFF のファイルから読んだ EXIF には、画像の構造のタグ（幅・ストリップなど）が入っている
    let tiff_exif = raw_exif(&std::fs::read(&tiff).unwrap()).unwrap();
    assert!(ExifBlock::parse(&tiff_exif).unwrap().ifd0.contains_key(&0x0100));

    let path = dir.join("from_tiff.jpg");
    save_edited(&sample(), &path, SaveOptions::default(), Some(&tiff_exif), true).unwrap();
    let block = saved_block(&path);
    for tag in [0x0100, 0x0101, 0x0102, 0x0103, 0x0111, 0x0115, 0x0117] {
        assert!(!block.ifd0.contains_key(&tag), "0x{tag:04x} は外す");
    }
    assert!(read_exif_info(&std::fs::read(&path).unwrap()).entries.iter().any(|e| e.tag == "Model"));
}

#[test]
fn exif_pixel_size_follows_the_saved_image() {
    use imageeditorrt_core::save::prepare_exif;
    use imageeditorrt_core::tiff::{TAG_PIXEL_X, TAG_PIXEL_Y};
    let mut block = ExifBlock::parse(&raw_exif(&fixture("pentax.jpg")).unwrap()).unwrap();
    block.set_long(true, TAG_PIXEL_X, 6000);
    block.set_long(true, TAG_PIXEL_Y, 4000);
    let source = block.to_bytes(true);

    let prepared = ExifBlock::parse(&prepare_exif(&source, (300, 200), false, true).unwrap()).unwrap();
    assert_eq!(prepared.order.u32(&prepared.exif[&TAG_PIXEL_X].data), 300);
    assert_eq!(prepared.order.u32(&prepared.exif[&TAG_PIXEL_Y].data), 200);
    assert!(prepared.gps.is_none());
    assert!(prepared.maker_note.is_some());
}
