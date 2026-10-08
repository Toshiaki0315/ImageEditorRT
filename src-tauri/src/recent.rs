//! 最近使った項目（旧版にはない）: 開いたファイルを新しい順に 10 件まで覚え、「ファイル > 最近使った項目」に出す。
//!
//! 一覧は `~/Library/Application Support/ImageEditorRT/recent.json`（パスの文字列の並び）に残す。なくなった
//! ファイルはメニューに出さない。選ばれた項目は、Finder から開くときと同じ "open-paths" のイベントで画面に送る
//! （未保存の変更の確認は画面が行う）。クリップボードの画像など、ファイルのない画像は入れない。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::menu::{MenuItemBuilder, PredefinedMenuItem};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::open::OPEN_PATHS_EVENT;

/// 「最近使った項目」のサブメニューの ID と、項目の ID の頭（後ろに一覧の中の番号）・「項目を消去」の ID。
pub const MENU_ID: &str = "recent";
const ITEM_PREFIX: &str = "recent:";
const CLEAR_ID: &str = "recent-clear";
/// 覚えておく数。
const LIMIT: usize = 10;
const FILE_NAME: &str = "recent.json";

/// 最近使った項目の一覧（新しい順）。
#[derive(Default)]
pub struct RecentStore(Mutex<Vec<PathBuf>>);

/// path を先頭に足した一覧（同じファイルは 1 つにし、LIMIT 件まで）。
pub(crate) fn pushed(list: &[PathBuf], path: &Path) -> Vec<PathBuf> {
    let key = |p: &Path| p.to_string_lossy().to_lowercase();
    std::iter::once(path.to_path_buf())
        .chain(list.iter().filter(|p| key(p) != key(path)).cloned())
        .take(LIMIT)
        .collect()
}

/// ファイルから一覧を読む（壊れていれば空）。
pub(crate) fn read_list(path: &Path) -> Vec<PathBuf> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .map(|list| list.into_iter().map(PathBuf::from).take(LIMIT).collect())
        .unwrap_or_default()
}

/// 一覧をファイルに書く（書けなくても使い続ける）。
pub(crate) fn write_list(path: &Path, list: &[PathBuf]) {
    let strings: Vec<String> = list.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    if let (Some(parent), Ok(text)) = (path.parent(), serde_json::to_string_pretty(&strings)) {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::write(path, text);
    }
}

fn store_path() -> Option<PathBuf> {
    imageeditorrt_core::presets::app_data_path(FILE_NAME)
}

impl RecentStore {
    /// 起動時: ファイルから読む。
    pub fn load(&self) {
        if let (Some(path), Ok(mut list)) = (store_path(), self.0.lock()) {
            *list = read_list(&path);
        }
    }

    /// 開いたファイルを先頭に足して書く。
    pub fn add(&self, path: &Path) {
        if let Ok(mut list) = self.0.lock() {
            *list = pushed(&list, path);
            if let Some(file) = store_path() {
                write_list(&file, &list);
            }
        }
    }

    /// 一覧を空にして書く。
    pub fn clear(&self) {
        if let Ok(mut list) = self.0.lock() {
            list.clear();
            if let Some(file) = store_path() {
                write_list(&file, &list);
            }
        }
    }

    /// メニューに出す項目（今もあるファイルだけ。番号は一覧の中の番号）。
    fn shown(&self) -> Vec<(usize, PathBuf)> {
        self.0
            .lock()
            .map(|list| list.iter().cloned().enumerate().filter(|(_, p)| p.exists()).collect())
            .unwrap_or_default()
    }

    fn get(&self, index: usize) -> Option<PathBuf> {
        self.0.lock().ok()?.get(index).cloned()
    }
}

/// メニューに出す名前（ファイル名と、入っているフォルダの名前）。
pub(crate) fn label(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match path.parent().and_then(Path::file_name) {
        Some(folder) => format!("{name}（{}）", folder.to_string_lossy()),
        None => name,
    }
}

/// 「最近使った項目」のサブメニューを今の一覧に合わせて作り直す。
pub fn refresh_menu<R: Runtime>(app: &AppHandle<R>) {
    let Some(submenu) = crate::menu::find_submenu(app, MENU_ID) else { return };
    let store = app.state::<RecentStore>();
    let shown = store.shown();
    if let Ok(items) = submenu.items() {
        for item in items {
            let _ = submenu.remove(&item);
        }
    }
    for (index, path) in &shown {
        if let Ok(item) = MenuItemBuilder::with_id(format!("{ITEM_PREFIX}{index}"), label(path)).build(app) {
            let _ = submenu.append(&item);
        }
    }
    if !shown.is_empty() {
        if let Ok(separator) = PredefinedMenuItem::separator(app) {
            let _ = submenu.append(&separator);
        }
    }
    if let Ok(clear) = MenuItemBuilder::with_id(CLEAR_ID, "項目を消去").enabled(!shown.is_empty()).build(app)
    {
        let _ = submenu.append(&clear);
    }
}

/// メニューの項目が「最近使った項目」のものなら処理して true（それ以外は false）。
pub fn on_menu<R: Runtime>(app: &AppHandle<R>, id: &str) -> bool {
    if id == CLEAR_ID {
        app.state::<RecentStore>().clear();
        refresh_menu(app);
        return true;
    }
    let Some(index) = id.strip_prefix(ITEM_PREFIX).and_then(|n| n.parse::<usize>().ok()) else {
        return false;
    };
    if let Some(path) = app.state::<RecentStore>().get(index) {
        let _ = app.emit(OPEN_PATHS_EVENT, vec![path.to_string_lossy().into_owned()]);
    }
    true
}

/// 開けたファイルを一覧に足し、メニューを作り直す。
pub fn opened<R: Runtime>(app: &AppHandle<R>, path: &Path) {
    app.state::<RecentStore>().add(path);
    refresh_menu(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_without_duplicates_up_to_ten() {
        let mut list: Vec<PathBuf> = Vec::new();
        for i in 0..12 {
            list = pushed(&list, Path::new(&format!("/p/{i}.jpg")));
        }
        assert_eq!(list.len(), 10);
        assert_eq!(list[0], PathBuf::from("/p/11.jpg"));
        assert_eq!(list[9], PathBuf::from("/p/2.jpg"));
        // もう一度開いたら先頭へ（大文字・小文字の違いは同じファイル）
        let again = pushed(&list, Path::new("/P/5.JPG"));
        assert_eq!(again[0], PathBuf::from("/P/5.JPG"));
        assert_eq!(again.len(), 10);
        assert_eq!(again.iter().filter(|p| p.to_string_lossy().to_lowercase() == "/p/5.jpg").count(), 1);
    }

    #[test]
    fn reads_and_writes_the_list() {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-recent-{}", std::process::id()));
        let file = dir.join("sub/recent.json");
        let list = vec![PathBuf::from("/a/写真.jpg"), PathBuf::from("/b/c.png")];
        write_list(&file, &list);
        assert_eq!(read_list(&file), list);
        std::fs::write(&file, "broken").unwrap();
        assert!(read_list(&file).is_empty());
        assert!(read_list(&dir.join("none.json")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn shown_skips_missing_files_and_labels_with_the_folder() {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-recent-shown-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let there = dir.join("a.jpg");
        std::fs::write(&there, b"x").unwrap();
        let store = RecentStore(Mutex::new(vec![dir.join("gone.jpg"), there.clone()]));
        assert_eq!(store.shown(), vec![(1, there.clone())]);
        assert_eq!(store.get(1), Some(there.clone()));
        let folder = dir.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(label(&there), format!("a.jpg（{folder}）"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
