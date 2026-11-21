#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod files;
mod update;

use std::path::{Path, PathBuf};
use tauri::{DragDropEvent, Manager, WebviewEvent};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            files::queue_open(
                app,
                files::launch_paths(args.into_iter().skip(1).map(PathBuf::from), Path::new(&cwd)),
            );
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(files::Files::default())
        .setup(|app| {
            if let Ok(cwd) = std::env::current_dir() {
                files::queue_open(
                    app.handle(),
                    files::launch_paths(std::env::args_os().skip(1).map(PathBuf::from), &cwd),
                );
            }
            Ok(())
        })
        .on_webview_event(|webview, event| {
            if let WebviewEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                files::queue_open(webview.app_handle(), paths.iter().cloned());
            }
        })
        .invoke_handler(tauri::generate_handler![
            files::pending_open,
            files::take_pending_open,
            files::pick_open,
            files::pick_save,
            files::open_native,
            files::read_native,
            files::begin_native_write,
            files::write_native,
            files::commit_native,
            files::recent_files,
            files::pick_recent,
            files::remember_native,
            files::forget_native,
            update::check_desktop_update,
            update::download_desktop_update,
            update::install_desktop_update,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run CashGear FotoEditor");
}
