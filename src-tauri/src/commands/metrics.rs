use crate::core::metrics::{MetricsCollector, ServerMetrics};
use crate::core::validate::validate_connection_id;
use crate::AppState;
use tauri::State;

/// Get real-time metrics from a Redis instance
#[tauri::command]
pub async fn get_metrics(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<ServerMetrics, String> {
    validate_connection_id(&connection_id)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    let collector = MetricsCollector::new();
    collector.collect(&mut *conn).await
}
