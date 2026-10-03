//! macOS とのやりとり: メニューの状態・マップで開く・終了の確認。

use std::sync::atomic::{AtomicBool, Ordering};

use imageeditorrt_core::exif_info::GpsPosition;
use tauri::{AppHandle, Emitter};

use crate::menu;

/// 画面が終了してよいと確かめたか（Dock の「終了」などで届く終了の求めを、それまでは止める）。
struct QuitGate(AtomicBool);

impl QuitGate {
    const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// 画面が確かめた（この後の終了の求めは止めない）。
    fn confirm(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// 終了の求めを止めるか（画面がまだ確かめていなければ止める）。
    fn holds(&self) -> bool {
        !self.0.load(Ordering::SeqCst)
    }
}

static QUIT: QuitGate = QuitGate::new();
/// 終了の求めを画面に知らせるイベント（画面が未保存の変更を確かめて、quit_app を呼ぶ）。
const QUIT_EVENT: &str = "quit-requested";

/// アプリを終了する（画面が未保存の変更を確かめた後に呼ぶ）。
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    QUIT.confirm();
    app.exit(0);
}

/// Dock の「終了」・ログアウトなどで届いた終了の求めを止めるか。画面がまだ確かめていなければ止め、
/// 画面に知らせる（画面が未保存の変更を確かめて quit_app を呼ぶ）。
pub(crate) fn hold_exit(app: &AppHandle) -> bool {
    if !QUIT.holds() {
        return false;
    }
    let _ = app.emit(QUIT_EVENT, ());
    true
}

/// メニューの項目を使える・使えないにする（100% で表示・画面に合わせる）。
#[tauri::command]
pub fn set_menu_enabled(id: String, enabled: bool, app: AppHandle) -> Result<(), String> {
    menu::set_enabled(&app, &id, enabled).map_err(|e| e.to_string())
}

/// メニューのチェックの付く項目の状態を変える（環境設定に残した状態に合わせる）。
#[tauri::command]
pub fn set_menu_checked(id: String, checked: bool, app: AppHandle) -> Result<(), String> {
    menu::set_checked(&app, &id, checked).map_err(|e| e.to_string())
}

/// 撮影した場所を macOS のマップアプリで開く（「EXIF」タブの「マップで開く」。押したときだけ）。
#[tauri::command]
pub fn open_map(latitude: f64, longitude: f64) -> Result<(), String> {
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return Err("位置情報が正しくありません".into());
    }
    let url = GpsPosition { latitude, longitude }.map_url();
    std::process::Command::new("/usr/bin/open").arg(url).status().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_is_held_until_confirmed() {
        let gate = QuitGate::new();
        assert!(gate.holds());
        assert!(gate.holds()); // 何度求められても、確かめるまでは止める
        gate.confirm();
        assert!(!gate.holds());
    }
}
