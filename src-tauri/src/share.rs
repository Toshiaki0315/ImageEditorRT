//! 共有（macOS の共有の一覧。AirDrop・メッセージ・メール・写真に追加など。旧版にはない）。
//!
//! 原寸で加工した画像を一時フォルダに書き出し、そのファイルを共有の一覧に渡す。一時フォルダはアプリを終えるとき
//! （と起動したとき）に消す。

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use imageeditorrt_core::pipeline::{self, EditSettings};
use imageeditorrt_core::save::{self, SaveOptions};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_app_kit::{NSSharingServicePicker, NSView};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSRectEdge, NSSize, NSString, NSURL};
use tauri::{State, WebviewWindow};

use crate::saving::save_pool;
use crate::state::AppState;

thread_local! {
    /// 出している共有の一覧（閉じるまで持っておく。メインスレッドだけで触る）
    static PICKER: RefCell<Option<Retained<NSSharingServicePicker>>> = const { RefCell::new(None) };
}

/// 共有するファイルを置く一時フォルダ。
fn share_dir() -> PathBuf {
    std::env::temp_dir().join("ImageEditorRT-share")
}

/// 共有に使った一時ファイルを消す（アプリを終えるとき・起動したとき）。
pub fn clean() {
    let _ = std::fs::remove_dir_all(share_dir());
}

/// 共有するファイルの場所: 一時フォルダの中の、毎回違うフォルダに `<元の名前>_edited.<拡張子>`（貼り付けた画像は PNG）。
fn share_path(source: Option<&Path>, stamp: u128) -> PathBuf {
    let (stem, suffix) = match source {
        Some(path) => (
            path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "画像".into()),
            save::save_suffix(path),
        ),
        None => (save::PASTED_NAME.to_string(), "png".to_string()),
    };
    share_dir().join(stamp.to_string()).join(format!("{stem}_edited.{suffix}"))
}

/// 今の設定を原寸でかけた画像を一時ファイルに書き出し（保存の設定に従う）、共有の一覧を出す。
/// at は一覧を出す位置（ウィンドウの中の、左上からの px）。
#[tauri::command]
pub async fn share_image(
    settings: EditSettings,
    options: SaveOptions,
    at: (f64, f64),
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> Result<(), String> {
    let opened = state.opened()?;
    let settings = opened.shown(settings, false);
    let source = opened.source.clone();
    let stamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let path = share_path(source.path.as_deref(), stamp);
    let written = path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_pool().install(|| -> Result<(), String> {
            std::fs::create_dir_all(written.parent().expect("フォルダの中のファイル"))
                .map_err(|e| e.to_string())?;
            let original = opened.prepared(&opened.original, &settings);
            let edited = pipeline::apply_edits(&original, &settings).map_err(|e| e.to_string())?;
            let is_tiff = source.format == Some(imageeditorrt_core::formats::Format::Tiff);
            save::save_edited(&edited, &written, options, source.exif.as_deref(), is_tiff)
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    let shown = window.clone();
    window.run_on_main_thread(move || show_picker(&shown, &path, at)).map_err(|e| e.to_string())
}

/// 共有の一覧を、ウィンドウの at（左上からの px）に出す（メインスレッドで呼ぶ）。
fn show_picker(window: &WebviewWindow, path: &Path, (x, y): (f64, f64)) {
    let Ok(view) = window.ns_view() else { return };
    // SAFETY: Tauri が返すのは、ウィンドウが開いている間生きている NSView
    let view: &NSView = unsafe { &*view.cast::<NSView>() };
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let item: &AnyObject = &url;
    let items = NSArray::from_slice(&[item]);
    // SAFETY: 共有できるもの（ファイルの URL）だけを並べて渡す
    let picker = unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items) };
    // ビューの座標が下から上なら、上からの位置に直す
    let y = if view.isFlipped() { y } else { view.frame().size.height - y };
    let rect = NSRect::new(NSPoint::new(x, y), NSSize::new(1.0, 1.0));
    picker.showRelativeToRect_ofView_preferredEdge(rect, view, NSRectEdge::MinY);
    PICKER.with(|p| *p.borrow_mut() = Some(picker));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_paths_keep_the_name_and_format() {
        let path = share_path(Some(Path::new("/photos/IMG_0001.HEIC")), 42);
        assert_eq!(path, share_dir().join("42/IMG_0001_edited.HEIC"));
        // RAW は JPEG、貼り付けた画像は PNG
        assert_eq!(share_path(Some(Path::new("/a/b.CR3")), 1).extension().unwrap(), "jpg");
        assert_eq!(share_path(None, 1).extension().unwrap(), "png");
    }
}
