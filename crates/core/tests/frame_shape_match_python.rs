//! フレーム・形が旧版（core/frames.py・core/shapes.py・core/pipeline.py）と同じ結果になるか。
//! fixtures/frame は旧版で作った期待値（fixtures/frame/make.py）。

use std::path::{Path, PathBuf};

use image::{GrayImage, RgbaImage};
use imageeditorrt_core::frames::{self, FrameType};
use imageeditorrt_core::pipeline::{apply_edits, effective_crop, output_size, render_preview, EditSettings};
use imageeditorrt_core::shapes::{self, ShapeType};
use imageeditorrt_core::transform::CropRect;
use serde_json::Value;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/frame")
}

fn png(name: &str) -> RgbaImage {
    image::open(dir().join(name)).unwrap().to_rgba8()
}

fn cases() -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir().join("cases.json")).unwrap()).unwrap()
}

fn pair(v: &Value) -> (u32, u32) {
    (v[0].as_u64().unwrap() as u32, v[1].as_u64().unwrap() as u32)
}

fn rect(v: &Value) -> Option<CropRect> {
    let a = v.as_array()?;
    Some(CropRect::new(a[0].as_i64()?, a[1].as_i64()?, a[2].as_i64()?, a[3].as_i64()?))
}

fn frame(v: &Value) -> FrameType {
    serde_json::from_value(v.clone()).unwrap()
}

fn shape(v: &Value) -> ShapeType {
    serde_json::from_value(v.clone()).unwrap()
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
fn masks() {
    for c in cases()["masks"].as_array().unwrap() {
        let got =
            shapes::shape_mask(pair(&c["size"]), shape(&c["shape"]), c["radius"].as_u64().unwrap() as u32);
        match c["file"].as_str() {
            None => assert!(got.is_none(), "{c}"),
            Some(file) => {
                let expected: GrayImage = image::open(dir().join(file)).unwrap().to_luma8();
                let got = got.expect("マスクがある");
                let different = got.pixels().zip(expected.pixels()).filter(|(a, b)| a != b).count();
                assert_eq!(different, 0, "{file}");
            }
        }
    }
}

#[test]
fn shapes_and_frames() {
    let (source, tall, opaque) = (png("source.png"), png("tall.png"), png("opaque.png"));
    let white = Some(frames::FRAME_COLOR);
    for (name, got) in [
        ("shape_rounded_clear.png", shapes::apply_shape(&source, ShapeType::Rounded, 20, None)),
        ("shape_circle_fill.png", shapes::apply_shape(&source, ShapeType::Circle, 10, white)),
        ("shape_rounded_fill_opaque.png", shapes::apply_shape(&opaque, ShapeType::Rounded, 30, white)),
        ("frame_polaroid.png", frames::add_frame(&source, FrameType::Polaroid)),
        ("frame_instax_tall.png", frames::add_frame(&tall, FrameType::InstaxMini)),
        ("frame_instax_wide.png", frames::add_frame(&source, FrameType::InstaxMini)),
    ] {
        assert_eq!(diff(&got, &png(name)), (0, 0), "{name}");
    }
}

#[test]
fn margins_and_sizes() {
    for c in cases()["margins"].as_array().unwrap() {
        let (f, size) = (frame(&c["frame"]), pair(&c["size"]));
        let m = frames::frame_margins(f, size);
        assert_eq!(
            [m.0, m.1, m.2, m.3].map(u64::from).to_vec(),
            c["margins"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect::<Vec<_>>(),
            "{c}"
        );
        let aspect = frames::window_aspect(f, size).unwrap();
        assert_eq!(
            (aspect.0, aspect.1),
            (c["aspect"][0].as_f64().unwrap(), c["aspect"][1].as_f64().unwrap()),
            "{c}"
        );
        assert_eq!(frames::framed_size(size, f), pair(&c["framed"]), "{c}");
        let b = frames::margin_box(size, f).unwrap();
        let expected: Vec<u64> =
            c["margin_box"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
        assert_eq!([b.0, b.1, b.2, b.3].map(u64::from).to_vec(), expected, "{c}");
    }
    for c in cases()["crops"].as_array().unwrap() {
        let got = effective_crop(pair(&c["size"]), rect(&c["crop"]), frame(&c["frame"]), shape(&c["shape"]));
        assert_eq!(got, rect(&c["result"]), "{c}");
    }
}

#[test]
fn flows() {
    let source = png("source.png");
    for (i, c) in cases()["flows"].as_array().unwrap().iter().enumerate() {
        let settings = EditSettings {
            frame: frame(&c["frame"]),
            shape: shape(&c["shape"]),
            corner_radius: c["radius"].as_u64().unwrap() as u32,
            crop: rect(&c["crop"]),
            width: c["width"].as_u64().map(|v| v as u32),
            vignette: 30,
            ..EditSettings::default()
        };
        assert_eq!(output_size(source.dimensions(), &settings).unwrap(), pair(&c["output"]), "{c}");
        assert_eq!(
            diff(&apply_edits(&source, &settings).unwrap(), &png(&format!("flow_{i}.png"))),
            (0, 0),
            "flow_{i}"
        );
        let trimmed = render_preview(&source, &settings, 1.0, true);
        assert_eq!(diff(&trimmed, &png(&format!("flow_{i}_trimmed.png"))), (0, 0), "flow_{i}_trimmed");
        let full = render_preview(&source, &settings, 1.0, false);
        assert_eq!(diff(&full, &png(&format!("flow_{i}_full.png"))), (0, 0), "flow_{i}_full");
    }
}
