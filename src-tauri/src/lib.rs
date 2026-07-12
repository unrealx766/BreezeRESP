mod commands;
mod core;

use std::sync::Mutex;
use tauri::{Manager, Listener};

pub struct AppState {
    pub pool_manager: Mutex<core::pool::ConnectionPoolManager>,
    pub config_store: Mutex<core::config_store::ConfigStore>,
    pub pipeline_store: Mutex<core::pipeline_store::PipelineStore>,
    pub shadow_store: Mutex<core::shadow_store::ShadowStore>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus the existing window when a second instance is launched
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            // Listen for "app-ready" event from frontend to show the window
            // This ensures the splash screen is rendered before the window appears
            let handle = app.handle().clone();
            app.listen("app-ready", move |_| {
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            });

            // Safety fallback: show the window after 3s even if frontend never emits "app-ready"
            let fallback_handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                if let Some(window) = fallback_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            });

            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("./data"));

            // Retrieve (or generate) the encryption key from the OS Keychain.
            let key = core::keystore::get_or_create_key()
                .expect("Failed to obtain encryption key from system Keychain");

            let config_store = core::config_store::ConfigStore::new(data_dir.clone(), key);
            let pipeline_store = core::pipeline_store::PipelineStore::new(data_dir, key);

            app.manage(AppState {
                pool_manager: Mutex::new(core::pool::ConnectionPoolManager::new()),
                config_store: Mutex::new(config_store),
                pipeline_store: Mutex::new(pipeline_store),
                shadow_store: Mutex::new(core::shadow_store::ShadowStore::new()),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Connection commands
            commands::connection::connect,
            commands::connection::disconnect,
            commands::connection::test_connection,
            commands::connection::get_connections,
            commands::connection::save_connection,
            commands::connection::delete_connection,
            commands::connection::switch_db,
            // Cascade commands
            commands::cascade::scan_keys,
            commands::cascade::get_key_detail,
            commands::cascade::delete_key,
            commands::cascade::set_key_ttl,
            commands::cascade::rename_key,
            commands::cascade::db_size,
            commands::cascade::set_value,
            // Pipeline commands
            commands::pipeline::execute_pipeline,
            commands::pipeline::save_pipeline,
            commands::pipeline::list_pipelines,
            commands::pipeline::delete_pipeline,
            // Sandbox commands
            commands::sandbox::sandbox_preview,
            commands::sandbox::sandbox_apply,
            commands::sandbox::sandbox_cancel,
            commands::sandbox::sandbox_rollback,
            // Metrics commands
            commands::metrics::get_metrics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
