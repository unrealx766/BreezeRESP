use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub password: String,
    pub db: u8,
    pub ssl: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub db: u8,
    pub ssl: bool,
    pub status: String,
}

/// Connect to a Redis instance
#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<ConnectionInfo, String> {
    // Remove stale pool if exists (config may have changed)
    {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        let _ = pm.remove(&config.id);
    }

    // Create or get pool
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        let pw = if config.password.is_empty() {
            None
        } else {
            Some(config.password.as_str())
        };
        pm.get_or_create(&config.id, &config.host, config.port, pw, config.db)?
    };

    // Verify connection with PING
    let mut conn = pool.get().await.map_err(|e| format!("Pool get error: {}", e))?;
    let _: String = redis::cmd("PING")
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("PING failed: {}", e))?;

    Ok(ConnectionInfo {
        id: config.id,
        name: config.name,
        host: config.host,
        port: config.port,
        db: config.db,
        ssl: config.ssl,
        status: "connected".to_string(),
    })
}

/// Disconnect from a Redis instance
#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
    pm.remove(&id)
}

/// Test a Redis connection without persisting it
#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<bool, String> {
    let test_id = format!("__test_{}", config.id);
    let pw = if config.password.is_empty() {
        None
    } else {
        Some(config.password.as_str())
    };

    // Create temp pool
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_or_create(&test_id, &config.host, config.port, pw, config.db)?
    };

    // Test with PING
    let result = async {
        let mut conn = pool.get().await.map_err(|e| format!("Pool get error: {}", e))?;
        let _: String = redis::cmd("PING")
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("PING failed: {}", e))?;
        Ok::<bool, String>(true)
    }
    .await;

    // Remove temp pool
    {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        let _ = pm.remove(&test_id);
    }

    result
}

/// Get list of saved connections
#[tauri::command]
pub async fn get_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionInfo>, String> {
    let cs = state.config_store.lock().map_err(|e| e.to_string())?;
    let stored = cs.load()?;
    Ok(stored
        .into_iter()
        .map(|c| ConnectionInfo {
            id: c.id,
            name: c.name,
            host: c.host,
            port: c.port,
            db: c.db,
            ssl: c.ssl,
            status: "disconnected".to_string(),
        })
        .collect())
}

/// Save a connection configuration
#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<(), String> {
    let cs = state.config_store.lock().map_err(|e| e.to_string())?;
    let mut connections = cs.load()?;

    // Update existing or append
    let stored = crate::core::config_store::StoredConnection {
        id: config.id.clone(),
        name: config.name,
        host: config.host,
        port: config.port,
        password: config.password,
        db: config.db,
        ssl: config.ssl,
    };

    if let Some(idx) = connections.iter().position(|c| c.id == config.id) {
        connections[idx] = stored;
    } else {
        connections.push(stored);
    }

    cs.save(&connections)
}

/// Switch the database for an active connection
#[tauri::command]
pub async fn switch_db(
    state: State<'_, AppState>,
    id: String,
    db: u8,
) -> Result<(), String> {
    // Get current connection info from config store
    let (host, port, password) = {
        let cs = state.config_store.lock().map_err(|e| e.to_string())?;
        let connections = cs.load()?;
        let conn = connections
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| format!("Connection not found: {}", id))?;
        (conn.host.clone(), conn.port, conn.password.clone())
    };

    // Remove old pool
    {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        let _ = pm.remove(&id);
    }

    // Create new pool with the new DB
    let pw = if password.is_empty() { None } else { Some(password.as_str()) };
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_or_create(&id, &host, port, pw, db)?
    };

    // Verify with PING
    let mut conn = pool.get().await.map_err(|e| format!("Pool get error: {}", e))?;
    let _: String = redis::cmd("PING")
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("PING failed: {}", e))?;

    Ok(())
}

/// Delete a saved connection
#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<(), String> {
    // Also disconnect if connected
    {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        let _ = pm.remove(&id);
    }

    let cs = state.config_store.lock().map_err(|e| e.to_string())?;
    let mut connections = cs.load()?;
    connections.retain(|c| c.id != id);
    cs.save(&connections)
}
