//! 複数の画像に同じ加工をまとめてかけて保存する（一括処理。Python 版の core/batch.py と同じ）。
//!
//! かける加工はプリセットと同じ組み合わせ（テイスト・色の調整・ディテール・ジオラマ・フレーム・形・
//! 文字）と、選べば投稿加工（位置情報を消す・顔や文字を見つけて隠す）。トリミング範囲と回転・反転は画像ごとに違うのでかけない（フレーム・円のときは各画像の
//! 中央をその比で切り抜く）。元の画像は変えない。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::formats;
use crate::pipeline::{effective_crop, EditSettings};
use crate::presets::Preset;
use crate::privacy::{Region, RegionKind};
use crate::save::{self, SaveOptions};
use crate::transform::{CropRect, SizeError, MAX_SIZE, MIN_SIZE};

/// 一括処理の設定。long_side を指定すると、写真の長辺をその px にリサイズする（フレームはその外側に付く）。
#[derive(Clone, Debug, PartialEq)]
pub struct BatchOptions {
    pub look: Preset,
    pub long_side: Option<u32>,
    pub save: SaveOptions,
    /// 投稿加工（位置情報を消す・顔や文字を見つけて隠す。旧版にはない）
    pub privacy: BatchPrivacy,
}

/// まとめて処理の投稿加工。顔・文字は 1 枚ごとに見つけ、kind・strength・stamp で隠す。
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BatchPrivacy {
    /// 保存の設定にかかわらず、位置情報を残さない
    pub remove_gps: bool,
    pub faces: bool,
    pub text: bool,
    pub kind: RegionKind,
    pub strength: u32,
    pub stamp: String,
}

impl Default for BatchPrivacy {
    fn default() -> Self {
        Self {
            remove_gps: false,
            faces: false,
            text: false,
            kind: RegionKind::Blur,
            strength: crate::privacy::STRENGTH_DEFAULT,
            stamp: crate::privacy::STAMP_DEFAULT.into(),
        }
    }
}

impl BatchOptions {
    /// 保存の設定（位置情報を消すなら、位置情報を残さない）。
    pub fn save_options(&self) -> SaveOptions {
        SaveOptions { keep_gps: self.save.keep_gps && !self.privacy.remove_gps, ..self.save }
    }
}

/// 画像（回転・反転しない、向きは読み込み時に直してある）の顔・文字を見つけ、隠す範囲を返す。
/// 速さのため、プレビューと同じ長辺 1600px に縮めた画像で探し、原寸の座標に直す。
#[cfg(target_os = "macos")]
pub fn privacy_regions(image: &image::RgbaImage, privacy: &BatchPrivacy) -> Result<Vec<Region>, String> {
    if !privacy.faces && !privacy.text {
        return Ok(Vec::new());
    }
    let size = image.dimensions();
    let (small, factor) = crate::pipeline::make_preview(image, crate::pipeline::PREVIEW_MAX_SIDE);
    let to_original = |r: CropRect| {
        let scale = |v: i64| (v as f64 / factor).round() as i64;
        CropRect::new(scale(r.x), scale(r.y), scale(r.width), scale(r.height))
    };
    let mut rects = Vec::new();
    if privacy.faces {
        let faces = crate::faces::detect_faces(&small)?;
        rects.extend(faces.into_iter().filter_map(|r| crate::faces::cover_rect(to_original(r), size)));
    }
    if privacy.text {
        let texts = crate::text_regions::detect_text(&small)?;
        rects.extend(texts.into_iter().filter_map(|r| crate::transform::clamp_crop(to_original(r), size)));
    }
    Ok(rects
        .into_iter()
        .map(|rect| Region {
            kind: privacy.kind,
            rect,
            strength: privacy.strength,
            stamp: privacy.stamp.clone(),
        })
        .collect())
}

/// 1 枚分の結果。成功なら保存先、失敗なら理由。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub source: PathBuf,
    pub output: Option<PathBuf>,
    pub error: Option<String>,
}

/// image_size（回転・反転する前、向きは読み込み時に直してある）の画像にかける設定（加工と、長辺の指定からリサイズ）。
pub fn batch_settings(options: &BatchOptions, image_size: (u32, u32)) -> Result<EditSettings, SizeError> {
    let settings = options.look.apply(&EditSettings::default());
    let Some(long_side) = options.long_side else { return Ok(settings) };
    if !(MIN_SIZE..=MAX_SIZE).contains(&long_side) {
        return Err(SizeError { name: "長辺", value: long_side });
    }
    // フレーム・円の比に合わせて切り抜いた後の写真の向きで、長辺を決める
    let (width, height) = effective_crop(image_size, None, settings.frame, settings.shape)
        .map_or(image_size, |r| (r.width as u32, r.height as u32));
    Ok(if width >= height {
        EditSettings { width: Some(long_side), ..settings }
    } else {
        EditSettings { height: Some(long_side), ..settings }
    })
}

/// 保存先のパス。out_dir に元と同じ名前・拡張子で保存する（保存できない形式なら .jpg）。
///
/// 同じ名前のファイルがすでにある、または元のファイルそのものになる場合は `<名前>_edited`、
/// `_edited_2` … と、既存のファイルと重ならない名前にする。
pub fn output_path(source: &Path, out_dir: &Path) -> PathBuf {
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let same_name = out_dir.join(format!("{stem}.{}", save::save_suffix(source)));
    std::iter::once(same_name)
        .chain(save::edited_names(source, out_dir))
        .find(|candidate| !candidate.exists() && !save::is_same_file(candidate, source))
        .expect("候補は終わりなく続く")
}

/// 同じファイルかを比べるためのキー（macOS のファイルシステムは大文字・小文字を区別しない）。
fn path_key(path: &Path) -> String {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().to_lowercase()
}

/// ファイルとフォルダの一覧から、対応形式の画像を重複なく集める。
///
/// フォルダは直下の画像だけ（サブフォルダは見ない）を名前順に加える。隠しファイルは除く。
/// existing はすでに一覧にある画像（加えない）。
pub fn collect_images(paths: &[PathBuf], existing: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen: HashSet<String> = existing.iter().map(|p| path_key(p)).collect();
    let mut images = Vec::new();
    let mut add = |path: PathBuf| {
        if seen.insert(path_key(&path)) {
            images.push(path);
        }
    };
    for path in paths {
        if path.is_dir() {
            let Ok(entries) = std::fs::read_dir(path) else { continue };
            let mut children: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
            children.sort_by_key(|p| {
                p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
            });
            for child in children {
                let hidden = child.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
                if child.is_file() && formats::is_supported(&child) && !hidden {
                    add(child);
                }
            }
        } else if path.is_file() && formats::is_supported(path) {
            add(path.clone());
        }
    }
    images
}

/// 1 枚を読み込み、加工して out_dir に保存し、保存先を返す（元の画像は変えない）。
#[cfg(target_os = "macos")]
pub fn process_image(source: &Path, out_dir: &Path, options: &BatchOptions) -> Result<PathBuf, String> {
    use crate::formats::Format;

    let loaded = crate::load::load_file(source).map_err(|e| e.to_string())?;
    let decoded = loaded.decoded;
    let mut settings = batch_settings(options, decoded.image.dimensions()).map_err(|e| e.to_string())?;
    settings.regions = privacy_regions(&decoded.image, &options.privacy)?;
    let mut image = decoded.image;
    // 肌をなめらかに: 縮めた画像で顔を見つけ、ほかの加工より前にかける（画面と同じ前処理）
    if settings.skin_smooth > 0 {
        let (small, _) = crate::pipeline::make_preview(&image, crate::pipeline::PREVIEW_MAX_SIDE);
        let faces = crate::faces::detect_faces(&small)?;
        let sources =
            crate::prepare::Sources { faces: Some(&faces), mask: None, preview_width: small.width() };
        if let Some(prepared) = crate::prepare::prepare(&image, &settings, &sources) {
            image = prepared;
        }
    }
    let edited = crate::pipeline::apply_edits(&image, &settings).map_err(|e| e.to_string())?;
    let path = output_path(source, out_dir);
    std::fs::create_dir_all(out_dir).map_err(|e| format!("保存先のフォルダを作れません（{e}）"))?;
    let is_tiff = decoded.format == Format::Tiff;
    save::save_edited(&edited, &path, options.save_options(), loaded.raw_exif.as_deref(), is_tiff)
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// 画像を順に処理して結果の一覧を返す。
///
/// 1 枚ごとに、処理する前に cancelled() を確かめ、true なら残りを処理せずに終える（処理中の 1 枚は
/// 最後まで処理する）。progress(処理済みの枚数, 次の画像) で進み具合を知らせる。読み込めない・
/// 保存できない画像は結果に理由を入れて次に進む。
pub fn run_batch(
    sources: &[PathBuf],
    mut process: impl FnMut(&Path) -> Result<PathBuf, String>,
    mut progress: impl FnMut(usize, &Path),
    cancelled: impl Fn() -> bool,
) -> Vec<BatchResult> {
    let mut results = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        if cancelled() {
            break;
        }
        progress(index, source);
        let result = match process(source) {
            Ok(output) => BatchResult { source: source.clone(), output: Some(output), error: None },
            Err(error) => BatchResult { source: source.clone(), output: None, error: Some(error) },
        };
        results.push(result);
    }
    results
}

/// 終わったときに知らせる文（旧版と同じ。処理できなかった画像は最大 10 件と理由）。
pub fn summary(results: &[BatchResult], cancelled: bool, out_dir: &Path) -> String {
    let saved = results.iter().filter(|r| r.output.is_some()).count();
    let failed: Vec<&BatchResult> = results.iter().filter(|r| r.error.is_some()).collect();
    let head = if cancelled { "中止しました。" } else { "" };
    let mut message = format!("{head}{saved} 枚を保存しました。\n保存先: {}", out_dir.display());
    if !failed.is_empty() {
        let lines: Vec<String> = failed
            .iter()
            .take(10)
            .map(|r| {
                let name = r.source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                format!("・{name}（{}）", r.error.as_deref().unwrap_or_default())
            })
            .collect();
        let more =
            if failed.len() > 10 { format!("\n…ほか {} 枚", failed.len() - 10) } else { String::new() };
        message += &format!("\n\n{} 枚は処理できませんでした:\n{}{more}", failed.len(), lines.join("\n"));
    }
    message
}
