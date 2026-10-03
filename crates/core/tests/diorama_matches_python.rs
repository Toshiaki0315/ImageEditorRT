//! ジオラマ風が旧版（core/diorama.py）と同じ結果になるか。
//! fixtures/diorama は旧版で作った期待値（fixtures/diorama/make.py）。

use std::path::{Path, PathBuf};

use image::RgbaImage;
use imageeditorrt_core::diorama::{diorama, DioramaDirection, DioramaSettings};
use imageeditorrt_core::pipeline::{apply_edits, render_preview, EditSettings};
use imageeditorrt_core::transform::CropRect;
use serde::Deserialize;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/diorama")
}

fn png(name: &str) -> RgbaImage {
    image::open(dir().join(name)).unwrap().to_rgba8()
}

#[derive(Deserialize)]
struct Case {
    blur: u32,
    direction: DioramaDirection,
    position: u32,
    width: u32,
    vivid: u32,
    /// 旧版の (左, 上, 右, 下)
    area: Option<[i64; 4]>,
    file: String,
}

fn diff(a: &RgbaImage, b: &RgbaImage) -> (usize, u8) {
    assert_eq!(a.dimensions(), b.dimensions());
    let mut count = 0;
    let mut worst = 0;
    for (x, y) in a.pixels().zip(b.pixels()) {
        let d = x.0.iter().zip(y.0).map(|(p, q)| p.abs_diff(q)).max().unwrap();
        if d > 0 {
            count += 1;
            worst = worst.max(d);
        }
    }
    (count, worst)
}

#[test]
fn each_case_matches() {
    let cases: Vec<Case> =
        serde_json::from_str(&std::fs::read_to_string(dir().join("cases.json")).unwrap()).unwrap();
    let source = png("source.png");
    let mut failures = Vec::new();
    for case in &cases {
        let settings = DioramaSettings {
            blur: case.blur,
            direction: case.direction,
            position: case.position,
            width: case.width,
            vivid: case.vivid,
        };
        let area = case.area.map(|[l, t, r, b]| CropRect::new(l, t, r - l, b - t));
        let (count, worst) = diff(&diorama(&source, &settings, area, None), &png(&case.file));
        if count > 0 {
            failures.push(format!("{}: {count} 画素が違う（最大の差 {worst}）", case.file));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn pipeline_matches() {
    let source = png("source.png");
    let settings = EditSettings {
        diorama_blur: 80,
        diorama_position: 40,
        diorama_vivid: 50,
        crop: Some(CropRect::new(10, 10, 120, 90)),
        ..EditSettings::default()
    };
    assert_eq!(diff(&apply_edits(&source, &settings).unwrap(), &png("apply_edits.png")), (0, 0));
    assert_eq!(diff(&render_preview(&source, &settings, 1.0, false), &png("render_preview.png")), (0, 0));
    let vertical = EditSettings {
        diorama_blur: 60,
        diorama_direction: DioramaDirection::Vertical,
        diorama_position: 30,
        ..EditSettings::default()
    };
    assert_eq!(diff(&apply_edits(&source, &vertical).unwrap(), &png("apply_edits_vertical.png")), (0, 0));
}
