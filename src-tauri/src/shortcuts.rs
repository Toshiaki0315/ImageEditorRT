//! キーボードショートカットの変更（旧版にはない）: メニューの項目ごとにショートカットを変える・外す・既定に戻す。
//!
//! 既定はメニューを作るときに覚えたもの（`menu::default_shortcut_of`）。変えた分だけをアプリのデータのフォルダの
//! `shortcuts.json` に覚え（`core::shortcuts`）、起動したときと変えたときにメニューの項目に当てる。

use std::collections::BTreeMap;

use imageeditorrt_core::presets::app_data_path;
use imageeditorrt_core::shortcuts::{self, FILE_NAME};
use serde::Serialize;
use tauri::menu::MenuItemKind;
use tauri::{AppHandle, Runtime};

use crate::menu;

/// ショートカットを変えられない項目（中身が変わるメニュー・最近使った項目など）の ID の頭。
const SKIP_PREFIXES: [&str; 3] = ["recent", "preset-", "about"];

/// 一覧の 1 行。
#[derive(Clone, Serialize)]
pub struct ShortcutItem {
    id: String,
    /// メニューの名前（「ファイル」など）と項目の名前
    menu: String,
    label: String,
    default: String,
    current: String,
}

fn overrides() -> BTreeMap<String, String> {
    app_data_path(FILE_NAME).map(|path| shortcuts::load(&path)).unwrap_or_default()
}

/// メニューの項目（変えられるもの）を、メニューの順に並べる（メニューの名前・項目）。
fn items<R: Runtime>(app: &AppHandle<R>) -> Vec<(String, MenuItemKind<R>)> {
    let mut list = Vec::new();
    let Some(bar) = app.menu() else { return list };
    for top in bar.items().unwrap_or_default() {
        let Some(sub) = top.as_submenu() else { continue };
        let title = sub.text().unwrap_or_default();
        for item in sub.items().unwrap_or_default() {
            let id = item.id().0.clone();
            if SKIP_PREFIXES.iter().any(|p| id.starts_with(p)) {
                continue;
            }
            if item.as_menuitem().is_some() || item.as_check_menuitem().is_some() {
                list.push((title.clone(), item));
            }
        }
    }
    list
}

/// 項目のショートカットを変える（空なら外す）。
fn apply_to<R: Runtime>(item: &MenuItemKind<R>, accelerator: &str) {
    let value = (!accelerator.is_empty()).then_some(accelerator);
    if let Some(m) = item.as_menuitem() {
        let _ = m.set_accelerator(value);
    } else if let Some(c) = item.as_check_menuitem() {
        let _ = c.set_accelerator(value);
    }
}

/// 起動したとき: 覚えておいた変更をメニューに当てる。
pub fn apply_saved<R: Runtime>(app: &AppHandle<R>) {
    let saved = overrides();
    if saved.is_empty() {
        return;
    }
    for (_, item) in items(app) {
        if let Some(accelerator) = saved.get(&item.id().0) {
            apply_to(&item, accelerator);
        }
    }
}

/// 今のショートカットの一覧。
fn list<R: Runtime>(app: &AppHandle<R>) -> Vec<ShortcutItem> {
    let saved = overrides();
    items(app)
        .into_iter()
        .map(|(menu_title, item)| {
            let id = item.id().0.clone();
            let label = item
                .as_menuitem()
                .and_then(|m| m.text().ok())
                .or_else(|| item.as_check_menuitem().and_then(|c| c.text().ok()))
                .unwrap_or_default();
            let default = menu::default_shortcut_of(&id);
            let current = saved.get(&id).cloned().unwrap_or_else(|| default.clone());
            ShortcutItem { id, menu: menu_title, label, default, current }
        })
        .collect()
}

/// 「ショートカット…」の一覧。
#[tauri::command]
pub fn shortcut_list<R: Runtime>(app: AppHandle<R>) -> Vec<ShortcutItem> {
    list(&app)
}

/// 項目のショートカットを変える（空なら外す）。ほかの項目と重なればエラー。変えた後の一覧を返す。
#[tauri::command]
pub fn set_shortcut<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    accelerator: String,
) -> Result<Vec<ShortcutItem>, String> {
    let accelerator = shortcuts::normalize(&accelerator)?;
    let current = list(&app);
    let map: BTreeMap<String, String> = current.iter().map(|i| (i.id.clone(), i.current.clone())).collect();
    if let Some(other) = shortcuts::conflict(&map, &accelerator, &id) {
        let label = current.iter().find(|i| i.id == other).map_or(other, |i| i.label.as_str());
        return Err(format!("「{label}」と同じショートカットです。先にそちらを変えてください"));
    }
    let item = current.iter().find(|i| i.id == id).ok_or("その項目はありません")?;
    let mut saved = overrides();
    if accelerator == item.default {
        saved.remove(&id);
    } else {
        saved.insert(id.clone(), accelerator.clone());
    }
    let path = app_data_path(FILE_NAME).ok_or("アプリのデータのフォルダが分かりません")?;
    shortcuts::save(&path, &saved)?;
    if let Some((_, menu_item)) = items(&app).into_iter().find(|(_, i)| i.id().0 == id) {
        apply_to(&menu_item, &accelerator);
    }
    Ok(list(&app))
}

/// すべて既定に戻す。
#[tauri::command]
pub fn reset_shortcuts<R: Runtime>(app: AppHandle<R>) -> Result<Vec<ShortcutItem>, String> {
    let path = app_data_path(FILE_NAME).ok_or("アプリのデータのフォルダが分かりません")?;
    shortcuts::save(&path, &BTreeMap::new())?;
    for (_, item) in items(&app) {
        apply_to(&item, &menu::default_shortcut_of(&item.id().0));
    }
    Ok(list(&app))
}
