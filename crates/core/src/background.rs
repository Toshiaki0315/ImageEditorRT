//! 背景を消す・ぼかす・置き換える（旧版にはない）: 被写体のマスクで、背景を透明・色にするか、ぼかすか、別の画像にする。
//!
//! マスク（被写体 255・背景 0。途中の値は境目のなめらかさ）は macOS の Vision で作る（`foreground`）。
//! ここでは、どの大きさの画像にもマスクを合わせて（拡大・縮小して）かける。処理は元の画像に対して、ほかの
//! すべての加工の前にかける（回転・切り抜き・テイストなどは、背景を消した後の画像にかかる）。

use image::{GrayImage, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PIXELS_PER_TASK;

/// 背景の扱い。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Background {
    /// そのまま（消さない）
    #[default]
    Keep,
    /// 透明にする（PNG・TIFF などで保存すると透明のまま。JPEG では白になる）
    Transparent,
    /// 色で塗る（色は `background_color`。既定は白。JSON の名前は旧い「白にする」のまま）
    White,
    /// ぼかす（ポートレート風。強さは `background_blur`）
    Blur,
    /// 画像に置き換える（画像は `background_image`。写真いっぱいに敷き、はみ出す分は切る。読めなければそのまま）
    Image,
}

/// 背景のぼかしの既定の強さ（1〜100）。
pub const BLUR_DEFAULT: u32 = 50;
/// 強さ 100 のときのぼかしの半径（写真の短辺に対する割合）。
const BLUR_MAX_RATIO: f64 = 0.04;

/// 画像に mask（画像と違う大きさなら合わせる）をかけて、背景を透明・白・ぼかした新しい画像を返す。
/// blur はぼかすときの強さ（1〜100）。Keep なら複製を返す。
pub fn apply_background(image: &RgbaImage, mask: &GrayImage, mode: Background, blur: u32) -> RgbaImage {
    apply_background_with(image, mask, mode, blur, [255, 255, 255])
}

/// apply_background と同じ。「色で塗る」の色は color。
pub fn apply_background_with(
    image: &RgbaImage,
    mask: &GrayImage,
    mode: Background,
    blur: u32,
    color: [u8; 3],
) -> RgbaImage {
    let mut out = image.clone();
    if mode == Background::Keep {
        return out;
    }
    let (width, height) = image.dimensions();
    let fitted = fit_mask(mask, (width, height));
    if mode == Background::Blur {
        return blur_background(image, &fitted, blur);
    }
    out.as_mut().par_chunks_exact_mut(4).zip(fitted.par_iter()).with_min_len(PIXELS_PER_TASK).for_each(
        |(p, &m)| {
            // 被写体である度合い（元の透明度も掛ける）
            let keep = u32::from(p[3]) * u32::from(m);
            match mode {
                Background::Transparent => p[3] = ((keep + 127) / 255) as u8,
                Background::White => {
                    // 色の上に重ねる（被写体の度合いで、元の色と塗る色を混ぜる）
                    for (c, fill) in p[..3].iter_mut().zip(color) {
                        let mixed = u32::from(*c) * keep + u32::from(fill) * (255 * 255 - keep);
                        *c = ((mixed + 255 * 255 / 2) / (255 * 255)) as u8;
                    }
                    p[3] = 255;
                }
                Background::Keep | Background::Blur | Background::Image => {}
            }
        },
    );
    out
}

/// 画像に mask（画像と違う大きさなら合わせる）をかけて、背景を backdrop に置き換えた新しい画像を返す。
/// backdrop は縦横比を保って写真いっぱいに広げ（縮め）、はみ出す分は真ん中を残して切る。透明な部分は backdrop が透ける。
pub fn replace_background(image: &RgbaImage, mask: &GrayImage, backdrop: &RgbaImage) -> RgbaImage {
    let (width, height) = image.dimensions();
    let fitted = fit_mask(mask, (width, height));
    let behind = cover(backdrop, (width, height));
    let mut out = image.clone();
    out.as_mut()
        .par_chunks_exact_mut(4)
        .zip(behind.as_raw().par_chunks_exact(4))
        .zip(fitted.par_iter())
        .with_min_len(PIXELS_PER_TASK)
        .for_each(|((p, b), &m)| {
            // 被写体の度合い（元の透明度も掛ける）で、被写体を背景の画像の上に重ねる
            let keep = u32::from(p[3]) * u32::from(m);
            let rest = 255 * 255 - keep;
            let alpha = keep + rest * u32::from(b[3]) / 255;
            if alpha == 0 {
                p.fill(0);
                return;
            }
            for c in 0..3 {
                let back = u32::from(b[c]) * rest / 255 * u32::from(b[3]);
                p[c] = ((u32::from(p[c]) * keep + back + alpha / 2) / alpha) as u8;
            }
            p[3] = ((alpha + 127) / 255) as u8;
        });
    out
}

/// image を縦横比を保って size いっぱいに広げ（縮め）、はみ出す分は真ん中を残して切る。
fn cover(image: &RgbaImage, (width, height): (u32, u32)) -> RgbaImage {
    let (w, h) = (f64::from(image.width().max(1)), f64::from(image.height().max(1)));
    let scale = (f64::from(width) / w).max(f64::from(height) / h);
    let scaled =
        ((w * scale).round().max(f64::from(width)) as u32, (h * scale).round().max(f64::from(height)) as u32);
    let resized = crate::resize::resize(image, scaled.0, scaled.1);
    image::imageops::crop_imm(&resized, (scaled.0 - width) / 2, (scaled.1 - height) / 2, width, height)
        .to_image()
}

/// mask を size の大きさに合わせた画素の並び（同じ大きさならそのまま）。
pub fn fit_mask(mask: &GrayImage, (width, height): (u32, u32)) -> Vec<u8> {
    if mask.dimensions() == (width, height) {
        mask.as_raw().clone()
    } else {
        crate::resize::resize_gray_bilinear(mask.as_raw(), mask.dimensions(), (width, height))
    }
}

/// 背景だけをぼかす。被写体の色が背景ににじまないよう、背景の部分（1 − マスク）を重みにしてぼかし
/// （重みで割り戻す）、元の画像とマスクで混ぜる。半径は写真の短辺に比例させる（プレビューと保存で同じ見え方）。
fn blur_background(image: &RgbaImage, mask: &[u8], blur: u32) -> RgbaImage {
    let (width, height) = image.dimensions();
    let radius = f64::from(width.min(height)) * BLUR_MAX_RATIO * f64::from(blur.clamp(1, 100)) / 100.0;
    // 色に背景の重みを掛け、重みを透明度の場所に入れてぼかす
    let mut weighted = image.clone();
    weighted.as_mut().par_chunks_exact_mut(4).zip(mask.par_iter()).with_min_len(PIXELS_PER_TASK).for_each(
        |(p, &m)| {
            let weight = 255 - u32::from(m);
            for c in &mut p[..3] {
                *c = ((u32::from(*c) * weight + 127) / 255) as u8;
            }
            p[3] = weight as u8;
        },
    );
    let blurred = crate::blur::gaussian_blur(&weighted, radius as f32);
    let mut out = image.clone();
    out.as_mut()
        .par_chunks_exact_mut(4)
        .zip(blurred.as_raw().par_chunks_exact(4))
        .zip(mask.par_iter())
        .with_min_len(PIXELS_PER_TASK)
        .for_each(|((p, b), &m)| {
            let weight = u32::from(b[3]);
            if weight == 0 {
                return; // まわりに背景がない（被写体の中）
            }
            let keep = u32::from(m);
            for c in 0..3 {
                // ぼかした背景の色（重みで割り戻す）を、被写体の度合いで元の色と混ぜる
                let background = (u32::from(b[c]) * 255 + weight / 2) / weight;
                let mixed = u32::from(p[c]) * keep + background.min(255) * (255 - keep);
                p[c] = ((mixed + 127) / 255) as u8;
            }
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgba};

    /// 左半分が被写体（255）、右半分が背景（0）のマスク。
    fn half_mask(width: u32, height: u32) -> GrayImage {
        GrayImage::from_fn(width, height, |x, _| if x < width / 2 { Luma([255]) } else { Luma([0]) })
    }

    #[test]
    fn transparent_and_white_backgrounds() {
        let image = RgbaImage::from_pixel(8, 4, Rgba([10, 100, 200, 255]));
        let mask = half_mask(8, 4);
        let clear = apply_background(&image, &mask, Background::Transparent, BLUR_DEFAULT);
        assert_eq!(clear.get_pixel(1, 1).0, [10, 100, 200, 255]);
        assert_eq!(clear.get_pixel(6, 1)[3], 0);
        let white = apply_background(&image, &mask, Background::White, BLUR_DEFAULT);
        assert_eq!(white.get_pixel(1, 1).0, [10, 100, 200, 255]);
        assert_eq!(white.get_pixel(6, 1).0, [255, 255, 255, 255]);
        // 色を選べば、背景はその色（被写体はそのまま）
        let green = apply_background_with(&image, &mask, Background::White, BLUR_DEFAULT, [20, 180, 60]);
        assert_eq!(
            (green.get_pixel(1, 1).0, green.get_pixel(6, 1).0),
            ([10, 100, 200, 255], [20, 180, 60, 255])
        );
        assert_eq!(apply_background(&image, &mask, Background::Keep, BLUR_DEFAULT), image);
    }

    #[test]
    fn background_is_replaced_by_the_covering_image() {
        let image = RgbaImage::from_pixel(8, 4, Rgba([10, 100, 200, 255]));
        let mask = half_mask(8, 4);
        // 正方形の背景の画像（上が赤・下が緑）は、横長の写真いっぱいに広げ、上下のはみ出しを切る（真ん中が残る）
        let backdrop =
            RgbaImage::from_fn(
                4,
                4,
                |_, y| if y < 2 { Rgba([255, 0, 0, 255]) } else { Rgba([0, 255, 0, 255]) },
            );
        let out = replace_background(&image, &mask, &backdrop);
        assert_eq!(out.dimensions(), (8, 4));
        assert_eq!(out.get_pixel(1, 1).0, [10, 100, 200, 255], "被写体はそのまま");
        let top = out.get_pixel(6, 0).0;
        let bottom = out.get_pixel(6, 3).0;
        assert!(top[0] > 200 && top[1] < 60 && top[3] == 255, "{top:?}");
        assert!(bottom[1] > 200 && bottom[0] < 60 && bottom[3] == 255, "{bottom:?}");
        // 背景の画像の大きさはどれでもよい（縦長・大きい画像でも写真の大きさになる）
        let tall = RgbaImage::from_pixel(30, 90, Rgba([0, 0, 255, 255]));
        let out = replace_background(&image, &mask, &tall);
        assert_eq!(out.get_pixel(7, 2).0, [0, 0, 255, 255]);
        // 透明な背景の画像なら、背景は透明（被写体は残る）
        let clear = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 0]));
        let out = replace_background(&image, &mask, &clear);
        assert_eq!((out.get_pixel(1, 1).0, out.get_pixel(6, 1)[3]), ([10, 100, 200, 255], 0));
    }

    #[test]
    fn mask_is_fitted_to_the_image_size() {
        // 4 × 2 のマスクを 40 × 20 の画像に合わせる。左の端は被写体、右の端は背景
        let image = RgbaImage::from_pixel(40, 20, Rgba([50, 50, 50, 255]));
        let clear = apply_background(&image, &half_mask(4, 2), Background::Transparent, BLUR_DEFAULT);
        assert_eq!(clear.dimensions(), (40, 20));
        assert_eq!(clear.get_pixel(2, 10)[3], 255);
        assert_eq!(clear.get_pixel(37, 10)[3], 0);
        // 境目は途中の透明度（なめらか）
        assert!(clear.pixels().any(|p| p[3] > 0 && p[3] < 255));
    }

    #[test]
    fn soft_edges_and_existing_transparency() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 128]));
        let mask = GrayImage::from_pixel(1, 1, Luma([128]));
        // 元の透明度（半分）× マスク（半分）= 4 分の 1
        assert_eq!(
            apply_background(&image, &mask, Background::Transparent, BLUR_DEFAULT).get_pixel(0, 0)[3],
            64
        );
        // 白: 黒を 4 分の 1 だけ残して白に混ぜる
        let white = apply_background(&image, &mask, Background::White, BLUR_DEFAULT);
        assert_eq!(white.get_pixel(0, 0).0, [191, 191, 191, 255]);
    }

    #[test]
    fn blur_keeps_the_subject_and_softens_the_background() {
        // 左半分が被写体（赤）、右半分が背景（白黒の細かい縞）
        let image = RgbaImage::from_fn(200, 100, |x, _| {
            if x < 100 {
                Rgba([220, 30, 30, 255])
            } else if x % 4 < 2 {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });
        let mask = half_mask(200, 100);
        let out = apply_background(&image, &mask, Background::Blur, 50);
        // 被写体はそのまま（縁から離れたところ）
        assert_eq!(out.get_pixel(20, 50), image.get_pixel(20, 50));
        // 背景の縞はならされて灰色に近づく
        let p = out.get_pixel(160, 50);
        assert!((100..=160).contains(&p[0]), "{p:?}");
        // 被写体の赤が背景ににじまない（境目のすぐ右も赤くならない）
        let edge = out.get_pixel(103, 50);
        assert!(i32::from(edge[0]) - i32::from(edge[1]) < 30, "{edge:?}");
        // 強いほどよくならされる
        let spread = |blur: u32| {
            let out = apply_background(&image, &mask, Background::Blur, blur);
            (150..190).map(|x| (i32::from(out.get_pixel(x, 50)[0]) - 128).abs()).sum::<i32>()
        };
        assert!(spread(90) <= spread(5));
    }

    #[test]
    fn blur_looks_the_same_at_any_size() {
        // 同じ写真の原寸と半分で、背景のならされ方（縞の残り方）がほぼ同じ
        let make = |scale: u32| {
            RgbaImage::from_fn(200 * scale, 100 * scale, |x, _| {
                if x < 100 * scale {
                    Rgba([220, 30, 30, 255])
                } else if (x / scale / 8).is_multiple_of(2) {
                    Rgba([255, 255, 255, 255])
                } else {
                    Rgba([0, 0, 0, 255])
                }
            })
        };
        let contrast = |scale: u32| {
            let image = make(scale);
            let out = apply_background(&image, &half_mask(200 * scale, 100 * scale), Background::Blur, 40);
            let row: Vec<i32> =
                (150 * scale..190 * scale).map(|x| i32::from(out.get_pixel(x, 50 * scale)[0])).collect();
            row.iter().max().unwrap() - row.iter().min().unwrap()
        };
        assert!((contrast(1) - contrast(2)).abs() <= 25, "{} {}", contrast(1), contrast(2));
    }
}
