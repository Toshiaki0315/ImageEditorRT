//! プリセット（名前付きの加工の組み合わせ。旧版 FR-UI-59）の画面とのやりとり。
//!
//! 一覧は起動時に読み、保存・削除のたびにファイルへ書く（書けたときだけ一覧を変える）。
//! 保存先は `~/Library/Application Support/ImageEditorRT/presets.json`。まだなければ旧版の
//! `~/Library/Application Support/ImageEditor/presets.json` を読む（書くのはいつも新しい保存先）。

use std::path::PathBuf;
use std::sync::Mutex;

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

/// 読み込んだプリセットの一覧。
#[derive(Default)]
pub struct PresetStore(Mutex<Vec<Preset>>);

/// 起動時に読んだ結果。error は読めなかったときに知らせる文言（そのときはプリセットなしで使う）。
#[derive(Serialize)]
pub struct Loaded {
    names: Vec<String>,
    error: Option<String>,
}

/// 名前を確かめた結果。
#[derive(Serialize)]
pub struct NameCheck {
    /// 前後の空白を除き、長すぎれば切り詰めた名前（空なら保存しない）
    name: String,
    /// 同じ名前のプリセットがある（上書きを確かめる）
    exists: bool,
}

fn path() -> Result<PathBuf, String> {
    presets::presets_path().ok_or_else(|| "プリセットの保存先が分かりません（HOME がありません）".to_string())
}

fn names(list: &[Preset]) -> Vec<String> {
    list.iter().map(|p| p.name.clone()).collect()
}

/// 起動時: プリセットを読む。
#[tauri::command]
pub fn load_presets(store: State<'_, PresetStore>) -> Result<Loaded, String> {
    let result = path()
        .map_err(PresetError)
        .and_then(|path| presets::load_presets_with_legacy(&path, presets::legacy_presets_path().as_deref()));
    let mut list = store.0.lock().map_err(|e| e.to_string())?;
    let error = match result {
        Ok(loaded) => {
            *list = loaded;
            None
        }
        Err(e) => Some(e.to_string()),
    };
    Ok(Loaded { names: names(&list), error })
}

/// 保存ダイアログの初期の名前（「プリセット 1」… の空いている名前）。
#[tauri::command]
pub fn default_preset_name(store: State<'_, PresetStore>) -> Result<String, String> {
    Ok(presets::default_preset_name(&store.0.lock().map_err(|e| e.to_string())?))
}

/// 入力した名前を整え、同じ名前があるかを返す。
#[tauri::command]
pub fn check_preset_name(name: String, store: State<'_, PresetStore>) -> Result<NameCheck, String> {
    let name = presets::normalize_name(&name);
    let exists = store.0.lock().map_err(|e| e.to_string())?.iter().any(|p| p.name == name);
    Ok(NameCheck { name, exists })
}

/// 今の加工を name で保存する（同じ名前は置き換える）。保存後の名前の一覧を返す。
#[tauri::command]
pub fn save_preset(
    name: String,
    settings: EditSettings,
    store: State<'_, PresetStore>,
) -> Result<Vec<String>, String> {
    let mut list = store.0.lock().map_err(|e| e.to_string())?;
    let updated = presets::upsert_preset(&list, Preset::from_settings(&name, &settings));
    presets::save_presets(&path()?, &updated).map_err(|e| e.to_string())?;
    *list = updated;
    Ok(names(&list))
}

/// name のプリセットを削除する。削除後の名前の一覧を返す。
#[tauri::command]
pub fn delete_preset(name: String, store: State<'_, PresetStore>) -> Result<Vec<String>, String> {
    let mut list = store.0.lock().map_err(|e| e.to_string())?;
    let updated = presets::remove_preset(&list, &name);
    presets::save_presets(&path()?, &updated).map_err(|e| e.to_string())?;
    *list = updated;
    Ok(names(&list))
}

/// 今の設定に name のプリセットの加工を当てはめた設定（サイズ・範囲・向きはそのまま）。
#[tauri::command]
pub fn apply_preset(
    name: String,
    settings: EditSettings,
    store: State<'_, PresetStore>,
) -> Result<EditSettings, String> {
    let list = store.0.lock().map_err(|e| e.to_string())?;
    let preset =
        list.iter().find(|p| p.name == name).ok_or_else(|| format!("プリセット「{name}」がありません"))?;
    Ok(preset.apply(&settings))
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
    let list = names(&store.0.lock().map_err(|e| e.to_string())?);
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
