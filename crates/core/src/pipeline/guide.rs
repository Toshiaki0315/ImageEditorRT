//! ジオラマのピントの帯のガイド（プレビュー・100% 表示に重ねる線の位置）。

use serde::Serialize;

use super::{effective_crop, scale_settings, EditSettings};
use crate::diorama::{self, DioramaDirection};
use crate::frames;
use crate::transform::{self, CropRect, SizeError};

/// プレビューに重ねる、ジオラマのピントの帯のガイドの線。
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DioramaGuide {
    /// 横の帯なら true（線は横に引く）
    pub horizontal: bool,
    /// 線の位置（表示している画像の高さ（縦の帯なら幅）に対する割合）と、実線かどうか。
    /// 実線はくっきり残す範囲の端、破線はぼけきる位置。写真の範囲の外にも続ける
    pub lines: Vec<(f64, bool)>,
}

/// 全体表示のプレビューに重ねるジオラマのガイド。帯の位置は、実際に切り抜く範囲（なければ全体）に対する。
///
/// preview_size は回転・反転する前のプレビューの大きさ、factor は原寸に対する縮小率。
pub fn diorama_guide(preview_size: (u32, u32), settings: &EditSettings, factor: f64) -> DioramaGuide {
    let size = settings.orientation.size(preview_size);
    let scaled = scale_settings(settings, factor);
    let area =
        effective_crop(size, scaled.crop, settings.frame, settings.shape).unwrap_or(CropRect::whole(size));
    let horizontal = settings.diorama_direction == DioramaDirection::Horizontal;
    let (start, length, total) = if horizontal {
        (area.y as f64, area.height as f64, f64::from(size.1))
    } else {
        (area.x as f64, area.width as f64, f64::from(size.0))
    };
    guide_lines(settings, horizontal, start, length, total)
}

/// 100% 表示（原寸で処理した保存結果）に重ねるジオラマのガイド。帯の位置は、保存結果のうち
/// フレームの余白を除いた写真の部分に対する（リサイズ後の大きさで計算する）。
pub fn actual_size_diorama_guide(
    original_size: (u32, u32),
    settings: &EditSettings,
) -> Result<DioramaGuide, SizeError> {
    let size = settings.orientation.size(original_size);
    let photo = effective_crop(size, settings.crop, settings.frame, settings.shape)
        .map_or(size, |r| (r.width as u32, r.height as u32));
    let photo = transform::fit_size(photo, settings.width, settings.height, settings.keep_aspect)?;
    let (left, top, right, bottom) = frames::frame_margins(settings.frame, photo);
    let horizontal = settings.diorama_direction == DioramaDirection::Horizontal;
    let (start, length, total) = if horizontal {
        (top, photo.1, top + photo.1 + bottom)
    } else {
        (left, photo.0, left + photo.0 + right)
    };
    Ok(guide_lines(settings, horizontal, f64::from(start), f64::from(length), f64::from(total)))
}

/// 帯の 4 本の線（表示している画像の中の、写真の始まり start・長さ length・画像全体 total から）。
fn guide_lines(
    settings: &EditSettings,
    horizontal: bool,
    start: f64,
    length: f64,
    total: f64,
) -> DioramaGuide {
    let band = diorama::diorama_band(&settings.diorama());
    let at = |fraction: f64| (start + fraction * length) / total;
    DioramaGuide {
        horizontal,
        lines: vec![
            (at(band.blur_start), false),
            (at(band.sharp_start), true),
            (at(band.sharp_end), true),
            (at(band.blur_end), false),
        ],
    }
}
