//! 色の調整（露出〜経年劣化）が旧版（core/effects.py）と同じ結果になるか。
//! fixtures/adjust は旧版で作った期待値（fixtures/adjust/make.py）。

use std::path::{Path, PathBuf};

use image::RgbaImage;
use imageeditorrt_core::adjust;
use imageeditorrt_core::pipeline::{apply_edits, render_preview, EditSettings};
use imageeditorrt_core::transform::CropRect;
use serde::Deserialize;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adjust")
}

fn png(name: &str) -> RgbaImage {
    image::open(dir().join(name)).unwrap().to_rgba8()
}

#[derive(Deserialize)]
struct Case {
    file: String,
    source: String,
    effect: String,
    amount: f64,
}

/// 違う画素の数と、いちばん大きい差。
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

fn apply(effect: &str, amount: f64, image: &mut RgbaImage) {
    match effect {
        "exposure" => adjust::apply_lut(image, &adjust::exposure_lut(amount)),
        "brightness" => adjust::apply_lut(image, &adjust::brightness_lut(amount as i32)),
        "contrast" => adjust::apply_lut(image, &adjust::contrast_lut(amount as i32)),
        "temperature" => adjust::apply_lut(image, &adjust::temperature_lut(amount as u32)),
        "saturation" => adjust::saturation(image, amount as i32),
        "vignette" => adjust::vignette(image, amount as u32),
        "aging" => adjust::aging(image, amount as u32),
        other => panic!("知らない効果: {other}"),
    }
}

#[test]
fn each_effect_matches() {
    let cases: Vec<Case> =
        serde_json::from_str(&std::fs::read_to_string(dir().join("cases.json")).unwrap()).unwrap();
    assert!(cases.len() >= 30);
    let mut failures = Vec::new();
    for case in &cases {
        let mut image = png(&case.source);
        apply(&case.effect, case.amount, &mut image);
        let (count, worst) = diff(&image, &png(&case.file));
        if count > 0 {
            failures.push(format!("{}: {count} 画素が違う（最大の差 {worst}）", case.file));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn pipeline_matches() {
    let source = png("source.png");
    let combined = EditSettings {
        exposure: 0.6,
        brightness: 15,
        contrast: 25,
        temperature: 4800,
        saturation: 30,
        vignette: 40,
        aging: 20,
        ..EditSettings::default()
    };
    assert_eq!(diff(&apply_edits(&source, &combined).unwrap(), &png("apply_edits.png")), (0, 0));

    let cropped = EditSettings {
        vignette: 70,
        aging: 10,
        saturation: -20,
        crop: Some(CropRect::new(8, 6, 40, 30)),
        ..EditSettings::default()
    };
    assert_eq!(diff(&render_preview(&source, &cropped, 1.0, false), &png("render_preview.png")), (0, 0));
}
