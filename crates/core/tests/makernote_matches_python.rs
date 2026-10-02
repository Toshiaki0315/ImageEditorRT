//! MakerNote の読み取りが Python 版（core/makernote.py）と同じ結果になるか。
//! fixtures/makernote は Python 版のテスト部品で作った EXIF と、Python 版で読んだ結果
//! （expected.json）。

use std::collections::BTreeMap;
use std::path::Path;

use imageeditorrt_core::makernote::read_maker_note;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Expected {
    format: String,
    size: usize,
    tags: Vec<(String, String)>,
}

#[test]
fn same_as_python() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/makernote");
    let expected: BTreeMap<String, Option<Expected>> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
    assert!(expected.len() >= 10);
    for (name, want) in expected {
        let tiff = std::fs::read(dir.join(format!("{name}.tiff"))).unwrap();
        let got = read_maker_note(&tiff);
        match (want, got) {
            (None, None) => {}
            (Some(want), Some(got)) => {
                assert_eq!(got.format, want.format, "{name}");
                assert_eq!(got.size, want.size, "{name}");
                assert_eq!(got.tags, want.tags, "{name}");
            }
            (want, got) => panic!("{name}: Python {want:?} / Rust {got:?}"),
        }
    }
}
