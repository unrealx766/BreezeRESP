use crate::core::slowlog::{SlowlogCollector, SlowlogInfo};
use crate::core::validate::validate_connection_id;
use crate::AppState;
use tauri::State;
use std::path::PathBuf;

/// Get slow-log entries from a Redis instance
#[tauri::command]
pub async fn get_slowlog(
    state: State<'_, AppState>,
    connection_id: String,
    count: Option<u64>,
) -> Result<SlowlogInfo, String> {
    validate_connection_id(&connection_id)?;

    let count = count.unwrap_or(128).min(1000);

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    let collector = SlowlogCollector::new();
    // Cluster: collect from all masters and merge
    if let Some(cluster) = conn.as_cluster() {
        return collector.collect_cluster(cluster, count).await;
    }
    collector.collect(&mut conn, count).await
}

/// Save exported slowlog data to user's download directory.
/// Returns the full file path.
#[tauri::command]
pub async fn save_slowlog_export(
    content: String,
    filename: String,
) -> Result<String, String> {
    // Sanitize filename
    let safe_name = filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect::<String>();

    if safe_name.is_empty() {
        return Err("Invalid filename".to_string());
    }

    // Get user's download directory
    let download_dir = dirs_download().ok_or("Cannot find download directory")?;
    let file_path = download_dir.join(&safe_name);

    std::fs::write(&file_path, &content)
        .map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(file_path.to_string_lossy().to_string())
}

/// Get the user's download directory.
pub(crate) fn dirs_download() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let home = std::env::var("USERPROFILE").ok()?;
        Some(PathBuf::from(home).join("Downloads"))
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").ok()?;
        Some(PathBuf::from(home).join("Downloads"))
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").ok()?;
        Some(PathBuf::from(home).join("Downloads"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Reveal an exported file in the system file manager.
#[tauri::command]
pub async fn open_file_location(path: String) -> Result<(), String> {
    let file_path = PathBuf::from(&path);
    if !file_path.exists() {
        return Err(format!("File not found: {}", path));
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path))
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {}", e))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| format!("Failed to open finder: {}", e))?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        // Try file managers that support selecting the file, fall back to opening the directory
        let dir = file_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let result = std::process::Command::new("dbus-send")
            .args([
                "--print-reply",
                "--dest=org.freedesktop.FileManager1",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
                &format!("array:string:file://{}", path),
                "string:",
            ])
            .spawn();
        if result.is_err() {
            std::process::Command::new("xdg-open")
                .arg(&dir)
                .spawn()
                .map_err(|e| format!("Failed to open file manager: {}", e))?;
        }
        return Ok(());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err("Unsupported platform".to_string())
    }
}
