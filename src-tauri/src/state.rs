//! アプリ本体の状態（開いている画像）と、読み込んだ画像を状態に置くまでの共通の処理。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use image::{GrayImage, RgbaImage};
use imageeditorrt_core::decode;
use imageeditorrt_core::exif_info::ExifInfo;
use imageeditorrt_core::formats::{self, Format};
use imageeditorrt_core::pipeline::{self, EditSettings, PREVIEW_MAX_SIDE};
use imageeditorrt_core::transform::CropRect;
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
    /// 顔の枠（肌をなめらかに。プレビュー用の画像の座標、回転・反転する前）。まだ探していなければ None
    pub(crate) faces: Option<Arc<Vec<CropRect>>>,
}

/// 開いている画像の、処理に使うものの写し（共有の参照なので複製は軽い）。鍵を持つのは取り出すあいだだけにし、
/// 処理は写しを持って別のスレッドで行う（その間もほかの問い合わせに答えられる）。
#[derive(Clone)]
pub(crate) struct Opened {
    pub(crate) original: Arc<RgbaImage>,
    pub(crate) preview: Arc<RgbaImage>,
    /// プレビュー用の画像の、原寸に対する縮小率
    pub(crate) factor: f64,
    pub(crate) source: Source,
    mask: Option<Arc<GrayImage>>,
    faces: Option<Arc<Vec<CropRect>>>,
}

impl Opened {
    /// 表示に使う設定。comparing（加工前の表示）なら、向きと切り抜く範囲だけを残す。
    pub(crate) fn shown(&self, settings: EditSettings, comparing: bool) -> EditSettings {
        if comparing {
            pipeline::before_settings(self.original.dimensions(), &settings)
        } else {
            settings
        }
    }

    /// image（原本かプレビュー用の画像）に、settings の前もってかける処理（肌をなめらかに・背景）をかけたもの。
    /// かける処理がなければそのまま。重いので別のスレッドで呼ぶ。
    pub(crate) fn prepared(&self, image: &Arc<RgbaImage>, settings: &EditSettings) -> Arc<RgbaImage> {
        let sources = imageeditorrt_core::prepare::Sources {
            faces: self.faces.as_deref().map(Vec::as_slice),
            mask: self.mask.as_deref(),
            preview_width: self.preview.width(),
        };
        imageeditorrt_core::prepare::prepare(image, settings, &sources)
            .map_or_else(|| image.clone(), Arc::new)
    }
}

impl AppState {
    /// 開いている画像の写し。画像がなければ「画像が読み込まれていません」。
    pub(crate) fn opened(&self) -> Result<Opened, String> {
        let loaded = self.0.lock().map_err(|e| e.to_string())?;
        let missing = || "画像が読み込まれていません".to_string();
        Ok(Opened {
            original: loaded.original.clone().ok_or_else(missing)?,
            preview: loaded.preview.clone().ok_or_else(missing)?,
            factor: loaded.factor,
            source: loaded.source.clone(),
            mask: loaded.mask.clone().flatten(),
            faces: loaded.faces.clone(),
        })
    }

    /// 画像ごとに 1 回だけ作るもの（被写体のマスク・顔の枠）を作って覚える。cached が Some なら作らずにそれを返す。
    /// 作っている間に別の画像を開いていれば覚えない。
    pub(crate) async fn remember<T, O>(
        &self,
        cached: impl FnOnce(&Loaded) -> Option<O>,
        make: fn(&RgbaImage) -> Result<T, String>,
        summary: fn(&T) -> O,
        keep: impl FnOnce(&mut Loaded, T),
    ) -> Result<O, String>
    where
        T: Send + 'static,
    {
        let preview = {
            let loaded = self.0.lock().map_err(|e| e.to_string())?;
            if let Some(value) = cached(&loaded) {
                return Ok(value);
            }
            loaded.preview.clone().ok_or("画像が読み込まれていません")?
        };
        let source = preview.clone();
        let made = blocking(move || make(&source)).await?;
        let result = summary(&made);
        let mut loaded = self.0.lock().map_err(|e| e.to_string())?;
        if loaded.preview.as_ref().is_some_and(|p| Arc::ptr_eq(p, &preview)) {
            keep(&mut loaded, made);
        }
        Ok(result)
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
        faces: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use imageeditorrt_core::background::Background;
    use imageeditorrt_core::filters::FilterType;
    use imageeditorrt_core::frames::FrameType;
    use imageeditorrt_core::transform::Orientation;

    fn opened(mask: Option<GrayImage>) -> Opened {
        Opened {
            original: Arc::new(RgbaImage::new(400, 300)),
            preview: Arc::new(RgbaImage::new(200, 150)),
            factor: 0.5,
            source: Source::default(),
            mask: mask.map(Arc::new),
            faces: None,
        }
    }

    #[test]
    fn before_settings_only_while_comparing() {
        let settings = EditSettings {
            filter: FilterType::Sepia,
            frame: FrameType::Polaroid,
            orientation: Orientation::new(90, false),
            crop: Some(CropRect::new(10, 10, 200, 100)),
            ..EditSettings::default()
        };
        let opened = opened(None);
        // 比べていなければそのまま
        assert_eq!(opened.shown(settings.clone(), false), settings);
        // 比べている間は加工前（向きと、フレームの比に合わせた範囲だけ）
        let before = opened.shown(settings.clone(), true);
        assert_eq!(before, pipeline::before_settings((400, 300), &settings));
        assert_eq!(
            (before.filter, before.frame, before.orientation),
            (FilterType::None, FrameType::None, settings.orientation)
        );
    }

    #[test]
    fn prepared_keeps_the_image_when_nothing_to_do() {
        let opened = opened(Some(GrayImage::from_pixel(200, 150, image::Luma([0]))));
        // 背景がそのままなら同じ画像（複製しない）
        let same = opened.prepared(&opened.original, &EditSettings::default());
        assert!(Arc::ptr_eq(&same, &opened.original));
        // 背景を白に: プレビューの大きさのマスクを原寸に合わせてかける
        let white = EditSettings { background: Background::White, ..EditSettings::default() };
        let out = opened.prepared(&opened.original, &white);
        assert_eq!(out.dimensions(), (400, 300));
        assert!(out.pixels().all(|p| p.0 == [255, 255, 255, 255]));
    }
}
