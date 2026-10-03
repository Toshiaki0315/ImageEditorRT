//! 編集設定の既定値（EditSettings::default()）が、画面（TypeScript の defaultSettings()）と同じか。
//!
//! 既定値は Rust と TypeScript に二重に書いているので、両方がこのファイル
//! （tests-ts/fixtures/default-settings.json）と同じかを確かめる。片方だけ変えるとどちらかのテストが落ちる。
//! 既定値を変えたときは、両方とこのファイルをそろえる。

use std::path::Path;

use imageeditorrt_core::pipeline::EditSettings;
use serde_json::Value;

#[test]
fn rust_defaults_match_the_shared_file() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests-ts/fixtures/default-settings.json");
    let shared: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let rust = serde_json::to_value(EditSettings::default()).unwrap();
    assert_eq!(
        rust,
        shared,
        "Rust の既定値が {} と違います。Rust の既定値:\n{}",
        path.display(),
        serde_json::to_string_pretty(&rust).unwrap()
    );
}
