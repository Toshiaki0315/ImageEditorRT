//! 主なメーカー（Canon・Nikon・Sony・Apple・Fujifilm・Olympus・Casio・DJI）の MakerNote の表示が、
//! 旧版（exifread）で読んだ結果と同じになるか。fixtures/makernote_makers は旧版のテスト部品で作った
//! EXIF と、旧版の core/exif_info.py で読んだ結果（expected.json。make.py で作る）。

use std::collections::BTreeMap;
use std::path::Path;

use imageeditorrt_core::exif_info::{read_exif_info, Group};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Expected {
    maker_note: Option<String>,
    tags: Vec<(String, String)>,
    user_comment: bool,
}

#[test]
fn same_as_python() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/makernote_makers");
    let expected: BTreeMap<String, Expected> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
    assert!(expected.len() >= 19);
    let mut failures = Vec::new();
    for (name, want) in expected {
        let file = std::fs::read(dir.join(format!("{name}.tiff"))).unwrap();
        let info = read_exif_info(&file);
        let tags: Vec<(String, String)> = info
            .entries
            .iter()
            .filter(|e| e.group == Group::MakerNote)
            .map(|e| (e.tag.clone(), e.value.clone()))
            .collect();
        let user_comment = info.entries.iter().any(|e| e.tag == "UserComment");
        if info.maker_note != want.maker_note || tags != want.tags || user_comment != want.user_comment {
            failures.push(format!(
                "{name}:\n  Python {:?} {:?} {}\n  Rust   {:?} {:?} {}",
                want.maker_note, want.tags, want.user_comment, info.maker_note, tags, user_comment
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
