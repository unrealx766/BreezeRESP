use crate::AppState;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub change_type: String, // "added", "modified", "deleted"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPreview {
    pub command: String,
    pub diff: Vec<DiffEntry>,
    pub snapshot_id: String,
}

/// Get the current string representation of a key's value
async fn get_key_value_string(
    conn: &mut deadpool_redis::Connection,
    key: &str,
) -> Option<String> {
    let type_str: String = redis::cmd("TYPE")
        .arg(key)
        .query_async(&mut **conn)
        .await
        .ok()?;

    if type_str == "none" {
        return None;
    }

    match type_str.as_str() {
        "string" => {
            let val: String = conn.get(key).await.ok()?;
            Some(val)
        }
        "hash" => {
            let fields: Vec<(String, String)> = conn.hgetall(key).await.ok()?;
            Some(serde_json::to_string(&fields).unwrap_or_default())
        }
        "list" => {
            let items: Vec<String> = conn.lrange(key, 0, -1).await.ok()?;
            Some(serde_json::to_string(&items).unwrap_or_default())
        }
        "set" => {
            let members: Vec<String> = conn.smembers(key).await.ok()?;
            Some(serde_json::to_string(&members).unwrap_or_default())
        }
        "zset" => {
            let members: Vec<(String, f64)> = redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .arg("WITHSCORES")
                .query_async(&mut **conn)
                .await
                .ok()?;
            Some(serde_json::to_string(&members).unwrap_or_default())
        }
        _ => Some("(unsupported type)".to_string()),
    }
}

/// Extract the key(s) affected by a Redis command
fn extract_keys(parts: &[&str]) -> Vec<String> {
    if parts.len() < 2 {
        return vec![];
    }
    let cmd = parts[0].to_uppercase();
    match cmd.as_str() {
        "SET" | "GET" | "DEL" | "EXPIRE" | "PERSIST" | "RENAME" | "TTL" | "TYPE"
        | "APPEND" | "INCR" | "DECR" | "SETNX" | "GETSET" | "SETEX" | "PSETEX" => {
            vec![parts[1].to_string()]
        }
        "HSET" | "HGET" | "HDEL" | "HGETALL" | "HMSET" | "HINCRBY" | "HLEN" => {
            vec![parts[1].to_string()]
        }
        "LPUSH" | "RPUSH" | "LPOP" | "RPOP" | "LRANGE" | "LLEN" | "LINDEX" => {
            vec![parts[1].to_string()]
        }
        "SADD" | "SREM" | "SMEMBERS" | "SCARD" | "SISMEMBER" => {
            vec![parts[1].to_string()]
        }
        "ZADD" | "ZREM" | "ZRANGE" | "ZCARD" | "ZSCORE" | "ZRANK" => {
            vec![parts[1].to_string()]
        }
        "MSET" => {
            // MSET key value key value ...
            parts[1..].iter().step_by(2).map(|s| s.to_string()).collect()
        }
        "MGET" => {
            // MGET key key key ...
            parts[1..].iter().map(|s| s.to_string()).collect()
        }
        _ => {
            // Default: assume second token is key
            vec![parts[1].to_string()]
        }
    }
}

/// Execute a command in sandbox mode and return the diff preview
#[tauri::command]
pub async fn sandbox_preview(
    state: State<'_, AppState>,
    connection_id: String,
    command: String,
) -> Result<SandboxPreview, String> {
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    // Parse command
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Err("Empty command".to_string());
    }
    let affected_keys = extract_keys(&parts);

    // Step 1: Capture before state
    let mut before_state = HashMap::new();
    for key in &affected_keys {
        if let Some(val) = get_key_value_string(&mut conn, key).await {
            before_state.insert(key.clone(), val);
        }
    }

    // Step 2: Create snapshot
    let snap_id = uuid::Uuid::new_v4().to_string();
    {
        let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
        let mut ss = ss;
        ss.create_snapshot(&snap_id, &command, before_state.clone());
    }

    // Step 3: Execute command
    let cmd_name = parts[0];
    let cmd_args = &parts[1..];
    let mut redis_cmd = redis::cmd(cmd_name);
    for arg in cmd_args {
        redis_cmd.arg(*arg);
    }
    let _: redis::Value = redis_cmd
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("Command error: {}", e))?;

    // Step 4: Capture after state
    let mut after_state = HashMap::new();
    for key in &affected_keys {
        if let Some(val) = get_key_value_string(&mut conn, key).await {
            after_state.insert(key.clone(), val);
        }
    }

    // Step 5: Set after state and compute diff
    let diff_result = {
        let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
        let mut ss = ss;
        ss.set_after_state(&snap_id, after_state);
        ss.compute_diff(&snap_id)
    };

    // Step 6: Rollback - restore original state
    for key in &affected_keys {
        match before_state.get(key) {
            Some(val) => {
                // Restore the original value (best effort - works for string type)
                let _: Result<(), _> = conn.set(key, val).await;
            }
            None => {
                // Key didn't exist before, delete it
                let _: Result<i64, _> = redis::cmd("DEL")
                    .arg(key)
                    .query_async(&mut *conn)
                    .await;
            }
        }
    }

    // Step 7: Keep snapshot for later apply/rollback (don't clean up)

    // Convert DiffResult to Vec<DiffEntry>
    let mut diff = Vec::new();
    if let Some(dr) = diff_result {
        for (key, val) in dr.added {
            diff.push(DiffEntry {
                path: key,
                before: None,
                after: Some(val),
                change_type: "added".to_string(),
            });
        }
        for (key, before, after) in dr.modified {
            diff.push(DiffEntry {
                path: key,
                before: Some(before),
                after: Some(after),
                change_type: "modified".to_string(),
            });
        }
        for (key, val) in dr.deleted {
            diff.push(DiffEntry {
                path: key,
                before: Some(val),
                after: None,
                change_type: "deleted".to_string(),
            });
        }
    }

    Ok(SandboxPreview { command, diff, snapshot_id: snap_id })
}

/// Apply a sandboxed change to the actual Redis instance
#[tauri::command]
pub async fn sandbox_apply(
    state: State<'_, AppState>,
    connection_id: String,
    command: String,
) -> Result<bool, String> {
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Err("Empty command".to_string());
    }

    let mut redis_cmd = redis::cmd(parts[0]);
    for arg in &parts[1..] {
        redis_cmd.arg(*arg);
    }
    let _: redis::Value = redis_cmd
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("Apply error: {}", e))?;

    Ok(true)
}

/// Rollback a sandboxed change by restoring the before-state
#[tauri::command]
pub async fn sandbox_rollback(
    state: State<'_, AppState>,
    connection_id: String,
    before_state: HashMap<String, String>,
    added_keys: Vec<String>,
) -> Result<bool, String> {
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    // Restore each key to its before-state
    for (key, val) in &before_state {
        let _: Result<(), _> = redis::cmd("SET")
            .arg(key)
            .arg(val)
            .query_async(&mut *conn)
            .await;
    }

    // Delete keys that were added by the command (didn't exist before)
    for key in &added_keys {
        let _: Result<i64, _> = redis::cmd("DEL")
            .arg(key)
            .query_async(&mut *conn)
            .await;
    }

    Ok(true)
}
