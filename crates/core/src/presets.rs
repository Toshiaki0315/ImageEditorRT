//! 加工設定のプリセット（名前付きの加工の組み合わせ）の保存・読み込み（Python 版の core/presets.py と同じ）。
//!
//! ファイルの形式は旧版と同じ JSON（`{"version": 1, "presets": [...]}`、項目名は snake_case）。
//! 一覧の中の壊れた項目（知らないテイストなど）は読み飛ばし、項目がなければ既定値にする。

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::curve::{self, HslAdjust, HSL_BANDS};
use crate::diorama::DioramaDirection;
use crate::filters::FilterType;
use crate::frames::FrameType;
use crate::logo::LogoSettings;
use crate::pipeline::{EditSettings, FILTER_STRENGTH_FULL, FILTER_STRENGTH_MAX};
use crate::shapes::ShapeType;
use crate::text::{TextFont, TextPosition, TextSettings};

/// ファイルの形式の版。
pub const PRESETS_VERSION: u32 = 1;
/// プリセット名の最大の文字数。
pub const PRESET_NAME_MAX: usize = 50;
/// 保存先のフォルダ（~/Library/Application Support の下）。
const APP_FOLDER: &str = "ImageEditorRT";
/// 旧版（Python 版）の保存先のフォルダ。こちらにまだファイルがなければ、旧版のものを読む。
const LEGACY_APP_FOLDER: &str = "ImageEditor";
const FILE_NAME: &str = "presets.json";

/// プリセットのファイルを読み書きできないとき。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetError(pub String);

impl fmt::Display for PresetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PresetError {}

/// 名前付きの加工の組み合わせ。テイスト・色の調整・ディテール・ジオラマ・フレーム・形・文字を持つ。
/// サイズ変更・トリミング・回転は画像ごとの設定なので含めない。JSON の項目の並びは旧版と同じ。
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Preset {
    pub name: String,
    pub filter: FilterType,
    /// テイストの強さ（旧版にはない）。ファイルには既定（100%）以外のときだけ書く（旧版のファイルと同じ形を保つ）
    #[serde(skip_serializing_if = "is_full_strength")]
    pub filter_strength: u32,
    pub exposure: f64,
    pub brightness: i32,
    pub contrast: i32,
    /// ハイライト・シャドウ（旧版にはない）。ファイルには 0 以外のときだけ書く
    #[serde(skip_serializing_if = "is_zero")]
    pub highlights: i32,
    #[serde(skip_serializing_if = "is_zero")]
    pub shadows: i32,
    pub temperature: u32,
    pub saturation: i32,
    pub vignette: u32,
    pub aging: u32,
    pub sharpen: u32,
    pub blur: u32,
    pub denoise: u32,
    pub diorama_blur: u32,
    pub diorama_direction: DioramaDirection,
    pub diorama_position: u32,
    pub diorama_width: u32,
    pub diorama_vivid: u32,
    pub frame: FrameType,
    pub shape: ShapeType,
    pub corner_radius: u32,
    pub text: TextSettings,
    /// トーンカーブ・色ごとの調整（旧版にはない）。ファイルには既定以外のときだけ書く
    #[serde(skip_serializing_if = "is_identity_curve")]
    pub tone_curve: Vec<[u8; 2]>,
    #[serde(skip_serializing_if = "is_neutral_hsl")]
    pub hsl: [HslAdjust; HSL_BANDS],
    /// ロゴの透かし（旧版にはない。ファイルの場所で覚える）。ロゴがなければファイルに書かない
    #[serde(skip_serializing_if = "LogoSettings::is_empty")]
    pub logo: LogoSettings,
    /// 肌をなめらかに（旧版にはない）。0 以外のときだけファイルに書く
    #[serde(skip_serializing_if = "is_zero_u32")]
    pub skin_smooth: u32,
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

fn is_identity_curve(points: &Vec<[u8; 2]>) -> bool {
    *points == curve::identity_curve()
}

fn is_neutral_hsl(bands: &[HslAdjust; HSL_BANDS]) -> bool {
    bands.iter().all(HslAdjust::is_neutral)
}

impl Preset {
    /// 今の設定から、加工の組み合わせだけを取り出したプリセットを作る。
    pub fn from_settings(name: &str, s: &EditSettings) -> Self {
        Self {
            name: name.to_string(),
            filter: s.filter,
            filter_strength: s.filter_strength,
            exposure: s.exposure,
            brightness: s.brightness,
            contrast: s.contrast,
            highlights: s.highlights,
            shadows: s.shadows,
            temperature: s.temperature,
            saturation: s.saturation,
            vignette: s.vignette,
            aging: s.aging,
            sharpen: s.sharpen,
            blur: s.blur,
            denoise: s.denoise,
            diorama_blur: s.diorama_blur,
            diorama_direction: s.diorama_direction,
            diorama_position: s.diorama_position,
            diorama_width: s.diorama_width,
            diorama_vivid: s.diorama_vivid,
            frame: s.frame,
            shape: s.shape,
            corner_radius: s.corner_radius,
            text: s.text.clone(),
            tone_curve: s.tone_curve.clone(),
            hsl: s.hsl,
            logo: s.logo.clone(),
            skin_smooth: s.skin_smooth,
        }
    }

    /// 設定にプリセットの加工を当てはめた新しい設定を返す（サイズ・範囲・向きはそのまま）。
    pub fn apply(&self, settings: &EditSettings) -> EditSettings {
        EditSettings {
            filter: self.filter,
            filter_strength: self.filter_strength,
            exposure: self.exposure,
            brightness: self.brightness,
            contrast: self.contrast,
            highlights: self.highlights,
            shadows: self.shadows,
            temperature: self.temperature,
            saturation: self.saturation,
            vignette: self.vignette,
            aging: self.aging,
            sharpen: self.sharpen,
            blur: self.blur,
            denoise: self.denoise,
            diorama_blur: self.diorama_blur,
            diorama_direction: self.diorama_direction,
            diorama_position: self.diorama_position,
            diorama_width: self.diorama_width,
            diorama_vivid: self.diorama_vivid,
            frame: self.frame,
            shape: self.shape,
            corner_radius: self.corner_radius,
            text: self.text.clone(),
            tone_curve: self.tone_curve.clone(),
            hsl: self.hsl,
            logo: self.logo.clone(),
            skin_smooth: self.skin_smooth,
            ..settings.clone()
        }
    }
}

fn is_full_strength(strength: &u32) -> bool {
    *strength == FILTER_STRENGTH_FULL
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}

/// Python の str.strip() が取り除く空白か（Rust の is_whitespace に加えて、区切りの制御文字 0x1C〜0x1F）。
fn is_python_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// プリセット名の前後の空白を除き、長すぎる名前を切り詰める。
pub fn normalize_name(name: &str) -> String {
    name.trim_matches(is_python_space).chars().take(PRESET_NAME_MAX).collect()
}

/// 読み込んだプリセット（imported）を一覧の末尾に足した新しい一覧と、足した数を返す（プリセットの読み込み。
/// 旧版にはない）。同じ名前があれば「名前 (2)」「名前 (3)」… と空いている名前に変えて足す（今のものは消さない）。
pub fn merge_presets(presets: &[Preset], imported: Vec<Preset>) -> (Vec<Preset>, usize) {
    let mut list = presets.to_vec();
    let count = imported.len();
    for mut preset in imported {
        if list.iter().any(|p| p.name == preset.name) {
            let base = preset.name.clone();
            let free =
                (2..).map(|n| format!("{base} ({n})")).find(|name| list.iter().all(|p| &p.name != name));
            preset.name = free.expect("空いている名前は必ずある");
        }
        list.push(preset);
    }
    (list, count)
}

/// プリセットのファイル（書き出したもの・presets.json）を読む。ファイルがない・形式が正しくなければエラー。
pub fn read_presets_file(path: &Path) -> Result<Vec<Preset>, PresetError> {
    if !path.is_file() {
        return Err(PresetError(format!("プリセットのファイルがありません: {}", path.display())));
    }
    load_presets(path)
}

/// 同じ名前があれば置き換え、なければ末尾に加えた新しい一覧を返す。
pub fn upsert_preset(presets: &[Preset], preset: Preset) -> Vec<Preset> {
    let mut list = presets.to_vec();
    match list.iter_mut().find(|p| p.name == preset.name) {
        Some(slot) => *slot = preset,
        None => list.push(preset),
    }
    list
}

/// name のプリセットを除いた新しい一覧を返す。
pub fn remove_preset(presets: &[Preset], name: &str) -> Vec<Preset> {
    presets.iter().filter(|p| p.name != name).cloned().collect()
}

/// まだ使われていない「プリセット 1」「プリセット 2」… の名前。
pub fn default_preset_name(presets: &[Preset]) -> String {
    (1..)
        .map(|n| format!("プリセット {n}"))
        .find(|name| presets.iter().all(|p| &p.name != name))
        .expect("必ず見つかる")
}

/// 保存先 `~/Library/Application Support/ImageEditorRT/presets.json`。
pub fn presets_path() -> Option<PathBuf> {
    app_data_path(FILE_NAME)
}

/// アプリのデータのファイル `~/Library/Application Support/ImageEditorRT/<name>`（最近使った項目など）。
/// 環境変数 IMAGEEDITORRT_DATA_DIR があればそのフォルダ（画面の通しの確認で、使っている人のデータに触れないため）。
pub fn app_data_path(name: &str) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV).filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir).join(name));
    }
    application_support().map(|d| d.join(APP_FOLDER).join(name))
}

/// アプリのデータの置き場所を変える環境変数。
pub const DATA_DIR_ENV: &str = "IMAGEEDITORRT_DATA_DIR";

/// 旧版の保存先 `~/Library/Application Support/ImageEditor/presets.json`。
pub fn legacy_presets_path() -> Option<PathBuf> {
    application_support().map(|d| d.join(LEGACY_APP_FOLDER).join(FILE_NAME))
}

fn application_support() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library").join("Application Support"))
}

/// path のプリセットを読む。path にまだファイルがなければ、legacy（旧版のファイル）を読む。
/// 旧版のファイルは読むだけで、保存するのはいつも path。
pub fn load_presets_with_legacy(path: &Path, legacy: Option<&Path>) -> Result<Vec<Preset>, PresetError> {
    match legacy {
        Some(legacy) if !path.exists() => load_presets(legacy),
        _ => load_presets(path),
    }
}

/// ファイルからプリセットの一覧を読む。ファイルがなければ空の一覧。
///
/// 読めない・形式が違うファイルは PresetError。一覧の中の壊れた項目は読み飛ばし、
/// 同じ名前が続けば最初のものを使う。
pub fn load_presets(path: &Path) -> Result<Vec<Preset>, PresetError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(PresetError(format!("プリセットを読み込めません: {}\n({e})", path.display()))),
    };
    let format_error =
        || PresetError(format!("プリセットのファイルの形式が正しくありません: {}", path.display()));
    let data: Value = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| serde_json::from_str(text).ok())
        .ok_or_else(format_error)?;
    let items = data.get("presets").and_then(Value::as_array).ok_or_else(format_error)?;
    let mut presets: Vec<Preset> = Vec::new();
    for item in items {
        if let Some(preset) = preset_from_value(item) {
            if presets.iter().all(|p| p.name != preset.name) {
                presets.push(preset);
            }
        }
    }
    Ok(presets)
}

/// プリセットの一覧をファイルに書く（一時ファイルに書いてから置き換えるので、途中で失敗しても元のファイルを壊さない）。
pub fn save_presets(path: &Path, presets: &[Preset]) -> Result<(), PresetError> {
    let text = presets_json(presets);
    let temporary = path.with_file_name(format!(
        "{}.tmp",
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    ));
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&temporary, text.as_bytes())?;
        std::fs::rename(&temporary, path)
    };
    write().map_err(|e| PresetError(format!("プリセットを保存できません: {}\n({e})", path.display())))
}

/// ファイルに書く JSON（旧版の json.dumps(ensure_ascii=False, indent=2) と同じ形と、最後の改行）。
pub fn presets_json(presets: &[Preset]) -> String {
    #[derive(Serialize)]
    struct File<'a> {
        version: u32,
        presets: &'a [Preset],
    }
    let mut text = serde_json::to_string_pretty(&File { version: PRESETS_VERSION, presets })
        .expect("プリセットは JSON にできる");
    text.push('\n');
    text
}

// --- 1 件分の読み取り（旧版の _preset_from_dict と同じく、壊れていれば None） ----------

/// Python の int(x)（bool は不可。小数は 0 の方向に切り捨て）。
fn python_int(raw: &Value) -> Option<i64> {
    match raw {
        Value::Number(n) => {
            n.as_i64().or_else(|| n.as_f64().filter(|f| f.is_finite()).map(|f| f.trunc() as i64))
        }
        _ => None,
    }
}

/// Python の float(x)（bool は 1.0・0.0、数を表す文字列も読む）。
fn python_float(raw: &Value) -> Option<f64> {
    match raw {
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim_matches(is_python_space).parse().ok(),
        _ => None,
    }
}

fn enum_value<T: serde::de::DeserializeOwned>(raw: &Value) -> Option<T> {
    serde_json::from_value(raw.clone()).ok()
}

/// 整数の項目。型に収まらない値（負の周辺減光など）は壊れた項目とみなす。
fn int_value<T: TryFrom<i64>>(raw: &Value) -> Option<T> {
    T::try_from(python_int(raw)?).ok()
}

fn text_from_value(item: &Value) -> Option<TextSettings> {
    let item = item.as_object()?;
    let default = TextSettings::default();
    let text = match item.get("text") {
        Some(v) => v.as_str()?.to_string(),
        None => default.text,
    };
    let color = match item.get("color") {
        Some(v) => {
            let list = v.as_array().filter(|l| l.len() == 3)?;
            let mut color = [0u8; 3];
            for (out, c) in color.iter_mut().zip(list) {
                // 整数だけ（小数・bool は不可）。範囲の外は 0〜255 に収める
                let value = c.as_i64().or_else(|| c.as_u64().map(|_| i64::MAX))?;
                *out = value.clamp(0, 255) as u8;
            }
            color
        }
        None => default.color,
    };
    let opacity = match item.get("opacity") {
        Some(Value::Bool(_)) => return None,
        Some(Value::String(s)) => u32::try_from(s.trim_matches(is_python_space).parse::<i64>().ok()?).ok()?,
        Some(v) => int_value(v)?,
        None => default.opacity,
    };
    let size = match item.get("size") {
        Some(Value::Bool(_)) => return None,
        Some(v) => python_float(v)? as f32,
        None => default.size,
    };
    let font: TextFont = match item.get("font") {
        Some(v) => enum_value(v)?,
        None => default.font,
    };
    let position: TextPosition = match item.get("position") {
        Some(v) => enum_value(v)?,
        None => default.position,
    };
    Some(TextSettings { text, font, size, color, opacity, position })
}

/// トーンカーブの点（[x, y] の並び。正しくなければ壊れた項目）。
fn curve_from_value(raw: &Value) -> Option<Vec<[u8; 2]>> {
    let points: Vec<[u8; 2]> = raw
        .as_array()?
        .iter()
        .map(|p| {
            let pair = p.as_array().filter(|a| a.len() == 2)?;
            Some([u8::try_from(pair[0].as_u64()?).ok()?, u8::try_from(pair[1].as_u64()?).ok()?])
        })
        .collect::<Option<_>>()?;
    curve::is_valid_curve(&points).then_some(points)
}

/// 色ごとの調整（8 色分の {hue, saturation, lightness}。範囲の外は収める）。
fn hsl_from_value(raw: &Value) -> Option<[HslAdjust; HSL_BANDS]> {
    let items = raw.as_array().filter(|a| a.len() == HSL_BANDS)?;
    let mut bands = [HslAdjust::default(); HSL_BANDS];
    for (band, item) in bands.iter_mut().zip(items) {
        let field = |name: &str, max: i64| -> Option<i32> {
            Some(item.get(name).map_or(Some(0), Value::as_i64)?.clamp(-max, max) as i32)
        };
        *band = HslAdjust {
            hue: field("hue", i64::from(curve::HSL_HUE_MAX))?,
            saturation: field("saturation", 100)?,
            lightness: field("lightness", 100)?,
        };
    }
    Some(bands)
}

fn preset_from_value(item: &Value) -> Option<Preset> {
    let item = item.as_object()?;
    let name = normalize_name(item.get("name")?.as_str()?);
    if name.is_empty() {
        return None;
    }
    let mut p = Preset::from_settings(&name, &EditSettings::default());
    for (key, raw) in item {
        // 数の項目は bool を受け付けない（旧版と同じ）
        let number = || if raw.is_boolean() { None } else { Some(raw) };
        match key.as_str() {
            "filter" => p.filter = enum_value(raw)?,
            "filter_strength" => p.filter_strength = int_value::<u32>(number()?)?.min(FILTER_STRENGTH_MAX),
            "frame" => p.frame = enum_value(raw)?,
            "shape" => p.shape = enum_value(raw)?,
            "diorama_direction" => p.diorama_direction = enum_value(raw)?,
            "text" => p.text = text_from_value(raw)?,
            "tone_curve" => p.tone_curve = curve_from_value(raw)?,
            "hsl" => p.hsl = hsl_from_value(raw)?,
            "logo" => p.logo = enum_value(raw)?,
            "skin_smooth" => p.skin_smooth = int_value::<u32>(number()?)?.min(100),
            "exposure" => p.exposure = python_float(raw)?,
            "brightness" => p.brightness = int_value(number()?)?,
            "contrast" => p.contrast = int_value(number()?)?,
            "highlights" => p.highlights = int_value::<i32>(number()?)?.clamp(-100, 100),
            "shadows" => p.shadows = int_value::<i32>(number()?)?.clamp(-100, 100),
            "temperature" => p.temperature = int_value(number()?)?,
            "saturation" => p.saturation = int_value(number()?)?,
            "vignette" => p.vignette = int_value(number()?)?,
            "aging" => p.aging = int_value(number()?)?,
            "sharpen" => p.sharpen = int_value(number()?)?,
            "blur" => p.blur = int_value(number()?)?,
            "denoise" => p.denoise = int_value(number()?)?,
            "diorama_blur" => p.diorama_blur = int_value(number()?)?,
            "diorama_position" => p.diorama_position = int_value(number()?)?,
            "diorama_width" => p.diorama_width = int_value(number()?)?,
            "diorama_vivid" => p.diorama_vivid = int_value(number()?)?,
            "corner_radius" => p.corner_radius = int_value(number()?)?,
            _ => {} // 知らない項目は無視する
        }
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(normalize_name("  夕焼け \n"), "夕焼け");
        assert_eq!(normalize_name(&"あ".repeat(60)).chars().count(), PRESET_NAME_MAX);
        let presets = vec![Preset::from_settings("プリセット 1", &EditSettings::default())];
        assert_eq!(default_preset_name(&presets), "プリセット 2");
        assert_eq!(default_preset_name(&[]), "プリセット 1");
    }

    #[test]
    fn upsert_and_remove_keep_the_order() {
        let a = Preset::from_settings("A", &EditSettings::default());
        let b = Preset::from_settings("B", &EditSettings::default());
        let list = upsert_preset(&upsert_preset(&[], a.clone()), b.clone());
        let changed = Preset { exposure: 1.5, ..a.clone() };
        let list = upsert_preset(&list, changed.clone());
        assert_eq!(list, vec![changed, b.clone()]);
        assert_eq!(remove_preset(&list, "A"), vec![b]);
    }

    #[test]
    fn apply_keeps_size_crop_and_orientation() {
        let settings = EditSettings {
            width: Some(100),
            crop: Some(crate::transform::CropRect::new(1, 2, 30, 40)),
            orientation: crate::transform::Orientation::new(90, true),
            exposure: 2.0,
            ..EditSettings::default()
        };
        let preset =
            Preset { filter: FilterType::Sepia, ..Preset::from_settings("P", &EditSettings::default()) };
        let applied = preset.apply(&settings);
        assert_eq!((applied.filter, applied.exposure), (FilterType::Sepia, 0.0));
        assert_eq!(
            (applied.width, applied.crop, applied.orientation),
            (settings.width, settings.crop, settings.orientation)
        );
        assert_eq!(Preset::from_settings("P", &applied), preset);
    }

    #[test]
    fn filter_strength_is_written_only_when_not_full() {
        let full = Preset::from_settings(
            "A",
            &EditSettings { filter: FilterType::Sepia, ..EditSettings::default() },
        );
        assert!(!presets_json(std::slice::from_ref(&full)).contains("filter_strength"));
        let half = Preset { filter_strength: 40, ..full.clone() };
        let text = presets_json(std::slice::from_ref(&half));
        assert!(text.contains("\"filter_strength\": 40"), "{text}");
        // 読み直すと同じ。書いていなければ 100%、上限（200%）を超える値は 200% に収める
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(preset_from_value(&value["presets"][0]), Some(half.clone()));
        assert_eq!(preset_from_value(&serde_json::json!({"name": "B"})).unwrap().filter_strength, 100);
        assert_eq!(
            preset_from_value(&serde_json::json!({"name": "C", "filter_strength": 250}))
                .unwrap()
                .filter_strength,
            200
        );
        assert_eq!(
            preset_from_value(&serde_json::json!({"name": "E", "filter_strength": 150}))
                .unwrap()
                .filter_strength,
            150
        );
        assert_eq!(preset_from_value(&serde_json::json!({"name": "D", "filter_strength": -1})), None);
        // 当てはめると強さも変わる
        assert_eq!(half.apply(&EditSettings::default()).filter_strength, 40);
    }

    #[test]
    fn missing_file_is_empty_and_legacy_is_read_until_saved() {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-presets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("new/presets.json");
        let legacy = dir.join("old/presets.json");
        assert_eq!(load_presets_with_legacy(&path, Some(&legacy)).unwrap(), vec![]);
        let old = vec![Preset::from_settings("旧版", &EditSettings::default())];
        save_presets(&legacy, &old).unwrap();
        assert_eq!(load_presets_with_legacy(&path, Some(&legacy)).unwrap(), old);
        // 新しい保存先に保存したら、そちらを読む
        save_presets(&path, &[]).unwrap();
        assert_eq!(load_presets_with_legacy(&path, Some(&legacy)).unwrap(), vec![]);
        assert!(!path.with_file_name("presets.json.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn highlights_and_shadows_are_written_only_when_set() {
        let plain = Preset::from_settings("A", &EditSettings::default());
        let text = presets_json(std::slice::from_ref(&plain));
        assert!(!text.contains("highlights") && !text.contains("shadows"), "{text}");
        let set = Preset { highlights: -30, shadows: 45, ..plain.clone() };
        let text = presets_json(std::slice::from_ref(&set));
        assert!(text.contains("\"highlights\": -30") && text.contains("\"shadows\": 45"), "{text}");
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(preset_from_value(&value["presets"][0]), Some(set.clone()));
        // 範囲の外は -100〜100 に収める
        let wide =
            preset_from_value(&serde_json::json!({"name": "B", "highlights": 300, "shadows": -150})).unwrap();
        assert_eq!((wide.highlights, wide.shadows), (100, -100));
        let applied = set.apply(&EditSettings::default());
        assert_eq!((applied.highlights, applied.shadows), (-30, 45));
    }

    #[test]
    fn tone_curve_and_hsl_are_written_only_when_set() {
        let plain = Preset::from_settings("A", &EditSettings::default());
        let text = presets_json(std::slice::from_ref(&plain));
        assert!(!text.contains("tone_curve") && !text.contains("hsl"), "{text}");
        let mut hsl = [HslAdjust::default(); HSL_BANDS];
        hsl[2] = HslAdjust { hue: 10, saturation: -20, lightness: 30 };
        let set = Preset { tone_curve: vec![[0, 10], [128, 150], [255, 240]], hsl, ..plain.clone() };
        let text = presets_json(std::slice::from_ref(&set));
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(preset_from_value(&value["presets"][0]), Some(set.clone()));
        // 正しくない曲線は壊れた項目、範囲の外の色の調整は収める
        assert_eq!(
            preset_from_value(&serde_json::json!({"name": "B", "tone_curve": [[0, 0], [0, 5], [255, 255]]})),
            None
        );
        let wide = serde_json::json!({"name": "C", "hsl": [{"hue": 99}, {}, {}, {}, {}, {}, {}, {"lightness": -500}]});
        let wide = preset_from_value(&wide).unwrap();
        assert_eq!((wide.hsl[0].hue, wide.hsl[7].lightness), (30, -100));
        let applied = set.apply(&EditSettings::default());
        assert_eq!((applied.tone_curve, applied.hsl), (set.tone_curve, set.hsl));
    }

    #[test]
    fn logo_is_written_only_when_set() {
        let plain = Preset::from_settings("A", &EditSettings::default());
        assert!(!presets_json(std::slice::from_ref(&plain)).contains("logo"));
        let logo = LogoSettings { path: "/Users/me/logo.png".into(), size: 12.5, ..LogoSettings::default() };
        let set = Preset { logo, ..plain };
        let text = presets_json(std::slice::from_ref(&set));
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(preset_from_value(&value["presets"][0]), Some(set.clone()));
        assert_eq!(set.apply(&EditSettings::default()).logo, set.logo);
    }

    #[test]
    fn export_then_import_with_renamed_duplicates() {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-preset-io-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sepia = Preset {
            filter: FilterType::Sepia,
            ..Preset::from_settings("セピア", &EditSettings::default())
        };
        let warm =
            Preset { temperature: 5000, ..Preset::from_settings("暖かく", &EditSettings::default()) };
        let file = dir.join("書き出し.json");
        save_presets(&file, &[sepia.clone(), warm.clone()]).unwrap();
        let imported = read_presets_file(&file).unwrap();
        assert_eq!(imported, vec![sepia.clone(), warm.clone()]);
        // 「セピア」はもうあるので「セピア (2)」、それもあれば「セピア (3)」
        let existing = vec![sepia.clone(), Preset { name: "セピア (2)".into(), ..sepia.clone() }];
        let (merged, added) = merge_presets(&existing, imported);
        assert_eq!(added, 2);
        let names: Vec<&str> = merged.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["セピア", "セピア (2)", "セピア (3)", "暖かく"]);
        assert_eq!(merged[2].filter, FilterType::Sepia);
        // ない・壊れたファイル
        assert!(read_presets_file(&dir.join("none.json")).is_err());
        std::fs::write(dir.join("broken.json"), "not json").unwrap();
        assert!(read_presets_file(&dir.join("broken.json")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
