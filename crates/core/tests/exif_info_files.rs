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
fn canon_maker_note_is_read_without_names() {
    // Canon のタグの名前は試作ではまだ持たない（ヘッダーのない IFD として番号で出す）
    let info = read_exif_info(&fixture("canon.jpg"));
    assert_eq!(info.maker_note.as_deref(), Some("不明な形式"));
    let note = info.entries.iter().find(|e| e.tag == "Tag 0x0006").unwrap();
    assert_eq!(note.value, "Canon EOS R5 IMAGE TYPE");
}

#[test]
#[cfg(target_os = "macos")]
fn heic_has_orientation() {
    let info = read_exif_info(&fixture("rotated.heic"));
    let orientation = info.entries.iter().find(|e| e.tag == "Orientation").expect("HEIC の EXIF を読める");
    assert!(orientation.label.contains('（'), "{}", orientation.label);
}
