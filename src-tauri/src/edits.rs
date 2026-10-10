//! 写真ごとの加工の記録（旧版にはない）: 保存したとき・加工したまま別の写真を開いたときに、写真ごとの加工の設定を
//! 覚えておき、同じ写真を次に開いたときに「前回の加工を続ける」で当てはめられるようにする。元の画像は変えない。
//!
//! 記録は `~/Library/Application Support/ImageEditorRT/edits.json`。写真はファイルの場所で見分け、ファイルの大きさと
//! 更新日時が覚えたときと違えば（写真が書き換わっていれば）使わない。新しい順に LIMIT 件まで覚える。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use imageeditorrt_core::pipeline::EditSettings;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// 覚えておく写真の数。
const LIMIT: usize = 200;
const FILE_NAME: &str = "edits.json";

/// 1 枚分の記録。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Entry {
    path: PathBuf,
    /// 覚えたときのファイルの大きさ（バイト）と更新日時（UNIX 時刻の秒）
    size: u64,
    modified: u64,
    settings: EditSettings,
}

/// 写真ごとの加工の記録（新しい順）。
#[derive(Default)]
pub struct EditsStore(Mutex<Option<Vec<Entry>>>);

/// ファイルの大きさと更新日時（見分けに使う）。読めなければ None。
fn stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some((meta.len(), modified))
}

fn same_file(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// entry を先頭に足した一覧（同じ写真は 1 つにし、LIMIT 件まで）。
pub(crate) fn upserted(list: &[Entry], entry: Entry) -> Vec<Entry> {
    let path = entry.path.clone();
    std::iter::once(entry)
        .chain(list.iter().filter(|e| !same_file(&e.path, &path)).cloned())
        .take(LIMIT)
        .collect()
}

/// path の写真の記録（ファイルの大きさ・更新日時が覚えたときと同じものだけ）。
pub(crate) fn found(list: &[Entry], path: &Path, (size, modified): (u64, u64)) -> Option<EditSettings> {
    list.iter()
        .find(|e| same_file(&e.path, path) && e.size == size && e.modified == modified)
        .map(|e| e.settings.clone())
}

fn store_path() -> Option<PathBuf> {
    imageeditorrt_core::presets::app_data_path(FILE_NAME)
}

fn read_list(path: &Path) -> Vec<Entry> {
    std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn write_list(path: &Path, list: &[Entry]) {
    if let (Some(parent), Ok(text)) = (path.parent(), serde_json::to_string(list)) {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::write(path, text);
    }
}

impl EditsStore {
    /// 一覧（はじめて使うときにファイルから読む）を f に渡す。
    fn with<T>(&self, f: impl FnOnce(&mut Vec<Entry>) -> T) -> Option<T> {
        let mut guard = self.0.lock().ok()?;
        let list = guard.get_or_insert_with(|| store_path().map(|p| read_list(&p)).unwrap_or_default());
        Some(f(list))
    }
}

/// 開いている写真の今の加工を覚える（元のファイルのない画像・加工していない写真は覚えない）。
#[tauri::command]
pub fn remember_edits(
    settings: EditSettings,
    state: State<'_, AppState>,
    edits: State<'_, EditsStore>,
) -> Result<(), String> {
    let path = {
        let loaded = state.0.lock().map_err(|e| e.to_string())?;
        loaded.source.path.clone()
    };
    let Some(path) = path else { return Ok(()) };
    if settings == EditSettings::default() {
        return Ok(());
    }
    let Some((size, modified)) = stamp(&path) else { return Ok(()) };
    edits.with(|list| {
        *list = upserted(list, Entry { path, size, modified, settings });
        if let Some(file) = store_path() {
            write_list(&file, list);
        }
    });
    Ok(())
}

/// 開いている写真の前回の加工（なければ None）。
#[tauri::command]
pub fn recall_edits(
    state: State<'_, AppState>,
    edits: State<'_, EditsStore>,
) -> Result<Option<EditSettings>, String> {
    let path = {
        let loaded = state.0.lock().map_err(|e| e.to_string())?;
        loaded.source.path.clone()
    };
    let Some(path) = path else { return Ok(None) };
    let Some(current) = stamp(&path) else { return Ok(None) };
    Ok(edits.with(|list| found(list, &path, current)).flatten())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, exposure: f64) -> Entry {
        Entry {
            path: path.into(),
            size: 10,
            modified: 100,
            settings: EditSettings { exposure, ..EditSettings::default() },
        }
    }

    #[test]
    fn newest_first_one_per_photo_and_limited() {
        let list = upserted(&[], entry("/a.jpg", 1.0));
        let list = upserted(&list, entry("/b.jpg", 2.0));
        // 同じ写真（大文字・小文字の違いも同じ）は新しいものだけ
        let list = upserted(&list, entry("/A.JPG", 3.0));
        assert_eq!(list.iter().map(|e| e.settings.exposure).collect::<Vec<_>>(), [3.0, 2.0]);
        let mut many = Vec::new();
        for i in 0..LIMIT + 5 {
            many = upserted(&many, entry(&format!("/{i}.jpg"), 0.5));
        }
        assert_eq!(many.len(), LIMIT);
        assert_eq!(many[0].path, PathBuf::from(format!("/{}.jpg", LIMIT + 4)), "古いものから消す");
    }

    #[test]
    fn changed_photos_are_not_resumed() {
        let list = [entry("/a.jpg", 1.5)];
        assert_eq!(found(&list, Path::new("/a.jpg"), (10, 100)).map(|s| s.exposure), Some(1.5));
        // 大きさか更新日時が違えば、写真が書き換わっているので使わない
        assert_eq!(found(&list, Path::new("/a.jpg"), (11, 100)), None);
        assert_eq!(found(&list, Path::new("/a.jpg"), (10, 101)), None);
        assert_eq!(found(&list, Path::new("/b.jpg"), (10, 100)), None);
    }

    #[test]
    fn list_round_trips_through_the_file() {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-edits-{}", std::process::id()));
        let file = dir.join("edits.json");
        let list = vec![entry("/a.jpg", 1.0), entry("/b.jpg", -0.5)];
        write_list(&file, &list);
        assert_eq!(read_list(&file), list);
        std::fs::write(&file, b"broken").unwrap();
        assert!(read_list(&file).is_empty(), "壊れていれば空");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
