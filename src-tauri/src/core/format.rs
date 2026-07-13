use redis::AsyncCommands;

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
/// Accepts pre-fetched type to avoid redundant TYPE call.
pub async fn get_key_value_string_with_type(
    conn: &mut deadpool_redis::Connection,
    key: &str,
    type_str: &str,
) -> Option<String> {
    match type_str {
        "string" => {
            let val: Vec<u8> = conn.get(key).await.ok()?;
            Some(String::from_utf8_lossy(&val).into_owned())
        }
        "hash" => {
            let mut fields: Vec<(Vec<u8>, Vec<u8>)> = conn.hgetall(key).await.ok()?;
            fields.sort_by(|a, b| a.0.cmp(&b.0));
            let fields: Vec<(String, String)> = fields
                .into_iter()
                .map(|(f, v)| {
                    (
                        String::from_utf8_lossy(&f).into_owned(),
                        String::from_utf8_lossy(&v).into_owned(),
                    )
                })
                .collect();
            Some(serde_json::to_string(&fields).unwrap_or_default())
        }
        "list" => {
            let items: Vec<Vec<u8>> = conn.lrange(key, 0, -1).await.ok()?;
            let items: Vec<String> = items
                .into_iter()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .collect();
            Some(serde_json::to_string(&items).unwrap_or_default())
        }
        "set" => {
            let mut members: Vec<Vec<u8>> = conn.smembers(key).await.ok()?;
            members.sort();
            let members: Vec<String> = members
                .into_iter()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .collect();
            Some(serde_json::to_string(&members).unwrap_or_default())
        }
        "zset" => {
            let mut members: Vec<(Vec<u8>, f64)> = redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .arg("WITHSCORES")
                .query_async(&mut **conn)
                .await
                .ok()?;
            members.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
            let members: Vec<(String, f64)> = members
                .into_iter()
                .map(|(m, s)| (String::from_utf8_lossy(&m).into_owned(), s))
                .collect();
            Some(serde_json::to_string(&members).unwrap_or_default())
        }
        _ => Some("(unsupported type)".to_string()),
    }
}
