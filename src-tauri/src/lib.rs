//! ImageEditorRT のアプリ本体。画像処理は imageeditorrt-core に任せ、
//! ここでは画面（TypeScript）との受け渡し・メニュー・ファイルを開く経路だけを扱う。
//!
//! コマンドは役割ごとのファイルに置く: image（開く・プレビュー）・saving（保存）・settings（選択肢と計算）・
//! system（メニューの状態・マップ・終了）・presets・batch・bench（計測）・diagnostics（ログ・起動確認）。

mod batch;
mod bench;
#[cfg(target_os = "macos")]
mod clipboard;
mod diagnostics;
mod image;
mod menu;
mod open;
mod presets;
mod recent;
mod saving;
mod settings;
#[cfg(target_os = "macos")]
mod share;
mod state;
mod system;

use std::path::PathBuf;

use tauri::Manager;

use crate::state::AppState;

/// ウィンドウのタイトル（画像を開くと「ファイル名 — ImageEditorRT」）。
const APP_NAME: &str = "ImageEditorRT";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ビルドの後の起動確認（画面を出さずに、画像を読めるかだけを確かめて終わる）
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if let Some(i) = args.iter().position(|a| a == diagnostics::SMOKE_TEST_FLAG) {
        let file = args.get(i + 1).map(PathBuf::from);
        std::process::exit(diagnostics::smoke_test(file.as_deref()));
    }
    // 想定外のエラー（パニック）はログに書き、画面で知らせる（旧版 NFR-04）
    diagnostics::install_panic_hook();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .manage(presets::PresetStore::default())
        .manage(batch::BatchState::default())
        .manage(recent::RecentStore::default())
        .manage(open::Pending::from_args(std::env::args_os().skip(1)))
        .menu(menu::build)
        .on_menu_event(menu::on_event)
        .setup(|app| {
            diagnostics::set_app(app.handle().clone());
            // 最近使った項目を読み、メニューに出す
            app.state::<recent::RecentStore>().load();
            recent::refresh_menu(app.handle());
            // 計測ではウィンドウが隠れていると描画が止まるので、前に出す
            if bench::bench_mode() {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_focus();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            image::open_path,
            image::render_preview,
            image::filter_thumbnails,
            image::detect_faces,
            image::detect_text,
            image::auto_straighten,
            image::auto_crop,
            image::neighbor_image,
            image::auto_adjust,
            image::prepare_background,
            image::prepare_faces,
            image::collage_choices,
            image::make_collage,
            image::render_actual_size,
            image::close_image,
            image::diorama_guide,
            image::clipboard_contents,
            image::clipboard_text,
            image::open_clipboard_image,
            saving::save_image,
            saving::save_sizes,
            #[cfg(target_os = "macos")]
            share::share_image,
            #[cfg(target_os = "macos")]
            saving::copy_image,
            saving::default_save_path,
            settings::supported_formats,
            settings::filter_types,
            settings::frame_shape_types,
            settings::text_options,
            settings::check_lut,
            settings::aspect_ratios,
            settings::effective_crop,
            settings::crop_drag,
            settings::crop_spin,
            settings::crop_fit,
            settings::crop_orient,
            settings::stamp_list,
            settings::apply_look,
            settings::resolve_size,
            settings::rotate_size,
            settings::output_size,
            system::set_menu_enabled,
            system::set_menu_checked,
            system::open_map,
            system::reveal_in_finder,
            system::quit_app,
            diagnostics::report_unexpected,
            open::take_pending_paths,
            presets::load_presets,
            presets::default_preset_name,
            presets::check_preset_name,
            presets::save_preset,
            presets::delete_preset,
            presets::export_presets,
            presets::import_presets,
            presets::apply_preset,
            presets::show_preset_menu,
            batch::batch_current_source,
            batch::batch_collect,
            batch::run_batch,
            batch::cancel_batch,
            bench::open_sample,
            bench::bench_mode,
            bench::bench_save_path,
            bench::bench_jpeg_path,
            bench::log,
            bench::report
        ])
        .build(tauri::generate_context!())
        .expect("ImageEditorRT を起動できませんでした");
    // 前に終わったときに残った共有の一時ファイルを消す
    #[cfg(target_os = "macos")]
    share::clean();
    app.run(|handle, event| {
        // Dock の「終了」・ログアウトなどで届く終了の求めは、画面が未保存の変更を確かめるまで止める
        if let tauri::RunEvent::ExitRequested { code: None, api, .. } = &event {
            if system::hold_exit(handle) {
                api.prevent_exit();
                return;
            }
        }
        // 共有に使った一時ファイルを消す
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Exit = event {
            share::clean();
        }
        // Finder の「このアプリケーションで開く」・Dock のアイコンへのドロップ
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Opened { urls } = event {
            open::opened(handle, urls);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (handle, event);
    });
}
