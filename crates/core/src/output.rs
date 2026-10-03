//! 「出力」タブのサイズ変更（幅・高さ・縦横比を保持）。旧版の ui/settings_panel.py の計算を移したもの。
//!
//! 欄に出す幅・高さと、編集設定（EditSettings の width・height）に渡す値を求める。

use serde::{Deserialize, Serialize};

use crate::pipeline::{effective_crop, EditSettings};
use crate::transform::{fit_size, MAX_SIZE, MIN_SIZE};

/// 最後に手で変えた欄。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    #[default]
    Width,
    Height,
}

/// サイズ変更の欄の状態。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeState {
    pub width: u32,
    pub height: u32,
    /// 手で変えたか（変えるまでは、トリミング後の大きさを出す）
    pub edited: bool,
    pub last: Side,
    pub keep_aspect: bool,
}

impl Default for SizeState {
    fn default() -> Self {
        Self { width: MIN_SIZE, height: MIN_SIZE, edited: false, last: Side::Width, keep_aspect: true }
    }
}

/// 欄に出す値と、編集設定に渡す値。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeResult {
    /// 欄に出す幅・高さ
    pub state: SizeState,
    /// リサイズ前の大きさ（トリミング後。フレーム・円の比に切り抜いた後）
    pub base: (u32, u32),
    /// EditSettings の width・height（リサイズしないなら None）
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// リサイズ前の大きさ: トリミング後（フレームがあれば写真部分の比、フレームなしの円は正方形に切り抜いた後）。
/// 切り抜かないなら原寸（回転・反転した後）。
pub fn base_size(original_size: (u32, u32), settings: &EditSettings) -> (u32, u32) {
    let size = settings.orientation.size(original_size);
    effective_crop(size, settings.crop, settings.frame, settings.shape)
        .map_or(size, |r| (r.width as u32, r.height as u32))
}

/// 今の設定（範囲・フレームなど）と欄の状態から、欄に出す値と編集設定に渡す値を求める。
///
/// - 手で変えるまでは、トリミング後の大きさを出し、リサイズしない
/// - 縦横比を保持するときは、最後に変えた側から他方を計算し、その側だけを渡す
/// - 欄の値がトリミング後の大きさと同じなら、リサイズしない
pub fn resolve(original_size: (u32, u32), settings: &EditSettings, state: SizeState) -> SizeResult {
    let base = base_size(original_size, settings);
    let clamp = |v: u32| v.clamp(MIN_SIZE, MAX_SIZE);
    let mut state = SizeState { width: clamp(state.width), height: clamp(state.height), ..state };
    if !state.edited {
        (state.width, state.height) = (clamp(base.0), clamp(base.1));
    } else if state.keep_aspect {
        let size = match state.last {
            Side::Width => fit_size(base, Some(state.width), None, true),
            Side::Height => fit_size(base, None, Some(state.height), true),
        };
        if let Ok((w, h)) = size {
            (state.width, state.height) = (clamp(w), clamp(h));
        }
    }
    let (width, height) = if (state.width, state.height) == base {
        (None, None)
    } else if state.keep_aspect {
        match state.last {
            Side::Width => (Some(state.width), None),
            Side::Height => (None, Some(state.height)),
        }
    } else {
        (Some(state.width), Some(state.height))
    };
    SizeResult { state, base, width, height }
}

/// 90° 回転したとき: 手で変えた幅・高さを入れ替える（旧版 FR-UI-55）。
pub fn rotate(state: SizeState) -> SizeState {
    if !state.edited {
        return state;
    }
    let last = if state.last == Side::Width { Side::Height } else { Side::Width };
    SizeState { width: state.height, height: state.width, last, ..state }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::FrameType;
    use crate::transform::CropRect;

    fn edited(width: u32, height: u32, last: Side, keep_aspect: bool) -> SizeState {
        SizeState { width, height, edited: true, last, keep_aspect }
    }

    #[test]
    fn untouched_shows_the_trimmed_size() {
        let settings =
            EditSettings { crop: Some(CropRect::new(10, 10, 300, 200)), ..EditSettings::default() };
        let r = resolve((4000, 3000), &settings, SizeState::default());
        assert_eq!((r.state.width, r.state.height), (300, 200));
        assert_eq!((r.width, r.height), (None, None));
        // フレームがあれば写真部分の比に切り抜いた後の大きさ
        let framed = EditSettings { frame: FrameType::Polaroid, ..EditSettings::default() };
        assert_eq!(base_size((4000, 3000), &framed), (3000, 3000));
    }

    #[test]
    fn keep_aspect_computes_the_other_side() {
        let settings = EditSettings::default();
        let r = resolve((4000, 3000), &settings, edited(1000, 1, Side::Width, true));
        assert_eq!((r.state.width, r.state.height), (1000, 750));
        assert_eq!((r.width, r.height), (Some(1000), None));
        let r = resolve((4000, 3000), &settings, edited(1, 600, Side::Height, true));
        assert_eq!((r.state.width, r.state.height), (800, 600));
        assert_eq!((r.width, r.height), (None, Some(600)));
    }

    #[test]
    fn free_aspect_passes_both() {
        let r = resolve((4000, 3000), &EditSettings::default(), edited(1000, 300, Side::Width, false));
        assert_eq!((r.width, r.height), (Some(1000), Some(300)));
    }

    #[test]
    fn same_as_base_means_no_resize() {
        let r = resolve((4000, 3000), &EditSettings::default(), edited(4000, 3000, Side::Width, false));
        assert_eq!((r.width, r.height), (None, None));
    }

    #[test]
    fn values_are_clamped_and_rotation_swaps() {
        let r = resolve((4000, 3000), &EditSettings::default(), edited(50000, 0, Side::Width, false));
        assert_eq!((r.state.width, r.state.height), (MAX_SIZE, MIN_SIZE));
        let rotated = rotate(edited(1000, 750, Side::Width, true));
        assert_eq!((rotated.width, rotated.height, rotated.last), (750, 1000, Side::Height));
        assert_eq!(rotate(SizeState::default()), SizeState::default());
    }
}
