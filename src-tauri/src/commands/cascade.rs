use crate::core::validate::{
    validate_connection_id, validate_key, validate_pattern, validate_scan_count, validate_ttl,
    reject_null_bytes,
};
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
    validate_connection_id(&connection_id)?;
    validate_pattern(&pattern)?;
    validate_scan_count(count)?;

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
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;

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
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;

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
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    validate_ttl(ttl)?;

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
    validate_connection_id(&connection_id)?;
    validate_key(&old_key)?;
    validate_key(&new_key)?;

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

/// Get the total number of keys in the current database
#[tauri::command]
pub async fn db_size(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<u64, String> {
    validate_connection_id(&connection_id)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;
    let size: u64 = redis::cmd("DBSIZE")
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("DBSIZE error: {}", e))?;
    Ok(size)
}

/// Set/update a value in Redis (supports all data types)
///
/// Actions:
///   - "set": set full value (string → SET, hash → HSET field, list → LSET index, set → SREM+SADD, zset → ZADD)
///   - "delete_field": remove a sub-element (HDEL, LREM 1, SREM, ZREM)
///   - "add_field": add a new sub-element (HSET new field, RPUSH, SADD, ZADD)
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn set_value(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    key_type: String,
    action: String,
    field: Option<String>,
    value: Option<String>,
    index: Option<i64>,
    score: Option<f64>,
    old_value: Option<String>,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;

    // Validate optional parameters for injection/null-byte attacks
    if let Some(ref f) = field {
        reject_null_bytes(f, "field")?;
    }
    if let Some(ref v) = value {
        reject_null_bytes(v, "value")?;
    }
    if let Some(ref ov) = old_value {
        reject_null_bytes(ov, "old_value")?;
    }

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    match action.as_str() {
        "set" => match key_type.as_str() {
            "string" => {
                let val = value.ok_or("value is required for string SET")?;
                let _: () = redis::cmd("SET")
                    .arg(&key)
                    .arg(&val)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("SET error: {}", e))?;
                Ok(true)
            }
            "hash" => {
                let f = field.ok_or("field is required for hash HSET")?;
                let val = value.ok_or("value is required for hash HSET")?;
                let _: () = redis::cmd("HSET")
                    .arg(&key)
                    .arg(&f)
                    .arg(&val)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("HSET error: {}", e))?;
                Ok(true)
            }
            "list" => {
                let idx = index.ok_or("index is required for list LSET")?;
                let val = value.ok_or("value is required for list LSET")?;
                let _: () = redis::cmd("LSET")
                    .arg(&key)
                    .arg(idx)
                    .arg(&val)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("LSET error: {}", e))?;
                Ok(true)
            }
            "set" => {
                let old = old_value.ok_or("old_value is required for set rename")?;
                let new_val = value.ok_or("value is required for set rename")?;
                // Remove old, add new
                let _: i64 = redis::cmd("SREM")
                    .arg(&key)
                    .arg(&old)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("SREM error: {}", e))?;
                let _: i64 = redis::cmd("SADD")
                    .arg(&key)
                    .arg(&new_val)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("SADD error: {}", e))?;
                Ok(true)
            }
            "zset" => {
                let old_member = old_value.as_deref().unwrap_or("");
                let new_member = value.ok_or("value is required for zset")?;
                let s = score.ok_or("score is required for zset ZADD")?;
                // If member name changed, remove old
                if !old_member.is_empty() && old_member != new_member {
                    let _: i64 = redis::cmd("ZREM")
                        .arg(&key)
                        .arg(old_member)
                        .query_async(&mut *conn)
                        .await
                        .map_err(|e| format!("ZREM error: {}", e))?;
                }
                let _: () = redis::cmd("ZADD")
                    .arg(&key)
                    .arg(s)
                    .arg(&new_member)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("ZADD error: {}", e))?;
                Ok(true)
            }
            _ => Err(format!("Unsupported key type: {}", key_type)),
        },
        "delete_field" => match key_type.as_str() {
            "hash" => {
                let f = field.ok_or("field is required for hash HDEL")?;
                let _: i64 = redis::cmd("HDEL")
                    .arg(&key)
                    .arg(&f)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("HDEL error: {}", e))?;
                Ok(true)
            }
            "list" => {
                let val = value.ok_or("value is required for list LREM")?;
                let _: i64 = redis::cmd("LREM")
                    .arg(&key)
                    .arg(1)
                    .arg(&val)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("LREM error: {}", e))?;
                Ok(true)
            }
            "set" => {
                let val = value.ok_or("value is required for set SREM")?;
                let _: i64 = redis::cmd("SREM")
                    .arg(&key)
                    .arg(&val)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("SREM error: {}", e))?;
                Ok(true)
            }
            "zset" => {
                let member = value.ok_or("value is required for zset ZREM")?;
                let _: i64 = redis::cmd("ZREM")
                    .arg(&key)
                    .arg(&member)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("ZREM error: {}", e))?;
                Ok(true)
            }
            _ => Err(format!("delete_field not supported for type: {}", key_type)),
        },
        "add_field" => match key_type.as_str() {
            "hash" => {
                let f = field.ok_or("field is required for hash HSET")?;
                let val = value.unwrap_or_default();
                let _: () = redis::cmd("HSET")
                    .arg(&key)
                    .arg(&f)
                    .arg(&val)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("HSET error: {}", e))?;
                Ok(true)
            }
            "list" => {
                let val = value.unwrap_or_default();
                let _: i64 = redis::cmd("RPUSH")
                    .arg(&key)
                    .arg(&val)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("RPUSH error: {}", e))?;
                Ok(true)
            }
            "set" => {
                let val = value.ok_or("value is required for set SADD")?;
                let _: i64 = redis::cmd("SADD")
                    .arg(&key)
                    .arg(&val)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("SADD error: {}", e))?;
                Ok(true)
            }
            "zset" => {
                let member = value.ok_or("value is required for zset ZADD")?;
                let s = score.unwrap_or(0.0);
                let _: () = redis::cmd("ZADD")
                    .arg(&key)
                    .arg(s)
                    .arg(&member)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("ZADD error: {}", e))?;
                Ok(true)
            }
            _ => Err(format!("add_field not supported for type: {}", key_type)),
        },
        _ => Err(format!("Unknown action: {}", action)),
    }
}
