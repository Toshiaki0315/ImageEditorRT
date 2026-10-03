//! ヒストグラムの数え方が旧版（core/pipeline.py の render_preview_with_histogram）と同じか。
//! fixtures/histogram/cases.json は旧版で数えた結果（make.py で作る）。画像は fixtures/frame のもの。

use std::path::Path;

use imageeditorrt_core::filters::FilterType;
use imageeditorrt_core::frames::FrameType;
use imageeditorrt_core::pipeline::{render_preview_with_histogram, EditSettings};
use imageeditorrt_core::shapes::ShapeType;
use imageeditorrt_core::text::TextSettings;
use imageeditorrt_core::transform::CropRect;
use serde_json::Value;

fn from<T: serde::de::DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap()
}

fn counts(v: &Value) -> Vec<u32> {
    from(v)
}

#[test]
fn same_as_python() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let cases: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("histogram/cases.json")).unwrap()).unwrap();
    assert_eq!(cases.len(), 7);
    for c in &cases {
        let source =
            image::open(dir.join(format!("frame/{}.png", c["image"].as_str().unwrap()))).unwrap().to_rgba8();
        let crop = c["crop"].as_array().map(|v| {
            let n = |i: usize| v[i].as_i64().unwrap();
            CropRect::new(n(0), n(1), n(2), n(3))
        });
        let settings = EditSettings {
            crop,
            frame: from::<FrameType>(&c["frame"]),
            shape: from::<ShapeType>(&c["shape"]),
            corner_radius: c["radius"].as_u64().unwrap() as u32,
            filter: c.get("filter").map_or(FilterType::None, from),
            vignette: c.get("vignette").map_or(0, |v| v.as_u64().unwrap() as u32),
            exposure: c.get("exposure").map_or(0.0, |v| v.as_f64().unwrap()),
            text: TextSettings {
                text: c.get("text").map_or(String::new(), from),
                size: 20.0,
                ..TextSettings::default()
            },
            ..EditSettings::default()
        };
        let trimmed = c["trimmed"].as_bool().unwrap();
        let (_, h) = render_preview_with_histogram(&source, &settings, 1.0, trimmed);
        assert_eq!(h.red, counts(&c["red"]), "R {c:?}");
        assert_eq!(h.green, counts(&c["green"]), "G");
        assert_eq!(h.blue, counts(&c["blue"]), "B");
        assert_eq!(h.luma, counts(&c["luma"]), "輝度");
    }
}
