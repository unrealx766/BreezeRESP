mod commands;
mod core;

use std::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub pool_manager: Mutex<core::pool::ConnectionPoolManager>,
    pub config_store: Mutex<core::config_store::ConfigStore>,
    pub shadow_store: Mutex<core::shadow_store::ShadowStore>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("./data"));

            // Use a fixed 32-byte key derived from app name (in production, use system keychain)
            let mut key = [0u8; 32];
            let app_name = b"BreezeRESP";
            key[..app_name.len()].copy_from_slice(app_name);

            let config_store = core::config_store::ConfigStore::new(data_dir, key);

            app.manage(AppState {
                pool_manager: Mutex::new(core::pool::ConnectionPoolManager::new()),
                config_store: Mutex::new(config_store),
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
            // Sandbox commands
            commands::sandbox::sandbox_preview,
            commands::sandbox::sandbox_apply,
            commands::sandbox::sandbox_rollback,
            // Metrics commands
            commands::metrics::get_metrics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
