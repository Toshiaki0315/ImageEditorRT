//! アプリ本体の状態（開いている画像）と、読み込んだ画像を状態に置くまでの共通の処理。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use image::{GrayImage, RgbaImage};
use imageeditorrt_core::background::{self, Background};
use imageeditorrt_core::decode;
use imageeditorrt_core::exif_info::ExifInfo;
use imageeditorrt_core::formats::{self, Format};
use imageeditorrt_core::pipeline::{self, PREVIEW_MAX_SIDE};
use serde::Serialize;
use tauri::WebviewWindow;

use crate::APP_NAME;

/// 読み込んだ原本（不変）と、それを縮めたプレビュー用の画像・縮小率、元のファイルの情報。
#[derive(Default)]
pub(crate) struct Loaded {
    /// 保存のときは別のスレッドで使うので、複製せずに共有する
    pub(crate) original: Option<Arc<RgbaImage>>,
    /// プレビューの処理中に鍵を持ち続けないよう、複製せずに共有する
    pub(crate) preview: Option<Arc<RgbaImage>>,
    pub(crate) factor: f64,
    pub(crate) source: Source,
    /// 被写体のマスク（背景を消す。プレビュー用の画像と同じ大きさ・同じ向き）。まだ作っていなければ None、
    /// 作ったが被写体がなければ Some(None)。画像ごとに 1 回だけ作る
    pub(crate) mask: Option<Option<Arc<GrayImage>>>,
}

impl Loaded {
    /// settings の背景の扱いでかけるマスク（そのまま・マスクがなければ None）。
    pub(crate) fn background(&self, settings_background: Background) -> Option<(Arc<GrayImage>, Background)> {
        if settings_background == Background::Keep {
            return None;
        }
        self.mask.clone().flatten().map(|mask| (mask, settings_background))
    }
}

/// image（原本またはプレビュー用の画像）に、背景を消すマスクをかける（なければそのまま）。重いので別のスレッドで呼ぶ。
pub(crate) fn with_background(
    image: Arc<RgbaImage>,
    mask: Option<(Arc<GrayImage>, Background)>,
) -> Arc<RgbaImage> {
    match mask {
        Some((mask, mode)) => Arc::new(background::apply_background(&image, &mask, mode)),
        None => image,
    }
}

/// 元のファイル（保存の名前・元の画像への上書きの防止・EXIF を残すのに使う）。
#[derive(Clone, Default)]
pub(crate) struct Source {
    /// 元のファイルのパス（計測用の画像などファイルがなければ None）
    pub(crate) path: Option<PathBuf>,
    pub(crate) format: Option<Format>,
    /// 元の EXIF（TIFF の部分）
    pub(crate) exif: Option<Vec<u8>>,
    /// クリップボードから貼り付けた画像（保存の初期の名前を「クリップボード_日時.png」にする）
    pub(crate) pasted: bool,
}

/// 開いている画像（画面の操作はどれもこれを見る）。
#[derive(Default)]
pub(crate) struct AppState(pub(crate) Mutex<Loaded>);

/// 読み込みの結果（画面に出す情報）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenInfo {
    /// ファイル名（パスなし）
    name: String,
    format: Option<Format>,
    width: u32,
    height: u32,
    preview_width: u32,
    preview_height: u32,
    /// 透明・半透明の画素があるか（プレビューで市松模様を出す）
    has_alpha: bool,
    /// 1 より大きければ先頭のフレーム（ページ）だけを扱っている
    frame_count: usize,
    /// 読み込み（ファイルの読み込み＋画素にする）・縮小にかかった時間 (ms)
    decode_ms: f64,
    resize_ms: f64,
    exif: ExifInfo,
}

pub(crate) fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// 重い処理（読み込み・縮小など）を、非同期の処理のスレッドを止めないよう別のスレッドで行う。
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| e.to_string())?
}

/// 読み込んだ画像からプレビュー用の縮小版を作り、画面に返す情報と、覚えておく状態をそろえる（別のスレッドで呼ぶ）。
pub(crate) fn prepare(
    name: String,
    decoded: decode::Decoded,
    decode_ms: f64,
    exif: ExifInfo,
    source: Source,
) -> (OpenInfo, Loaded) {
    let start = Instant::now();
    let original = decoded.image;
    let (small, factor) = pipeline::make_preview(&original, PREVIEW_MAX_SIDE);
    let info = OpenInfo {
        format: Some(decoded.format),
        width: original.width(),
        height: original.height(),
        preview_width: small.width(),
        preview_height: small.height(),
        has_alpha: formats::has_transparency(&small),
        frame_count: decoded.frame_count,
        decode_ms,
        resize_ms: elapsed_ms(start),
        exif,
        name,
    };
    let loaded = Loaded {
        original: Some(Arc::new(original)),
        preview: Some(Arc::new(small)),
        factor,
        source,
        mask: None,
    };
    (info, loaded)
}

/// 読み込んだ画像を今の画像にし、ウィンドウのタイトルを「ファイル名 — ImageEditorRT」にする。
pub(crate) fn store(
    state: &AppState,
    window: &WebviewWindow,
    (info, loaded): (OpenInfo, Loaded),
) -> Result<OpenInfo, String> {
    let _ = window.set_title(&format!("{} — {APP_NAME}", info.name));
    *state.0.lock().map_err(|e| e.to_string())? = loaded;
    Ok(info)
}
