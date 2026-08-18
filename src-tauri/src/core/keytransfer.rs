use serde::{Deserialize, Serialize};

use crate::core::pool::AnyConn;

/// Hard cap on elements fetched per composite key during JSON export.
const MAX_EXPORT_ELEMENTS: usize = 100_000;
/// Chunk size for multi-argument write commands during import.
const IMPORT_CHUNK_SIZE: usize = 500;
/// Maximum import file size (256 MB) to keep parsing bounded.
const MAX_IMPORT_FILE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportWarning {
    pub key: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub exported: usize,
    pub warnings: Vec<ExportWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFailure {
    pub key: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub total: usize,
    pub succeeded: usize,
    pub skipped: usize,
    pub failed: Vec<ImportFailure>,
}

// ---------------------------------------------------------------------------
// base64 (self-contained; payload round-trip must be lossless)
// ---------------------------------------------------------------------------

const B64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64_ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_ALPHABET[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_ALPHABET[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Result<u32, String> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((c - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(format!("Invalid base64 character: {}", c as char)),
        }
    }

    let bytes: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    let content_len = bytes.iter().take_while(|&&b| b != b'=').count();
    let mut out = Vec::with_capacity(content_len * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0;
    for &b in &bytes {
        if b == b'=' {
            break;
        }
        buf = (buf << 6) | val(b)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Binary-safe scalar encoding inside the JSON export format:
//   {"utf8": "..."} for valid UTF-8 payloads, {"b64": "..."} otherwise.
// ---------------------------------------------------------------------------

fn scalar_to_json(bytes: &[u8]) -> serde_json::Value {
    match std::str::from_utf8(bytes) {
        Ok(s) => serde_json::json!({ "utf8": s }),
        Err(_) => serde_json::json!({ "b64": b64_encode(bytes) }),
    }
}

fn json_to_scalar(v: &serde_json::Value) -> Result<Vec<u8>, String> {
    if let Some(s) = v.get("utf8").and_then(|x| x.as_str()) {
        return Ok(s.as_bytes().to_vec());
    }
    if let Some(s) = v.get("b64").and_then(|x| x.as_str()) {
        return b64_decode(s);
    }
    Err("Invalid scalar encoding in export file".to_string())
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Reads a key's PTTL in milliseconds (-1 = no expiry, -2 = missing key).
async fn pttl(conn: &mut AnyConn, key: &str) -> i64 {
    redis::cmd("PTTL")
        .arg(key)
        .query_async::<i64>(conn)
        .await
        .unwrap_or(-2)
}

fn type_str_of(conn_result: Result<String, redis::RedisError>) -> String {
    conn_result.unwrap_or_else(|_| "none".to_string())
}

/// Build one export entry for a key in the human-readable "json" format.
/// Returns `None` when the key vanished mid-export.
async fn export_entry_json(
    conn: &mut AnyConn,
    key: &str,
    key_type: &str,
    ttl_ms: i64,
) -> Result<Option<serde_json::Value>, String> {
    let base = |value: serde_json::Value| {
        serde_json::json!({
            "key": key,
            "type": key_type,
            "ttlMs": ttl_ms,
            "value": value,
        })
    };

    match key_type {
        "string" => {
            let raw: Option<Vec<u8>> = redis::cmd("GET").arg(key).query_async(conn).await
                .map_err(|e| format!("GET error: {}", e))?;
            Ok(raw.map(|v| base(scalar_to_json(&v))))
        }
        "hash" => {
            let flat: Vec<Vec<u8>> = redis::cmd("HGETALL").arg(key).query_async(conn).await
                .map_err(|e| format!("HGETALL error: {}", e))?;
            if flat.len() > 2 * MAX_EXPORT_ELEMENTS {
                return Err(format!(
                    "hash exceeds export element limit ({})",
                    MAX_EXPORT_ELEMENTS
                ));
            }
            let mut fields = Vec::with_capacity(flat.len() / 2);
            for pair in flat.chunks_exact(2) {
                fields.push(serde_json::json!([scalar_to_json(&pair[0]), scalar_to_json(&pair[1])]));
            }
            Ok(Some(base(serde_json::json!({ "fields": fields }))))
        }
        "list" => {
            let len: u64 = redis::cmd("LLEN").arg(key).query_async(conn).await.unwrap_or(0);
            if len as usize > MAX_EXPORT_ELEMENTS {
                return Err(format!(
                    "list exceeds export element limit ({})",
                    MAX_EXPORT_ELEMENTS
                ));
            }
            let values: Vec<Vec<u8>> = redis::cmd("LRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .query_async(conn)
                .await
                .map_err(|e| format!("LRANGE error: {}", e))?;
            let list: Vec<serde_json::Value> = values.iter().map(|v| scalar_to_json(v)).collect();
            Ok(Some(base(serde_json::json!({ "values": list }))))
        }
        "set" => {
            let card: u64 = redis::cmd("SCARD").arg(key).query_async(conn).await.unwrap_or(0);
            if card as usize > MAX_EXPORT_ELEMENTS {
                return Err(format!(
                    "set exceeds export element limit ({})",
                    MAX_EXPORT_ELEMENTS
                ));
            }
            let members: Vec<Vec<u8>> = redis::cmd("SMEMBERS").arg(key).query_async(conn).await
                .map_err(|e| format!("SMEMBERS error: {}", e))?;
            let list: Vec<serde_json::Value> = members.iter().map(|v| scalar_to_json(v)).collect();
            Ok(Some(base(serde_json::json!({ "members": list }))))
        }
        "zset" => {
            let card: u64 = redis::cmd("ZCARD").arg(key).query_async(conn).await.unwrap_or(0);
            if card as usize > MAX_EXPORT_ELEMENTS {
                return Err(format!(
                    "zset exceeds export element limit ({})",
                    MAX_EXPORT_ELEMENTS
                ));
            }
            // ZRANGE ... WITHSCORES returns [member, score, member, score, ...]
            let raw: Vec<redis::Value> = redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .arg("WITHSCORES")
                .query_async(conn)
                .await
                .map_err(|e| format!("ZRANGE error: {}", e))?;
            let mut members = Vec::with_capacity(raw.len() / 2);
            let mut i = 0;
            while i + 1 < raw.len() {
                let member: Vec<u8> = redis::from_redis_value(&raw[i])
                    .map_err(|e| format!("zset member parse error: {}", e))?;
                let score: f64 = redis::from_redis_value(&raw[i + 1]).unwrap_or(0.0);
                members.push(serde_json::json!({ "m": scalar_to_json(&member), "score": score }));
                i += 2;
            }
            Ok(Some(base(serde_json::json!({ "members": members }))))
        }
        "stream" => {
            let raw: redis::Value = redis::cmd("XRANGE")
                .arg(key)
                .arg("-")
                .arg("+")
                .arg("COUNT")
                .arg(MAX_EXPORT_ELEMENTS as u64)
                .query_async(conn)
                .await
                .map_err(|e| format!("XRANGE error: {}", e))?;
            let entries = parse_xrange_entries(&raw)?;
            Ok(Some(base(serde_json::json!({ "entries": entries }))))
        }
        other => Err(format!("unsupported key type for JSON export: {}", other)),
    }
}

/// Parse an XRANGE response into JSON entries `[{id, fields: [[k,v]...]}]`.
fn parse_xrange_entries(raw: &redis::Value) -> Result<Vec<serde_json::Value>, String> {
    let items = match raw {
        redis::Value::Array(items) => items,
        _ => return Err("Unexpected XRANGE response format".to_string()),
    };
    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        let pair = match item {
            redis::Value::Array(pair) if pair.len() == 2 => pair,
            _ => return Err("Unexpected XRANGE entry format".to_string()),
        };
        let id: String = redis::from_redis_value(&pair[0])
            .map_err(|e| format!("stream id parse error: {}", e))?;
        let field_vals = match &pair[1] {
            redis::Value::Array(fv) => fv,
            _ => return Err("Unexpected XRANGE field format".to_string()),
        };
        let mut fields = Vec::with_capacity(field_vals.len() / 2);
        let mut i = 0;
        while i + 1 < field_vals.len() {
            let f: Vec<u8> = redis::from_redis_value(&field_vals[i])
                .map_err(|e| format!("stream field parse error: {}", e))?;
            let v: Vec<u8> = redis::from_redis_value(&field_vals[i + 1])
                .map_err(|e| format!("stream value parse error: {}", e))?;
            fields.push(serde_json::json!([scalar_to_json(&f), scalar_to_json(&v)]));
            i += 2;
        }
        entries.push(serde_json::json!({ "id": id, "fields": fields }));
    }
    Ok(entries)
}

/// Build one export entry in the lossless "dump" format (DUMP + PTTL).
async fn export_entry_dump(
    conn: &mut AnyConn,
    key: &str,
    ttl_ms: i64,
) -> Result<Option<serde_json::Value>, String> {
    let payload: Option<Vec<u8>> = redis::cmd("DUMP")
        .arg(key)
        .query_async(conn)
        .await
        .map_err(|e| format!("DUMP error: {}", e))?;
    Ok(payload.map(|p| {
        serde_json::json!({
            "key": key,
            "ttlMs": ttl_ms.max(-1),
            "dumpBase64": b64_encode(&p),
        })
    }))
}

/// Export the given keys into one document. `format` is "json" or "dump".
/// Unsupported / vanished keys are collected as warnings instead of aborting.
pub async fn export_keys(
    conn: &mut AnyConn,
    keys: &[String],
    format: &str,
    source: &str,
) -> Result<(String, usize, Vec<ExportWarning>), String> {
    if format != "json" && format != "dump" {
        return Err(format!("Unsupported export format: {}", format));
    }

    let mut entries = Vec::with_capacity(keys.len());
    let mut warnings = Vec::new();

    for key in keys {
        let key_type = type_str_of(
            redis::cmd("TYPE").arg(key).query_async::<String>(conn).await,
        );
        if key_type == "none" {
            warnings.push(ExportWarning {
                key: key.clone(),
                error: "key does not exist".to_string(),
            });
            continue;
        }
        let ttl_ms = pttl(conn, key).await.max(-1);

        let entry = if format == "dump" {
            export_entry_dump(conn, key, ttl_ms).await
        } else {
            export_entry_json(conn, key, &key_type, ttl_ms).await
        };

        match entry {
            Ok(Some(e)) => entries.push(e),
            Ok(None) => warnings.push(ExportWarning {
                key: key.clone(),
                error: "key expired during export".to_string(),
            }),
            Err(e) => warnings.push(ExportWarning {
                key: key.clone(),
                error: e,
            }),
        }
    }

    let doc = serde_json::json!({
        "version": 1,
        "format": format,
        "app": "BreezeRESP",
        "source": source,
        "exportedAt": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        "entries": entries,
    });

    let content =
        serde_json::to_string(&doc).map_err(|e| format!("Serialize error: {}", e))?;
    Ok((content, entries.len(), warnings))
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// Read + validate an export file, returning the parsed document.
pub fn read_export_file(path: &str) -> Result<serde_json::Value, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Cannot read file: {}", e))?;
    if meta.len() > MAX_IMPORT_FILE_BYTES {
        return Err(format!(
            "Import file too large ({} bytes, max {})",
            meta.len(),
            MAX_IMPORT_FILE_BYTES
        ));
    }
    let raw = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    serde_json::from_slice(&raw).map_err(|e| format!("Invalid JSON in import file: {}", e))
}

/// Peek at the document format + entry count without importing (for preview).
pub fn inspect_export_doc(doc: &serde_json::Value) -> Result<(String, usize), String> {
    let format = doc
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if format != "json" && format != "dump" {
        return Err("Unrecognised export file format".to_string());
    }
    let count = doc
        .get("entries")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    Ok((format, count))
}

async fn key_exists(conn: &mut AnyConn, key: &str) -> bool {
    redis::cmd("EXISTS")
        .arg(key)
        .query_async::<u64>(conn)
        .await
        .unwrap_or(0)
        > 0
}

/// Import all entries from a parsed export document.
/// `policy`: "skip" keeps existing keys, "replace" overwrites them.
pub async fn import_keys(
    conn: &mut AnyConn,
    doc: &serde_json::Value,
    policy: &str,
) -> Result<ImportResult, String> {
    if policy != "skip" && policy != "replace" {
        return Err(format!("Unsupported import policy: {}", policy));
    }
    let (format, _count) = inspect_export_doc(doc)?;
    let entries = doc
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut result = ImportResult {
        total: entries.len(),
        succeeded: 0,
        skipped: 0,
        failed: Vec::new(),
    };

    for entry in &entries {
        let key = entry
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if key.is_empty() {
            result.skipped += 1;
            continue;
        }

        let outcome = if format == "dump" {
            import_entry_dump(conn, entry, policy).await
        } else {
            import_entry_json(conn, entry, policy).await
        };

        match outcome {
            Ok(true) => result.succeeded += 1,
            Ok(false) => result.skipped += 1,
            Err(e) => result.failed.push(ImportFailure { key: key.clone(), error: e }),
        }
    }

    Ok(result)
}

/// Restore one DUMP-format entry via RESTORE. Ok(true) = restored,
/// Ok(false) = skipped (key exists and policy is skip).
async fn import_entry_dump(
    conn: &mut AnyConn,
    entry: &serde_json::Value,
    policy: &str,
) -> Result<bool, String> {
    let key = entry.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let ttl_ms = entry.get("ttlMs").and_then(|v| v.as_i64()).unwrap_or(-1);
    let b64 = entry
        .get("dumpBase64")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing dumpBase64".to_string())?;
    let payload = b64_decode(b64)?;

    if policy == "skip" && key_exists(conn, key).await {
        return Ok(false);
    }

    // RESTORE ttl: 0 means "no expiry".
    let restore_ttl: i64 = if ttl_ms > 0 { ttl_ms } else { 0 };
    let mut cmd = redis::cmd("RESTORE");
    cmd.arg(key).arg(restore_ttl).arg(&payload[..]);
    if policy == "replace" {
        cmd.arg("REPLACE");
    }
    let _: () = cmd
        .query_async(conn)
        .await
        .map_err(|e| format!("RESTORE error: {}", e))?;
    Ok(true)
}

/// Write one JSON-format entry back with the type-appropriate commands.
async fn import_entry_json(
    conn: &mut AnyConn,
    entry: &serde_json::Value,
    policy: &str,
) -> Result<bool, String> {
    let key = entry.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let key_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let ttl_ms = entry.get("ttlMs").and_then(|v| v.as_i64()).unwrap_or(-1);
    let value = entry
        .get("value")
        .ok_or_else(|| "missing value".to_string())?;

    if policy == "skip" && key_exists(conn, key).await {
        return Ok(false);
    }
    // Replace policy: drop the old key first so the write starts from a
    // clean type (SET/HSET/... on a wrong-typed key would fail).
    if policy == "replace" {
        let _: u64 = redis::cmd("DEL").arg(key).query_async(conn).await.unwrap_or(0);
    }

    match key_type {
        "string" => {
            let raw = json_to_scalar(value)?;
            let _: () = redis::cmd("SET")
                .arg(key)
                .arg(&raw[..])
                .query_async(conn)
                .await
                .map_err(|e| format!("SET error: {}", e))?;
        }
        "hash" => {
            let fields = value
                .get("fields")
                .and_then(|v| v.as_array())
                .ok_or_else(|| "missing hash fields".to_string())?;
            let mut flat: Vec<Vec<u8>> = Vec::with_capacity(fields.len() * 2);
            for pair in fields {
                let pair = pair.as_array().ok_or_else(|| "invalid hash field pair".to_string())?;
                if pair.len() != 2 {
                    return Err("invalid hash field pair".to_string());
                }
                flat.push(json_to_scalar(&pair[0])?);
                flat.push(json_to_scalar(&pair[1])?);
            }
            for chunk in flat.chunks(IMPORT_CHUNK_SIZE * 2) {
                let mut cmd = redis::cmd("HSET");
                cmd.arg(key);
                for item in chunk {
                    cmd.arg(&item[..]);
                }
                let _: u64 = cmd
                    .query_async(conn)
                    .await
                    .map_err(|e| format!("HSET error: {}", e))?;
            }
        }
        "list" => {
            let values = value
                .get("values")
                .and_then(|v| v.as_array())
                .ok_or_else(|| "missing list values".to_string())?;
            let raw: Vec<Vec<u8>> = values
                .iter()
                .map(json_to_scalar)
                .collect::<Result<_, _>>()?;
            for chunk in raw.chunks(IMPORT_CHUNK_SIZE) {
                let mut cmd = redis::cmd("RPUSH");
                cmd.arg(key);
                for item in chunk {
                    cmd.arg(&item[..]);
                }
                let _: u64 = cmd
                    .query_async(conn)
                    .await
                    .map_err(|e| format!("RPUSH error: {}", e))?;
            }
        }
        "set" => {
            let members = value
                .get("members")
                .and_then(|v| v.as_array())
                .ok_or_else(|| "missing set members".to_string())?;
            let raw: Vec<Vec<u8>> = members
                .iter()
                .map(json_to_scalar)
                .collect::<Result<_, _>>()?;
            for chunk in raw.chunks(IMPORT_CHUNK_SIZE) {
                let mut cmd = redis::cmd("SADD");
                cmd.arg(key);
                for item in chunk {
                    cmd.arg(&item[..]);
                }
                let _: u64 = cmd
                    .query_async(conn)
                    .await
                    .map_err(|e| format!("SADD error: {}", e))?;
            }
        }
        "zset" => {
            let members = value
                .get("members")
                .and_then(|v| v.as_array())
                .ok_or_else(|| "missing zset members".to_string())?;
            let mut raw: Vec<(f64, Vec<u8>)> = Vec::with_capacity(members.len());
            for m in members {
                let member = m
                    .get("m")
                    .ok_or_else(|| "missing zset member value".to_string())?;
                let score = m.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
                raw.push((score, json_to_scalar(member)?));
            }
            for chunk in raw.chunks(IMPORT_CHUNK_SIZE) {
                let mut cmd = redis::cmd("ZADD");
                cmd.arg(key);
                for (score, member) in chunk {
                    cmd.arg(*score).arg(&member[..]);
                }
                let _: u64 = cmd
                    .query_async(conn)
                    .await
                    .map_err(|e| format!("ZADD error: {}", e))?;
            }
        }
        "stream" => {
            let entries = value
                .get("entries")
                .and_then(|v| v.as_array())
                .ok_or_else(|| "missing stream entries".to_string())?;
            // Explicit-id XADD requires ascending ids; sort defensively.
            let mut parsed: Vec<(String, Vec<(Vec<u8>, Vec<u8>)>)> =
                Vec::with_capacity(entries.len());
            for e in entries {
                let id = e
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "missing stream entry id".to_string())?
                    .to_string();
                let fields = e
                    .get("fields")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| "missing stream entry fields".to_string())?;
                let mut pairs = Vec::with_capacity(fields.len());
                for pair in fields {
                    let pair = pair
                        .as_array()
                        .ok_or_else(|| "invalid stream field pair".to_string())?;
                    if pair.len() != 2 {
                        return Err("invalid stream field pair".to_string());
                    }
                    pairs.push((json_to_scalar(&pair[0])?, json_to_scalar(&pair[1])?));
                }
                parsed.push((id, pairs));
            }
            parsed.sort_by(|a, b| a.0.cmp(&b.0));
            for (id, pairs) in &parsed {
                let mut cmd = redis::cmd("XADD");
                cmd.arg(key).arg(id);
                for (f, v) in pairs {
                    cmd.arg(&f[..]).arg(&v[..]);
                }
                let _: String = cmd
                    .query_async(conn)
                    .await
                    .map_err(|e| format!("XADD error: {}", e))?;
            }
        }
        other => return Err(format!("unsupported key type for JSON import: {}", other)),
    }

    // Restore the TTL last so writes above are not time-boxed.
    if ttl_ms > 0 {
        let _: u64 = redis::cmd("PEXPIRE")
            .arg(key)
            .arg(ttl_ms)
            .query_async(conn)
            .await
            .unwrap_or(0);
    }

    Ok(true)
}
