//! テスト用の画像から EXIF・GPS・MakerNote を読む。

use imageeditorrt_core::exif_info::{read_exif_info, Group};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

#[test]
fn pentax_jpeg() {
    let info = read_exif_info(&fixture("pentax.jpg"));
    for e in &info.entries {
        println!("{:?} {} = {}", e.group, e.label, e.value);
    }
    let value = |group: Group, tag: &str| {
        info.entries.iter().find(|e| e.group == group && e.tag == tag).map(|e| e.value.clone())
    };
    assert_eq!(value(Group::Image, "Make").as_deref(), Some("PENTAX"));
    assert_eq!(info.maker_note.as_deref(), Some("Pentax"));
    assert_eq!(value(Group::MakerNote, "SerialNumber").as_deref(), Some("1234567"));
    assert_eq!(value(Group::MakerNote, "ISO").as_deref(), Some("200"));
    assert_eq!(value(Group::MakerNote, "FirmwareVersion").as_deref(), Some("1.40"));
    assert!(info.gps.is_some());
    assert!(value(Group::Gps, "GPSLatitude").unwrap().contains('°'));
    // グループの順に並ぶ
    assert!(info.entries.windows(2).all(|w| w[0].group <= w[1].group));
    // ほかの IFD を指すだけのタグは出さない
    assert!(value(Group::Image, "ExifIFDPointer").is_none());
}

#[test]
fn canon_maker_note_has_names() {
    // Canon は旧版（exifread）と同じタグの名前で出す
    let info = read_exif_info(&fixture("canon.jpg"));
    assert_eq!(info.maker_note.as_deref(), Some("Canon"));
    let notes: Vec<(&str, &str)> = info
        .entries
        .iter()
        .filter(|e| e.group == Group::MakerNote)
        .map(|e| (e.tag.as_str(), e.value.as_str()))
        .collect();
    assert_eq!(
        notes,
        [("ImageType", "Canon EOS R5 IMAGE TYPE"), ("FirmwareVersion", "Firmware Version 1.8.1")]
    );
}

#[test]
#[cfg(target_os = "macos")]
fn heic_has_orientation() {
    let info = read_exif_info(&fixture("rotated.heic"));
    let orientation = info.entries.iter().find(|e| e.tag == "Orientation").expect("HEIC の EXIF を読める");
    assert!(orientation.label.contains('（'), "{}", orientation.label);
}

#[test]
#[cfg(target_os = "macos")]
fn tiff_with_only_image_structure_is_empty() {
    // EXIF のない画像を TIFF で保存すると、画像の構造のタグだけが入る
    use imageeditorrt_core::save::{save_edited, SaveOptions};
    let path = std::env::temp_dir().join(format!("imageeditorrt-structure-{}.tif", std::process::id()));
    let image = image::RgbaImage::from_pixel(4, 3, image::Rgba([1, 2, 3, 255]));
    save_edited(&image, &path, SaveOptions::default(), None, false).unwrap();
    let info = read_exif_info(&std::fs::read(&path).unwrap());
    std::fs::remove_file(&path).unwrap();
    assert!(!info.entries.is_empty(), "構造のタグは読める");
    assert!(info.empty, "表示する情報はない");
    // 撮影情報のある JPEG は空ではない
    assert!(!read_exif_info(&fixture("pentax.jpg")).empty);
}
