//! 回転・反転・トリミング・リサイズが旧版（core/transform.py・core/pipeline.py）と同じ結果になるか。
//! fixtures/transform は旧版で作った期待値（fixtures/transform/make.py）。

use std::path::{Path, PathBuf};

use image::RgbaImage;
use imageeditorrt_core::pipeline::{apply_edits, output_size, scale_settings, EditSettings};
use imageeditorrt_core::transform::{
    aspect_drag_rect, clamp_crop, constrain_rect, fit_aspect, fit_size, transform_rect, AspectRatio,
    CropRect, OrientOp, Orientation,
};
use serde_json::Value;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transform")
}

fn png(name: &str) -> RgbaImage {
    image::open(dir().join(name)).unwrap().to_rgba8()
}

fn cases(key: &str) -> Vec<Value> {
    let all: Value =
        serde_json::from_str(&std::fs::read_to_string(dir().join("cases.json")).unwrap()).unwrap();
    all[key].as_array().unwrap().clone()
}

fn rect(v: &Value) -> Option<CropRect> {
    let a = v.as_array()?;
    Some(CropRect::new(a[0].as_i64()?, a[1].as_i64()?, a[2].as_i64()?, a[3].as_i64()?))
}

fn pair(v: &Value) -> (u32, u32) {
    (v[0].as_u64().unwrap() as u32, v[1].as_u64().unwrap() as u32)
}

fn aspect(v: &Value) -> (f64, f64) {
    (v[0].as_f64().unwrap(), v[1].as_f64().unwrap())
}

fn op(v: &Value) -> OrientOp {
    serde_json::from_value(v.clone()).unwrap()
}

fn opt_u32(v: &Value) -> Option<u32> {
    v.as_u64().map(|v| v as u32)
}

/// 旧版の EditSettings の一部（向き・範囲・大きさ）から設定を作る。
fn settings(v: &Value) -> EditSettings {
    EditSettings {
        orientation: Orientation::new(v["rotation"].as_u64().unwrap() as u32, v["mirror"].as_bool().unwrap()),
        crop: rect(&v["crop"]),
        width: opt_u32(&v["width"]),
        height: opt_u32(&v["height"]),
        keep_aspect: v["keepAspect"].as_bool().unwrap(),
        ..EditSettings::default()
    }
}

#[test]
fn eight_orientations() {
    let source = png("source.png");
    for rotation in [0, 90, 180, 270] {
        for mirror in [false, true] {
            let expected = png(&format!("orient_{rotation}_{}.png", u8::from(mirror)));
            assert_eq!(
                Orientation::new(rotation, mirror).transpose(&source),
                expected,
                "{rotation} {mirror}"
            );
        }
    }
}

#[test]
fn orientation_apply() {
    for c in cases("orientation_apply") {
        let start = Orientation::new(c[0].as_u64().unwrap() as u32, c[1].as_bool().unwrap());
        let expected = Orientation::new(c[3].as_u64().unwrap() as u32, c[4].as_bool().unwrap());
        assert_eq!(start.apply(op(&c[2])), expected, "{c}");
    }
}

#[test]
fn rect_calculations() {
    for c in cases("transform_rect") {
        assert_eq!(Some(transform_rect(rect(&c[0]).unwrap(), pair(&c[1]), op(&c[2]))), rect(&c[3]), "{c}");
    }
    for c in cases("clamp_crop") {
        assert_eq!(clamp_crop(rect(&c[0]).unwrap(), pair(&c[1])), rect(&c[2]), "{c}");
    }
    for c in cases("fit_aspect") {
        assert_eq!(Some(fit_aspect(rect(&c[0]).unwrap(), aspect(&c[1]))), rect(&c[2]), "{c}");
    }
    for c in cases("constrain_rect") {
        assert_eq!(constrain_rect(rect(&c[0]).unwrap(), aspect(&c[1]), pair(&c[2])), rect(&c[3]), "{c}");
    }
    for c in cases("aspect_drag_rect") {
        let anchor = (c[0][0].as_i64().unwrap(), c[0][1].as_i64().unwrap());
        let point = (c[1][0].as_i64().unwrap(), c[1][1].as_i64().unwrap());
        assert_eq!(Some(aspect_drag_rect(anchor, point, aspect(&c[2]), pair(&c[3]))), rect(&c[4]), "{c}");
    }
}

#[test]
fn sizes() {
    for c in cases("fit_size") {
        let got = fit_size(pair(&c[0]), opt_u32(&c[1]), opt_u32(&c[2]), c[3].as_bool().unwrap()).unwrap();
        assert_eq!(got, pair(&c[4]), "{c}");
    }
    for c in cases("fit_size_errors") {
        assert!(fit_size(pair(&c[0]), opt_u32(&c[1]), opt_u32(&c[2]), true).is_err(), "{c}");
    }
    for c in cases("output_size") {
        assert_eq!(output_size((4000, 3000), &settings(&c[0])).unwrap(), pair(&c[1]), "{c}");
    }
}

#[test]
fn aspect_ratios() {
    let all = cases("aspect_ratios");
    assert_eq!(all.len(), AspectRatio::ALL.len());
    for (c, ratio) in all.iter().zip(AspectRatio::ALL) {
        assert_eq!(ratio.label(), c[1].as_str().unwrap(), "{c}");
        let expected = |v: &Value| v.as_array().map(|_| aspect(v));
        assert_eq!(ratio.ratio(false), expected(&c[2]), "{c}");
        assert_eq!(ratio.ratio(true), expected(&c[3]), "{c}");
    }
}

#[test]
fn scaled_settings() {
    for c in cases("scale_settings") {
        let scaled = scale_settings(&settings(&c[0]), c[1].as_f64().unwrap());
        let expected = settings(&c[2]);
        assert_eq!(
            (scaled.crop, scaled.width, scaled.height),
            (expected.crop, expected.width, expected.height),
            "{c}"
        );
    }
}

/// 画素ごとの差がどれも tolerance 以下か。
fn assert_close(got: &RgbaImage, expected: &RgbaImage, tolerance: u8, name: &str) {
    assert_eq!(got.dimensions(), expected.dimensions(), "{name}");
    let worst = got.as_raw().iter().zip(expected.as_raw()).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
    assert!(worst <= tolerance, "{name}: 最大の差 {worst}");
}

#[test]
fn lanczos_resize_matches_pillow() {
    // Pillow の Image.resize(LANCZOS) と画素まで同じ
    let gradient = png("gradient.png");
    for (w, h) in [(17, 13), (80, 61), (40, 9)] {
        let got = imageeditorrt_core::transform::resize_to(&gradient, (w, h));
        assert_close(&got, &png(&format!("resize_{w}x{h}.png")), 0, &format!("{w}x{h}"));
    }
}

#[test]
fn apply_edits_orient_crop_resize() {
    let flow: Value =
        serde_json::from_str(&std::fs::read_to_string(dir().join("apply_edits.json")).unwrap()).unwrap();
    let got = apply_edits(&png("gradient.png"), &settings(&flow)).unwrap();
    assert_close(&got, &png("apply_edits.png"), 0, "apply_edits");
}
