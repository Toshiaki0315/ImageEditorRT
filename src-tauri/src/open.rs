//! 画面の外から画像を開く経路（コマンドライン引数・Finder の「このアプリケーションで開く」・
//! Dock のアイコンへのドロップ）。
//!
//! 画面の準備ができる前に届いたファイルは取っておき、画面が take_pending_paths で受け取る。
//! 準備ができた後に届いたものは "open-paths" のイベントで画面に送る。

use std::ffi::OsString;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State};

/// 画面に送るイベントの名前（中身はファイルのパスの一覧）。
pub const OPEN_PATHS_EVENT: &str = "open-paths";

#[derive(Default)]
struct Inner {
    ready: bool,
    paths: Vec<String>,
}

/// 画面に渡す前のファイル。
#[derive(Default)]
pub struct Pending(Mutex<Inner>);

impl Pending {
    /// コマンドライン引数のファイルを取っておく（"-" で始まる引数は macOS などが付けるものなので除く）。
    pub fn from_args(args: impl Iterator<Item = OsString>) -> Self {
        let paths = args.map(|a| a.to_string_lossy().into_owned()).filter(|a| !a.starts_with('-')).collect();
        Self(Mutex::new(Inner { ready: false, paths }))
    }

    /// 届いたファイル。画面の準備ができていれば、取っておかずにそのまま返す。
    fn push_or_take(&self, paths: Vec<String>) -> Option<Vec<String>> {
        let mut inner = self.0.lock().ok()?;
        if inner.ready {
            Some(paths)
        } else {
            inner.paths.extend(paths);
            None
        }
    }

    fn take(&self) -> Vec<String> {
        match self.0.lock() {
            Ok(mut inner) => {
                inner.ready = true;
                std::mem::take(&mut inner.paths)
            }
            Err(_) => Vec::new(),
        }
    }
}

/// 画面の準備ができたときに呼ぶ。それまでに届いたファイルを返す（複数あれば画面が先頭の 1 枚を開く）。
#[tauri::command]
pub fn take_pending_paths(pending: State<'_, Pending>) -> Vec<String> {
    pending.take()
}

/// macOS からファイルを開くよう頼まれたとき。
#[cfg(target_os = "macos")]
pub fn opened(app: &AppHandle, urls: Vec<tauri::Url>) {
    let paths: Vec<String> =
        urls.iter().filter_map(|u| u.to_file_path().ok()).map(|p| p.to_string_lossy().into_owned()).collect();
    if paths.is_empty() {
        return;
    }
    if let Some(paths) = app.state::<Pending>().push_or_take(paths) {
        let _ = app.emit(OPEN_PATHS_EVENT, paths);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_skip_flags() {
        let pending = Pending::from_args(["-psn_0_12345", "a.jpg", "b.png"].into_iter().map(OsString::from));
        assert_eq!(pending.take(), ["a.jpg", "b.png"]);
    }

    #[test]
    fn keeps_paths_until_ready() {
        let pending = Pending::default();
        assert_eq!(pending.push_or_take(vec!["a.jpg".into()]), None);
        assert_eq!(pending.take(), ["a.jpg"]);
        // 準備ができた後はそのまま返す
        assert_eq!(pending.push_or_take(vec!["b.jpg".into()]), Some(vec!["b.jpg".to_string()]));
        assert!(pending.take().is_empty());
    }
}
