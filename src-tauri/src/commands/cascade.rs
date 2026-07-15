use crate::core::validate::{
    validate_connection_id, validate_key, validate_pattern, validate_scan_count, validate_ttl,
    reject_null_bytes,
};
use crate::AppState;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Maximum number of items to return for composite types (hash/list/set/zset).
/// Prevents UI freeze when opening very large keys.
const MAX_DISPLAY_ITEMS: usize = 1000;

/// Detect content encoding from bytes.
/// Returns Some(encoding_name) if detected, None for unrecognised binary data.
fn detect_content_encoding(data: &[u8]) -> Option<&'static str> {
    if data.is_empty() {
        return Some("UTF-8");
    }

    // 1. BOM detection (UTF-16/32/8)
    if let Some((encoding, _bom_len)) = encoding_rs::Encoding::for_bom(data) {
        return Some(encoding.name());
    }

    // 2. Pure ASCII
    if data.iter().all(|&b| b.is_ascii()) {
        return Some("ASCII");
    }

    // 3. Valid UTF-8
    if std::str::from_utf8(data).is_ok() {
        return Some("UTF-8");
    }

    // 4. Heuristic detection for CJK and other legacy encodings
    //    encoding_rs doesn't expose a generic detector, so we check
    //    byte-range patterns that are characteristic of each encoding.

    // GB18030 / GBK: high-byte pairs in ranges 0x81-0xFE lead byte,
    //   0x40-0x7E / 0x80-0xFE trail byte.  GB18030 also has 4-byte
    //   sequences (0x81-0xFE)(0x30-0x39)(0x81-0xFE)(0x30-0x39).
    if looks_like_gbk(data) {
        // Distinguish GBK vs GB18030 by checking for 4-byte sequences
        if has_gb18030_four_byte(data) {
            return Some("GB18030");
        }
        return Some("GBK");
    }

    // Shift_JIS: lead 0x81-0x9F/0xE0-0xEF, trail 0x40-0x7E/0x80-0xFC
    if looks_like_shift_jis(data) {
        return Some("Shift_JIS");
    }

    // EUC-JP: lead 0x8E/0x8F/0xA1-0xFE
    if looks_like_euc_jp(data) {
        return Some("EUC-JP");
    }

    // Big5: lead 0xA1-0xF9, trail 0x40-0x7E/0xA1-0xFE
    if looks_like_big5(data) {
        return Some("Big5");
    }

    // EUC-KR: lead 0xA1-0xFE, trail 0xA1-0xFE
    if looks_like_euc_kr(data) {
        return Some("EUC-KR");
    }

    // Cannot determine encoding – treat as opaque binary
    None
}

/// Detect encoding for composite types (hash/list/set/zset).
/// Zero-copy fast path: check ASCII/UTF-8 across all parts without allocation.
/// Fallback: sample up to 16 non-ASCII elements for heuristic detection.
fn detect_multi_encoding(parts: &[&[u8]]) -> Option<&'static str> {
    if parts.is_empty() {
        return Some("UTF-8");
    }

    // Zero-copy fast path: check properties across all parts
    let mut all_ascii = true;
    let mut all_utf8 = true;

    for part in parts {
        if part.is_empty() {
            continue;
        }
        if all_ascii && !part.iter().all(|&b| b.is_ascii()) {
            all_ascii = false;
        }
        if all_utf8 && std::str::from_utf8(part).is_err() {
            all_utf8 = false;
        }
        // Early exit: if neither ASCII nor UTF-8, go to fallback
        if !all_ascii && !all_utf8 {
            break;
        }
    }

    if all_ascii {
        return Some("ASCII");
    }
    if all_utf8 {
        return Some("UTF-8");
    }

    // Fallback: sample non-ASCII elements (max 16) for heuristic detection
    const MAX_SAMPLES: usize = 16;
    let mut sampled = 0;
    for part in parts {
        if sampled >= MAX_SAMPLES {
            break;
        }
        if part.iter().all(|&b| b.is_ascii()) {
            continue;
        }
        if let Some(enc) = detect_content_encoding(part) {
            return Some(enc);
        }
        sampled += 1;
    }

    None
}

// ---- heuristic helpers ----

fn looks_like_gbk(data: &[u8]) -> bool {
    let mut i = 0;
    let mut db_count = 0;
    while i < data.len() {
        let b = data[i];
        if b.is_ascii() {
            i += 1;
            continue;
        }
        if (0x81..=0xFE).contains(&b) && i + 1 < data.len() {
            let t = data[i + 1];
            if (0x40..=0x7E).contains(&t) || (0x80..=0xFE).contains(&t) {
                db_count += 1;
                i += 2;
                continue;
            }
        }
        return false; // invalid byte
    }
    db_count >= 2 // require at least 2 double-byte chars
}

fn has_gb18030_four_byte(data: &[u8]) -> bool {
    let mut i = 0;
    while i + 3 < data.len() {
        let b1 = data[i];
        if (0x81..=0xFE).contains(&b1) {
            let b2 = data[i + 1];
            if (0x30..=0x39).contains(&b2) {
                let b3 = data[i + 2];
                let b4 = data[i + 3];
                if (0x81..=0xFE).contains(&b3) && (0x30..=0x39).contains(&b4) {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn looks_like_shift_jis(data: &[u8]) -> bool {
    let mut i = 0;
    let mut db_count = 0;
    while i < data.len() {
        let b = data[i];
        if b.is_ascii() || (0xA1..=0xDF).contains(&b) {
            // single-byte katakana or ASCII
            i += 1;
            continue;
        }
        if (0x81..=0x9F).contains(&b) || (0xE0..=0xEF).contains(&b) {
            if i + 1 < data.len() {
                let t = data[i + 1];
                if (0x40..=0x7E).contains(&t) || (0x80..=0xFC).contains(&t) {
                    db_count += 1;
                    i += 2;
                    continue;
                }
            }
        }
        return false;
    }
    db_count >= 2
}

fn looks_like_euc_jp(data: &[u8]) -> bool {
    let mut i = 0;
    let mut db_count = 0;
    while i < data.len() {
        let b = data[i];
        if b.is_ascii() {
            i += 1;
            continue;
        }
        // SS2 (0x8E) + 0xA1-0xDF = half-width katakana
        if b == 0x8E && i + 1 < data.len() && (0xA1..=0xDF).contains(&data[i + 1]) {
            db_count += 1;
            i += 2;
            continue;
        }
        // SS3 (0x8F) + JIS X 0212
        if b == 0x8F && i + 2 < data.len()
            && (0xA1..=0xFE).contains(&data[i + 1])
            && (0xA1..=0xFE).contains(&data[i + 2])
        {
            db_count += 1;
            i += 3;
            continue;
        }
        if (0xA1..=0xFE).contains(&b) && i + 1 < data.len() && (0xA1..=0xFE).contains(&data[i + 1]) {
            db_count += 1;
            i += 2;
            continue;
        }
        return false;
    }
    db_count >= 2
}

fn looks_like_big5(data: &[u8]) -> bool {
    let mut i = 0;
    let mut db_count = 0;
    while i < data.len() {
        let b = data[i];
        if b.is_ascii() {
            i += 1;
            continue;
        }
        if (0xA1..=0xF9).contains(&b) && i + 1 < data.len() {
            let t = data[i + 1];
            if (0x40..=0x7E).contains(&t) || (0xA1..=0xFE).contains(&t) {
                db_count += 1;
                i += 2;
                continue;
            }
        }
        return false;
    }
    db_count >= 2
}

fn looks_like_euc_kr(data: &[u8]) -> bool {
    let mut i = 0;
    let mut db_count = 0;
    while i < data.len() {
        let b = data[i];
        if b.is_ascii() {
            i += 1;
            continue;
        }
        if (0xA1..=0xFE).contains(&b) && i + 1 < data.len() && (0xA1..=0xFE).contains(&data[i + 1]) {
            db_count += 1;
            i += 2;
            continue;
        }
        return false;
    }
    db_count >= 2
}

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

    // Pipeline: batch TYPE + TTL for all keys (1 round-trip)
    let mut pipe = redis::pipe();
    for key in &keys {
        pipe.cmd("TYPE").arg(key);
    }
    for key in &keys {
        pipe.cmd("TTL").arg(key);
    }

    let n = keys.len();
    let expected = 2 * n;
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

    // MEMORY USAGE may not exist on Redis < 4.0; query separately with graceful fallback
    let sizes: Vec<u64> = {
        let mut size_pipe = redis::pipe();
        for key in &keys {
            size_pipe.cmd("MEMORY").arg("USAGE").arg(key);
        }
        let size_vals: Vec<redis::Value> = size_pipe
            .query_async(&mut *conn)
            .await
            .unwrap_or_default();
        if size_vals.len() == n {
            size_vals.iter().map(|v| redis::from_redis_value::<u64>(v).unwrap_or(0)).collect()
        } else {
            // MEMORY USAGE not supported or partial failure — all sizes default to 0
            vec![0u64; n]
        }
    };

    for (i, key) in keys.iter().enumerate() {
        let type_str = redis::from_redis_value::<String>(&values[i])
            .unwrap_or_else(|_| "none".to_string());

        let ttl: i64 = redis::from_redis_value::<i64>(&values[n + i])
            .unwrap_or(-1);

        result.push(RedisKeyInfo {
            key: key.clone(),
            key_type: type_str,
            ttl,
            size: sizes[i],
        });
    }

    Ok((next_cursor, result))
}

/// Check if a Redis version string satisfies >= 7.4.0 (for field-level TTL support).
fn redis_supports_field_ttl(version: &str) -> bool {
    let mut parts = version.split('.');
    let major: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    major > 7 || (major == 7 && minor >= 4)
}

/// Get detailed value of a specific key with pagination support
#[tauri::command]
pub async fn get_key_detail(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    offset: Option<u64>,
    limit: Option<u64>,
    filter: Option<String>,
    redis_version: Option<String>,
) -> Result<KeyDetail, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;

    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(MAX_DISPLAY_ITEMS as u64);
    let filter = filter.filter(|f| !f.is_empty());

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    // Step 1: TYPE + TTL + OBJECT ENCODING (safe commands, 1 round-trip)
    // MEMORY USAGE is queried separately because it doesn't exist on Redis < 4.0
    let (type_str, ttl, encoding) = {
        let mut pipe = redis::pipe();
        pipe.cmd("TYPE").arg(&key)
            .cmd("TTL").arg(&key)
            .cmd("OBJECT").arg("ENCODING").arg(&key);
        let values: Vec<redis::Value> = pipe
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("Pipeline error: {}", e))?;

        if values.len() < 3 {
            return Err(format!("Failed to get key metadata: expected 3 results, got {}", values.len()));
        }

        let type_str: String = redis::from_redis_value(&values[0])
            .map_err(|e| format!("TYPE error: {}", e))?;
        let ttl: i64 = redis::from_redis_value(&values[1]).unwrap_or(-1);
        let encoding: String = redis::from_redis_value(&values[2]).unwrap_or_else(|_| "unknown".to_string());
        (type_str, ttl, encoding)
    };

    // MEMORY USAGE — graceful fallback for Redis < 4.0
    let size: u64 = redis::cmd("MEMORY")
        .arg("USAGE")
        .arg(&key)
        .query_async(&mut *conn)
        .await
        .unwrap_or(0);

    // Step 2: Send only the matching count command (avoids WRONGTYPE errors)
    let total_count: usize = match type_str.as_str() {
        "hash" => redis::cmd("HLEN").arg(&key).query_async(&mut *conn).await.unwrap_or(0),
        "list" => redis::cmd("LLEN").arg(&key).query_async(&mut *conn).await.unwrap_or(0),
        "set" => redis::cmd("SCARD").arg(&key).query_async(&mut *conn).await.unwrap_or(0),
        "zset" => redis::cmd("ZCARD").arg(&key).query_async(&mut *conn).await.unwrap_or(0),
        _ => 0,
    };

    let key_info = RedisKeyInfo {
        key: key.clone(),
        key_type: type_str.clone(),
        ttl,
        size,
    };

    // Fetch value based on type
    let value = match type_str.as_str() {
        "string" => {
            let val: Vec<u8> = conn
                .get(&key)
                .await
                .map_err(|e| format!("GET error: {}", e))?;
            let content_encoding = detect_content_encoding(&val);
            // Hex-encode raw bytes for binary-safe multi-view display (Text / Hex)
            let value_hex: String = val.iter().map(|b| format!("{:02x}", b)).collect();
            let val = String::from_utf8_lossy(&val).into_owned();
            serde_json::json!({ "type": "string", "value": val, "valueHex": value_hex, "encoding": encoding, "contentEncoding": content_encoding })
        }
        "hash" => {
            // Use HSCAN for pagination (avoids loading all data for large hashes)
            let mut all_fields = Vec::new();
            let mut cursor: u64 = 0;
            let pattern_lower = filter.as_ref().map(|p| p.to_lowercase());
            let need_count = if filter.is_some() { usize::MAX } else { (offset + limit) as usize };
            loop {
                let (next_cursor, batch): (u64, Vec<(Vec<u8>, Vec<u8>)>) = redis::cmd("HSCAN")
                    .arg(&key)
                    .arg(cursor)
                    .arg("COUNT")
                    .arg(1000)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("HSCAN error: {}", e))?;
                for (f, v) in batch {
                    if let Some(ref pat) = pattern_lower {
                        let field_lower = String::from_utf8_lossy(&f).to_lowercase();
                        if field_lower.contains(pat) {
                            all_fields.push((f, v));
                        }
                    } else {
                        all_fields.push((f, v));
                    }
                }
                cursor = next_cursor;
                if cursor == 0 { break; }
                // Early exit when we have enough (only without filter)
                if pattern_lower.is_none() && all_fields.len() >= need_count {
                    break;
                }
            }
            let matched_count = if filter.is_some() { all_fields.len() } else { total_count };
            let fields: Vec<(Vec<u8>, Vec<u8>)> = all_fields.into_iter().skip(offset as usize).take(limit as usize).collect();
            let truncated = if filter.is_some() { false } else { total_count > (offset + limit) as usize };

            // Fetch per-field TTLs via HTTL (Redis >= 7.4.0 only).
            let fetch_field_ttl = redis_version
                .as_deref()
                .map(redis_supports_field_ttl)
                .unwrap_or(false);
            let field_ttls: Option<Vec<i64>> = if fetch_field_ttl && !fields.is_empty() {
                let mut ttl_pipe = redis::pipe();
                // HTTL supports up to ~128 fields per call; batch in chunks of 128
                const HTTL_BATCH: usize = 128;
                for chunk in fields.chunks(HTTL_BATCH) {
                    ttl_pipe
                        .cmd("HTTL")
                        .arg(&key)
                        .arg("FIELDS")
                        .arg(chunk.len())
                        .arg(chunk.iter().map(|(f, _)| f.as_slice()).collect::<Vec<_>>());
                }
                let ttl_vals: Vec<redis::Value> = ttl_pipe
                    .query_async(&mut *conn)
                    .await
                    .unwrap_or_default();
                // Flatten all batch results into a single Vec<i64>
                let mut merged: Vec<i64> = Vec::with_capacity(fields.len());
                let mut ok = true;
                for v in &ttl_vals {
                    match v {
                        redis::Value::Array(arr) => {
                            for item in arr {
                                merged.push(redis::from_redis_value::<i64>(item).unwrap_or(-1));
                            }
                        }
                        _ => { ok = false; break; }
                    }
                }
                if ok && merged.len() == fields.len() { Some(merged) } else { None }
            } else {
                None
            };

            let parts: Vec<&[u8]> = fields.iter().flat_map(|(f, v)| vec![f.as_slice(), v.as_slice()]).collect();
            let content_encoding = detect_multi_encoding(&parts);
            let fields_json: Vec<serde_json::Value> = fields
                .iter()
                .enumerate()
                .map(|(i, (f, v))| {
                    let field = String::from_utf8_lossy(f).into_owned();
                    let field_hex: String = f.iter().map(|b| format!("{:02x}", b)).collect();
                    let value = String::from_utf8_lossy(v).into_owned();
                    let value_hex: String = v.iter().map(|b| format!("{:02x}", b)).collect();
                    let mut obj = serde_json::json!({ "field": field, "fieldHex": field_hex, "value": value, "valueHex": value_hex });
                    if let Some(ref ttls) = field_ttls {
                        obj["ttl"] = serde_json::json!(ttls[i]);
                    }
                    obj
                })
                .collect();
            let has_field_ttl = field_ttls.is_some();
            serde_json::json!({ "type": "hash", "fields": fields_json, "encoding": encoding, "contentEncoding": content_encoding, "totalCount": matched_count, "truncated": truncated, "hasFieldTtl": has_field_ttl })
        }
        "list" => {
            let (items, matched_count, truncated, original_indices) = if let Some(ref pattern) = filter {
                // With filter: fetch all items and filter globally, tracking original indices
                let all: Vec<Vec<u8>> = conn
                    .lrange(&key, 0, -1)
                    .await
                    .map_err(|e| format!("LRANGE error: {}", e))?;
                let pattern_lower = pattern.to_lowercase();
                let filtered: Vec<(usize, Vec<u8>)> = all.into_iter().enumerate().filter(|(_, b)| {
                    String::from_utf8_lossy(b).to_lowercase().contains(&pattern_lower)
                }).collect();
                let matched = filtered.len();
                let page_items: Vec<(usize, Vec<u8>)> = filtered.into_iter().skip(offset as usize).take(limit as usize).collect();
                let indices: Vec<usize> = page_items.iter().map(|(idx, _)| *idx).collect();
                let values: Vec<Vec<u8>> = page_items.into_iter().map(|(_, v)| v).collect();
                (values, matched, false, Some(indices))
            } else {
                // No filter: use LRANGE with offset/limit (efficient)
                let page_items: Vec<Vec<u8>> = conn
                    .lrange(&key, offset as isize, (offset + limit) as isize - 1)
                    .await
                    .map_err(|e| format!("LRANGE error: {}", e))?;
                let truncated = total_count > (offset + limit) as usize;
                (page_items, total_count, truncated, None)
            };
            let parts: Vec<&[u8]> = items.iter().map(|b| b.as_slice()).collect();
            let content_encoding = detect_multi_encoding(&parts);
            let items_hex: Vec<String> = items.iter().map(|b| b.iter().map(|byte| format!("{:02x}", byte)).collect()).collect();
            let items: Vec<String> = items
                .into_iter()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .collect();
            let mut list_json = serde_json::json!({ "type": "list", "items": items, "itemsHex": items_hex, "encoding": encoding, "contentEncoding": content_encoding, "totalCount": matched_count, "truncated": truncated });
            if let Some(indices) = original_indices {
                list_json["originalIndices"] = serde_json::json!(indices);
            }
            list_json
        }
        "set" => {
            // Use SSCAN for pagination with optional filter
            let mut all_members = Vec::new();
            let mut cursor: u64 = 0;
            let pattern_lower = filter.as_ref().map(|p| p.to_lowercase());
            let need_count = if filter.is_some() { usize::MAX } else { (offset + limit) as usize };
            loop {
                let (next_cursor, batch): (u64, Vec<Vec<u8>>) = redis::cmd("SSCAN")
                    .arg(&key)
                    .arg(cursor)
                    .arg("COUNT")
                    .arg(1000)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("SSCAN error: {}", e))?;
                for m in batch {
                    if let Some(ref pat) = pattern_lower {
                        let member_lower = String::from_utf8_lossy(&m).to_lowercase();
                        if member_lower.contains(pat) {
                            all_members.push(m);
                        }
                    } else {
                        all_members.push(m);
                    }
                }
                cursor = next_cursor;
                if cursor == 0 { break; }
                // Early exit if we have enough (only when no filter)
                if pattern_lower.is_none() && all_members.len() >= need_count {
                    break;
                }
            }
            let matched_count = if filter.is_some() { all_members.len() } else { total_count };
            let display_members: Vec<Vec<u8>> = all_members.into_iter().skip(offset as usize).take(limit as usize).collect();
            let truncated = if filter.is_some() { false } else { total_count > (offset + limit) as usize };
            let parts: Vec<&[u8]> = display_members.iter().map(|b| b.as_slice()).collect();
            let content_encoding = detect_multi_encoding(&parts);
            let members_hex: Vec<String> = display_members.iter().map(|b| b.iter().map(|byte| format!("{:02x}", byte)).collect()).collect();
            let members: Vec<String> = display_members
                .iter()
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .collect();
            serde_json::json!({ "type": "set", "members": members, "membersHex": members_hex, "encoding": encoding, "contentEncoding": content_encoding, "totalCount": matched_count, "truncated": truncated })
        }
        "zset" => {
            // Use ZSCAN for pagination (ZRANGE LIMIT only works with BYSCORE/BYLEX)
            let mut all_members = Vec::new();
            let mut cursor: u64 = 0;
            let pattern_lower = filter.as_ref().map(|p| p.to_lowercase());
            let need_count = if filter.is_some() { usize::MAX } else { (offset + limit) as usize };
            loop {
                let (next_cursor, batch): (u64, Vec<(Vec<u8>, f64)>) = redis::cmd("ZSCAN")
                    .arg(&key)
                    .arg(cursor)
                    .arg("COUNT")
                    .arg(1000)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("ZSCAN error: {}", e))?;
                for (m, s) in batch {
                    if let Some(ref pat) = pattern_lower {
                        let member_lower = String::from_utf8_lossy(&m).to_lowercase();
                        if member_lower.contains(pat) {
                            all_members.push((m, s));
                        }
                    } else {
                        all_members.push((m, s));
                    }
                }
                cursor = next_cursor;
                if cursor == 0 { break; }
                // Early exit when we have enough (only without filter)
                if pattern_lower.is_none() && all_members.len() >= need_count {
                    break;
                }
            }
            let matched_count = if filter.is_some() { all_members.len() } else { total_count };
            let members: Vec<(Vec<u8>, f64)> = all_members.into_iter().skip(offset as usize).take(limit as usize).collect();
            let truncated = if filter.is_some() { false } else { total_count > (offset + limit) as usize };
            let parts: Vec<&[u8]> = members.iter().map(|(m, _)| m.as_slice()).collect();
            let content_encoding = detect_multi_encoding(&parts);
            let members_json: Vec<serde_json::Value> = members
                .into_iter()
                .map(|(m, s)| {
                    let member = String::from_utf8_lossy(&m).into_owned();
                    let member_hex: String = m.iter().map(|b| format!("{:02x}", b)).collect();
                    serde_json::json!({ "member": member, "memberHex": member_hex, "score": s })
                })
                .collect();
            serde_json::json!({ "type": "zset", "members": members_json, "encoding": encoding, "contentEncoding": content_encoding, "totalCount": matched_count, "truncated": truncated })
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

/// Set TTL on individual hash fields (Redis >= 7.4.0: HEXPIRE / HPERSIST).
///
/// `ttl > 0`  → HEXPIRE key ttl FIELDS 1 field
/// `ttl == -1` → HPERSIST key FIELDS 1 field
#[tauri::command]
pub async fn set_hash_field_ttl(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    field: String,
    ttl: i64,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    reject_null_bytes(&field, "field")?;
    validate_ttl(ttl)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    if ttl > 0 {
        // HEXPIRE key seconds FIELDS numfields field [field ...]
        let result: i64 = redis::cmd("HEXPIRE")
            .arg(&key)
            .arg(ttl)
            .arg("FIELDS")
            .arg(1)
            .arg(&field)
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("HEXPIRE error: {}", e))?;
        // HEXPIRE returns 1 on success, 0 if field doesn't exist
        Ok(result > 0)
    } else if ttl == -1 {
        // HPERSIST key FIELDS numfields field [field ...]
        let result: i64 = redis::cmd("HPERSIST")
            .arg(&key)
            .arg("FIELDS")
            .arg(1)
            .arg(&field)
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("HPERSIST error: {}", e))?;
        Ok(result > 0)
    } else {
        Err("Invalid TTL value".to_string())
    }
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
///   - "rename_field": rename a hash field (HGET → HDEL → HSET)
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
        "rename_field" => match key_type.as_str() {
            "hash" => {
                let old_field = old_value.ok_or("old_value (old field name) is required for hash rename_field")?;
                let new_field = field.ok_or("field (new field name) is required for hash rename_field")?;
                if old_field == new_field {
                    return Ok(true);
                }
                // Check new field doesn't already exist
                let exists: bool = redis::cmd("HEXISTS")
                    .arg(&key)
                    .arg(&new_field)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("HEXISTS error: {}", e))?;
                if exists {
                    return Err("Target field already exists".to_string());
                }
                // Get old field value
                let val: Option<Vec<u8>> = redis::cmd("HGET")
                    .arg(&key)
                    .arg(&old_field)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("HGET error: {}", e))?;
                let val = val.ok_or("Source field does not exist".to_string())?;
                // Delete old field and set new field atomically via pipeline
                let mut pipe = redis::pipe();
                pipe.cmd("HDEL").arg(&key).arg(&old_field);
                pipe.cmd("HSET").arg(&key).arg(&new_field).arg(val);
                let _: Vec<redis::Value> = pipe
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("Rename field pipeline error: {}", e))?;
                Ok(true)
            }
            _ => Err(format!("rename_field not supported for type: {}", key_type)),
        },
        _ => Err(format!("Unknown action: {}", action)),
    }
}

/// Create a new key with optional initial data and TTL.
///
/// `initial_data` is a JSON value whose shape depends on `key_type`:
///   - string: `"some value"` (a JSON string)
///   - hash:   `[["field","value"], ...]` (array of pairs)
///   - list:   `["item1","item2",...]` (array of strings)
///   - set:    `["m1","m2",...]` (array of strings)
///   - zset:   `[["member",1.0],...]` (array of [member, score] pairs)
///
/// `field_ttl` is only used for hash type (Redis >= 7.4) to set per-field expiry
/// right after creation. Value is in seconds.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_key(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    key_type: String,
    ttl: Option<i64>,
    initial_data: Option<serde_json::Value>,
    field_ttl: Option<i64>,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    if let Some(t) = ttl {
        validate_ttl(t)?;
    }

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    match key_type.as_str() {
        "string" => {
            let val = match initial_data {
                Some(serde_json::Value::String(s)) => s,
                _ => String::new(),
            };
            let _: () = redis::cmd("SET")
                .arg(&key)
                .arg(&val)
                .query_async(&mut *conn)
                .await
                .map_err(|e| format!("SET error: {}", e))?;
        }
        "hash" => {
            // Accept [[field, value], ...] or {field: value, ...}
            let pairs: Vec<(String, String)> = match initial_data {
                Some(serde_json::Value::Array(arr)) => {
                    let mut result = Vec::new();
                    for item in arr {
                        if let serde_json::Value::Array(pair) = item {
                            if pair.len() >= 2 {
                                let f = pair[0].as_str().unwrap_or("").to_string();
                                let v = pair[1].as_str().unwrap_or("").to_string();
                                if !f.is_empty() {
                                    reject_null_bytes(&f, "field")?;
                                    reject_null_bytes(&v, "value")?;
                                    result.push((f, v));
                                }
                            }
                        }
                    }
                    result
                }
                Some(serde_json::Value::Object(map)) => {
                    let mut result = Vec::new();
                    for (f, v) in map {
                        let v_str = v.as_str().unwrap_or("").to_string();
                        reject_null_bytes(&f, "field")?;
                        reject_null_bytes(&v_str, "value")?;
                        result.push((f, v_str));
                    }
                    result
                }
                _ => Vec::new(),
            };
            if !pairs.is_empty() {
                let mut pipe = redis::pipe();
                for (f, v) in &pairs {
                    pipe.cmd("HSET").arg(&key).arg(f).arg(v);
                }
                let _: Vec<redis::Value> = pipe
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("HSET error: {}", e))?;
            } else {
                // Ensure key exists even with no data
                let _: () = redis::cmd("HSETNX")
                    .arg(&key)
                    .arg("__init__")
                    .arg("")
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("HSET error: {}", e))?;
                let _: i64 = redis::cmd("HDEL")
                    .arg(&key)
                    .arg("__init__")
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("HDEL error: {}", e))?;
            }
            // Set field TTL for hash fields (Redis >= 7.4)
            if let Some(fttl) = field_ttl {
                if fttl > 0 && !pairs.is_empty() {
                    let fields: Vec<&str> = pairs.iter().map(|(f, _)| f.as_str()).collect();
                    let _: i64 = redis::cmd("HEXPIRE")
                        .arg(&key)
                        .arg(fttl)
                        .arg("FIELDS")
                        .arg(fields.len())
                        .arg(&fields)
                        .query_async(&mut *conn)
                        .await
                        .map_err(|e| format!("HEXPIRE error: {}", e))?;
                }
            }
        }
        "list" => {
            let items: Vec<String> = match initial_data {
                Some(serde_json::Value::Array(arr)) => {
                    arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
                }
                _ => Vec::new(),
            };
            if !items.is_empty() {
                let mut pipe = redis::pipe();
                for item in &items {
                    reject_null_bytes(item, "value")?;
                    pipe.cmd("RPUSH").arg(&key).arg(item);
                }
                let _: Vec<redis::Value> = pipe
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("RPUSH error: {}", e))?;
            } else {
                // Create empty list key
                let _: i64 = redis::cmd("RPUSH")
                    .arg(&key)
                    .arg("__init__")
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("RPUSH error: {}", e))?;
                let _: i64 = redis::cmd("LREM")
                    .arg(&key)
                    .arg(1)
                    .arg("__init__")
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("LREM error: {}", e))?;
            }
        }
        "set" => {
            let members: Vec<String> = match initial_data {
                Some(serde_json::Value::Array(arr)) => {
                    arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
                }
                _ => Vec::new(),
            };
            if !members.is_empty() {
                let mut pipe = redis::pipe();
                for m in &members {
                    reject_null_bytes(m, "value")?;
                    pipe.cmd("SADD").arg(&key).arg(m);
                }
                let _: Vec<redis::Value> = pipe
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("SADD error: {}", e))?;
            } else {
                // Create empty set key
                let _: i64 = redis::cmd("SADD")
                    .arg(&key)
                    .arg("__init__")
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("SADD error: {}", e))?;
                let _: i64 = redis::cmd("SREM")
                    .arg(&key)
                    .arg("__init__")
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("SREM error: {}", e))?;
            }
        }
        "zset" => {
            // Accept [[member, score], ...]
            let pairs: Vec<(String, f64)> = match initial_data {
                Some(serde_json::Value::Array(arr)) => {
                    let mut result = Vec::new();
                    for item in arr {
                        if let serde_json::Value::Array(pair) = item {
                            if pair.len() >= 2 {
                                let m = pair[0].as_str().unwrap_or("").to_string();
                                let s = pair[1].as_f64().unwrap_or(0.0);
                                if !m.is_empty() {
                                    reject_null_bytes(&m, "value")?;
                                    result.push((m, s));
                                }
                            }
                        }
                    }
                    result
                }
                _ => Vec::new(),
            };
            if !pairs.is_empty() {
                let mut pipe = redis::pipe();
                for (m, s) in &pairs {
                    pipe.cmd("ZADD").arg(&key).arg(s).arg(m);
                }
                let _: Vec<redis::Value> = pipe
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("ZADD error: {}", e))?;
            } else {
                // Create empty zset key
                let _: () = redis::cmd("ZADD")
                    .arg(&key)
                    .arg(0)
                    .arg("__init__")
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("ZADD error: {}", e))?;
                let _: i64 = redis::cmd("ZREM")
                    .arg(&key)
                    .arg("__init__")
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("ZREM error: {}", e))?;
            }
        }
        _ => return Err(format!("Unsupported key type: {}", key_type)),
    }

    // Set key TTL if provided
    if let Some(t) = ttl {
        if t > 0 {
            let _: bool = redis::cmd("EXPIRE")
                .arg(&key)
                .arg(t)
                .query_async(&mut *conn)
                .await
                .map_err(|e| format!("EXPIRE error: {}", e))?;
        }
    }

    Ok(true)
}

/// Batch add fields/members/items to an existing key.
///
/// `items` is a JSON array whose shape depends on `key_type`:
///   - hash: `[["field","value"], ...]`
///   - list: `["item1","item2",...]`
///   - set:  `["m1","m2",...]`
///   - zset: `[["member",1.0],...]`
///
/// `field_ttl` is only for hash type (Redis >= 7.4), sets per-field TTL in seconds.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn batch_add_fields(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    key_type: String,
    items: serde_json::Value,
    field_ttl: Option<i64>,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    match key_type.as_str() {
        "hash" => {
            let pairs: Vec<(String, String)> = match items {
                serde_json::Value::Array(arr) => {
                    let mut result = Vec::new();
                    for item in arr {
                        if let serde_json::Value::Array(pair) = item {
                            if pair.len() >= 2 {
                                let f = pair[0].as_str().unwrap_or("").to_string();
                                let v = pair[1].as_str().unwrap_or("").to_string();
                                if !f.is_empty() {
                                    reject_null_bytes(&f, "field")?;
                                    reject_null_bytes(&v, "value")?;
                                    result.push((f, v));
                                }
                            }
                        }
                    }
                    result
                }
                _ => return Err("items must be an array".to_string()),
            };
            if pairs.is_empty() {
                return Ok(true);
            }
            let mut pipe = redis::pipe();
            for (f, v) in &pairs {
                pipe.cmd("HSET").arg(&key).arg(f).arg(v);
            }
            let _: Vec<redis::Value> = pipe
                .query_async(&mut *conn)
                .await
                .map_err(|e| format!("HSET error: {}", e))?;
            // Set field TTL if provided (Redis >= 7.4)
            if let Some(fttl) = field_ttl {
                if fttl > 0 {
                    let fields: Vec<&str> = pairs.iter().map(|(f, _)| f.as_str()).collect();
                    let _: i64 = redis::cmd("HEXPIRE")
                        .arg(&key)
                        .arg(fttl)
                        .arg("FIELDS")
                        .arg(fields.len())
                        .arg(&fields)
                        .query_async(&mut *conn)
                        .await
                        .map_err(|e| format!("HEXPIRE error: {}", e))?;
                }
            }
        }
        "list" => {
            let vals: Vec<String> = match items {
                serde_json::Value::Array(arr) => {
                    let mut result = Vec::new();
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            let s = s.to_string();
                            reject_null_bytes(&s, "value")?;
                            result.push(s);
                        }
                    }
                    result
                }
                _ => return Err("items must be an array".to_string()),
            };
            if !vals.is_empty() {
                let mut pipe = redis::pipe();
                for v in &vals {
                    pipe.cmd("RPUSH").arg(&key).arg(v);
                }
                let _: Vec<redis::Value> = pipe
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("RPUSH error: {}", e))?;
            }
        }
        "set" => {
            let members: Vec<String> = match items {
                serde_json::Value::Array(arr) => {
                    let mut result = Vec::new();
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            let s = s.to_string();
                            reject_null_bytes(&s, "value")?;
                            result.push(s);
                        }
                    }
                    result
                }
                _ => return Err("items must be an array".to_string()),
            };
            if !members.is_empty() {
                let _: i64 = redis::cmd("SADD")
                    .arg(&key)
                    .arg(&members)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("SADD error: {}", e))?;
            }
        }
        "zset" => {
            let pairs: Vec<(String, f64)> = match items {
                serde_json::Value::Array(arr) => {
                    let mut result = Vec::new();
                    for item in arr {
                        if let serde_json::Value::Array(pair) = item {
                            if pair.len() >= 2 {
                                let m = pair[0].as_str().unwrap_or("").to_string();
                                let s = pair[1].as_f64().unwrap_or(0.0);
                                if !m.is_empty() {
                                    reject_null_bytes(&m, "value")?;
                                    result.push((m, s));
                                }
                            }
                        }
                    }
                    result
                }
                _ => return Err("items must be an array".to_string()),
            };
            if !pairs.is_empty() {
                let mut pipe = redis::pipe();
                for (m, s) in &pairs {
                    pipe.cmd("ZADD").arg(&key).arg(s).arg(m);
                }
                let _: Vec<redis::Value> = pipe
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| format!("ZADD error: {}", e))?;
            }
        }
        _ => return Err(format!("batch_add_fields not supported for type: {}", key_type)),
    }

    Ok(true)
}
