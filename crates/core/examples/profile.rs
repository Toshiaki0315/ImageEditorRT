//! 原寸 6000×4000・重い設定の処理の内訳を測る: `cargo run --release -p imageeditorrt-core --example profile`

use std::time::Instant;

use imageeditorrt_core::{adjust, blur, effects, filters, pipeline::EditSettings, sample::synthetic_photo};

fn main() {
    let image = synthetic_photo(6000, 4000);
    let reference = 4000.0;
    let settings = EditSettings::heavy();
    let time = |name: &str, f: &mut dyn FnMut()| {
        let start = Instant::now();
        f();
        println!("{name:<20} {:7.1} ms", start.elapsed().as_secs_f64() * 1000.0);
    };
    time("gaussian_blur r=8", &mut || drop(blur::gaussian_blur(&image, 8.0)));
    time("gaussian_blur r=64", &mut || drop(blur::gaussian_blur(&image, 64.0)));
    time("denoise 50", &mut || drop(effects::denoise(&image, 50, reference)));
    time("blur 10", &mut || drop(effects::blur(&image, 10, reference)));
    time("sharpen 50", &mut || drop(effects::sharpen(&image, 50, reference, None)));
    time("filter hdr", &mut || {
        let mut i = image.clone();
        filters::apply_filter(&mut i, filters::FilterType::Hdr);
    });
    time("vignette 50", &mut || {
        let mut i = image.clone();
        adjust::vignette(&mut i, 50);
    });
    time("aging 30", &mut || {
        let mut i = image.clone();
        adjust::aging(&mut i, 30);
    });
    time("clone", &mut || drop(image.clone()));
    time("enhance_color 1.15", &mut || {
        let mut i = image.clone();
        imageeditorrt_core::pillow::enhance_color(&mut i, 1.15);
    });
    time("apply_lut", &mut || {
        let mut i = image.clone();
        adjust::apply_lut(&mut i, &adjust::curve_lut(|x| x.powf(0.85)));
    });
    time("unsharp r=4.8", &mut || drop(blur::unsharp_mask(&image, 4.8, 100, 2)));
    time("add_grain", &mut || {
        let mut i = image.clone();
        adjust::add_grain(&mut i, 4, 1);
    });
    time("apply_edits heavy", &mut || {
        drop(imageeditorrt_core::pipeline::apply_edits(&image, &settings).unwrap())
    });
}
