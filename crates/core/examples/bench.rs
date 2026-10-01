//! 試作のベンチマーク: `cargo run --release -p imageeditorrt-core --example bench`
//!
//! Python 版（scripts/bench.py）と同じく、6000×4000 の画像を長辺 1600px に縮めたプレビューで、
//! 重い設定の処理時間を測る。

use std::time::{Duration, Instant};

use image::{imageops, Rgba, RgbaImage};
use imageeditorrt_core::preview::{render, Settings};

const PREVIEW_MAX_SIDE: u32 = 1600;

fn measure<T>(repeat: usize, mut f: impl FnMut() -> T) -> (Duration, Duration, T) {
    let mut times = Vec::with_capacity(repeat);
    let mut last = None;
    for _ in 0..repeat {
        let start = Instant::now();
        last = Some(f());
        times.push(start.elapsed());
    }
    times.sort();
    (times[0], times[times.len() / 2], last.unwrap())
}

fn ms(d: Duration) -> String {
    format!("{:7.1} ms", d.as_secs_f64() * 1000.0)
}

fn sample(width: u32, height: u32) -> RgbaImage {
    // 写真らしく、なめらかな部分と細かい模様のある画像
    RgbaImage::from_fn(width, height, |x, y| {
        let fx = x as f32 / width as f32;
        let fy = y as f32 / height as f32;
        let fine = if (x / 3 + y / 5) % 2 == 0 { 25 } else { 0 };
        Rgba([
            (fx * 200.0) as u8 + fine,
            (fy * 180.0) as u8 + 30,
            ((1.0 - fx) * 150.0) as u8 + fine,
            255,
        ])
    })
}

fn main() {
    println!("rayon のスレッド数: {}", rayon::current_num_threads());
    let original = sample(6000, 4000);

    let (_, resize_ms, preview) = measure(3, || {
        let scale = PREVIEW_MAX_SIDE as f32 / original.width().max(original.height()) as f32;
        let w = (original.width() as f32 * scale).round() as u32;
        let h = (original.height() as f32 * scale).round() as u32;
        imageops::resize(&original, w, h, imageops::FilterType::Lanczos3)
    });
    println!("プレビュー用に縮小 (6000x4000 → {}x{}): {}", preview.width(), preview.height(), ms(resize_ms));

    let heavy = Settings::heavy();
    let (best, median, rendered) = measure(15, || render(&preview, &heavy));
    println!("プレビュー更新（重い設定）: 最小 {} / 中央値 {}", ms(best), ms(median));
    let light = Settings { exposure: 0.7, saturation: 20, ..Settings::default() };
    let (best_light, median_light, _) = measure(15, || render(&preview, &light));
    println!("プレビュー更新（露出＋彩度だけ）: 最小 {} / 中央値 {}", ms(best_light), ms(median_light));

    let (full_best, _, _) = measure(3, || render(&original, &heavy));
    println!("原寸処理 6000x4000（重い設定）: {}", ms(full_best));

    // Rust → WebView への受け渡しのやり方の候補ごとの、変換の時間
    let raw_bytes = rendered.as_raw().len();
    let (jpeg_ms, _, jpeg) = measure(5, || {
        let mut out = Vec::new();
        let encoder = jpeg_encoder::Encoder::new(&mut out, 85);
        encoder
            .encode(rendered.as_raw(), rendered.width() as u16, rendered.height() as u16, jpeg_encoder::ColorType::Rgba)
            .unwrap();
        out
    });
    println!("受け渡し: RGBA のまま {:.1} MB / JPEG (q85) に変換 {}（{:.0} KB）", raw_bytes as f64 / 1e6, ms(jpeg_ms), jpeg.len() as f64 / 1e3);

    #[cfg(target_os = "macos")]
    {
        let heic = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rotated.heic")).unwrap();
        let (first, _, _) = measure(1, || imageeditorrt_core::decode::decode(&heic).unwrap());
        let (warm, _, _) = measure(5, || imageeditorrt_core::decode::decode(&heic).unwrap());
        println!("HEIC 1500x1000 の読み込み: 初回 {} / 2 回目以降 {}", ms(first), ms(warm));
    }

    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/bench_heavy_preview.png");
    rendered.save(out).unwrap();
    println!("重い設定のプレビュー: {out}");
}
