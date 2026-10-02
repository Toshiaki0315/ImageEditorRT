//! テイスト（フィルター 23 種）が旧版（core/filters.py）と同じ結果になるか。
//! fixtures/filters は旧版で作った期待値（fixtures/filters/make.py）。

use std::path::{Path, PathBuf};

use image::RgbaImage;
use imageeditorrt_core::filters::{apply_filter, FilterType};
use serde::Deserialize;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/filters")
}

fn png(name: &str) -> RgbaImage {
    image::open(dir().join(name)).unwrap().to_rgba8()
}

#[derive(Deserialize)]
struct Case {
    filter: FilterType,
    label: String,
    source: String,
    file: String,
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

#[test]
fn every_filter_matches() {
    let cases: Vec<Case> =
        serde_json::from_str(&std::fs::read_to_string(dir().join("cases.json")).unwrap()).unwrap();
    assert_eq!(cases.len(), FilterType::ALL.len() * 2);
    let mut failures = Vec::new();
    for case in &cases {
        assert_eq!(case.filter.label(), case.label);
        let mut image = png(&case.source);
        apply_filter(&mut image, case.filter);
        let (count, worst) = diff(&image, &png(&case.file));
        if count > 0 {
            failures.push(format!("{}: {count} 画素が違う（最大の差 {worst}）", case.file));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
