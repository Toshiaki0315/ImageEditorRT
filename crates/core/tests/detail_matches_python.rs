//! ディテール（シャープ・ぼかし・ノイズ除去）が旧版（core/effects.py）と同じ結果になるか。
//! fixtures/detail は旧版で作った期待値（fixtures/detail/make.py）。

use std::path::{Path, PathBuf};

use image::RgbaImage;
use imageeditorrt_core::effects;
use imageeditorrt_core::pipeline::{apply_edits, render_preview, EditSettings};
use imageeditorrt_core::transform::CropRect;
use serde::Deserialize;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/detail")
}

fn png(name: &str) -> RgbaImage {
    image::open(dir().join(name)).unwrap().to_rgba8()
}

#[derive(Deserialize)]
struct Case {
    effect: String,
    amount: u32,
    reference: Option<f64>,
    output: Option<f64>,
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
fn each_effect_matches() {
    let cases: Vec<Case> =
        serde_json::from_str(&std::fs::read_to_string(dir().join("cases.json")).unwrap()).unwrap();
    let source = png("source.png");
    let short = f64::from(source.width().min(source.height()));
    let mut failures = Vec::new();
    for case in &cases {
        let reference = case.reference.unwrap_or(short);
        let got = match case.effect.as_str() {
            "sharpen" => effects::sharpen(&source, case.amount, reference, case.output),
            "blur" => effects::blur(&source, case.amount, reference),
            "denoise" => effects::denoise(&source, case.amount, reference),
            other => panic!("知らない効果: {other}"),
        };
        let (count, worst) = diff(&got, &png(&case.file));
        if count > 0 {
            failures.push(format!("{}: {count} 画素が違う（最大の差 {worst}）", case.file));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn pipeline_matches() {
    let settings = EditSettings {
        sharpen: 60,
        blur: 20,
        denoise: 40,
        crop: Some(CropRect::new(40, 20, 400, 360)),
        width: Some(300),
        ..EditSettings::default()
    };
    // 保存（原寸。リサイズも旧版と同じ計算）
    assert_eq!(diff(&apply_edits(&png("big.png"), &settings).unwrap(), &png("apply_edits.png")), (0, 0));
    // 縮小プレビュー（旧版で縮めた画像から作る）
    let preview = render_preview(&png("preview_source.png"), &settings, 0.25, false);
    assert_eq!(diff(&preview, &png("render_preview.png")), (0, 0));
}
