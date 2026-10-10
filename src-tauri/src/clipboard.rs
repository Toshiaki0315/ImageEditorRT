//! クリップボード（NSPasteboard）を読む（旧版 FR-UI-64 の貼り付け）。書き換えるのは「加工後の画像をコピー」のときだけ。

use std::path::PathBuf;

use objc2_app_kit::{
    NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardTypeTIFF,
};
use objc2_foundation::{NSData, NSURL};
use serde::Serialize;

/// クリップボードにあるもの。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contents {
    /// Finder でコピーしたファイル（ファイルの URL だけ）
    pub files: Vec<String>,
    /// 画像のデータがある（スクリーンショット・ほかのアプリでコピーした画像）
    pub has_image: bool,
    /// 文字がある
    pub has_text: bool,
}

/// クリップボードにあるものを調べる。
pub fn contents() -> Contents {
    let board = NSPasteboard::generalPasteboard();
    let mut files = Vec::new();
    if let Some(items) = board.pasteboardItems() {
        for item in items.iter() {
            // SAFETY: 定数の型名を渡して文字列を読むだけ
            let url = unsafe { item.stringForType(NSPasteboardTypeFileURL) };
            let path = url.and_then(|u| NSURL::URLWithString(&u)).and_then(|u| u.path());
            if let Some(path) = path {
                files.push(path.to_string());
            }
        }
    }
    // SAFETY: 定数の型名を渡して、データがあるかを確かめるだけ
    let (has_image, has_text) = unsafe {
        (
            board.dataForType(NSPasteboardTypePNG).is_some()
                || board.dataForType(NSPasteboardTypeTIFF).is_some(),
            board.stringForType(NSPasteboardTypeString).is_some(),
        )
    };
    Contents { files, has_image, has_text }
}

/// クリップボードの画像のデータ（PNG があれば PNG、なければ TIFF）。
pub fn image_data() -> Option<Vec<u8>> {
    let board = NSPasteboard::generalPasteboard();
    // SAFETY: 定数の型名を渡してデータを読むだけ
    let data = unsafe {
        board.dataForType(NSPasteboardTypePNG).or_else(|| board.dataForType(NSPasteboardTypeTIFF))
    }?;
    Some(data.to_vec())
}

/// クリップボードの文字（入力欄に貼り付ける）。
pub fn text() -> Option<String> {
    // SAFETY: 定数の型名を渡して文字列を読むだけ
    unsafe { NSPasteboard::generalPasteboard().stringForType(NSPasteboardTypeString) }.map(|s| s.to_string())
}

/// クリップボードを、画像（PNG と TIFF。どちらも同じ画像）に置き換える（「加工後の画像をコピー」）。
pub fn write_image(png: &[u8], tiff: &[u8]) -> Result<(), String> {
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    // SAFETY: 定数の型名と、作ったデータを渡すだけ（クリップボードが複製を持つ）
    let written = unsafe {
        board.setData_forType(Some(&NSData::with_bytes(png)), NSPasteboardTypePNG)
            && board.setData_forType(Some(&NSData::with_bytes(tiff)), NSPasteboardTypeTIFF)
    };
    if written {
        Ok(())
    } else {
        Err("クリップボードに書き込めません".into())
    }
}

/// ピクチャフォルダ（なければホーム）。貼り付けた画像の保存ダイアログの初期の場所。
pub fn pictures_or_home() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let pictures = home.join("Pictures");
    Some(if pictures.is_dir() { pictures } else { home })
}
