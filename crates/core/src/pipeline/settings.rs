//! 編集設定（EditSettings）と、その既定値・強さの範囲。

use serde::{Deserialize, Serialize};

use crate::adjust;
use crate::background::Background;
use crate::curve::{self, HslAdjust, HSL_BANDS};
use crate::diorama::{DioramaDirection, DioramaSettings};
use crate::filters::FilterType;
use crate::frames::FrameType;
use crate::local::LocalAdjust;
use crate::logo::LogoSettings;
use crate::lut::LutSettings;
use crate::privacy::Region;
use crate::shapes::{self, ShapeType};
use crate::text::TextSettings;
use crate::transform::{CropRect, Orientation};

/// テイストの強さの既定（100% = テイストのまま）。
pub const FILTER_STRENGTH_FULL: u32 = 100;
/// テイストの強さの上限（%）。100% を超えるとテイストによる変化を強める。
pub const FILTER_STRENGTH_MAX: u32 = 200;

/// プレビューの長辺（px）。
pub const PREVIEW_MAX_SIDE: u32 = 1600;
/// テイストの一覧の見本の長辺（px。切り抜いた写真の長辺をこの大きさにする）。
pub const THUMBNAIL_MAX_SIDE: u32 = 240;

/// 編集設定。トリミング範囲は、回転・反転した後の原寸画像の座標で持つ。JSON では camelCase。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EditSettings {
    pub orientation: Orientation,
    /// 水平の補正 -45〜45（度、正は時計回り、0 = なし）。回転・反転の後に回し、余白が出ないよう拡大する
    /// （大きさは変わらない。旧版にはない）
    pub straighten: f64,
    pub crop: Option<CropRect>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub keep_aspect: bool,
    pub filter: FilterType,
    /// テイストの強さ 0〜200（%）。元の写真とテイストをかけた写真を混ぜる（100 = テイストのまま、100 を超えると
    /// 変化を強める。旧版にはない）
    pub filter_strength: u32,
    /// 周辺減光 0〜100（0 = なし）
    pub vignette: u32,
    /// 経年劣化 0〜100（0 = なし）
    pub aging: u32,
    /// 色温度（ケルビン、6500 = 変化なし）
    pub temperature: u32,
    /// 彩度 -100〜+100（0 = 変化なし）
    pub saturation: i32,
    /// 明るさ -100〜+100（0 = 変化なし）
    pub brightness: i32,
    /// コントラスト -100〜+100（0 = 変化なし）
    pub contrast: i32,
    /// ハイライト・シャドウ -100〜+100（0 = 変化なし。明部・暗部だけを明るく・暗くする。旧版にはない）
    pub highlights: i32,
    pub shadows: i32,
    /// 露出 -5.0〜+5.0 EV（0 = 変化なし）
    pub exposure: f64,
    /// シャープ・ぼかし・ノイズ除去 0〜100（0 = なし）
    pub sharpen: u32,
    pub blur: u32,
    pub denoise: u32,
    /// ジオラマ風（ぼかし 0 = なし）。位置・幅は写真の高さに対する %
    pub diorama_blur: u32,
    pub diorama_direction: DioramaDirection,
    pub diorama_position: u32,
    pub diorama_width: u32,
    pub diorama_vivid: u32,
    pub text: TextSettings,
    pub frame: FrameType,
    pub shape: ShapeType,
    /// 角丸の半径（短辺に対する % 0〜50）
    pub corner_radius: u32,
    /// 投稿加工で隠す範囲（ぼかし・モザイクは回転・反転の直後に、スタンプは経年劣化の後にかける。旧版にはない）
    pub regions: Vec<Region>,
    /// 部分補正（円・帯の範囲の中だけの調整。色の調整の直後にかける。写真ごとの範囲なのでプリセットには入れない。旧版にはない）
    pub local_adjustments: Vec<LocalAdjust>,
    /// 背景を消す（旧版にはない）。被写体のマスクが要るので、ここの処理ではかけず、アプリ本体が元の画像に
    /// 前もってかける（`background::apply_background`）
    pub background: Background,
    /// 背景のぼかしの強さ 1〜100（背景を「ぼかす」とき）
    pub background_blur: u32,
    /// トーンカーブの点（x は 0〜255 で増えていく順、両端は x = 0・255。旧版にはない）
    pub tone_curve: Vec<[u8; 2]>,
    /// 色ごとの調整（赤・オレンジ・黄・緑・水色・青・紫・マゼンタ。旧版にはない）
    pub hsl: [HslAdjust; HSL_BANDS],
    /// ロゴの透かし（文字と同じく写真の上・フレームの余白に描く。旧版にはない）
    pub logo: LogoSettings,
    /// LUT（.cube。テイストの後にかける。ファイルの場所で覚える。旧版にはない）
    pub lut: LutSettings,
    /// 肌をなめらかに 0〜100（顔の枠が要るので、背景と同じくアプリ本体が元の画像に前もってかける。旧版にはない）
    pub skin_smooth: u32,
    /// 赤目の補正（肌をなめらかにと同じく、顔の枠を使って元の画像に前もってかける。旧版にはない）
    pub red_eye: bool,
}

impl Default for EditSettings {
    fn default() -> Self {
        Self {
            orientation: Orientation::default(),
            straighten: 0.0,
            crop: None,
            width: None,
            height: None,
            keep_aspect: true,
            filter: FilterType::None,
            filter_strength: FILTER_STRENGTH_FULL,
            vignette: 0,
            aging: 0,
            temperature: adjust::TEMPERATURE_NEUTRAL,
            saturation: 0,
            brightness: 0,
            contrast: 0,
            highlights: 0,
            shadows: 0,
            exposure: 0.0,
            sharpen: 0,
            blur: 0,
            denoise: 0,
            diorama_blur: 0,
            diorama_direction: DioramaDirection::Horizontal,
            diorama_position: 50,
            diorama_width: 20,
            diorama_vivid: 30,
            text: TextSettings::default(),
            frame: FrameType::None,
            shape: ShapeType::Rectangle,
            corner_radius: shapes::CORNER_RADIUS_DEFAULT,
            regions: Vec::new(),
            background: Background::Keep,
            background_blur: crate::background::BLUR_DEFAULT,
            tone_curve: curve::identity_curve(),
            hsl: [HslAdjust::default(); HSL_BANDS],
            logo: LogoSettings::default(),
            lut: LutSettings::default(),
            skin_smooth: 0,
            red_eye: false,
            local_adjustments: Vec::new(),
        }
    }
}

impl EditSettings {
    /// 旧版のベンチマークの「重い設定」（ほぼすべての効果をかける）。
    pub fn heavy() -> Self {
        Self {
            exposure: 0.5,
            brightness: 10,
            contrast: 20,
            temperature: 5000,
            saturation: 20,
            denoise: 50,
            blur: 10,
            sharpen: 50,
            diorama_blur: 80,
            filter: FilterType::Hdr,
            vignette: 50,
            aging: 30,
            text: TextSettings { text: "© 2026 写真".into(), ..TextSettings::default() },
            ..Self::default()
        }
    }

    /// ジオラマ風の加工の設定をまとめて返す。
    pub fn diorama(&self) -> DioramaSettings {
        DioramaSettings {
            blur: self.diorama_blur,
            direction: self.diorama_direction,
            position: self.diorama_position,
            width: self.diorama_width,
            vivid: self.diorama_vivid,
        }
    }
}
