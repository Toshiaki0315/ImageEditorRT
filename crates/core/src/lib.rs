//! ImageEditorRT の画像処理・EXIF。Tauri（画面）には依存しない。

/// 画素ごとの処理を並列にするとき、1 つの仕事にまとめる画素の数の下限
/// （細かく分けすぎると、分ける手間のほうが大きくなる）。
pub const PIXELS_PER_TASK: usize = 4096;

pub mod adjust;
pub mod auto;
pub mod background;
pub mod batch;
pub mod blur;
pub mod crop;
pub mod curve;
#[cfg(target_os = "macos")]
pub mod decode;
pub mod diorama;
pub mod effects;
pub mod encode;
pub mod exif_info;
pub mod exifread_note;
#[cfg(target_os = "macos")]
pub mod faces;
#[rustfmt::skip]
mod exifread_tables;
pub mod filters;
#[cfg(target_os = "macos")]
pub mod foreground;
pub mod formats;
pub mod frames;
pub mod histogram;
#[cfg(target_os = "macos")]
pub mod horizon;
#[cfg(target_os = "macos")]
pub mod load;
pub mod makernote;
pub mod output;
pub mod pillow;
pub mod pipeline;
pub mod presets;
pub mod privacy;
mod pyfmt;
pub mod pyrandom;
pub mod resize;
pub mod sample;
pub mod save;
pub mod shapes;
pub mod text;
#[cfg(target_os = "macos")]
pub mod text_regions;
pub mod tiff;
pub mod transform;
