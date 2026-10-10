//! LUT（カラーグレーディング用の 3D LUT、.cube ファイル）をかける（旧版にはない）。
//!
//! .cube（Adobe / Resolve の形式）の 3D LUT を読み、画素ごとに 3 次元の線形補間（trilinear）で色を引く。
//! 強さ（%）で元の色と混ぜる。ファイルは場所で覚え、一度読んだ LUT はファイルの更新日時が変わるまで覚えておく。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// 強さの既定値（%）。
pub const LUT_STRENGTH_DEFAULT: u32 = 100;
/// 格子の数の上限（これより大きな LUT は扱わない）。
const MAX_SIZE: usize = 256;

/// LUT の設定。path が空なら LUT なし。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LutSettings {
    /// .cube ファイルの場所
    pub path: String,
    /// 強さ 0〜100（%）
    pub strength: u32,
}

impl Default for LutSettings {
    fn default() -> Self {
        Self { path: String::new(), strength: LUT_STRENGTH_DEFAULT }
    }
}

impl LutSettings {
    pub fn is_empty(&self) -> bool {
        self.path.trim().is_empty()
    }
}

/// 読んだ 3D LUT。表は赤がいちばん速く変わる順（.cube と同じ）で、0〜1 の色。
#[derive(Clone, Debug, PartialEq)]
pub struct Lut3d {
    pub title: String,
    pub size: usize,
    domain_min: [f32; 3],
    domain_max: [f32; 3],
    table: Vec<[f32; 3]>,
}

/// .cube を読む。読めなければ理由を返す。
pub fn parse_cube(text: &str) -> Result<Lut3d, String> {
    let mut title = String::new();
    let mut size = 0usize;
    let (mut domain_min, mut domain_max) = ([0.0f32; 3], [1.0f32; 3]);
    let mut table = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let first = words.next().unwrap_or_default();
        let floats = |words: std::str::SplitWhitespace| -> Result<[f32; 3], String> {
            let values: Vec<f32> =
                words.map(str::parse).collect::<Result<_, _>>().map_err(|_| bad(number))?;
            values.try_into().map_err(|_| bad(number))
        };
        match first {
            "TITLE" => title = line["TITLE".len()..].trim().trim_matches('"').to_string(),
            "LUT_3D_SIZE" => {
                size = words.next().and_then(|w| w.parse().ok()).ok_or_else(|| bad(number))?;
                if !(2..=MAX_SIZE).contains(&size) {
                    return Err(format!("LUT の大きさ（{size}）に対応していません（2〜{MAX_SIZE}）"));
                }
            }
            "LUT_1D_SIZE" => {
                return Err("1D の LUT には対応していません（3D の LUT を選んでください）".into())
            }
            "DOMAIN_MIN" => domain_min = floats(words)?,
            "DOMAIN_MAX" => domain_max = floats(words)?,
            _ if first.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '-' || c == '.') => {
                let rest = std::iter::once(first).chain(words).collect::<Vec<_>>().join(" ");
                table.push(floats(rest.split_whitespace())?);
            }
            // ほかのキーワード（LUT_IN_VIDEO_RANGE など）は読み飛ばす
            _ => {}
        }
    }
    if size == 0 {
        return Err("LUT_3D_SIZE がありません（3D の .cube ファイルを選んでください）".into());
    }
    if table.len() != size * size * size {
        return Err(format!(
            "色の数が合いません（{} 個。{size}³ = {} 個のはず）",
            table.len(),
            size * size * size
        ));
    }
    if (0..3).any(|c| domain_max[c] <= domain_min[c]) {
        return Err("DOMAIN_MIN・DOMAIN_MAX が正しくありません".into());
    }
    Ok(Lut3d { title, size, domain_min, domain_max, table })
}

fn bad(number: usize) -> String {
    format!("{} 行目を読めません", number + 1)
}

impl Lut3d {
    /// 0〜1 の色 rgb を LUT で変換する（3 次元の線形補間）。
    pub fn lookup(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        let last = (n - 1) as f32;
        let coord: [f32; 3] = std::array::from_fn(|c| {
            let t = (rgb[c] - self.domain_min[c]) / (self.domain_max[c] - self.domain_min[c]);
            t.clamp(0.0, 1.0) * last
        });
        let base: [usize; 3] = std::array::from_fn(|c| (coord[c].floor() as usize).min(n - 2));
        let frac: [f32; 3] = std::array::from_fn(|c| coord[c] - base[c] as f32);
        let at = |r: usize, g: usize, b: usize| self.table[r + g * n + b * n * n];
        let mut out = [0.0f32; 3];
        for (dr, wr) in [(0, 1.0 - frac[0]), (1, frac[0])] {
            for (dg, wg) in [(0, 1.0 - frac[1]), (1, frac[1])] {
                for (db, wb) in [(0, 1.0 - frac[2]), (1, frac[2])] {
                    let weight = wr * wg * wb;
                    if weight == 0.0 {
                        continue;
                    }
                    let v = at(base[0] + dr, base[1] + dg, base[2] + db);
                    for c in 0..3 {
                        out[c] += v[c] * weight;
                    }
                }
            }
        }
        out
    }
}

/// 画像に LUT をかける（strength は 0〜100%。透過はそのまま）。
pub fn apply_lut(image: &mut RgbaImage, lut: &Lut3d, strength: u32) {
    let amount = strength.min(100) as f32 / 100.0;
    if amount == 0.0 {
        return;
    }
    image.as_mut().par_chunks_exact_mut(4).with_min_len(4096).for_each(|p| {
        let rgb = [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0);
        let mapped = lut.lookup(rgb);
        for c in 0..3 {
            let v = rgb[c] + (mapped[c] - rgb[c]) * amount;
            p[c] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    });
}

/// .cube ファイルを読む（同じファイル（場所と更新日時）は読み直さない）。
pub fn load_lut(path: &str) -> Result<Arc<Lut3d>, String> {
    type Cache = HashMap<(String, Option<SystemTime>), Arc<Lut3d>>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let modified = std::fs::metadata(path).map_err(|e| format!("LUT を開けません（{e}）"))?.modified().ok();
    let key = (path.to_string(), modified);
    let mut cache = CACHE.get_or_init(Default::default).lock().map_err(|e| e.to_string())?;
    if let Some(lut) = cache.get(&key) {
        return Ok(lut.clone());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("LUT を読めません（{e}）"))?;
    let lut = Arc::new(parse_cube(&text)?);
    cache.insert(key, lut.clone());
    Ok(lut)
}

/// 設定の LUT をかける（読めなければかけない）。
pub fn apply_settings(image: &mut RgbaImage, settings: &LutSettings) {
    if settings.is_empty() || settings.strength == 0 {
        return;
    }
    if let Ok(lut) = load_lut(&settings.path) {
        apply_lut(image, &lut, settings.strength);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// 大きさ n の LUT を、色の変換 f で作る（.cube の文字）。
    fn cube(n: usize, f: impl Fn([f32; 3]) -> [f32; 3]) -> String {
        let mut text = format!("# test\nTITLE \"Test\"\nLUT_3D_SIZE {n}\n");
        let step = |i: usize| i as f32 / (n - 1) as f32;
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let [x, y, z] = f([step(r), step(g), step(b)]);
                    text.push_str(&format!("{x:.6} {y:.6} {z:.6}\n"));
                }
            }
        }
        text
    }

    fn photo() -> RgbaImage {
        RgbaImage::from_fn(32, 16, |x, y| Rgba([(x * 8) as u8, (y * 16) as u8, 100, 200]))
    }

    #[test]
    fn identity_lut_changes_nothing() {
        let lut = parse_cube(&cube(17, |c| c)).unwrap();
        assert_eq!((lut.title.as_str(), lut.size), ("Test", 17));
        let mut image = photo();
        apply_lut(&mut image, &lut, 100);
        assert_eq!(image, photo());
    }

    #[test]
    fn colors_are_mapped_and_interpolated() {
        // 色を反転する LUT（大きさ 2 でも線形補間なので、どの色でも正しく反転する）
        let invert = parse_cube(&cube(2, |[r, g, b]| [1.0 - r, 1.0 - g, 1.0 - b])).unwrap();
        let mut image = photo();
        apply_lut(&mut image, &invert, 100);
        for (a, b) in image.pixels().zip(photo().pixels()) {
            assert!((0..3).all(|c| (i32::from(a[c]) - (255 - i32::from(b[c]))).abs() <= 1), "{a:?} / {b:?}");
            assert_eq!(a[3], b[3], "透過はそのまま");
        }
        // 強さ 50% なら元の色との中間（灰色 100 の反転 155 との間）
        let mut half = RgbaImage::from_pixel(1, 1, Rgba([100, 100, 100, 255]));
        apply_lut(&mut half, &invert, 50);
        assert_eq!(half.get_pixel(0, 0)[0], 128);
        assert_eq!(invert.lookup([0.25, 0.5, 0.75]).map(|v| (v * 100.0).round()), [75.0, 50.0, 25.0]);
    }

    #[test]
    fn bad_files_are_reported() {
        assert!(parse_cube("TITLE \"x\"\n0 0 0\n").unwrap_err().contains("LUT_3D_SIZE"));
        assert!(parse_cube("LUT_3D_SIZE 2\n0 0 0\n").unwrap_err().contains("色の数"));
        assert!(parse_cube("LUT_1D_SIZE 4\n").unwrap_err().contains("1D"));
        assert!(parse_cube("LUT_3D_SIZE 2\n0 0 x\n").unwrap_err().contains("2 行目"));
        // DOMAIN があっても読める
        let text =
            cube(2, |c| c).replace("LUT_3D_SIZE 2", "LUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1");
        assert!(parse_cube(&text).is_ok());
    }
}
