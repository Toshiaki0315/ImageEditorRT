//! メニューバー（旧版と同じく「ファイル」「編集」「表示」）。
//!
//! 画面で処理する項目は、"menu" のイベントで項目の ID を画面に送る。

use tauri::menu::{
    AboutMetadataBuilder, CheckMenuItemBuilder, Menu, MenuEvent, MenuItemBuilder, MenuItemKind,
    SubmenuBuilder,
};
use tauri::{AppHandle, Emitter, Runtime};

/// 画面に送るイベントの名前（中身は項目の ID）。
pub const MENU_EVENT: &str = "menu";
/// 「ImageEditorRT を終了」（未保存の変更があれば画面が確かめてから終わる）
pub const QUIT: &str = "quit";
/// 「ファイル > 開く…」
pub const OPEN: &str = "open";
/// 「ファイル > 次の写真」「前の写真」（同じフォルダの写真を名前順に。元のファイルのない画像では使えない）
pub const NEXT_PHOTO: &str = "next-photo";
pub const PREVIOUS_PHOTO: &str = "previous-photo";
/// 「ファイル > 保存…」
pub const SAVE: &str = "save";
/// 「ファイル > まとめて処理…」
pub const BATCH: &str = "batch";
/// 「ファイル > 複数の大きさで保存…」（よく使う大きさから選んで、続けて保存する）
pub const SAVE_SIZES: &str = "save-sizes";
/// 「ファイル > 保存したファイルを Finder で表示」（最後に保存したファイル。保存するまでは使えない）
pub const REVEAL_SAVED: &str = "reveal-saved";
/// 「ファイル > プリント…」（加工後の画像を macOS の印刷ダイアログへ。画像がないときは使えない）
pub const PRINT: &str = "print";
/// 「ファイル > 共有…」（加工後の画像を macOS の共有の一覧へ。画像がないときは使えない）
pub const SHARE: &str = "share";
/// 「ファイル > 並べて 1 枚に…」（2〜4 枚の写真を並べた画像を作って開く）
pub const COLLAGE: &str = "collage";
/// 「編集 > 元に戻す」（設定の変更。入力欄の文字は画面が入力欄に任せる）
pub const UNDO: &str = "undo";
/// 「編集 > やり直す」
pub const REDO: &str = "redo";
/// 「編集 > ペースト」（クリップボードの画像・ファイルを開く。入力欄では文字を貼り付けることもある）
pub const PASTE: &str = "paste";
/// 「編集 > 加工をコピー」「加工をペースト」（プリセットと同じ加工を、保存せずに次の写真へ使い回す）
pub const COPY_LOOK: &str = "copy-look";
pub const PASTE_LOOK: &str = "paste-look";
pub const KEEP_VERSION: &str = "keep-version";
pub const VERSIONS: &str = "versions";
/// 「編集 > 加工後の画像をコピー」（原寸で加工した画像をクリップボードへ。画像がないときは使えない）
pub const COPY_IMAGE: &str = "copy-image";
/// 「編集 > 文字・透かし…」
pub const TEXT: &str = "text";
/// 「表示 > 100% で表示」（原寸で処理した保存結果を 1px = 1 画素で見る）
pub const ACTUAL_SIZE: &str = "actual_size";
/// 「表示 > 画面に合わせる」（100% 表示から戻る）
pub const FIT: &str = "fit";
/// 「表示 > ヒストグラム」（チェックの付く項目。状態は画面が環境設定に残し、set_menu_checked で合わせる）
pub const HISTOGRAM: &str = "histogram";
/// 「表示 > 左右に分けて比べる」（チェックの付く項目。境目の左に加工前、右に加工後）
pub const SPLIT: &str = "split";
/// 「ヘルプ > ImageEditorRT の使い方」（機能とキーボードショートカットの一覧を画面に出す）
pub const HELP: &str = "help";

/// メニューバーを作る。
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let about = AboutMetadataBuilder::new().name(Some(super::APP_NAME)).build();
    let quit = MenuItemBuilder::with_id(QUIT, format!("{} を終了", super::APP_NAME))
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;
    let app_menu = SubmenuBuilder::new(app, super::APP_NAME)
        .about_with_text(format!("{} について", super::APP_NAME), Some(about))
        .separator()
        .services_with_text("サービス")
        .separator()
        .hide_with_text(format!("{} を隠す", super::APP_NAME))
        .hide_others_with_text("ほかを隠す")
        .show_all_with_text("すべてを表示")
        .separator()
        .item(&quit)
        .build()?;
    let open = MenuItemBuilder::with_id(OPEN, "開く…").accelerator("CmdOrCtrl+O").build(app)?;
    // 最近使った項目（中身は起動後に recent::refresh_menu が作る）
    let recent = SubmenuBuilder::with_id(app, crate::recent::MENU_ID, "最近使った項目").build()?;
    let next_photo = MenuItemBuilder::with_id(NEXT_PHOTO, "次の写真")
        .accelerator("CmdOrCtrl+]")
        .enabled(false)
        .build(app)?;
    let previous_photo = MenuItemBuilder::with_id(PREVIOUS_PHOTO, "前の写真")
        .accelerator("CmdOrCtrl+[")
        .enabled(false)
        .build(app)?;
    let save = MenuItemBuilder::with_id(SAVE, "保存…").accelerator("CmdOrCtrl+S").build(app)?;
    let save_sizes = MenuItemBuilder::with_id(SAVE_SIZES, "複数の大きさで保存…").build(app)?;
    let reveal_saved = MenuItemBuilder::with_id(REVEAL_SAVED, "保存したファイルを Finder で表示")
        .enabled(false)
        .build(app)?;
    let share = MenuItemBuilder::with_id(SHARE, "共有…").enabled(false).build(app)?;
    let print =
        MenuItemBuilder::with_id(PRINT, "プリント…").accelerator("CmdOrCtrl+P").enabled(false).build(app)?;
    let batch = MenuItemBuilder::with_id(BATCH, "まとめて処理…").build(app)?;
    let collage = MenuItemBuilder::with_id(COLLAGE, "並べて 1 枚に…").build(app)?;
    let file = SubmenuBuilder::new(app, "ファイル")
        .item(&open)
        .item(&recent)
        .item(&next_photo)
        .item(&previous_photo)
        .separator()
        .item(&save)
        .item(&save_sizes)
        .item(&reveal_saved)
        .item(&share)
        .item(&print)
        .item(&batch)
        .item(&collage)
        .separator()
        .close_window_with_text("閉じる")
        .build()?;
    // 入力欄で使う標準の編集の項目（ペーストは画像を開くこともあるので独自の項目）
    let text = MenuItemBuilder::with_id(TEXT, "文字・透かし…").accelerator("CmdOrCtrl+T").build(app)?;
    // 設定の変更を戻す（旧版 FR-UI-43）。戻せないときは使えない状態にし、入力欄の ⌘Z は入力欄に届く
    let undo =
        MenuItemBuilder::with_id(UNDO, "元に戻す").accelerator("CmdOrCtrl+Z").enabled(false).build(app)?;
    let redo = MenuItemBuilder::with_id(REDO, "やり直す")
        .accelerator("CmdOrCtrl+Shift+Z")
        .enabled(false)
        .build(app)?;
    let paste = MenuItemBuilder::with_id(PASTE, "ペースト").accelerator("CmdOrCtrl+V").build(app)?;
    let copy_image = MenuItemBuilder::with_id(COPY_IMAGE, "加工後の画像をコピー")
        .accelerator("CmdOrCtrl+Shift+C")
        .enabled(false)
        .build(app)?;
    // 加工の使い回し（画像がない・コピーしていないときは使えない。画面が set_menu_enabled で合わせる）
    let copy_look = MenuItemBuilder::with_id(COPY_LOOK, "加工をコピー")
        .accelerator("Alt+CmdOrCtrl+C")
        .enabled(false)
        .build(app)?;
    let paste_look = MenuItemBuilder::with_id(PASTE_LOOK, "加工をペースト")
        .accelerator("Alt+CmdOrCtrl+V")
        .enabled(false)
        .build(app)?;
    // 加工の途中の版（画像がないときは使えない）
    let keep_version = MenuItemBuilder::with_id(KEEP_VERSION, "今の加工を版として残す")
        .accelerator("Alt+CmdOrCtrl+K")
        .enabled(false)
        .build(app)?;
    let versions = MenuItemBuilder::with_id(VERSIONS, "版の一覧…").enabled(false).build(app)?;
    let edit = SubmenuBuilder::new(app, "編集")
        .item(&undo)
        .item(&redo)
        .separator()
        .cut_with_text("カット")
        .copy_with_text("コピー")
        .item(&paste)
        .select_all_with_text("すべてを選択")
        .separator()
        .item(&copy_image)
        .item(&copy_look)
        .item(&paste_look)
        .separator()
        .item(&keep_version)
        .item(&versions)
        .separator()
        .item(&text)
        .build()?;
    let histogram = CheckMenuItemBuilder::with_id(HISTOGRAM, "ヒストグラム")
        .accelerator("CmdOrCtrl+Shift+H")
        .checked(true)
        .build(app)?;
    // 画像を開くまで・100% 表示の間などは画面が使えない状態にする（set_menu_enabled）
    let actual_size = MenuItemBuilder::with_id(ACTUAL_SIZE, "100% で表示")
        .accelerator("CmdOrCtrl+1")
        .enabled(false)
        .build(app)?;
    let fit = MenuItemBuilder::with_id(FIT, "画面に合わせる")
        .accelerator("CmdOrCtrl+0")
        .enabled(false)
        .build(app)?;
    let split = CheckMenuItemBuilder::with_id(SPLIT, "左右に分けて比べる")
        .accelerator("CmdOrCtrl+B")
        .checked(false)
        .enabled(false)
        .build(app)?;
    let view = SubmenuBuilder::new(app, "表示")
        .item(&actual_size)
        .item(&fit)
        .separator()
        .item(&split)
        .item(&histogram)
        .separator()
        .fullscreen_with_text("フルスクリーンにする")
        .build()?;
    let window = SubmenuBuilder::new(app, "ウインドウ")
        .minimize_with_text("しまう")
        .maximize_with_text("拡大／縮小")
        .build()?;
    // ⌘?（? は Shift と / のキー。Tauri のメニューは物理キーで指定する）
    let usage = MenuItemBuilder::with_id(HELP, format!("{} の使い方", super::APP_NAME))
        .accelerator("CmdOrCtrl+Shift+/")
        .build(app)?;
    let help = SubmenuBuilder::new(app, "ヘルプ").item(&usage).build()?;
    // macOS のヘルプメニューにする（メニューの項目を探す欄が付く）
    #[cfg(target_os = "macos")]
    help.set_as_help_menu_for_nsapp()?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window, &help])
}

/// メニューの項目が選ばれたとき、画面に知らせる（最近使った項目はここで処理する）。
pub fn on_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    if crate::recent::on_menu(app, event.id().as_ref()) {
        return;
    }
    let _ = app.emit(MENU_EVENT, event.id().as_ref());
}

/// メニューバーの中から ID のサブメニューを探す（ほかのサブメニューの中にあるもの）。
pub fn find_submenu<R: Runtime>(app: &AppHandle<R>, id: &str) -> Option<tauri::menu::Submenu<R>> {
    find(app, id).ok().flatten().and_then(|item| item.as_submenu().cloned())
}

/// チェックの付く項目の状態を変える（画面の環境設定に合わせる）。
pub fn set_checked<R: Runtime>(app: &AppHandle<R>, id: &str, checked: bool) -> tauri::Result<()> {
    match find(app, id)? {
        Some(item) => item.as_check_menuitem().map_or(Ok(()), |c| c.set_checked(checked)),
        None => Ok(()),
    }
}

/// 項目を使える・使えないにする。
pub fn set_enabled<R: Runtime>(app: &AppHandle<R>, id: &str, enabled: bool) -> tauri::Result<()> {
    match find(app, id)? {
        Some(item) => match (item.as_menuitem(), item.as_check_menuitem()) {
            (Some(m), _) => m.set_enabled(enabled),
            (_, Some(c)) => c.set_enabled(enabled),
            _ => Ok(()),
        },
        None => Ok(()),
    }
}

/// サブメニューの中から ID の項目を探す。
fn find<R: Runtime>(app: &AppHandle<R>, id: &str) -> tauri::Result<Option<MenuItemKind<R>>> {
    let Some(menu) = app.menu() else { return Ok(None) };
    for item in menu.items()? {
        if let Some(found) = item.as_submenu().and_then(|sub| sub.get(id)) {
            return Ok(Some(found));
        }
    }
    Ok(None)
}
