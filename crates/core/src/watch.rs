//! フォルダの見張り（旧版にはない）: 見張るフォルダに新しく入った画像を見つける。
//!
//! 一定の間隔でフォルダの中身（直下の対応形式の画像と大きさ）を渡してもらい、見張り始めたときにはなかった画像の
//! うち、大きさが 2 回続けて同じ（書き込みが終わった）ものを、処理する画像として 1 回だけ返す。
//! 処理そのもの（まとめて処理と同じ）と、間隔を空けて繰り返すのはアプリ本体が行う。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// 見張りの状態。
#[derive(Debug, Default)]
pub struct WatchTracker {
    /// 見張り始めたときにあった画像・もう処理した画像（もう返さない）
    done: HashSet<PathBuf>,
    /// 前に見たときの大きさ（書き込み中かを見分ける）
    last_sizes: HashMap<PathBuf, u64>,
}

impl WatchTracker {
    /// 見張り始める。existing（今ある画像）は処理しない。
    pub fn new(existing: impl IntoIterator<Item = PathBuf>) -> Self {
        Self { done: existing.into_iter().collect(), last_sizes: HashMap::new() }
    }

    /// 今のフォルダの中身（画像と大きさ）を渡し、処理してよい新しい画像を返す（名前順）。
    /// 大きさが 0 のもの・前に見たときと大きさが違うもの（書き込み中）は、次に回す。
    pub fn poll(&mut self, files: &[(PathBuf, u64)]) -> Vec<PathBuf> {
        let mut ready = Vec::new();
        let mut sizes = HashMap::new();
        for (path, size) in files {
            if self.done.contains(path) {
                continue;
            }
            if *size > 0 && self.last_sizes.get(path) == Some(size) {
                ready.push(path.clone());
            } else {
                sizes.insert(path.clone(), *size);
            }
        }
        for path in &ready {
            self.done.insert(path.clone());
        }
        self.last_sizes = sizes;
        ready.sort();
        ready
    }

    /// 処理して書き出したファイル（保存先が見張るフォルダの中でも、もう一度は処理しない）。
    pub fn mark_done(&mut self, path: &Path) {
        self.done.insert(path.to_path_buf());
    }
}

/// フォルダの直下の対応形式の画像と、その大きさ（読めないものは除く）。
pub fn folder_images(folder: &Path) -> Vec<(PathBuf, u64)> {
    crate::batch::collect_images(&[folder.to_path_buf()], &[])
        .into_iter()
        .filter_map(|path| std::fs::metadata(&path).ok().map(|m| (path, m.len())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> PathBuf {
        PathBuf::from(format!("/w/{name}"))
    }

    #[test]
    fn new_files_are_returned_once_after_they_settle() {
        let mut tracker = WatchTracker::new([p("old.jpg")]);
        // 初めて見たときは、書き込み中かもしれないので待つ
        assert!(tracker.poll(&[(p("old.jpg"), 10), (p("b.jpg"), 100), (p("a.jpg"), 50)]).is_empty());
        // 大きさが変わった（書き込み中）ものは待ち、同じものは返す（名前順）
        assert_eq!(
            tracker.poll(&[(p("old.jpg"), 10), (p("b.jpg"), 100), (p("a.jpg"), 80)]),
            vec![p("b.jpg")]
        );
        assert_eq!(tracker.poll(&[(p("a.jpg"), 80), (p("b.jpg"), 100)]), vec![p("a.jpg")]);
        // 一度返したものは返さない
        assert!(tracker.poll(&[(p("a.jpg"), 80), (p("b.jpg"), 100)]).is_empty());
    }

    #[test]
    fn empty_files_and_outputs_are_skipped() {
        let mut tracker = WatchTracker::new([]);
        tracker.poll(&[(p("zero.png"), 0)]);
        assert!(tracker.poll(&[(p("zero.png"), 0)]).is_empty(), "大きさ 0 は待つ");
        tracker.mark_done(&p("out.jpg"));
        tracker.poll(&[(p("out.jpg"), 5)]);
        assert!(tracker.poll(&[(p("out.jpg"), 5)]).is_empty(), "書き出したものは処理しない");
    }

    #[test]
    fn folder_listing_has_sizes() {
        let dir = std::env::temp_dir().join(format!("ier-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.jpg"), b"12345").unwrap();
        std::fs::write(dir.join("note.txt"), b"x").unwrap();
        assert_eq!(folder_images(&dir), vec![(dir.join("a.jpg"), 5)]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
