use crate::core::validate::{validate_cluster_nodes, validate_connection_config, validate_connection_id};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::State;
use zeroize::Zeroize;

const CONN_TIMEOUT: Duration = Duration::from_secs(10);

/// Format a pool creation error, appending per-seed-node diagnostics when a
/// cluster connection fails to establish its initial connections. redis-rs
/// hides the underlying per-node errors behind a generic "Failed to create
/// initial connections" IoError, so we probe each seed node standalone to
/// expose the real cause (DNS resolution failure, unreachable node, auth
/// error, TLS failure, ...).
async fn cluster_diag_error(config: &ConnectionConfig, pw: Option<&str>, err: &impl ToString) -> String {
    let msg = format!("Pool get error: {}", err.to_string());
    if config.cluster && msg.contains("initial connections") {
        let diag = crate::core::pool::diagnose_cluster_seeds(
            &config.host,
            config.port,
            pw,
            config.ssl,
            &config.nodes,
        )
        .await;
        format!("{} | {}", msg, diag)
    } else {
        msg
    }
}

/// Format a verification PING timeout. For cluster connections this usually
/// means the topology advertises nodes the client cannot reach: after
/// topology discovery redis-rs only talks to the advertised nodes, and when
/// none of them are usable the request queues forever instead of failing.
async fn cluster_ping_timeout_error(config: &ConnectionConfig, pw: Option<&str>) -> String {
    if config.cluster {
        let diag = crate::core::pool::diagnose_cluster_seeds(
            &config.host,
            config.port,
            pw,
            config.ssl,
            &config.nodes,
        )
        .await;
        format!("PING timed out | {}", diag)
    } else {
        "PING timed out".to_string()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub password: String,
    pub db: u8,
    pub ssl: bool,
    #[serde(default)]
    pub pinned: bool,
    /// Redis Cluster mode (multi-DB is unavailable; db is always 0)
    #[serde(default)]
    pub cluster: bool,
    /// Extra cluster seed nodes (`host:port`), beyond the primary host/port
    #[serde(default)]
    pub nodes: Vec<String>,
    /// When true and password is empty, look up the saved password from config_store
    #[serde(default)]
    pub use_saved_password: Option<bool>,
    /// When true, preserve the existing password (don't overwrite with empty)
    #[serde(default)]
    pub keep_password: Option<bool>,
}

impl std::fmt::Debug for ConnectionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionConfig")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("password", &"[REDACTED]")
            .field("db", &self.db)
            .field("ssl", &self.ssl)
            .field("pinned", &self.pinned)
            .field("cluster", &self.cluster)
            .field("nodes", &self.nodes)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub db: u8,
    pub ssl: bool,
    pub status: String,
    #[serde(default)]
    pub pinned: bool,
    /// Redis Cluster mode
    #[serde(default)]
    pub cluster: bool,
    /// Extra cluster seed nodes (`host:port`)
    #[serde(default)]
    pub nodes: Vec<String>,
    /// Whether this connection has a password stored (for UI display)
    pub has_password: bool,
}

/// Connect to a Redis instance
#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<ConnectionInfo, String> {
    validate_connection_id(&config.id)?;
    validate_connection_config(&config.host, config.port, &config.name, &config.password)?;
    validate_cluster_nodes(&config.nodes)?;

    // Resolve password: if keep_password is set and password is empty, look up from config_store
    let resolved_password = if config.password.is_empty() && config.keep_password.unwrap_or(false) {
        let cs = state.config_store.lock().map_err(|e| e.to_string())?;
        let connections = cs.load()?;
        connections
            .iter()
            .find(|c| c.id == config.id)
            .map(|c| c.password.clone())
            .unwrap_or_default()
    } else {
        config.password.clone()
    };

    // Remove stale pool if exists (config may have changed)
    {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        let _ = pm.remove(&config.id);
    }

    // Create or get pool
    let pw = if resolved_password.is_empty() {
        None
    } else {
        Some(resolved_password.as_str())
    };
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_or_create(
            &config.id,
            &config.host,
            config.port,
            pw,
            config.db,
            config.ssl,
            config.cluster,
            &config.nodes,
        )?
    };

    // Capture has_password before the password is cleared
    let has_password = !resolved_password.is_empty();

    // Get a connection from the pool (with timeout to prevent TLS hang)
    let mut conn = match tokio::time::timeout(CONN_TIMEOUT, pool.get()).await {
        Err(_) => {
            return Err("Connection timed out. If using SSL/TLS, ensure the server supports TLS.".to_string())
        }
        Ok(Ok(conn)) => conn,
        Ok(Err(e)) => return Err(cluster_diag_error(&config, pw, &e).await),
    };

    // Verify the connection with PING (with timeout to prevent TLS hang)
    let _: String = match tokio::time::timeout(CONN_TIMEOUT, redis::cmd("PING").query_async(&mut conn)).await {
        Err(_) => return Err(cluster_ping_timeout_error(&config, pw).await),
        Ok(Err(e)) => return Err(format!("PING failed: {}", e)),
        Ok(Ok(v)) => v,
    };

    // Zeroize password from memory once it is no longer needed
    let mut resolved_password = resolved_password;
    resolved_password.zeroize();

    Ok(ConnectionInfo {
        id: config.id,
        name: config.name,
        host: config.host,
        port: config.port,
        db: if config.cluster { 0 } else { config.db },
        ssl: config.ssl,
        status: "connected".to_string(),
        pinned: config.pinned,
        cluster: config.cluster,
        nodes: config.nodes,
        has_password,
    })
}

/// Disconnect from a Redis instance
#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, id: String) -> Result<(), String> {
    validate_connection_id(&id)?;
    let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
    pm.remove(&id)?;
    drop(pm);

    // Tear down any active pubsub listener for this connection
    state.pubsub_manager.clear(&id);

    // Drop any in-flight cluster scan state
    state.cluster_scans.clear(&id);

    // Drop cached server capability profile
    crate::core::capability::clear_cached(&id);

    // Clear any pending sandbox state to prevent stale data leaking across connections
    let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
    let mut ss = ss;
    ss.clear_pending();

    Ok(())
}

/// Test a Redis connection without persisting it
#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<bool, String> {
    validate_connection_id(&config.id)?;
    validate_connection_config(&config.host, config.port, &config.name, &config.password)?;
    validate_cluster_nodes(&config.nodes)?;

    let test_id = format!("__test_{}", config.id);

    // Resolve password: if use_saved_password is set and password is empty, look up from config_store
    let resolved_password = if config.password.is_empty() && config.use_saved_password.unwrap_or(false) {
        let cs = state.config_store.lock().map_err(|e| e.to_string())?;
        let connections = cs.load()?;
        connections
            .iter()
            .find(|c| c.id == config.id)
            .map(|c| c.password.clone())
            .unwrap_or_default()
    } else {
        config.password.clone()
    };

    let pw = if resolved_password.is_empty() {
        None
    } else {
        Some(resolved_password.as_str())
    };

    // Create temp pool
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_or_create(
            &test_id,
            &config.host,
            config.port,
            pw,
            config.db,
            config.ssl,
            config.cluster,
            &config.nodes,
        )?
    };

    // Test with PING (with timeout to prevent TLS hang)
    let result = async {
        let mut conn = match tokio::time::timeout(CONN_TIMEOUT, pool.get()).await {
            Err(_) => {
                return Err("Connection timed out. If using SSL/TLS, ensure the server supports TLS.".to_string())
            }
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => return Err(cluster_diag_error(&config, pw, &e).await),
        };
        let _: String = match tokio::time::timeout(CONN_TIMEOUT, redis::cmd("PING").query_async(&mut conn)).await {
            Err(_) => return Err(cluster_ping_timeout_error(&config, pw).await),
            Ok(Err(e)) => return Err(format!("PING failed: {}", e)),
            Ok(Ok(v)) => v,
        };
        Ok::<bool, String>(true)
    }
    .await;

    // Clear password from memory after the test
    let mut resolved_password = resolved_password;
    resolved_password.zeroize();

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
            pinned: c.pinned,
            cluster: c.cluster,
            nodes: c.nodes,
            has_password: !c.password.is_empty(),
        })
        .collect())
}

/// Save a connection configuration
#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<(), String> {
    validate_connection_id(&config.id)?;
    validate_connection_config(&config.host, config.port, &config.name, &config.password)?;
    validate_cluster_nodes(&config.nodes)?;

    let cs = state.config_store.lock().map_err(|e| e.to_string())?;
    let mut connections = cs.load()?;

    // Resolve password: if keep_password is set, preserve existing password
    let password = if config.keep_password.unwrap_or(false) {
        if let Some(existing) = connections.iter().find(|c| c.id == config.id) {
            existing.password.clone()
        } else {
            config.password.clone()
        }
    } else {
        config.password.clone()
    };

    // Update existing or append
    let stored = crate::core::config_store::StoredConnection {
        id: config.id.clone(),
        name: config.name,
        host: config.host,
        port: config.port,
        password,
        db: if config.cluster { 0 } else { config.db },
        ssl: config.ssl,
        pinned: config.pinned,
        cluster: config.cluster,
        nodes: config.nodes,
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
    validate_connection_id(&id)?;

    // Get current connection info from config store
    let (host, port, password, ssl) = {
        let cs = state.config_store.lock().map_err(|e| e.to_string())?;
        let connections = cs.load()?;
        let conn = connections
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| format!("Connection not found: {}", id))?;
        if conn.cluster {
            return Err("Cluster mode does not support database switching".to_string());
        }
        (conn.host.clone(), conn.port, conn.password.clone(), conn.ssl)
    };

    // Remove old pool
    {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        let _ = pm.remove(&id);
    }

    // Active subscriptions were bound to the previous DB; tear them down.
    state.pubsub_manager.clear(&id);

    // Create new pool with the new DB
    let pw = if password.is_empty() { None } else { Some(password.as_str()) };
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_or_create(&id, &host, port, pw, db, ssl, false, &[])?
    };

    // Zeroize password from memory after pool creation
    let mut password = password;
    password.zeroize();

    // Verify with PING (with timeout to prevent TLS hang)
    let mut conn = tokio::time::timeout(CONN_TIMEOUT, pool.get())
        .await
        .map_err(|_| "Connection timed out. If using SSL/TLS, ensure the server supports TLS.".to_string())?
        .map_err(|e| format!("Pool get error: {}", e))?;
    let _: String = tokio::time::timeout(CONN_TIMEOUT, redis::cmd("PING").query_async(&mut conn))
        .await
        .map_err(|_| "PING timed out".to_string())?
        .map_err(|e| format!("PING failed: {}", e))?;

    Ok(())
}

/// Delete a saved connection
#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<(), String> {
    validate_connection_id(&id)?;

    // Also disconnect if connected
    {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        let _ = pm.remove(&id);
    }

    // Tear down any active pubsub listener for this connection
    state.pubsub_manager.clear(&id);

    // Drop any in-flight cluster scan state
    state.cluster_scans.clear(&id);

    // Drop cached server capability profile
    crate::core::capability::clear_cached(&id);

    // Clear any pending sandbox state
    {
        let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
        let mut ss = ss;
        ss.clear_pending();
    }

    let cs = state.config_store.lock().map_err(|e| e.to_string())?;
    let mut connections = cs.load()?;
    connections.retain(|c| c.id != id);
    cs.save(&connections)
}
