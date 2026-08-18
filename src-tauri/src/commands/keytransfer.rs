use crate::core::keytransfer::{
    export_keys as core_export_keys, import_keys as core_import_keys, inspect_export_doc,
    read_export_file, ExportResult, ImportResult,
};
use crate::core::validate::{
    reject_null_bytes, validate_connection_id, validate_key, validate_pattern,
};
use crate::AppState;
use tauri::State;

/// Maximum number of keys accepted by an explicit export call.
const MAX_EXPORT_KEYS: usize = 1000;
/// Default / hard caps for pattern-based exports.
const DEFAULT_PATTERN_LIMIT: usize = 1000;
const MAX_PATTERN_LIMIT: usize = 10_000;
/// Safety bound on SCAN iterations while collecting keys by pattern.
const MAX_SCAN_ROUNDS: usize = 1_000_000;

/// Timestamp suffix for generated export file names.
fn export_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string()
}

/// Sanitize and write export content into the user's download directory.
fn save_export_file(content: &str, format: &str) -> Result<String, String> {
    let filename = format!("breezeresp-keys-{}-{}.json", format, export_timestamp());
    let safe_name: String = filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect();

    let download_dir = crate::commands::slowlog::dirs_download()
        .ok_or("Cannot find download directory")?;
    let file_path = download_dir.join(&safe_name);
    std::fs::write(&file_path, content).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(file_path.to_string_lossy().to_string())
}

/// Export the given keys to a file in the user's download directory.
/// `format`: "json" (readable) or "dump" (lossless RESTORE payload).
#[tauri::command]
pub async fn export_keys(
    state: State<'_, AppState>,
    connection_id: String,
    keys: Vec<String>,
    format: String,
) -> Result<ExportResult, String> {
    validate_connection_id(&connection_id)?;
    if keys.is_empty() {
        return Err("No keys to export".to_string());
    }
    if keys.len() > MAX_EXPORT_KEYS {
        return Err(format!("Too many keys to export (max {})", MAX_EXPORT_KEYS));
    }
    for key in &keys {
        validate_key(key)?;
    }

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    let (content, exported, warnings) =
        core_export_keys(&mut conn, &keys, &format, &connection_id).await?;
    let path = save_export_file(&content, &format)?;
    Ok(ExportResult {
        path,
        exported,
        warnings,
    })
}

/// Collect keys matching a pattern via SCAN and export them to a file.
#[tauri::command]
pub async fn export_keys_by_pattern(
    state: State<'_, AppState>,
    connection_id: String,
    pattern: String,
    format: String,
    limit: Option<usize>,
) -> Result<ExportResult, String> {
    validate_connection_id(&connection_id)?;
    validate_pattern(&pattern)?;
    let limit = limit
        .unwrap_or(DEFAULT_PATTERN_LIMIT)
        .clamp(1, MAX_PATTERN_LIMIT);

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    // Collect matching keys. NOTE: cluster scans share the per-connection
    // scan state with the data browser, so only one scan runs at a time.
    let is_cluster = pool.is_cluster();
    let mut keys: Vec<String> = Vec::new();
    let mut cursor: u64 = 0;
    let mut rounds = 0;

    loop {
        rounds += 1;
        if rounds > MAX_SCAN_ROUNDS {
            return Err("Scan exceeded maximum rounds".to_string());
        }

        let (next, batch) = if let Some(cluster) = conn.as_cluster() {
            state
                .cluster_scans
                .scan_step(&connection_id, cluster, cursor, &pattern, 1000)
                .await?
        } else {
            let scan_val: redis::Value = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(1000)
                .query_async(&mut conn)
                .await
                .map_err(|e| format!("SCAN error: {}", e))?;
            let elements = match scan_val {
                redis::Value::Array(items) if items.len() == 2 => items,
                _ => return Err(format!("Unexpected SCAN response format: {:?}", scan_val)),
            };
            let next_cursor: u64 = redis::from_redis_value(&elements[0])
                .map_err(|e| format!("Failed to parse SCAN cursor: {}", e))?;
            let batch: Vec<String> = redis::from_redis_value(&elements[1])
                .map_err(|e| format!("Failed to parse SCAN keys: {}", e))?;
            (next_cursor, batch)
        };

        keys.extend(batch);
        if keys.len() >= limit {
            keys.truncate(limit);
            break;
        }
        cursor = next;
        if next == 0 {
            break;
        }
        // Standalone SCAN already returned real cursors; cluster uses the
        // synthetic 1-continues / 0-done protocol from ClusterScanManager.
        if !is_cluster && cursor == 0 {
            break;
        }
    }

    if keys.is_empty() {
        return Err("No keys matched the pattern".to_string());
    }

    let (content, exported, warnings) =
        core_export_keys(&mut conn, &keys, &format, &connection_id).await?;
    let path = save_export_file(&content, &format)?;
    Ok(ExportResult {
        path,
        exported,
        warnings,
    })
}

/// Preview an export file: returns the detected format and entry count.
#[tauri::command]
pub async fn inspect_import_file(file_path: String) -> Result<(String, usize), String> {
    reject_null_bytes(&file_path, "file_path")?;
    let doc = read_export_file(&file_path)?;
    inspect_export_doc(&doc)
}

/// Import keys from a previously exported file.
/// `policy`: "skip" keeps existing keys, "replace" overwrites them.
#[tauri::command]
pub async fn import_keys(
    state: State<'_, AppState>,
    connection_id: String,
    file_path: String,
    policy: String,
) -> Result<ImportResult, String> {
    validate_connection_id(&connection_id)?;
    reject_null_bytes(&file_path, "file_path")?;

    let doc = read_export_file(&file_path)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    core_import_keys(&mut conn, &doc, &policy).await
}
