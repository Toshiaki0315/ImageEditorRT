//! 保存用に EXIF を書き直しても、MakerNote が元の位置に残る（Python 版 #107 と同じ）。

use imageeditorrt_core::tiff::{insert_exif_into_jpeg, tiff_block, ExifBlock, TAG_ORIENTATION};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

#[test]
fn maker_note_stays_at_original_offset() {
    let jpeg = fixture("canon.jpg");
    let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&jpeg)).unwrap();
    let raw = exif.buf();
    let mut block = ExifBlock::parse(raw).unwrap();
    let (at, note) = block.maker_note.clone().expect("MakerNote がある");
    block.set_short(false, TAG_ORIENTATION, 1);

    let written = block.to_bytes(true);
    let tiff = tiff_block(&written).unwrap();
    assert_eq!(&tiff[at..at + note.len()], &note[..]);

    // JPEG に差し込んで読み直しても、標準のタグが読める
    let saved = insert_exif_into_jpeg(&jpeg, &written).unwrap();
    let reread = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&saved)).unwrap();
    let make = reread.get_field(exif::Tag::Make, exif::In::PRIMARY).unwrap();
    assert_eq!(make.display_value().to_string(), "\"Canon\"");
}
