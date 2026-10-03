//! ImageEditorRT の画像処理・EXIF。Tauri（画面）には依存しない。

/// 画素ごとの処理を並列にするとき、1 つの仕事にまとめる画素の数の下限
/// （細かく分けすぎると、分ける手間のほうが大きくなる）。
pub const PIXELS_PER_TASK: usize = 4096;

pub mod adjust;
pub mod blur;
pub mod crop;
#[cfg(target_os = "macos")]
pub mod decode;
pub mod diorama;
pub mod effects;
pub mod encode;
pub mod exif_info;
pub mod exifread_note;
#[rustfmt::skip]
mod exifread_tables;
pub mod filters;
pub mod formats;
pub mod frames;
pub mod makernote;
pub mod output;
pub mod pillow;
pub mod pipeline;
pub mod pyrandom;
pub mod resize;
pub mod sample;
pub mod save;
pub mod shapes;
pub mod text;
pub mod tiff;
pub mod transform;
