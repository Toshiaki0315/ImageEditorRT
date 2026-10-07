//! 傾きの自動補正（水平の補正の「自動」。旧版にはない）。処理は端末の中だけ。
//!
//! まず macOS の Vision（VNDetectHorizonRequest）で水平線を探す。Vision ははっきりした水平線のある写真でも
//! 見つけないことが多いので、見つからなければ写真の中の長い直線から傾きを求める（水平・垂直の線は、
//! 傾いていなければ 0° か 90° を向くはず、という考え方。建物・階段・水平線などに効く）。

use image::RgbaImage;
use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_foundation::{NSArray, NSDictionary};
use objc2_vision::{VNDetectHorizonRequest, VNImageRequestHandler, VNRequest};

use crate::faces::cg_image;
use crate::transform::STRAIGHTEN_MAX;

/// 画像の水平線を見つけ、その傾きを打ち消す水平の補正の角度（度、正は時計回り、-45〜45° に収め 0.1° 刻み）を返す。
/// 水平線が見つからなければ None。
pub fn straighten_angle(image: &RgbaImage) -> Result<Option<f64>, String> {
    let cg = cg_image(image).ok_or("画像を水平線の認識に渡せません")?;
    // GPU を使えない環境では CPU だけでやり直す（顔の認識と同じ）
    let horizon = detect(&cg, false).or_else(|_| detect(&cg, true))?.map(f64::to_degrees);
    // 写真の傾きは、反時計回り（右が上がっている）を正とする。打ち消すには同じだけ時計回りに回す
    Ok(horizon.or_else(|| line_tilt(image)).map(|degrees| {
        let degrees = degrees.clamp(-STRAIGHTEN_MAX, STRAIGHTEN_MAX);
        (degrees * 10.0).round() / 10.0
    }))
}

/// 線を探すときの画像の長辺（px。縮めて速くする）。
const LINE_SIDE: u32 = 512;
/// 輪郭の向きを測る前のぼかしの半径（px。鋭い輪郭では Sobel の勾配の向きが 0°・90° 寄りにずれる）。
const LINE_BLUR: f32 = 1.5;
/// 傾きとして探す範囲（度）。これより大きく傾いた線は、斜めのものとみなして使わない。
const LINE_RANGE: f64 = 20.0;
/// 角度の刻み（度）と、輪郭の向きのまわりで票を入れる幅（度）。
const LINE_STEP: f64 = 0.25;
const LINE_SPREAD: f64 = 1.5;
/// 直線の位置の刻み（px）。
const RHO_STEP: f64 = 2.0;
/// 輪郭として使う画素（輪郭の強い順に、全体のこの割合）。
const EDGE_FRACTION: f64 = 0.12;
/// 一番長い直線が、短辺のこの割合より短ければ、傾きは分からないとする。
const MIN_LINE: f64 = 0.15;
/// 傾きを決めるのに使う直線の数（長い順）。
const TOP_LINES: usize = 8;

/// 写真の中の長い直線から、写真の傾き（度、反時計回りが正）を求める。分からなければ None。
///
/// 輪郭の画素ごとに、その向きのまわり（±1.5°）の直線に票を入れ（ハフ変換）、票の多い（長い）直線の傾きを
/// 票で重み付けした中央値にする。水平に近い線（水平線・段など）と垂直に近い線（建物の柱など）の両方を使い、
/// 傾きが ±20° を超える線は使わない。細かい模様は長い直線にならないので、票が集まらない。
pub fn line_tilt(image: &RgbaImage) -> Option<f64> {
    let small = crate::blur::gaussian_blur(&crate::resize::fit_long_side(image, LINE_SIDE), LINE_BLUR);
    let (width, height) = small.dimensions();
    if width < 3 || height < 3 {
        return None;
    }
    let gray: Vec<f64> = small
        .pixels()
        .map(|p| 0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2]))
        .collect();
    let at = |x: u32, y: u32| gray[(y * width + x) as usize];
    // 輪郭の画素: (x, y, 線の傾き, 水平に近い線か, 強さ)
    let mut edges: Vec<(f64, f64, f64, bool, f64)> = Vec::new();
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            // Sobel の勾配（y は上向きを正にする）
            let gx = at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2.0 * at(x - 1, y)
                - at(x - 1, y + 1);
            let gy = at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1)
                - at(x - 1, y + 1)
                - 2.0 * at(x, y + 1)
                - at(x + 1, y + 1);
            let strength = gx * gx + gy * gy;
            if strength < 100.0 {
                continue;
            }
            // 線の向きは勾配と直角（-90〜90°）。水平に近ければその角度、垂直に近ければ 90° からのずれが傾き
            let line = (gy.atan2(gx).to_degrees() - 90.0 + 90.0).rem_euclid(180.0) - 90.0;
            let (tilt, horizontal) =
                if line.abs() <= 45.0 { (line, true) } else { (line - 90.0 * line.signum(), false) };
            if tilt.abs() <= LINE_RANGE + LINE_SPREAD {
                edges.push((f64::from(x), f64::from(height - 1 - y), tilt, horizontal, strength));
            }
        }
    }
    if edges.is_empty() {
        return None;
    }
    // 強い輪郭だけを使う
    let keep = ((width * height) as f64 * EDGE_FRACTION) as usize;
    if edges.len() > keep {
        edges.select_nth_unstable_by(keep, |a, b| b.4.total_cmp(&a.4));
        edges.truncate(keep);
    }
    let angles = (2.0 * LINE_RANGE / LINE_STEP) as usize + 1;
    let diagonal = f64::from(width).hypot(f64::from(height));
    let rhos = (2.0 * diagonal / RHO_STEP) as usize + 1;
    // [水平に近い線, 垂直に近い線] の票
    let mut votes = [vec![0u32; angles * rhos], vec![0u32; angles * rhos]];
    let spread = (LINE_SPREAD / LINE_STEP) as i64;
    for &(x, y, tilt, horizontal, _) in &edges {
        let center = ((tilt + LINE_RANGE) / LINE_STEP).round() as i64;
        for a in (center - spread).max(0)..=(center + spread).min(angles as i64 - 1) {
            let theta = (a as f64 * LINE_STEP - LINE_RANGE).to_radians();
            let (sin, cos) = theta.sin_cos();
            // 水平に近い線: 傾き θ の直線までの符号付きの距離。垂直に近い線: 90° + θ の直線
            let rho = if horizontal { -x * sin + y * cos } else { x * cos + y * sin };
            let r = ((rho + diagonal) / RHO_STEP) as usize;
            votes[usize::from(!horizontal)][a as usize * rhos + r.min(rhos - 1)] += 1;
        }
    }
    // 票の多い直線（同じ直線の近くの重なりは 1 本に数える）
    let mut lines: Vec<(u32, f64)> = Vec::new();
    for family in &votes {
        let mut cells: Vec<(u32, usize)> =
            family.iter().enumerate().filter(|(_, &v)| v > 0).map(|(i, &v)| (v, i)).collect();
        cells.sort_unstable_by_key(|c| std::cmp::Reverse(c.0));
        let mut taken: Vec<(usize, usize)> = Vec::new();
        for (count, index) in cells {
            let (a, r) = (index / rhos, index % rhos);
            if taken.iter().any(|&(ta, tr)| ta.abs_diff(a) as i64 <= 2 * spread && tr.abs_diff(r) <= 3) {
                continue;
            }
            taken.push((a, r));
            lines.push((count, a as f64 * LINE_STEP - LINE_RANGE));
            if taken.len() >= TOP_LINES {
                break;
            }
        }
    }
    lines.sort_unstable_by_key(|l| std::cmp::Reverse(l.0));
    lines.truncate(TOP_LINES);
    let longest = lines.first()?.0;
    if f64::from(longest) < MIN_LINE * f64::from(width.min(height)) {
        return None;
    }
    // 票で重み付けした中央値
    lines.sort_by(|a, b| a.1.total_cmp(&b.1));
    let half = lines.iter().map(|l| l.0).sum::<u32>() / 2;
    let mut sum = 0;
    lines
        .iter()
        .find(|l| {
            sum += l.0;
            sum > half
        })
        .map(|l| l.1)
}

/// 水平線の傾き（ラジアン）。cpu_only なら CPU だけで認識する。
fn detect(cg: &objc2_core_graphics::CGImage, cpu_only: bool) -> Result<Option<f64>, String> {
    // SAFETY: CGImage は認識が終わるまで生きている。オプションは空
    let handler = unsafe {
        VNImageRequestHandler::initWithCGImage_options(
            VNImageRequestHandler::alloc(),
            cg,
            &NSDictionary::new(),
        )
    };
    // SAFETY: 引数のない初期化
    let request = unsafe { VNDetectHorizonRequest::new() };
    if cpu_only {
        // SAFETY: 認識の前に設定を変えるだけ
        #[allow(deprecated)]
        unsafe {
            request.setUsesCPUOnly(true)
        };
    }
    let requests: Retained<NSArray<VNRequest>> =
        NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(request.clone()))]);
    handler.performRequests_error(&requests).map_err(|e| format!("水平線を認識できません（{e}）"))?;
    // SAFETY: 認識が終わった後に結果を読む
    let results = unsafe { request.results() };
    // SAFETY: 結果の角度を読むだけ
    Ok(results.filter(|r| r.count() > 0).map(|r| unsafe { r.objectAtIndex(0).angle() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// 空（上）と海（下）の境目が、中心を通って degrees 度（反時計回りが正）傾いた画像。
    /// 写真と同じく、境目は画素の中で混ざる（ギザギザの段にしない）。
    fn seascape(degrees: f64) -> RgbaImage {
        let (sin, cos) = degrees.to_radians().sin_cos();
        RgbaImage::from_fn(640, 480, |x, y| {
            let (px, py) = (f64::from(x) + 0.5 - 320.0, f64::from(y) + 0.5 - 240.0);
            // 境目からの距離（下が正。右に行くほど境目が上がる = 反時計回り）
            let distance = px * sin + py * cos;
            let sea = (distance + 0.5).clamp(0.0, 1.0);
            let t = f64::from(y) / 480.0;
            let sky = [120.0 + 100.0 * t, 170.0 + 60.0 * t, 235.0];
            let water = [10.0, 60.0 - 40.0 * t, 110.0 - 60.0 * t];
            let mix = |i: usize| (sky[i] * (1.0 - sea) + water[i] * sea).round() as u8;
            Rgba([mix(0), mix(1), mix(2), 255])
        })
    }

    #[test]
    fn line_tilt_finds_the_tilt_of_the_horizon() {
        for degrees in [-12.0, -5.0, -1.5, 0.0, 3.0, 8.0] {
            let tilt = line_tilt(&seascape(degrees)).unwrap_or_else(|| panic!("{degrees}"));
            assert!((tilt - degrees).abs() <= 0.5, "{degrees} → {tilt}");
        }
    }

    #[test]
    fn straighten_angle_cancels_the_tilt() {
        // 右が上がった（反時計回りに 6° 傾いた）水平線は、時計回りに 6° 回すと水平になる
        let angle = straighten_angle(&seascape(6.0)).unwrap().unwrap();
        assert!((angle - 6.0).abs() <= 0.5, "{angle}");
        let straightened = crate::transform::straighten(&seascape(6.0), angle);
        let tilt = line_tilt(&straightened).unwrap();
        assert!(tilt.abs() <= 0.5, "{tilt}");
    }

    #[test]
    fn no_lines_means_no_tilt() {
        let flat = RgbaImage::from_pixel(300, 200, Rgba([120, 130, 140, 255]));
        assert_eq!(line_tilt(&flat), None);
        assert_eq!(straighten_angle(&flat).unwrap(), None);
    }
}
