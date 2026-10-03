//! プリセット（名前付きの加工の組み合わせ。旧版 FR-UI-59）の画面とのやりとり。
//!
//! 一覧は起動時に読み、保存・削除のたびにファイルへ書く（書けたときだけ一覧を変える）。
//! 保存先は `~/Library/Application Support/ImageEditorRT/presets.json`。まだなければ旧版の
//! `~/Library/Application Support/ImageEditor/presets.json` を読む（書くのはいつも新しい保存先）。

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use imageeditorrt_core::pipeline::EditSettings;
use imageeditorrt_core::presets::{self, Preset, PresetError};
use serde::Serialize;
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{LogicalPosition, State, WebviewWindow};

/// メニューの項目の ID の頭（後ろに一覧の中の番号を付ける）。選ばれると "menu" のイベントで画面に送る。
const APPLY_PREFIX: &str = "preset-apply:";
const DELETE_PREFIX: &str = "preset-delete:";
/// 「今の加工をプリセットとして保存…」
const SAVE_ID: &str = "preset-save";

/// 読み込んだプリセットの一覧。読み書きは保存先のパスを受け取る（テストでは一時フォルダを渡す）。
#[derive(Default)]
pub struct PresetStore(Mutex<Vec<Preset>>);

/// 起動時に読んだ結果。error は読めなかったときに知らせる文言（そのときはプリセットなしで使う）。
#[derive(Debug, PartialEq, Serialize)]
pub struct Loaded {
    names: Vec<String>,
    error: Option<String>,
}

/// 名前を確かめた結果。
#[derive(Debug, PartialEq, Serialize)]
pub struct NameCheck {
    /// 前後の空白を除き、長すぎれば切り詰めた名前（空なら保存しない）
    name: String,
    /// 同じ名前のプリセットがある（上書きを確かめる）
    exists: bool,
}

fn names(list: &[Preset]) -> Vec<String> {
    list.iter().map(|p| p.name.clone()).collect()
}

impl PresetStore {
    fn list(&self) -> Result<MutexGuard<'_, Vec<Preset>>, String> {
        self.0.lock().map_err(|e| e.to_string())
    }

    /// name のプリセット（まとめて処理でかける）。
    pub fn find(&self, name: &str) -> Option<Preset> {
        self.list().ok()?.iter().find(|p| p.name == name).cloned()
    }

    /// path（まだなければ legacy）からプリセットを読む。読めなければ一覧は空のままにし、知らせる文言を返す。
    fn load(&self, path: Result<PathBuf, String>, legacy: Option<&Path>) -> Result<Loaded, String> {
        let result =
            path.map_err(PresetError).and_then(|path| presets::load_presets_with_legacy(&path, legacy));
        let mut list = self.list()?;
        let error = match result {
            Ok(loaded) => {
                *list = loaded;
                None
            }
            Err(e) => Some(e.to_string()),
        };
        Ok(Loaded { names: names(&list), error })
    }

    fn default_name(&self) -> Result<String, String> {
        Ok(presets::default_preset_name(&self.list()?))
    }

    fn check_name(&self, name: &str) -> Result<NameCheck, String> {
        let name = presets::normalize_name(name);
        let exists = self.list()?.iter().any(|p| p.name == name);
        Ok(NameCheck { name, exists })
    }

    /// 一覧を updated にしてファイルに書く。書けなければ一覧は変えない。
    fn store(
        &self,
        path: &Path,
        update: impl FnOnce(&[Preset]) -> Vec<Preset>,
    ) -> Result<Vec<String>, String> {
        let mut list = self.list()?;
        let updated = update(&list);
        presets::save_presets(path, &updated).map_err(|e| e.to_string())?;
        *list = updated;
        Ok(names(&list))
    }

    fn save(&self, path: &Path, name: &str, settings: &EditSettings) -> Result<Vec<String>, String> {
        self.store(path, |list| presets::upsert_preset(list, Preset::from_settings(name, settings)))
    }

    fn delete(&self, path: &Path, name: &str) -> Result<Vec<String>, String> {
        self.store(path, |list| presets::remove_preset(list, name))
    }

    fn apply(&self, name: &str, settings: &EditSettings) -> Result<EditSettings, String> {
        let preset = self.find(name).ok_or_else(|| format!("プリセット「{name}」がありません"))?;
        Ok(preset.apply(settings))
    }
}

fn path() -> Result<PathBuf, String> {
    presets::presets_path().ok_or_else(|| "プリセットの保存先が分かりません（HOME がありません）".to_string())
}

/// 起動時: プリセットを読む。
#[tauri::command]
pub fn load_presets(store: State<'_, PresetStore>) -> Result<Loaded, String> {
    store.load(path(), presets::legacy_presets_path().as_deref())
}

/// 保存ダイアログの初期の名前（「プリセット 1」… の空いている名前）。
#[tauri::command]
pub fn default_preset_name(store: State<'_, PresetStore>) -> Result<String, String> {
    store.default_name()
}

/// 入力した名前を整え、同じ名前があるかを返す。
#[tauri::command]
pub fn check_preset_name(name: String, store: State<'_, PresetStore>) -> Result<NameCheck, String> {
    store.check_name(&name)
}

/// 今の加工を name で保存する（同じ名前は置き換える）。保存後の名前の一覧を返す。
#[tauri::command]
pub fn save_preset(
    name: String,
    settings: EditSettings,
    store: State<'_, PresetStore>,
) -> Result<Vec<String>, String> {
    store.save(&path()?, &name, &settings)
}

/// name のプリセットを削除する。削除後の名前の一覧を返す。
#[tauri::command]
pub fn delete_preset(name: String, store: State<'_, PresetStore>) -> Result<Vec<String>, String> {
    store.delete(&path()?, &name)
}

/// 今の設定に name のプリセットの加工を当てはめた設定（サイズ・範囲・向きはそのまま）。
#[tauri::command]
pub fn apply_preset(
    name: String,
    settings: EditSettings,
    store: State<'_, PresetStore>,
) -> Result<EditSettings, String> {
    store.apply(&name, &settings)
}

/// 「プリセット ▾」のメニューを (x, y)（ウィンドウの中の位置）に出す。
///
/// 一覧（選ぶと当てはめる）／「今の加工をプリセットとして保存…」／「削除」（一覧のサブメニュー）。
/// プリセットがなければ「（保存したプリセットはありません）」と出し、「削除」は選べない。
/// 画像がなければ、当てはめ・保存は選べない。
#[tauri::command]
pub async fn show_preset_menu(
    x: f64,
    y: f64,
    loaded: bool,
    window: WebviewWindow,
    store: State<'_, PresetStore>,
) -> Result<(), String> {
    let list = names(&store.list()?);
    let error = |e: tauri::Error| e.to_string();
    let mut menu = MenuBuilder::new(&window);
    if list.is_empty() {
        menu = menu.item(
            &MenuItemBuilder::new("（保存したプリセットはありません）")
                .enabled(false)
                .build(&window)
                .map_err(error)?,
        );
    }
    for (i, name) in list.iter().enumerate() {
        let item =
            MenuItemBuilder::with_id(format!("{APPLY_PREFIX}{i}"), name).enabled(loaded).build(&window);
        menu = menu.item(&item.map_err(error)?);
    }
    let save =
        MenuItemBuilder::with_id(SAVE_ID, "今の加工をプリセットとして保存…").enabled(loaded).build(&window);
    let mut delete = SubmenuBuilder::new(&window, "削除").enabled(!list.is_empty());
    for (i, name) in list.iter().enumerate() {
        let item = MenuItemBuilder::with_id(format!("{DELETE_PREFIX}{i}"), name).build(&window);
        delete = delete.item(&item.map_err(error)?);
    }
    let menu = menu
        .separator()
        .item(&save.map_err(error)?)
        .item(&delete.build().map_err(error)?)
        .build()
        .map_err(error)?;
    window.popup_menu_at(&menu, LogicalPosition::new(x, y)).map_err(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use imageeditorrt_core::filters::FilterType;
    use imageeditorrt_core::transform::CropRect;

    /// テストごとの空のフォルダ。
    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("imageeditorrt-app-presets-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sepia() -> EditSettings {
        EditSettings { filter: FilterType::Sepia, exposure: 1.0, ..EditSettings::default() }
    }

    #[test]
    fn save_overwrite_delete_and_reload() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("presets.json");
        let store = PresetStore::default();
        assert_eq!(store.load(Ok(path.clone()), None).unwrap(), Loaded { names: vec![], error: None });
        assert_eq!(store.default_name().unwrap(), "プリセット 1");
        assert_eq!(store.save(&path, "夕焼け", &sepia()).unwrap(), vec!["夕焼け"]);
        assert_eq!(store.save(&path, "B", &EditSettings::default()).unwrap(), vec!["夕焼け", "B"]);
        // 同じ名前は置き換える（並びはそのまま）
        assert_eq!(store.save(&path, "夕焼け", &EditSettings::default()).unwrap(), vec!["夕焼け", "B"]);
        assert_eq!(store.find("夕焼け").unwrap().filter, FilterType::None);
        assert_eq!(store.delete(&path, "B").unwrap(), vec!["夕焼け"]);
        // 書いたファイルを読み直すと同じ
        let again = PresetStore::default();
        assert_eq!(again.load(Ok(path), None).unwrap().names, vec!["夕焼け"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn names_are_normalized_and_checked() {
        let dir = temp_dir("names");
        let path = dir.join("presets.json");
        let store = PresetStore::default();
        store.save(&path, "夕焼け", &sepia()).unwrap();
        assert_eq!(
            store.check_name("  夕焼け  ").unwrap(),
            NameCheck { name: "夕焼け".into(), exists: true }
        );
        assert_eq!(store.check_name("新しい").unwrap(), NameCheck { name: "新しい".into(), exists: false });
        assert_eq!(store.check_name("   ").unwrap().name, "");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn apply_keeps_crop_and_unknown_name_is_an_error() {
        let dir = temp_dir("apply");
        let path = dir.join("presets.json");
        let store = PresetStore::default();
        store.save(&path, "夕焼け", &sepia()).unwrap();
        let current = EditSettings {
            crop: Some(CropRect::new(1, 2, 3, 4)),
            width: Some(100),
            ..EditSettings::default()
        };
        let applied = store.apply("夕焼け", &current).unwrap();
        assert_eq!((applied.filter, applied.exposure), (FilterType::Sepia, 1.0));
        assert_eq!((applied.crop, applied.width), (current.crop, current.width));
        assert_eq!(store.apply("ない", &current).unwrap_err(), "プリセット「ない」がありません");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn broken_file_and_write_errors_keep_the_list() {
        let dir = temp_dir("broken");
        let path = dir.join("presets.json");
        std::fs::write(&path, "{ broken").unwrap();
        let store = PresetStore::default();
        let loaded = store.load(Ok(path.clone()), None).unwrap();
        assert!(loaded.names.is_empty());
        assert!(loaded.error.unwrap().starts_with("プリセットのファイルの形式が正しくありません"));
        // 保存先がわからないときも、プリセットなしで使える
        let no_home = store.load(Err("保存先が分かりません".into()), None).unwrap();
        assert_eq!(no_home.error.as_deref(), Some("保存先が分かりません"));
        // 書けない場所（ファイルの下）には保存できず、一覧は変えない
        let good = dir.join("good.json");
        store.save(&good, "A", &EditSettings::default()).unwrap();
        let error = store.save(&path.join("x.json"), "B", &EditSettings::default()).unwrap_err();
        assert!(error.starts_with("プリセットを保存できません"), "{error}");
        assert_eq!(store.find("B"), None);
        assert!(store.find("A").is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_the_legacy_file_until_the_new_one_exists() {
        let dir = temp_dir("legacy");
        let legacy = dir.join("old/presets.json");
        presets::save_presets(&legacy, &[Preset::from_settings("旧版", &sepia())]).unwrap();
        let path = dir.join("new/presets.json");
        let store = PresetStore::default();
        assert_eq!(store.load(Ok(path.clone()), Some(&legacy)).unwrap().names, vec!["旧版"]);
        // 保存は新しい保存先に書く（旧版のファイルは変えない）
        store.save(&path, "新", &EditSettings::default()).unwrap();
        assert!(path.exists());
        assert_eq!(presets::load_presets(&legacy).unwrap().len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
