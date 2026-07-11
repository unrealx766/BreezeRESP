use crate::core::validate::{validate_command, validate_connection_id};
use crate::AppState;
use crate::core::format::{format_redis_value, format_for_display, get_key_value_string};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
    pub path: String,
    pub key_type: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub change_type: String, // "added", "modified", "deleted", "unchanged"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPreview {
    pub command: String,
    pub diff: Vec<DiffEntry>,
    pub command_result: Option<String>,
    pub snapshot_id: String,
}

/// Get the Redis type of a key (string, hash, list, set, zset, or "none" if missing)
async fn get_key_type(
    conn: &mut deadpool_redis::Connection,
    key: &str,
) -> String {
    redis::cmd("TYPE")
        .arg(key)
        .query_async::<String>(&mut **conn)
        .await
        .unwrap_or_else(|_| "none".to_string())
}

/// Read-only commands that don't modify data — show result directly
const READ_ONLY_COMMANDS: &[&str] = &[
    "GET", "MGET", "TYPE", "TTL", "PTTL", "EXISTS", "STRLEN",
    "HGET", "HGETALL", "HMGET", "HLEN", "HEXISTS", "HKEYS", "HVALS",
    "LRANGE", "LLEN", "LINDEX",
    "SMEMBERS", "SCARD", "SISMEMBER", "SRANDMEMBER",
    "ZRANGE", "ZCARD", "ZSCORE", "ZRANK", "ZRANGEBYSCORE",
    "KEYS", "SCAN", "DBSIZE", "INFO", "PING", "ECHO",
];

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
    validate_connection_id(&connection_id)?;
    validate_command(&command)?;

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
    let cmd_name_upper = parts[0].to_uppercase();

    // --- Read-only commands: execute directly and return result ---
    if READ_ONLY_COMMANDS.contains(&cmd_name_upper.as_str()) {
        let mut redis_cmd = redis::cmd(parts[0]);
        for arg in &parts[1..] {
            redis_cmd.arg(*arg);
        }
        let result: redis::Value = redis_cmd
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("Command error: {}", e))?;

        let formatted = format_redis_value(&result);
        return Ok(SandboxPreview {
            command,
            diff: vec![],
            command_result: Some(formatted),
            snapshot_id: String::new(),
        });
    }

    // --- Write commands: capture before/after, diff, rollback ---
    let affected_keys = extract_keys(&parts);

    // Step 1: Capture before state + key types
    let mut before_state = HashMap::new();
    let mut key_types: HashMap<String, String> = HashMap::new();
    for key in &affected_keys {
        let kt = get_key_type(&mut conn, key).await;
        key_types.insert(key.clone(), kt.clone());
        if kt != "none"
            && let Some(val) = get_key_value_string(&mut conn, key).await
        {
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

    // Step 4: Capture after state + update key types
    let mut after_state = HashMap::new();
    for key in &affected_keys {
        let kt = get_key_type(&mut conn, key).await;
        // Update key_type to reflect after-execution state
        key_types.insert(key.clone(), kt.clone());
        if kt != "none"
            && let Some(val) = get_key_value_string(&mut conn, key).await
        {
            after_state.insert(key.clone(), val);
        }
    }

    // Step 5: Set after state and compute diff
    let diff_result = {
        let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
        let mut ss = ss;
        ss.set_after_state(&snap_id, after_state);
        ss.compute_diff_with_unchanged(&snap_id)
    };

    // Step 6: Rollback - restore original state
    for key in &affected_keys {
        match before_state.get(key) {
            Some(val) => {
                let _: Result<(), _> = conn.set(key, val).await;
            }
            None => {
                let _: Result<i64, _> = redis::cmd("DEL")
                    .arg(key)
                    .query_async(&mut *conn)
                    .await;
            }
        }
    }

    // Convert DiffResult to Vec<DiffEntry> (with key_type and display formatting)
    let mut diff = Vec::new();
    if let Some(dr) = diff_result {
        for (key, val) in dr.added {
            let kt = key_types.get(&key).cloned().unwrap_or_default();
            diff.push(DiffEntry {
                path: key.clone(),
                key_type: Some(kt.clone()),
                before: None,
                after: Some(format_for_display(&val, &kt)),
                change_type: "added".to_string(),
            });
        }
        for (key, before, after) in dr.modified {
            let kt = key_types.get(&key).cloned().unwrap_or_default();
            diff.push(DiffEntry {
                path: key.clone(),
                key_type: Some(kt.clone()),
                before: Some(format_for_display(&before, &kt)),
                after: Some(format_for_display(&after, &kt)),
                change_type: "modified".to_string(),
            });
        }
        for (key, val) in dr.deleted {
            let kt = key_types.get(&key).cloned().unwrap_or_default();
            diff.push(DiffEntry {
                path: key.clone(),
                key_type: Some(kt.clone()),
                before: Some(format_for_display(&val, &kt)),
                after: None,
                change_type: "deleted".to_string(),
            });
        }
        for (key, val) in dr.unchanged {
            let kt = key_types.get(&key).cloned().unwrap_or_default();
            diff.push(DiffEntry {
                path: key.clone(),
                key_type: Some(kt.clone()),
                before: Some(format_for_display(&val, &kt)),
                after: Some(format_for_display(&val, &kt)),
                change_type: "unchanged".to_string(),
            });
        }
    }

    Ok(SandboxPreview { command, diff, command_result: None, snapshot_id: snap_id })
}

/// Apply a sandboxed change to the actual Redis instance
#[tauri::command]
pub async fn sandbox_apply(
    state: State<'_, AppState>,
    connection_id: String,
    command: String,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;
    validate_command(&command)?;

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
    validate_connection_id(&connection_id)?;

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
