use redis::AsyncCommands;

/// Restore a key to its original type and value in Redis.
/// Uses the key_type to determine the correct Redis command, avoiding
/// type corruption that would occur if we simply SET a JSON string.
pub async fn restore_key_value(
    conn: &mut deadpool_redis::Connection,
    key: &str,
    key_type: &str,
    serialized_value: &str,
) -> Result<(), String> {
    match key_type {
        "string" => {
            let _: () = conn.set(key, serialized_value).await
                .map_err(|e| format!("SET error: {}", e))?;
        }
        "hash" => {
            let pairs = serde_json::from_str::<Vec<(String, String)>>(serialized_value)
                .unwrap_or_default();
            // DEL then rebuild — must DEL first to remove any extra fields
            let _: i64 = redis::cmd("DEL").arg(key).query_async(&mut **conn).await
                .map_err(|e| format!("DEL error: {}", e))?;
            if !pairs.is_empty() {
                let mut cmd = redis::cmd("HSET");
                cmd.arg(key);
                for (f, v) in &pairs {
                    cmd.arg(f).arg(v);
                }
                let _: () = cmd.query_async(&mut **conn).await
                    .map_err(|e| format!("HSET error: {}", e))?;
            }
            // If pairs is empty, key remains deleted (empty hash ≈ no key in Redis)
        }
        "list" => {
            let items = serde_json::from_str::<Vec<String>>(serialized_value)
                .unwrap_or_default();
            let _: i64 = redis::cmd("DEL").arg(key).query_async(&mut **conn).await
                .map_err(|e| format!("DEL error: {}", e))?;
            if !items.is_empty() {
                let mut cmd = redis::cmd("RPUSH");
                cmd.arg(key);
                for item in &items {
                    cmd.arg(item);
                }
                let _: () = cmd.query_async(&mut **conn).await
                    .map_err(|e| format!("RPUSH error: {}", e))?;
            }
        }
        "set" => {
            let members = serde_json::from_str::<Vec<String>>(serialized_value)
                .unwrap_or_default();
            let _: i64 = redis::cmd("DEL").arg(key).query_async(&mut **conn).await
                .map_err(|e| format!("DEL error: {}", e))?;
            if !members.is_empty() {
                let mut cmd = redis::cmd("SADD");
                cmd.arg(key);
                for m in &members {
                    cmd.arg(m);
                }
                let _: () = cmd.query_async(&mut **conn).await
                    .map_err(|e| format!("SADD error: {}", e))?;
            }
        }
        "zset" => {
            let members = serde_json::from_str::<Vec<(String, f64)>>(serialized_value)
                .unwrap_or_default();
            let _: i64 = redis::cmd("DEL").arg(key).query_async(&mut **conn).await
                .map_err(|e| format!("DEL error: {}", e))?;
            if !members.is_empty() {
                let mut cmd = redis::cmd("ZADD");
                cmd.arg(key);
                for (m, s) in &members {
                    cmd.arg(s).arg(m);
                }
                let _: () = cmd.query_async(&mut **conn).await
                    .map_err(|e| format!("ZADD error: {}", e))?;
            }
        }
        _ => {
            // Fallback: treat as string
            let _: () = conn.set(key, serialized_value).await
                .map_err(|e| format!("SET error: {}", e))?;
        }
    }
    Ok(())
}

/// Format a redis::Value into a human-readable string.
/// Shared by pipeline and sandbox modules.
pub fn format_redis_value(val: &redis::Value) -> String {
    match val {
        redis::Value::Nil => "(nil)".to_string(),
        redis::Value::Int(n) => n.to_string(),
        redis::Value::BulkString(data) => String::from_utf8_lossy(data).to_string(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::Okay => "OK".to_string(),
        redis::Value::Array(arr) => {
            if arr.is_empty() {
                "(empty array)".to_string()
            } else {
                let lines: Vec<String> = arr
                    .iter()
                    .enumerate()
                    .map(|(i, v)| format!("  [{}] {}", i, format_redis_value(v)))
                    .collect();
                lines.join("\n")
            }
        }
        redis::Value::Map(map) => {
            if map.is_empty() {
                "(empty map)".to_string()
            } else {
                let lines: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("  {}: {}", format_redis_value(k), format_redis_value(v)))
                    .collect();
                format!("{{\n{}\n}}", lines.join(",\n"))
            }
        }
        _ => format!("{:?}", val),
    }
}

/// Format a stored value string for human-readable display in the diff UI.
/// For "string" type keys, returns the raw value as-is.
/// For complex types (hash, list, set, zset), parses JSON and pretty-prints.
pub fn format_for_display(raw: &str, key_type: &str) -> String {
    // String type: return raw value directly (no JSON reinterpretation)
    if key_type == "string" {
        return raw.to_string();
    }
    // Try parsing as array of tuples (hash/zset format)
    if let Ok(pairs) = serde_json::from_str::<Vec<(String, serde_json::Value)>>(raw) {
        if pairs.is_empty() {
            return "(empty)".to_string();
        }
        let lines: Vec<String> = pairs
            .iter()
            .map(|(k, v)| {
                let val_str = match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    other => other.to_string(),
                };
                format!("  {}: {}", k, val_str)
            })
            .collect();
        return format!("{{\n{}\n}}", lines.join(",\n"));
    }
    // Try parsing as string array (list/set format)
    if let Ok(items) = serde_json::from_str::<Vec<String>>(raw) {
        if items.is_empty() {
            return "(empty)".to_string();
        }
        let lines: Vec<String> = items
            .iter()
            .enumerate()
            .map(|(i, v)| format!("  [{}] {}", i, v))
            .collect();
        return lines.join("\n");
    }
    // Fallback: return raw string as-is (simple string values)
    raw.to_string()
}

/// Get the current canonical string representation of a key's value.
/// Used for storage, comparison, and rollback — must be stable/deterministic.
pub async fn get_key_value_string(
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
            let mut fields: Vec<(String, String)> = conn.hgetall(key).await.ok()?;
            // Sort by field name for deterministic serialization
            fields.sort_by(|a, b| a.0.cmp(&b.0));
            Some(serde_json::to_string(&fields).unwrap_or_default())
        }
        "list" => {
            let items: Vec<String> = conn.lrange(key, 0, -1).await.ok()?;
            Some(serde_json::to_string(&items).unwrap_or_default())
        }
        "set" => {
            let mut members: Vec<String> = conn.smembers(key).await.ok()?;
            // Sort for deterministic serialization
            members.sort();
            Some(serde_json::to_string(&members).unwrap_or_default())
        }
        "zset" => {
            let mut members: Vec<(String, f64)> = redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .arg("WITHSCORES")
                .query_async(&mut **conn)
                .await
                .ok()?;
            // Sort by score then by member name for deterministic order
            members.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
            Some(serde_json::to_string(&members).unwrap_or_default())
        }
        _ => Some("(unsupported type)".to_string()),
    }
}
