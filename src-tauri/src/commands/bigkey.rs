use crate::core::bigkey::{BigKeyAnalyzer, BigKeyBatch, MemoryStatItem};
use crate::core::validate::{validate_connection_id, validate_pattern, validate_scan_count};
use crate::AppState;
use tauri::State;

/// Scan one batch of keys and enrich them with memory / element metrics.
/// Follows the same cursor protocol as `scan_keys` (cluster scans use the
/// synthetic cursor managed by `ClusterScanManager`).
#[tauri::command]
pub async fn scan_big_keys(
    state: State<'_, AppState>,
    connection_id: String,
    pattern: String,
    cursor: u64,
    count: u64,
) -> Result<BigKeyBatch, String> {
    validate_connection_id(&connection_id)?;
    validate_pattern(&pattern)?;
    validate_scan_count(count)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    let (next_cursor, keys) = if let Some(cluster) = conn.as_cluster() {
        state
            .cluster_scans
            .scan_step(&connection_id, cluster, cursor, &pattern, count)
            .await?
    } else {
        let scan_val: redis::Value = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(count)
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("SCAN error: {}", e))?;

        let elements = match scan_val {
            redis::Value::Array(items) if items.len() == 2 => items,
            _ => return Err(format!("Unexpected SCAN response format: {:?}", scan_val)),
        };

        let next_cursor: u64 = redis::from_redis_value(&elements[0])
            .map_err(|e| format!("Failed to parse SCAN cursor: {}", e))?;
        let keys: Vec<String> = redis::from_redis_value(&elements[1])
            .map_err(|e| format!("Failed to parse SCAN keys: {}", e))?;
        (next_cursor, keys)
    };

    BigKeyAnalyzer::new()
        .enrich_batch(&mut conn, next_cursor, keys)
        .await
}

/// MEMORY STATS as a flat (dotted) name/value list, aggregated for clusters.
#[tauri::command]
pub async fn memory_stats(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<MemoryStatItem>, String> {
    validate_connection_id(&connection_id)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    BigKeyAnalyzer::new().memory_stats(&mut conn).await
}

/// MEMORY DOCTOR advice text (empty when unsupported).
#[tauri::command]
pub async fn memory_doctor(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<String, String> {
    validate_connection_id(&connection_id)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    BigKeyAnalyzer::new().memory_doctor(&mut conn).await
}
