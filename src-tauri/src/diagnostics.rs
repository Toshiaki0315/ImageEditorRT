//! 想定外のエラーの記録（旧版 NFR-04）と、ビルドの後の起動確認（`--smoke-test`、旧版 FR-APP-01）。
//!
//! - Rust のパニックは `~/Library/Logs/ImageEditorRT/imageeditorrt.log` に書き、画面に
//!   "unexpected-error" のイベントで知らせる（画面がダイアログで知らせる。アプリは終わらせない）
//! - 画面（TypeScript）の想定外のエラーも `report_unexpected` で同じファイルに書く

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use imageeditorrt_core::{decode, pipeline, sample, save};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// 画面に知らせるイベントの名前。
const UNEXPECTED_EVENT: &str = "unexpected-error";
const LOG_FILE: &str = "imageeditorrt.log";
/// 起動確認のフラグ。後ろに画像のパスを付けると、その画像を読めるかも確かめる。
pub const SMOKE_TEST_FLAG: &str = "--smoke-test";

/// パニックを画面に知らせるためのアプリ（起動後に設定する）。
static APP: OnceLock<AppHandle> = OnceLock::new();

/// 画面に知らせる中身。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Unexpected {
    message: String,
    log_path: String,
}

/// ログのファイル `~/Library/Logs/ImageEditorRT/imageeditorrt.log`。
pub fn log_path() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    Some(home.join("Library").join("Logs").join("ImageEditorRT").join(LOG_FILE))
}

/// 1970-01-01 からの日数を (年, 月, 日) にする（グレゴリオ暦）。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// ログに付ける日時（UTC）。
fn timestamp(now: SystemTime) -> String {
    let seconds = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let rest = seconds.rem_euclid(86_400);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// ログのファイルに 1 件書き足す（書けなくても止めない）。
pub fn append(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{}] {text}\n", timestamp(SystemTime::now()));
    }
}

/// パニックをログに書き、画面に知らせる（既定の表示＝標準エラー出力もそのまま行う）。
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = match (info.payload().downcast_ref::<&str>(), info.payload().downcast_ref::<String>()) {
            (Some(s), _) => (*s).to_string(),
            (_, Some(s)) => s.clone(),
            _ => "パニック".to_string(),
        };
        let place = info.location().map(|l| format!(" ({}:{})", l.file(), l.line())).unwrap_or_default();
        let backtrace = std::backtrace::Backtrace::force_capture();
        if let Some(path) = log_path() {
            append(&path, &format!("パニック: {message}{place}\n{backtrace}"));
            if let Some(app) = APP.get() {
                let payload = Unexpected { message, log_path: path.to_string_lossy().into_owned() };
                let _ = app.emit(UNEXPECTED_EVENT, payload);
            }
        }
        default(info);
    }));
}

/// 起動後に、パニックを知らせる先のアプリを覚える。
pub fn set_app(app: AppHandle) {
    let _ = APP.set(app);
}

/// 画面の想定外のエラーをログに書き、ログのパスを返す（画面がダイアログで知らせる）。
#[tauri::command]
pub fn report_unexpected(message: String) -> String {
    match log_path() {
        Some(path) => {
            append(&path, &format!("画面のエラー: {message}"));
            path.to_string_lossy().into_owned()
        }
        None => String::new(),
    }
}

/// 起動確認: 画面を出さずに、画像の読み込みとプレビューの処理ができるかを確かめる。
///
/// file を渡せばその画像を、なければ作った画像（PNG）を読む。読めれば 0、読めなければ 1 を返す。
pub fn smoke_test(file: Option<&Path>) -> i32 {
    let bytes = match file {
        Some(path) => match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("起動確認: {} を読めません（{e}）", path.display());
                return 1;
            }
        },
        None => match save::encode(&sample::synthetic_photo(64, 48), save::SaveFormat::Png, 90, None) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("起動確認: 確認用の画像を作れません（{e}）");
                return 1;
            }
        },
    };
    match decode::decode_file(&bytes) {
        Ok(decoded) => {
            let (preview, _) = pipeline::make_preview(&decoded.image, pipeline::PREVIEW_MAX_SIDE);
            let _ = pipeline::render_preview(&preview, &pipeline::EditSettings::default(), 1.0, false);
            let (width, height) = decoded.image.dimensions();
            println!("起動確認: OK（{width}×{height}、{:?}）", decoded.format);
            0
        }
        Err(e) => {
            eprintln!("起動確認: 画像を読めません（{e}）");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_utc_dates() {
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01 00:00:00 UTC");
        let t = UNIX_EPOCH + std::time::Duration::from_secs(1_791_017_045); // 2026-10-03 08:44:05 UTC
        assert_eq!(timestamp(t), "2026-10-03 08:44:05 UTC");
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn appends_to_the_log() {
        let dir = std::env::temp_dir().join(format!("imageeditorrt-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("a/imageeditorrt.log");
        append(&path, "1 件目");
        append(&path, "2 件目");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("] 1 件目") && text.contains("] 2 件目"), "{text}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn smoke_test_reads_images() {
        assert_eq!(smoke_test(None), 0);
        assert_eq!(smoke_test(Some(Path::new("/no/such/file.png"))), 1);
        let dir = std::env::temp_dir().join(format!("imageeditorrt-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("broken.png");
        std::fs::write(&broken, b"not an image").unwrap();
        assert_eq!(smoke_test(Some(&broken)), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
