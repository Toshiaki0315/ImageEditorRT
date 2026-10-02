//! 試作のベンチマーク: `cargo run --release -p imageeditorrt-core --example bench`
//!
//! Python 版（scripts/bench.py）と同じく、6000×4000 の画像を長辺 1600px に縮めたプレビューで、
//! 重い設定の処理時間を測る。

use std::time::{Duration, Instant};

use image::imageops;
use imageeditorrt_core::pipeline::{render_preview, EditSettings};
use imageeditorrt_core::resize::fit_long_side;
use imageeditorrt_core::sample::synthetic_photo;

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

fn main() {
    println!("rayon のスレッド数: {}", rayon::current_num_threads());
    let original = synthetic_photo(6000, 4000);

    let (_, resize_ms, preview) = measure(3, || fit_long_side(&original, PREVIEW_MAX_SIDE));
    println!("プレビュー用に縮小 (6000x4000 → {}x{}): {}", preview.width(), preview.height(), ms(resize_ms));
    let (_, image_resize_ms, _) = measure(3, || {
        imageops::resize(&original, preview.width(), preview.height(), imageops::FilterType::Lanczos3)
    });
    println!("  （参考: image クレートの Lanczos3 では {}）", ms(image_resize_ms));

    let heavy = EditSettings::heavy();
    let (best, median, rendered) = measure(15, || render_preview(&preview, &heavy, 1.0, false));
    println!("プレビュー更新（重い設定）: 最小 {} / 中央値 {}", ms(best), ms(median));
    let light = EditSettings { exposure: 0.7, saturation: 20, ..EditSettings::default() };
    let (best_light, median_light, _) = measure(15, || render_preview(&preview, &light, 1.0, false));
    println!("プレビュー更新（露出＋彩度だけ）: 最小 {} / 中央値 {}", ms(best_light), ms(median_light));

    let (full_best, _, _) =
        measure(3, || imageeditorrt_core::pipeline::apply_edits(&original, &heavy).unwrap());
    println!("原寸処理 6000x4000（重い設定）: {}", ms(full_best));

    // Rust → WebView への受け渡しのやり方の候補ごとの、変換の時間
    let raw_bytes = rendered.as_raw().len();
    let (jpeg_ms, _, jpeg) = measure(5, || imageeditorrt_core::encode::to_jpeg(&rendered, 85));
    println!(
        "受け渡し: RGBA のまま {:.1} MB / JPEG (q85) に変換 {}（{:.0} KB）",
        raw_bytes as f64 / 1e6,
        ms(jpeg_ms),
        jpeg.len() as f64 / 1e3
    );

    #[cfg(target_os = "macos")]
    {
        let heic =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rotated.heic")).unwrap();
        let (first, _, _) = measure(1, || imageeditorrt_core::decode::decode(&heic).unwrap());
        let (warm, _, _) = measure(5, || imageeditorrt_core::decode::decode(&heic).unwrap());
        println!("HEIC 1500x1000 の読み込み: 初回 {} / 2 回目以降 {}", ms(first), ms(warm));

        // NFR-01: 12MP (4000×3000) の JPEG を読み込んでから、プレビューを作るまで（画面への受け渡しは数 ms）
        let jpeg = imageeditorrt_core::encode::to_jpeg(&synthetic_photo(4000, 3000), 90);
        let (best, median, _) = measure(5, || {
            let decoded = imageeditorrt_core::decode::decode_file(&jpeg).unwrap();
            let small = fit_long_side(&decoded.image, PREVIEW_MAX_SIDE);
            let exif = imageeditorrt_core::exif_info::read_exif_info(&jpeg);
            (render_preview(&small, &EditSettings::default(), 1.0, false), exif)
        });
        let verdict = if median.as_secs_f64() < 1.0 { "OK" } else { "NG" };
        println!(
            "12MP の JPEG の読み込み → プレビュー (NFR-01, 1 秒以内): 最小 {} / 中央値 {} → {verdict}",
            ms(best),
            ms(median)
        );
    }

    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/bench_heavy_preview.png");
    rendered.save(out).unwrap();
    println!("重い設定のプレビュー: {out}");
}
