//! 実際のファイル（HEIC・向きの付いた JPEG）を ImageIO で読む。

#![cfg(target_os = "macos")]

use imageeditorrt_core::decode::decode;

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read(path).unwrap()
}

#[test]
fn heic_is_decoded_with_orientation() {
    // 1500×1000 で、向き 6（時計回りに 90° 回して見る）
    let start = std::time::Instant::now();
    let image = decode(&fixture("rotated.heic")).unwrap();
    eprintln!("HEIC 1500x1000 の読み込み: {:?}", start.elapsed());
    assert_eq!(image.dimensions(), (1000, 1500));
}

#[test]
fn jpeg_is_decoded_with_orientation() {
    // 64×48 の左半分が赤、右半分が青。向き 6 で回すと、上半分が赤になる
    let image = decode(&fixture("rotated.jpg")).unwrap();
    assert_eq!(image.dimensions(), (48, 64));
    let top = image.get_pixel(24, 5);
    let bottom = image.get_pixel(24, 58);
    assert!(top[0] > 180 && top[2] < 80, "上は赤: {top:?}");
    assert!(bottom[2] > 160 && bottom[0] < 130, "下は青: {bottom:?}");
}
