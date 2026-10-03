//! プリセットの読み書きが旧版（core/presets.py）と同じか。fixtures/presets は旧版で書いたファイル・
//! 壊れた項目や壊れたファイルと、旧版で読んだ結果（expected.json。make.py で作る）。

use std::collections::BTreeMap;
use std::path::Path;

use imageeditorrt_core::presets::{load_presets, presets_json, save_presets};
use serde_json::Value;

fn dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/presets")
}

#[test]
fn load_same_as_python() {
    let expected: BTreeMap<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(dir().join("expected.json")).unwrap()).unwrap();
    assert_eq!(expected.len(), 7);
    for (name, want) in expected {
        let path = dir().join(format!("{name}.json"));
        match load_presets(&path) {
            Ok(presets) => {
                let got = serde_json::to_value(&presets).unwrap();
                assert_eq!(&got, &want["presets"], "{name}");
            }
            Err(e) => {
                let message = e.to_string().replace(&path.display().to_string(), "<path>");
                assert_eq!(Value::String(message), want["error"], "{name}");
            }
        }
    }
}

#[test]
fn save_writes_the_same_bytes_as_python() {
    let path = dir().join("saved.json");
    let presets = load_presets(&path).unwrap();
    assert_eq!(presets.len(), 3);
    assert_eq!(presets_json(&presets), std::fs::read_to_string(&path).unwrap());
    // 書いて読み直すと同じ（フォルダがなければ作る）
    let out = std::env::temp_dir().join(format!("imageeditorrt-presets-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let target = out.join("a/b/presets.json");
    save_presets(&target, &presets).unwrap();
    assert_eq!(load_presets(&target).unwrap(), presets);
    std::fs::remove_dir_all(&out).unwrap();
}

#[test]
fn unreadable_file_is_an_error() {
    // フォルダはファイルとして読めない
    let error = load_presets(&dir()).unwrap_err();
    assert!(error.to_string().starts_with("プリセットを読み込めません: "), "{error}");
    // 書けない場所（ファイルの下）には保存できず、エラーになる
    let error = save_presets(&dir().join("saved.json/x.json"), &[]).unwrap_err();
    assert!(error.to_string().starts_with("プリセットを保存できません: "), "{error}");
}
