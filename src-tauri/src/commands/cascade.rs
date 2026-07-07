use crate::AppState;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisKeyInfo {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDetail {
    pub key: RedisKeyInfo,
    pub value: serde_json::Value,
    pub encoding: String,
}

/// Scan keys with pattern and count
#[tauri::command]
pub async fn scan_keys(
    state: State<'_, AppState>,
    connection_id: String,
    pattern: String,
    cursor: u64,
    count: u64,
) -> Result<(u64, Vec<RedisKeyInfo>), String> {
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    // SCAN to get keys - parse as raw Value for robust handling
    let scan_val: redis::Value = redis::cmd("SCAN")
        .arg(cursor)
        .arg("MATCH")
        .arg(&pattern)
        .arg("COUNT")
        .arg(count)
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("SCAN error: {}", e))?;

    // Parse SCAN response: [cursor, [keys...]]
    let elements = match scan_val {
        redis::Value::Array(items) if items.len() == 2 => items,
        _ => return Err(format!("Unexpected SCAN response format: {:?}", scan_val)),
    };

    let next_cursor: u64 = redis::from_redis_value(&elements[0])
        .map_err(|e| format!("Failed to parse SCAN cursor: {}", e))?;
    let keys: Vec<String> = redis::from_redis_value(&elements[1])
        .map_err(|e| format!("Failed to parse SCAN keys: {}", e))?;

    let mut result = Vec::with_capacity(keys.len());

    if keys.is_empty() {
        return Ok((next_cursor, result));
    }

    // Pipeline: batch TYPE + TTL + MEMORY USAGE for all keys (3 round-trips total)
    let mut pipe = redis::pipe();
    for key in &keys {
        pipe.cmd("TYPE").arg(key);
    }
    for key in &keys {
        pipe.cmd("TTL").arg(key);
    }
    for key in &keys {
        pipe.cmd("MEMORY").arg("USAGE").arg(key);
    }

    let n = keys.len();
    let expected = 3 * n;
    let values: Vec<redis::Value> = pipe
        .query_async(&mut *conn)
        .await
        .unwrap_or_default();

    // Fallback: if pipeline returned wrong count, query per-key
    if values.len() != expected {
        result.clear();
        for key in &keys {
            let type_str: String = redis::cmd("TYPE")
                .arg(key)
                .query_async(&mut *conn)
                .await
                .unwrap_or_else(|_| "none".to_string());
            let ttl: i64 = conn.ttl(key).await.unwrap_or(-1);
            let size: u64 = redis::cmd("MEMORY")
                .arg("USAGE")
                .arg(key)
                .query_async(&mut *conn)
                .await
                .unwrap_or(0);
            result.push(RedisKeyInfo {
                key: key.clone(),
                key_type: type_str,
                ttl,
                size,
            });
        }
        return Ok((next_cursor, result));
    }

    for (i, key) in keys.iter().enumerate() {
        let type_str = redis::from_redis_value::<String>(&values[i])
            .unwrap_or_else(|_| "none".to_string());

        let ttl: i64 = redis::from_redis_value::<i64>(&values[n + i])
            .unwrap_or(-1);

        let size: u64 = redis::from_redis_value::<u64>(&values[2 * n + i])
            .unwrap_or(0);

        result.push(RedisKeyInfo {
            key: key.clone(),
            key_type: type_str,
            ttl,
            size,
        });
    }

    Ok((next_cursor, result))
}

/// Get detailed value of a specific key
#[tauri::command]
pub async fn get_key_detail(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
) -> Result<KeyDetail, String> {
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    // Get type
    let type_str: String = redis::cmd("TYPE")
        .arg(&key)
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("TYPE error: {}", e))?;

    // Get TTL
    let ttl: i64 = conn.ttl(&key).await.unwrap_or(-1);

    // Get encoding
    let encoding: String = redis::cmd("OBJECT")
        .arg("ENCODING")
        .arg(&key)
        .query_async(&mut *conn)
        .await
        .unwrap_or_else(|_| "unknown".to_string());

    // Get size
    let size: u64 = redis::cmd("MEMORY")
        .arg("USAGE")
        .arg(&key)
        .query_async(&mut *conn)
        .await
        .unwrap_or(0);

    let key_info = RedisKeyInfo {
        key: key.clone(),
        key_type: type_str.clone(),
        ttl,
        size,
    };

    // Fetch value based on type
    let value = match type_str.as_str() {
        "string" => {
            let val: String = conn
                .get(&key)
                .await
                .map_err(|e| format!("GET error: {}", e))?;
            serde_json::json!({ "type": "string", "value": val, "encoding": encoding })
        }
        "hash" => {
            let fields: Vec<(String, String)> = conn
                .hgetall(&key)
                .await
                .map_err(|e| format!("HGETALL error: {}", e))?;
            let fields_json: Vec<serde_json::Value> = fields
                .into_iter()
                .map(|(f, v)| serde_json::json!({ "field": f, "value": v }))
                .collect();
            serde_json::json!({ "type": "hash", "fields": fields_json, "encoding": encoding })
        }
        "list" => {
            let items: Vec<String> = conn
                .lrange(&key, 0, -1)
                .await
                .map_err(|e| format!("LRANGE error: {}", e))?;
            serde_json::json!({ "type": "list", "items": items, "encoding": encoding })
        }
        "set" => {
            let members: Vec<String> = conn
                .smembers(&key)
                .await
                .map_err(|e| format!("SMEMBERS error: {}", e))?;
            serde_json::json!({ "type": "set", "members": members, "encoding": encoding })
        }
        "zset" => {
            let members: Vec<(String, f64)> = redis::cmd("ZRANGE")
                .arg(&key)
                .arg(0)
                .arg(-1)
                .arg("WITHSCORES")
                .query_async(&mut *conn)
                .await
                .map_err(|e| format!("ZRANGE error: {}", e))?;
            let members_json: Vec<serde_json::Value> = members
                .into_iter()
                .map(|(m, s)| serde_json::json!({ "member": m, "score": s }))
                .collect();
            serde_json::json!({ "type": "zset", "members": members_json, "encoding": encoding })
        }
        _ => {
            return Err(format!("Unsupported type: {}", type_str));
        }
    };

    Ok(KeyDetail {
        key: key_info,
        value,
        encoding,
    })
}

/// Delete a key
#[tauri::command]
pub async fn delete_key(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
) -> Result<bool, String> {
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    let deleted: i64 = redis::cmd("DEL")
        .arg(&key)
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("DEL error: {}", e))?;
    Ok(deleted > 0)
}

/// Set TTL on a key
#[tauri::command]
pub async fn set_key_ttl(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    ttl: i64,
) -> Result<bool, String> {
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    if ttl > 0 {
        let result: bool = redis::cmd("EXPIRE")
            .arg(&key)
            .arg(ttl)
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("EXPIRE error: {}", e))?;
        Ok(result)
    } else if ttl == -1 {
        let result: bool = redis::cmd("PERSIST")
            .arg(&key)
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("PERSIST error: {}", e))?;
        Ok(result)
    } else {
        Err("Invalid TTL value".to_string())
    }
}

/// Rename a key
#[tauri::command]
pub async fn rename_key(
    state: State<'_, AppState>,
    connection_id: String,
    old_key: String,
    new_key: String,
) -> Result<bool, String> {
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    let _: () = redis::cmd("RENAME")
        .arg(&old_key)
        .arg(&new_key)
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("RENAME error: {}", e))?;
    Ok(true)
}
