//! キーボードショートカットの変更（旧版にはない）: メニューの項目ごとのショートカットの書き方の確かめと、
//! 変えた分（既定との違い）の読み書き。
//!
//! 書き方は Tauri のメニューと同じ（`CmdOrCtrl+Shift+K` のように、修飾キーを + でつなぎ、最後にキー）。
//! 変えた分だけをアプリのデータのフォルダの `shortcuts.json` に覚える（外したものは空の文字）。

use std::collections::BTreeMap;
use std::path::Path;

/// 覚えるファイルの名前。
pub const FILE_NAME: &str = "shortcuts.json";

/// 修飾キー（書く順）。
const MODIFIERS: [&str; 5] = ["CmdOrCtrl", "Ctrl", "Alt", "Shift", "Super"];
/// 修飾キーなしでも使えるキー（F1〜F12）。
fn is_function_key(key: &str) -> bool {
    key.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=12).contains(&n))
}

/// 使えるキー（英字・数字・記号・F キー・特別なキー）。
fn is_key(key: &str) -> bool {
    let single = key.chars().count() == 1
        && key.chars().all(|c| c.is_ascii_alphanumeric() || "[]/\\,.;'-=`".contains(c));
    single
        || is_function_key(key)
        || matches!(
            key,
            "Space" | "Enter" | "Backspace" | "Delete" | "Escape" | "Tab" | "Up" | "Down" | "Left" | "Right"
        )
}

/// ショートカットの書き方を確かめて整える（修飾キーを決まった順に並べ、英字は大文字）。空ならショートカットなし。
pub fn normalize(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(String::new());
    }
    let parts: Vec<&str> = text.split('+').map(str::trim).collect();
    let (key, modifiers) = parts.split_last().ok_or("ショートカットが空です")?;
    let key = if key.chars().count() == 1 { key.to_ascii_uppercase() } else { key.to_string() };
    if !is_key(&key) {
        return Err(format!("使えないキーです: {key}"));
    }
    let mut found = Vec::new();
    for m in modifiers {
        let name = MODIFIERS
            .iter()
            .find(|known| {
                known.eq_ignore_ascii_case(m) || (m.eq_ignore_ascii_case("Cmd") && **known == "CmdOrCtrl")
            })
            .ok_or_else(|| format!("使えない修飾キーです: {m}"))?;
        if !found.contains(name) {
            found.push(*name);
        }
    }
    if found.is_empty() && !is_function_key(&key) {
        return Err("⌘・⌥・⌃・⇧ のどれかと組み合わせてください（F1〜F12 はそのままでも使えます）".into());
    }
    found.sort_by_key(|m| MODIFIERS.iter().position(|k| k == m));
    found.push(&key);
    Ok(found.join("+"))
}

/// shortcuts（項目の ID → ショートカット）の中で、accelerator と同じものを持つ項目（except を除く）。
pub fn conflict<'a>(
    shortcuts: &'a BTreeMap<String, String>,
    accelerator: &str,
    except: &str,
) -> Option<&'a str> {
    if accelerator.is_empty() {
        return None;
    }
    shortcuts
        .iter()
        .find(|(id, a)| id.as_str() != except && a.as_str() == accelerator)
        .map(|(id, _)| id.as_str())
}

/// 変えた分を読む（ファイルがない・壊れていれば何も変えていないことにする）。
pub fn load(path: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<BTreeMap<String, String>>(&text).ok())
        .map(|map| map.into_iter().filter(|(_, a)| a.is_empty() || normalize(a).is_ok()).collect())
        .unwrap_or_default()
}

/// 変えた分を書く。
pub fn save(path: &Path, overrides: &BTreeMap<String, String>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("ショートカットを保存できません（{e}）"))?;
    }
    let text = serde_json::to_string_pretty(overrides).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("ショートカットを保存できません（{e}）"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accelerators_are_checked_and_tidied() {
        assert_eq!(normalize("shift+cmdorctrl+k").unwrap(), "CmdOrCtrl+Shift+K");
        assert_eq!(normalize("Cmd+Alt+1").unwrap(), "CmdOrCtrl+Alt+1");
        assert_eq!(normalize("CmdOrCtrl+[").unwrap(), "CmdOrCtrl+[");
        assert_eq!(normalize("F5").unwrap(), "F5", "F キーはそのままでも使える");
        assert_eq!(normalize("  ").unwrap(), "", "空はショートカットなし");
        assert!(normalize("K").is_err(), "修飾キーなしの文字は使えない");
        assert!(normalize("CmdOrCtrl+Hyper+K").is_err());
        assert!(normalize("CmdOrCtrl+%").is_err());
    }

    #[test]
    fn conflicts_are_found() {
        let map: BTreeMap<String, String> = [
            ("save".to_string(), "CmdOrCtrl+S".to_string()),
            ("open".to_string(), "CmdOrCtrl+O".to_string()),
        ]
        .into();
        assert_eq!(conflict(&map, "CmdOrCtrl+S", "print"), Some("save"));
        assert_eq!(conflict(&map, "CmdOrCtrl+S", "save"), None, "自分とは重ならない");
        assert_eq!(conflict(&map, "", "print"), None);
    }

    #[test]
    fn overrides_round_trip_and_bad_entries_are_dropped() {
        let dir = std::env::temp_dir().join(format!("ier-shortcuts-{}", std::process::id()));
        let path = dir.join(FILE_NAME);
        let map: BTreeMap<String, String> =
            [("save".to_string(), "CmdOrCtrl+Shift+S".to_string()), ("print".to_string(), String::new())]
                .into();
        save(&path, &map).unwrap();
        assert_eq!(load(&path), map);
        std::fs::write(&path, r#"{"save": "Nope+X", "open": "CmdOrCtrl+K"}"#).unwrap();
        assert_eq!(load(&path), [("open".to_string(), "CmdOrCtrl+K".to_string())].into());
        std::fs::write(&path, "broken").unwrap();
        assert!(load(&path).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
