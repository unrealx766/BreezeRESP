use crate::core::server_admin::{
    client_kill as core_client_kill, client_list as core_client_list, cluster_topology,
    config_get as core_config_get, config_set as core_config_set, fetch_info,
    object_freq as core_object_freq, ClientInfo, ClusterTopology, InfoNode, KeyFreq,
};
use crate::core::validate::{
    reject_null_bytes, validate_connection_id, validate_key, validate_non_empty, validate_pattern,
};
use crate::AppState;
use tauri::State;

/// Maximum number of keys accepted by object_freq in one call.
const MAX_FREQ_KEYS: usize = 500;

/// Maximum length for a CONFIG parameter name / value.
const MAX_CONFIG_LEN: usize = 65_536;

/// Raw INFO text per node (standalone returns a single "server" entry).
#[tauri::command]
pub async fn get_info(
    state: State<'_, AppState>,
    connection_id: String,
    section: Option<String>,
) -> Result<Vec<InfoNode>, String> {
    validate_connection_id(&connection_id)?;
    if let Some(section) = &section {
        validate_non_empty(section, "section", 64)?;
        reject_null_bytes(section, "section")?;
    }

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    fetch_info(&mut conn, section.as_deref()).await
}

/// CONFIG GET as [name, value] pairs (merged across cluster masters).
#[tauri::command]
pub async fn config_get(
    state: State<'_, AppState>,
    connection_id: String,
    pattern: String,
) -> Result<Vec<(String, String)>, String> {
    validate_connection_id(&connection_id)?;
    validate_pattern(&pattern)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    core_config_get(&mut conn, &pattern).await
}

/// CONFIG SET applied to the whole deployment (all cluster nodes).
/// Host-level dangerous parameters (dir/dbfilename/logfile) are rejected.
#[tauri::command]
pub async fn config_set(
    state: State<'_, AppState>,
    connection_id: String,
    param: String,
    value: String,
) -> Result<(), String> {
    validate_connection_id(&connection_id)?;
    validate_non_empty(&param, "param", 128)?;
    reject_null_bytes(&param, "param")?;
    reject_null_bytes(&value, "value")?;
    if value.len() > MAX_CONFIG_LEN {
        return Err("config value exceeds maximum length".to_string());
    }

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    core_config_set(&mut conn, &param, &value).await
}

/// Structured CLIENT LIST rows (merged from every cluster node).
#[tauri::command]
pub async fn client_list(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<ClientInfo>, String> {
    validate_connection_id(&connection_id)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    core_client_list(&mut conn).await
}

/// CLIENT KILL ID <id>; broadcast to every cluster node.
#[tauri::command]
pub async fn client_kill(
    state: State<'_, AppState>,
    connection_id: String,
    client_id: u64,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    core_client_kill(&mut conn, client_id).await
}

/// Batched OBJECT FREQ lookup for hot-key detection (LFU policies only).
#[tauri::command]
pub async fn object_freq(
    state: State<'_, AppState>,
    connection_id: String,
    keys: Vec<String>,
) -> Result<Vec<KeyFreq>, String> {
    validate_connection_id(&connection_id)?;
    if keys.len() > MAX_FREQ_KEYS {
        return Err(format!("Too many keys (max {})", MAX_FREQ_KEYS));
    }
    for key in &keys {
        validate_key(key)?;
    }

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    core_object_freq(&mut conn, &keys).await
}

/// Cluster topology: nodes, roles, slot ranges and per-node metrics.
/// Standalone connections return `clusterEnabled: false`.
#[tauri::command]
pub async fn get_cluster_topology(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<ClusterTopology, String> {
    validate_connection_id(&connection_id)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    cluster_topology(&mut conn).await
}
