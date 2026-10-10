//! いろいろな形式・色の画像が、sRGB の RGBA（8bit）として読めるか（旧版 FR-IO-03〜08）。
//! fixtures/formats の画像は fixtures/formats/make.py（Pillow）で作ったもの。
#![cfg(target_os = "macos")]

use imageeditorrt_core::decode::{decode_file, DecodeError};
use imageeditorrt_core::formats::{has_transparency, Format};

fn open(name: &str) -> imageeditorrt_core::decode::Decoded {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/formats").join(name);
    decode_file(&std::fs::read(path).unwrap()).unwrap()
}

fn close(a: &[u8], b: [u8; 4], tolerance: u8) -> bool {
    a.iter().zip(b).all(|(&x, y)| x.abs_diff(y) <= tolerance)
}

#[test]
fn cmyk_jpeg_becomes_rgb() {
    let d = open("cmyk.jpg");
    assert_eq!(d.format, Format::Jpeg);
    let white = d.image.get_pixel(0, 3).0;
    let red = d.image.get_pixel(7, 3).0;
    assert!(white[..3].iter().all(|&v| v > 230), "{white:?}");
    assert!(red[0] > 180 && red[1] < 80 && red[2] < 80, "{red:?}");
    assert_eq!(red[3], 255);
}

#[test]
fn gray_16bit_png_becomes_8bit() {
    let d = open("gray16.png");
    assert!(close(&d.image.get_pixel(0, 0).0, [0, 0, 0, 255], 2), "{:?}", d.image.get_pixel(0, 0));
    assert!(close(&d.image.get_pixel(7, 0).0, [255, 255, 255, 255], 2), "{:?}", d.image.get_pixel(7, 0));
}

#[test]
fn palette_png_keeps_transparency() {
    let d = open("palette.png");
    assert_eq!(d.image.get_pixel(0, 0)[3], 0);
    let green = d.image.get_pixel(7, 0).0;
    assert!(green[3] == 255 && green[1] > 150 && green[0] < 40, "{green:?}");
    assert!(has_transparency(&d.image));
}

#[test]
fn animated_gif_reads_first_frame() {
    let d = open("animated.gif");
    assert_eq!(d.format, Format::Gif);
    assert_eq!(d.frame_count, 2);
    let p = d.image.get_pixel(3, 3).0;
    assert!(p[2] > 200 && p[0] < 40, "先頭（青）のフレーム: {p:?}");
}

#[test]
fn multi_page_tiff_reads_first_page() {
    let d = open("pages.tif");
    assert_eq!(d.format, Format::Tiff);
    assert_eq!(d.frame_count, 2);
    let p = d.image.get_pixel(3, 3).0;
    assert!(p[0] > 200 && p[1] < 40, "先頭（赤）のページ: {p:?}");
}

#[test]
fn bmp() {
    let d = open("plain.bmp");
    assert_eq!(d.format, Format::Bmp);
    assert_eq!(d.frame_count, 1);
    assert!(close(&d.image.get_pixel(1, 1).0, [10, 20, 30, 255], 2), "{:?}", d.image.get_pixel(1, 1));
    assert!(!has_transparency(&d.image));
}

#[test]
fn format_comes_from_contents_not_extension() {
    assert_eq!(open("png_named.jpg").format, Format::Png);
}

#[test]
fn unsupported_format_is_rejected() {
    // ImageIO は読めても、このアプリでは扱わない形式（WebP）
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/formats/unsupported.webp");
    match decode_file(&std::fs::read(path).unwrap()) {
        Err(DecodeError::UnsupportedFormat(uti)) => assert!(uti.contains("webp"), "{uti}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn display_p3_is_kept_as_p3() {
    // 広い色域の画像は Display P3 のまま読む（P3 の (200, 100, 50) はそのままの値）
    let d = open("display_p3.png");
    assert_eq!(d.color_space, imageeditorrt_core::color_space::ColorSpace::DisplayP3);
    let p = d.image.get_pixel(1, 1).0;
    assert!(p[0].abs_diff(200) <= 2 && p[1].abs_diff(100) <= 2 && p[2].abs_diff(50) <= 2, "{p:?}");
    // sRGB に直すと、赤がより強く、緑・青がより弱い色になる
    let mut srgb = d.image.clone();
    imageeditorrt_core::color_space::p3_to_srgb(&mut srgb);
    let p = srgb.get_pixel(1, 1).0;
    assert!(p[0] > 205 && p[1] < 98 && p[2] < 45, "{p:?}");
}
