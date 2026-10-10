//! 各形式で保存して読み直せるか、EXIF（MakerNote）が残るか（旧版 FR-IO-01〜05・12）。
#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};
use imageeditorrt_core::decode::decode_file;
use imageeditorrt_core::exif_info::{raw_exif, read_exif_info, Group};
use imageeditorrt_core::formats::Format;
use imageeditorrt_core::save::{save_edited, ExifRights, SaveOptions};
use imageeditorrt_core::tiff::{
    tiff_block, ExifBlock, TAG_ARTIST, TAG_COPYRIGHT, TAG_IMAGE_DESCRIPTION, TAG_MAKERNOTE,
};

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

#[test]
fn heic_keeps_exif_and_fits_the_size_limit() {
    let dir = TempDir::new("heic");
    // 細かい模様の写真（大きなファイルになる）
    let image = RgbaImage::from_fn(640, 480, |x, y| {
        Rgba([((x * 7 + y * 3) % 256) as u8, ((x * x + y) % 256) as u8, ((y * 11) % 256) as u8, 255])
    });
    let source = raw_exif(&fixture("pentax.jpg")).unwrap();
    let path = dir.join("pentax_edited.heic");
    let full = save_edited(&image, &path, SaveOptions::default(), Some(&source), false).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let decoded = decode_file(&bytes).unwrap();
    assert_eq!((decoded.format, decoded.image.dimensions()), (Format::Heif, (640, 480)));
    // EXIF（撮影日時・機種）は残り、位置情報は外す（既定）。MakerNote は ImageIO が書かない
    let info = read_exif_info(&bytes);
    let value = |tag: &str| info.entries.iter().find(|e| e.tag == tag).map(|e| e.value.clone());
    assert_eq!(value("Model").as_deref(), Some("PENTAX K-1"));
    assert!(value("DateTimeOriginal").is_some());
    assert!(info.gps.is_none());
    // 位置情報も残す設定なら残る
    let keep = SaveOptions { keep_gps: true, ..SaveOptions::default() };
    save_edited(&image, &path, keep, Some(&source), false).unwrap();
    assert!(read_exif_info(&std::fs::read(&path).unwrap()).gps.is_some(), "位置情報が残る");
    // ファイルの大きさの上限も JPEG と同じく効く
    let limit_kb = (full.bytes / 2 / 1024) as u32;
    let options = SaveOptions { max_kb: Some(limit_kb), ..SaveOptions::default() };
    let saved = save_edited(&image, &path, options, Some(&source), false).unwrap();
    assert!(saved.fitted && saved.bytes <= u64::from(limit_kb) * 1024, "{saved:?}");
}

#[test]
fn rights_are_written_and_maker_note_stays_intact() {
    let dir = TempDir::new("rights");
    let source = raw_exif(&fixture("canon.jpg")).unwrap();
    let original = ExifBlock::parse(&source).unwrap();
    let rights = ExifRights {
        copyright: "© 2026 野村".into(),
        artist: "Toshiaki".into(),
        description: "  ".into(), // 空白だけは書かない（元のまま）
    };
    let options = SaveOptions { rights, ..SaveOptions::default() };
    for name in ["r.jpg", "r.png", "r.tif", "r.heic"] {
        let path = dir.join(name);
        save_edited(&sample(), &path, options.clone(), Some(&source), false).unwrap();
        let block = saved_block(&path);
        let text = |tag: u16| {
            block.ifd0.get(&tag).map(|v| String::from_utf8_lossy(&v.data).trim_end_matches('\0').to_string())
        };
        assert_eq!(text(TAG_COPYRIGHT).as_deref(), Some("© 2026 野村"), "{name}");
        assert_eq!(text(TAG_ARTIST).as_deref(), Some("Toshiaki"), "{name}");
        assert_eq!(
            text(TAG_IMAGE_DESCRIPTION),
            original
                .ifd0
                .get(&TAG_IMAGE_DESCRIPTION)
                .map(|v| String::from_utf8_lossy(&v.data).trim_end_matches('\0').to_string()),
            "{name}"
        );
        assert!(block.exif.contains_key(&0x9003), "撮影日時は残る（{name}）");
    }
    // JPEG の MakerNote は元の位置・中身のまま
    let block = saved_block(&dir.join("r.jpg"));
    assert_eq!(block.maker_note, original.maker_note);

    // EXIF を残さない・元の EXIF がない画像でも、権利の情報だけは書く
    let bare = SaveOptions { keep_exif: false, ..options.clone() };
    let path = dir.join("bare.jpg");
    save_edited(&sample(), &path, bare, Some(&source), false).unwrap();
    let block = saved_block(&path);
    assert!(block.ifd0.contains_key(&TAG_COPYRIGHT));
    assert!(!block.ifd0.contains_key(&0x0110), "機種は書かない");
    save_edited(&sample(), &path, options, None, false).unwrap();
    assert!(saved_block(&path).ifd0.contains_key(&TAG_ARTIST));
    // 権利の情報がなく EXIF も残さないなら、EXIF は書かない
    save_edited(&sample(), &path, SaveOptions::default(), None, false).unwrap();
    assert!(raw_exif(&std::fs::read(&path).unwrap()).is_none());
}

#[test]
fn display_p3_is_kept_through_save() {
    use imageeditorrt_core::color_space::ColorSpace;
    let dir = TempDir::new("p3");
    // P3 の色のプロファイル付きの PNG（P3 の鮮やかな緑）
    let image = RgbaImage::from_pixel(16, 16, Rgba([30, 220, 40, 255]));
    let p3 = SaveOptions { color_space: ColorSpace::DisplayP3, ..SaveOptions::default() };
    let source = dir.join("p3.png");
    save_edited(&image, &source, p3.clone(), None, false).unwrap();
    let decoded = decode_file(&std::fs::read(&source).unwrap()).unwrap();
    assert_eq!(decoded.color_space, ColorSpace::DisplayP3, "広い色域なら P3 のまま読む");
    assert!(
        near(decoded.image.get_pixel(3, 3).0, [30, 220, 40, 255], 2),
        "{:?}",
        decoded.image.get_pixel(3, 3)
    );
    // JPEG・PNG・TIFF・HEIC は P3 のまま（プロファイルが付く）
    for name in ["o.jpg", "o.png", "o.tif", "o.heic"] {
        let path = dir.join(name);
        save_edited(&decoded.image, &path, p3.clone(), None, false).unwrap();
        let back = decode_file(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(back.color_space, ColorSpace::DisplayP3, "{name}");
        assert!(
            near(back.image.get_pixel(3, 3).0, [30, 220, 40, 255], 6),
            "{name}: {:?}",
            back.image.get_pixel(3, 3)
        );
    }
    // プロファイルを付けられない BMP は sRGB に直す（P3 の鮮やかな緑は、sRGB では赤が 0 に切れる）
    let bmp = dir.join("o.bmp");
    save_edited(&decoded.image, &bmp, p3, None, false).unwrap();
    let back = decode_file(&std::fs::read(&bmp).unwrap()).unwrap();
    assert_eq!(back.color_space, ColorSpace::Srgb);
    let p = back.image.get_pixel(3, 3).0;
    assert!(p[1] > 215 && p[0] < 20, "{p:?}");
    // sRGB の画像は sRGB のまま（プロファイルを付けない）
    let plain = dir.join("s.png");
    save_edited(&image, &plain, SaveOptions::default(), None, false).unwrap();
    assert_eq!(decode_file(&std::fs::read(&plain).unwrap()).unwrap().color_space, ColorSpace::Srgb);
}
