//! メニューバー（旧版と同じく「ファイル」「編集」「表示」）。
//!
//! 画面で処理する項目は、"menu" のイベントで項目の ID を画面に送る。

use tauri::menu::{AboutMetadataBuilder, Menu, MenuEvent, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Runtime};

/// 画面に送るイベントの名前（中身は項目の ID）。
pub const MENU_EVENT: &str = "menu";
/// 「ファイル > 開く…」
pub const OPEN: &str = "open";
/// 「ファイル > 保存…」
pub const SAVE: &str = "save";
/// 「編集 > 文字・透かし…」
pub const TEXT: &str = "text";

/// メニューバーを作る。
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let about = AboutMetadataBuilder::new().name(Some(super::APP_NAME)).build();
    let app_menu = SubmenuBuilder::new(app, super::APP_NAME)
        .about_with_text(format!("{} について", super::APP_NAME), Some(about))
        .separator()
        .services_with_text("サービス")
        .separator()
        .hide_with_text(format!("{} を隠す", super::APP_NAME))
        .hide_others_with_text("ほかを隠す")
        .show_all_with_text("すべてを表示")
        .separator()
        .quit_with_text(format!("{} を終了", super::APP_NAME))
        .build()?;
    let open = MenuItemBuilder::with_id(OPEN, "開く…").accelerator("CmdOrCtrl+O").build(app)?;
    let save = MenuItemBuilder::with_id(SAVE, "保存…").accelerator("CmdOrCtrl+S").build(app)?;
    let file = SubmenuBuilder::new(app, "ファイル")
        .item(&open)
        .item(&save)
        .separator()
        .close_window_with_text("閉じる")
        .build()?;
    // 入力欄で使う標準の編集の項目（元に戻す・貼り付けなどは後の Issue で画像の操作に広げる）
    let text = MenuItemBuilder::with_id(TEXT, "文字・透かし…").accelerator("CmdOrCtrl+T").build(app)?;
    let edit = SubmenuBuilder::new(app, "編集")
        .undo_with_text("取り消す")
        .redo_with_text("やり直す")
        .separator()
        .cut_with_text("カット")
        .copy_with_text("コピー")
        .paste_with_text("ペースト")
        .select_all_with_text("すべてを選択")
        .separator()
        .item(&text)
        .build()?;
    let view = SubmenuBuilder::new(app, "表示").fullscreen_with_text("フルスクリーンにする").build()?;
    let window = SubmenuBuilder::new(app, "ウインドウ")
        .minimize_with_text("しまう")
        .maximize_with_text("拡大／縮小")
        .build()?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window])
}

/// メニューの項目が選ばれたとき、画面に知らせる。
pub fn on_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let _ = app.emit(MENU_EVENT, event.id().as_ref());
}
