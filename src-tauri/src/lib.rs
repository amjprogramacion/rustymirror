mod cache;
mod commands;
mod hasher;
mod heic;
mod scanner;
mod types;

use commands::{FileListCache, ScanState};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(ScanState(Mutex::new(Arc::new(AtomicBool::new(false)))))
        .manage(FileListCache(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::scan_directories,
            commands::stop_scan,
            commands::delete_files,
            commands::log_message,
            commands::get_thumbnail,
            commands::open_file,
            commands::open_folder,
            commands::get_full_image,
            commands::directory_fingerprint,
            commands::get_cache_size,
            commands::clear_cache,
            commands::get_thumb_cache_size,
            commands::clear_thumb_cache,
            commands::is_network_path,
            commands::is_debug_build,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
