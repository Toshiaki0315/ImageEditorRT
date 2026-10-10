//! 保存するファイル名の書き方（旧版にはない）: `{名前}_{撮影日}` のような書き方から、保存の名前を作る。
//! 保存ダイアログの初期の名前と、まとめて処理の保存名に使う。

use std::path::{Path, PathBuf};

use crate::exif_info::CaptureDate;

/// 既定の書き方（今までの `<元の名前>_edited` と同じ）。
pub const DEFAULT_TEMPLATE: &str = "{名前}_edited";

/// 使える差し込み（書き方の中の文字と説明。画面のヒントに出す）。
pub const PLACEHOLDERS: [(&str, &str); 6] = [
    ("{名前}", "元のファイル名（拡張子なし）"),
    ("{撮影日}", "撮影日（20261011。なければ空）"),
    ("{撮影時刻}", "撮影時刻（1423。なければ空）"),
    ("{連番}", "まとめて処理の順番（001・002 …。1 枚の保存では 001）"),
    ("{幅}", "出力の幅（px。フレームは含まない）"),
    ("{高さ}", "出力の高さ（px。フレームは含まない）"),
];

/// 名前を作るための材料。
#[derive(Clone, Copy, Debug)]
pub struct NameContext<'a> {
    /// 元のファイル名（拡張子なし）
    pub stem: &'a str,
    /// 撮影日時（なければ None）
    pub date: Option<&'a CaptureDate>,
    /// 順番（1 から）
    pub number: usize,
    /// 保存する画像の幅・高さ
    pub size: (u32, u32),
}

/// 書き方 template に材料を差し込んだ名前（拡張子なし）。ファイル名に使えない文字（/ と :）は _ にし、
/// 空の差し込みで続いた区切り（_ - 空白 .）はひとつにまとめ、前後の区切りは除く。空になれば元の名前。
pub fn render(template: &str, context: &NameContext) -> String {
    let date = context.date;
    let values = [
        context.stem.to_string(),
        date.map(|d| format!("{:04}{:02}{:02}", d.year, d.month, d.day)).unwrap_or_default(),
        date.map(|d| format!("{:02}{:02}", d.hour, d.minute)).unwrap_or_default(),
        format!("{:03}", context.number),
        context.size.0.to_string(),
        context.size.1.to_string(),
    ];
    let mut text = template.to_string();
    for ((key, _), value) in PLACEHOLDERS.iter().zip(&values) {
        text = text.replace(key, value);
    }
    let cleaned: String = text.chars().map(|c| if matches!(c, '/' | ':') { '_' } else { c }).collect();
    let is_separator = |c: char| matches!(c, '_' | '-' | ' ' | '.');
    let mut out = String::new();
    for c in cleaned.chars() {
        if is_separator(c) && out.chars().last().is_some_and(is_separator) {
            continue;
        }
        out.push(c);
    }
    let out = out.trim_matches(is_separator).to_string();
    if out.is_empty() {
        context.stem.to_string()
    } else {
        out
    }
}

/// folder に、書き方の名前で保存するときのパス。すでにあるファイル・元のファイル（source）と重なれば
/// _2、_3 … を付ける。
pub fn template_path(
    template: &str,
    context: &NameContext,
    folder: &Path,
    suffix: &str,
    source: Option<&Path>,
) -> PathBuf {
    let base = render(template, context);
    (1..)
        .map(|n| match n {
            1 => folder.join(format!("{base}.{suffix}")),
            n => folder.join(format!("{base}_{n}.{suffix}")),
        })
        .find(|p| !p.exists() && !source.is_some_and(|s| crate::save::is_same_file(p, s)))
        .expect("候補は終わりなく続く")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context<'a>(date: Option<&'a CaptureDate>) -> NameContext<'a> {
        NameContext { stem: "IMG_0012", date, number: 7, size: (1080, 720) }
    }

    #[test]
    fn placeholders_are_filled() {
        let date = CaptureDate { year: 2026, month: 10, day: 9, hour: 8, minute: 5 };
        let c = context(Some(&date));
        assert_eq!(render(DEFAULT_TEMPLATE, &c), "IMG_0012_edited");
        assert_eq!(render("{撮影日}_{撮影時刻}_{名前}", &c), "20261009_0805_IMG_0012");
        assert_eq!(render("旅行-{連番}_{幅}x{高さ}", &c), "旅行-007_1080x720");
    }

    #[test]
    fn missing_values_and_bad_characters() {
        let c = context(None);
        // 撮影日がなければ空にし、続いた区切りをまとめる
        assert_eq!(render("{撮影日}_{名前}", &c), "IMG_0012");
        assert_eq!(render("{名前}__{撮影日}__x", &c), "IMG_0012_x");
        assert_eq!(render("a/b:c", &c), "a_b_c");
        // 空になれば元の名前
        assert_eq!(render("", &c), "IMG_0012");
        assert_eq!(render("{撮影日}", &c), "IMG_0012");
    }

    #[test]
    fn paths_do_not_overwrite() {
        let dir = std::env::temp_dir().join(format!("ier-naming-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("IMG_0012.jpg");
        std::fs::write(&source, b"x").unwrap();
        let c = context(None);
        // 元のファイルと同じ名前なら _2
        assert_eq!(template_path("{名前}", &c, &dir, "jpg", Some(&source)), dir.join("IMG_0012_2.jpg"));
        std::fs::write(dir.join("IMG_0012_edited.jpg"), b"x").unwrap();
        assert_eq!(
            template_path(DEFAULT_TEMPLATE, &c, &dir, "jpg", Some(&source)),
            dir.join("IMG_0012_edited_2.jpg")
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
