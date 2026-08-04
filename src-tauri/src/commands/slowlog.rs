use crate::core::slowlog::{SlowlogCollector, SlowlogInfo};
use crate::core::validate::validate_connection_id;
use crate::AppState;
use tauri::State;

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
