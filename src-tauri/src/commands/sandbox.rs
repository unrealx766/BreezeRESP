use crate::core::validate::{validate_command, validate_connection_id};
use crate::AppState;
use crate::core::format::{format_redis_value, format_for_display, get_key_value_string_with_type};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
    pub path: String,
    pub key_type: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub before_raw: Option<String>,
    pub after_raw: Option<String>,
    pub change_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxPreview {
    pub command: String,
    pub diff: Vec<DiffEntry>,
    pub command_result: Option<String>,
    pub snapshot_id: String,
    pub key_types: HashMap<String, String>,
    /// Precise inverse commands for rollback (computed per-command during simulation)
    pub rollback_commands: Vec<String>,
}

const READ_ONLY_COMMANDS: &[&str] = &[
    "GET", "MGET", "TYPE", "TTL", "PTTL", "EXISTS", "STRLEN",
    "HGET", "HGETALL", "HMGET", "HLEN", "HEXISTS", "HKEYS", "HVALS",
    "LRANGE", "LLEN", "LINDEX",
    "SMEMBERS", "SCARD", "SISMEMBER", "SRANDMEMBER",
    "ZRANGE", "ZCARD", "ZSCORE", "ZRANK", "ZRANGEBYSCORE",
    "KEYS", "SCAN", "DBSIZE", "INFO", "PING", "ECHO",
];

/// Parse a command string into parts, respecting quoted strings.
/// Supports double-quoted ("hello world") and single-quoted ('hello world') values.
/// Backslash escapes within double quotes are supported (\\, \").
fn parse_command_parts(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' if current.is_empty() => {
                chars.next();
            }
            '"' => {
                chars.next();
                while let Some(&inner) = chars.peek() {
                    if inner == '"' {
                        chars.next();
                        break;
                    }
                    if inner == '\\' {
                        chars.next();
                        if let Some(&escaped) = chars.peek() {
                            current.push(escaped);
                            chars.next();
                        }
                    } else {
                        current.push(inner);
                        chars.next();
                    }
                }
            }
            '\'' => {
                chars.next();
                while let Some(&inner) = chars.peek() {
                    if inner == '\'' {
                        chars.next();
                        break;
                    }
                    current.push(inner);
                    chars.next();
                }
            }
            ' ' | '\t' => {
                parts.push(std::mem::take(&mut current));
                chars.next();
            }
            _ => {
                current.push(c);
                chars.next();
            }
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

fn extract_keys(parts: &[&str]) -> Vec<String> {
    if parts.len() < 2 {
        return vec![];
    }
    let cmd = parts[0].to_uppercase();
    match cmd.as_str() {
        "SET" | "GET" | "DEL" | "EXPIRE" | "PERSIST" | "TTL" | "TYPE"
        | "APPEND" | "INCR" | "DECR" | "SETNX" | "GETSET" | "SETEX" | "PSETEX" => {
            vec![parts[1].to_string()]
        }
        "RENAME" | "RENAMENX" => {
            if parts.len() >= 3 {
                vec![parts[1].to_string(), parts[2].to_string()]
            } else {
                vec![parts[1].to_string()]
            }
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
        "MSET" => parts[1..].iter().step_by(2).map(|s| s.to_string()).collect(),
        "MGET" => parts[1..].iter().map(|s| s.to_string()).collect(),
        _ => vec![parts[1].to_string()],
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Local command simulation — compute after-state without touching Redis
// ───────────────────────────────────────────────────────────────────────────

/// Simulate a write command locally.
/// Returns the resulting state for every affected key.
/// • `Some(value)` = key exists with this serialised value
/// • `None`        = key was deleted / doesn't exist
fn simulate_write_command(
    cmd: &str,
    args: &[&str],
    current_values: &HashMap<String, Option<String>>,
    current_types: &HashMap<String, String>,
) -> Result<HashMap<String, Option<String>>, String> {
    let cmd_upper = cmd.to_uppercase();
    let mut result: HashMap<String, Option<String>> = HashMap::new();

    match cmd_upper.as_str() {
        // ── String commands ────────────────────────────────────────
        "SET" | "SETNX" | "GETSET" | "SETEX" | "PSETEX" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();

            let new_val = match cmd_upper.as_str() {
                "SETNX" => {
                    if cur.is_some() { cur.clone() } else { Some(args[1].to_string()) }
                }
                "GETSET" => args.get(1).map(|s| s.to_string()),
                "SETEX" => args.get(2).map(|s| s.to_string()),
                "PSETEX" => args.get(2).map(|s| s.to_string()),
                _ => Some(args[1].to_string()), // SET
            };
            result.insert(key, new_val);
        }

        "APPEND" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let new_val = format!("{}{}", cur.unwrap_or_default(), args[1]);
            result.insert(key, Some(new_val));
        }

        "INCR" | "DECR" | "INCRBY" | "DECRBY" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let base: i64 = cur.as_deref().unwrap_or("0").parse().unwrap_or(0);
            let delta: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
            let new_val = match cmd_upper.as_str() {
                "INCR" => base + 1,
                "DECR" => base - 1,
                "INCRBY" => base + delta,
                "DECRBY" => base - delta,
                _ => base,
            };
            result.insert(key, Some(new_val.to_string()));
        }

        // ── Hash commands ──────────────────────────────────────────
        "HSET" | "HMSET" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let kt = current_types.get(&key).map(|s| s.as_str()).unwrap_or("none");

            let mut fields: Vec<(String, String)> = if kt == "hash" {
                cur.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            // args[1..] = field, value, field, value, ...
            let field_args = &args[1..];
            let mut i = 0;
            while i + 1 < field_args.len() {
                let field = field_args[i].to_string();
                let value = field_args[i + 1].to_string();
                if let Some(pos) = fields.iter().position(|(f, _)| f == &field) {
                    fields[pos].1 = value;
                } else {
                    fields.push((field, value));
                }
                i += 2;
            }
            result.insert(key, Some(serde_json::to_string(&fields).unwrap_or_default()));
        }

        "HDEL" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let kt = current_types.get(&key).map(|s| s.as_str()).unwrap_or("none");

            if kt != "hash" {
                result.insert(key, cur);
                return Ok(result);
            }
            let mut fields: Vec<(String, String)> = cur.as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            for &field in &args[1..] {
                fields.retain(|(f, _)| f != field);
            }
            if fields.is_empty() {
                result.insert(key, None);
            } else {
                result.insert(key, Some(serde_json::to_string(&fields).unwrap_or_default()));
            }
        }

        "HINCRBY" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let kt = current_types.get(&key).map(|s| s.as_str()).unwrap_or("none");

            let mut fields: Vec<(String, String)> = if kt == "hash" {
                cur.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            let field = args[1].to_string();
            let increment: i64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            let current: i64 = fields
                .iter()
                .find(|(f, _)| f == &field)
                .map(|(_, v)| v.parse().unwrap_or(0))
                .unwrap_or(0);
            let new_val = (current + increment).to_string();

            if let Some(pos) = fields.iter().position(|(f, _)| f == &field) {
                fields[pos].1 = new_val;
            } else {
                fields.push((field, new_val));
            }
            result.insert(key, Some(serde_json::to_string(&fields).unwrap_or_default()));
        }

        // ── List commands ──────────────────────────────────────────
        "LPUSH" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let kt = current_types.get(&key).map(|s| s.as_str()).unwrap_or("none");

            let mut items: Vec<String> = if kt == "list" {
                cur.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            // LPUSH prepends each value (first arg ends up deepest)
            for &val in args[1..].iter().rev() {
                items.insert(0, val.to_string());
            }
            result.insert(key, Some(serde_json::to_string(&items).unwrap_or_default()));
        }

        "RPUSH" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let kt = current_types.get(&key).map(|s| s.as_str()).unwrap_or("none");

            let mut items: Vec<String> = if kt == "list" {
                cur.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            for &val in &args[1..] {
                items.push(val.to_string());
            }
            result.insert(key, Some(serde_json::to_string(&items).unwrap_or_default()));
        }

        "LPOP" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let kt = current_types.get(&key).map(|s| s.as_str()).unwrap_or("none");

            let mut items: Vec<String> = if kt == "list" {
                cur.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let count: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
            for _ in 0..count.min(items.len()) {
                items.remove(0);
            }
            if items.is_empty() {
                result.insert(key, None);
            } else {
                result.insert(key, Some(serde_json::to_string(&items).unwrap_or_default()));
            }
        }

        "RPOP" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let kt = current_types.get(&key).map(|s| s.as_str()).unwrap_or("none");

            let mut items: Vec<String> = if kt == "list" {
                cur.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let count: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
            for _ in 0..count.min(items.len()) {
                items.pop();
            }
            if items.is_empty() {
                result.insert(key, None);
            } else {
                result.insert(key, Some(serde_json::to_string(&items).unwrap_or_default()));
            }
        }

        // ── Set commands ───────────────────────────────────────────
        "SADD" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let kt = current_types.get(&key).map(|s| s.as_str()).unwrap_or("none");

            let mut members: Vec<String> = if kt == "set" {
                cur.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let existing: HashSet<String> = members.iter().cloned().collect();
            for &m in &args[1..] {
                if !existing.contains(m) {
                    members.push(m.to_string());
                }
            }
            result.insert(key, Some(serde_json::to_string(&members).unwrap_or_default()));
        }

        "SREM" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let kt = current_types.get(&key).map(|s| s.as_str()).unwrap_or("none");

            let mut members: Vec<String> = if kt == "set" {
                cur.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let to_remove: HashSet<&str> = args[1..].iter().copied().collect();
            members.retain(|m| !to_remove.contains(m.as_str()));
            if members.is_empty() {
                result.insert(key, None);
            } else {
                result.insert(key, Some(serde_json::to_string(&members).unwrap_or_default()));
            }
        }

        // ── Sorted-set commands ────────────────────────────────────
        "ZADD" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let kt = current_types.get(&key).map(|s| s.as_str()).unwrap_or("none");

            let mut members: Vec<(String, f64)> = if kt == "zset" {
                cur.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            // args: score, member, score, member, ...
            let score_args = &args[1..];
            let mut i = 0;
            while i + 1 < score_args.len() {
                let score: f64 = score_args[i].parse().unwrap_or(0.0);
                let member = score_args[i + 1].to_string();
                if let Some(pos) = members.iter().position(|(m, _)| m == &member) {
                    members[pos].1 = score;
                } else {
                    members.push((member, score));
                }
                i += 2;
            }
            result.insert(key, Some(serde_json::to_string(&members).unwrap_or_default()));
        }

        "ZREM" => {
            let key = args[0].to_string();
            let cur = current_values.get(&key).cloned().flatten();
            let kt = current_types.get(&key).map(|s| s.as_str()).unwrap_or("none");

            let mut members: Vec<(String, f64)> = if kt == "zset" {
                cur.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let to_remove: HashSet<&str> = args[1..].iter().copied().collect();
            members.retain(|(m, _)| !to_remove.contains(m.as_str()));
            if members.is_empty() {
                result.insert(key, None);
            } else {
                result.insert(key, Some(serde_json::to_string(&members).unwrap_or_default()));
            }
        }

        // ── Key-level commands ─────────────────────────────────────
        "DEL" => {
            for &key in args {
                result.insert(key.to_string(), None);
            }
        }

        "EXPIRE" | "PERSIST" | "TTL" | "PTTL" => {
            // These don't change the serialised value
            for &key in args {
                result.insert(
                    key.to_string(),
                    current_values.get(key).cloned().flatten(),
                );
            }
        }

        "RENAME" | "RENAMENX" => {
            if args.len() >= 2 {
                let src = args[0].to_string();
                let dst = args[1].to_string();
                let val = current_values.get(&src).cloned().flatten();
                result.insert(src, None);
                result.insert(dst, val);
            }
        }

        "MSET" => {
            let mut i = 0;
            while i + 1 < args.len() {
                result.insert(args[i].to_string(), Some(args[i + 1].to_string()));
                i += 2;
            }
        }

        // ── Fallback: reject unsupported commands ─────────────────
        _ => {
            return Err(format!(
                "Command '{}' is not supported in sandbox preview. \
                 Only common write commands can be previewed; \
                 use Pipeline for other commands.",
                cmd_upper
            ));
        }
    }

    Ok(result)
}

/// Determine the key-type string that results from a simulated write.
fn simulated_key_type(
    cmd: &str,
    before_type: &str,
    after_value: &Option<String>,
) -> String {
    if after_value.is_none() {
        return "none".to_string();
    }
    let cmd_upper = cmd.to_uppercase();
    match cmd_upper.as_str() {
        "SET" | "SETNX" | "GETSET" | "SETEX" | "PSETEX"
        | "APPEND" | "INCR" | "DECR" | "INCRBY" | "DECRBY" => "string".to_string(),
        "HSET" | "HMSET" | "HDEL" | "HINCRBY" => "hash".to_string(),
        "LPUSH" | "RPUSH" | "LPOP" | "RPOP" => "list".to_string(),
        "SADD" | "SREM" => "set".to_string(),
        "ZADD" | "ZREM" => "zset".to_string(),
        "RENAME" | "RENAMENX" => before_type.to_string(),
        _ => before_type.to_string(),
    }
}


// ───────────────────────────────────────────────────────────────────────────
// IPC commands
// ───────────────────────────────────────────────────────────────────────────

/// Execute command(s) in sandbox mode and return the diff preview.
///
/// Supports multiple commands separated by newlines. Comments (`#` or `--`)
/// and blank lines are ignored. Write commands are simulated locally; read-only
/// commands are executed directly against Redis.
///
/// ## Cumulative preview model (pure local simulation)
///
/// Preview never modifies Redis:
/// 1. Read current key data from Redis for ALL affected keys
/// 2. Use pending_state as baseline if available (cumulative)
/// 3. Locally simulate EACH write command sequentially
/// 4. Update pending_state with the final after-state
/// 5. Compute diff between baseline and final after-state
/// 6. Compute per-command rollback commands
#[tauri::command]
pub async fn sandbox_preview(
    state: State<'_, AppState>,
    connection_id: String,
    command: String,
) -> Result<SandboxPreview, String> {
    validate_connection_id(&connection_id)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    // Parse single command
    let cmd = command.trim().to_string();
    if cmd.is_empty() {
        return Err("Empty command".to_string());
    }
    validate_command(&cmd)?;

    // Classify command
    let parts = parse_command_parts(&cmd);
    if parts.is_empty() {
        return Err("Empty command".to_string());
    }
    let cmd_upper = parts[0].to_uppercase();

    // Read-only command: execute directly
    if READ_ONLY_COMMANDS.contains(&cmd_upper.as_str()) {
        let mut redis_cmd = redis::cmd(&parts[0]);
        for arg in &parts[1..] {
            redis_cmd.arg(arg);
        }
        let result: redis::Value = redis_cmd
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("Command error: {}", e))?;
        return Ok(SandboxPreview {
            command,
            diff: vec![],
            command_result: Some(format_redis_value(&result)),
            snapshot_id: String::new(),
            key_types: HashMap::new(),
            rollback_commands: vec![],
        });
    }

    // ── Write command: local simulation ──
    let write_cmds: Vec<Vec<String>> = vec![parts];
    let refs: Vec<&str> = write_cmds[0].iter().map(|s| s.as_str()).collect();
    let per_cmd_refs = vec![refs];
    let per_cmd_keys = vec![extract_keys(&per_cmd_refs[0])];
    let all_affected_keys: Vec<String> = per_cmd_keys[0].clone();

    // Step 1: Read current Redis state for ALL affected keys (batch)
    let mut redis_state: HashMap<String, Option<String>> = HashMap::new();
    let mut redis_key_types: HashMap<String, String> = HashMap::new();

    if !all_affected_keys.is_empty() {
        let mut pipe = redis::pipe();
        for key in &all_affected_keys {
            pipe.cmd("TYPE").arg(key);
        }
        let type_values: Vec<redis::Value> = pipe
            .query_async(&mut *conn)
            .await
            .unwrap_or_default();

        for (i, key) in all_affected_keys.iter().enumerate() {
            let kt = if i < type_values.len() {
                redis::from_redis_value::<String>(&type_values[i]).unwrap_or_else(|_| "none".to_string())
            } else {
                "none".to_string()
            };
            redis_key_types.insert(key.clone(), kt.clone());
            if kt != "none" {
                let val = get_key_value_string_with_type(&mut conn, key, &kt).await;
                redis_state.insert(key.clone(), val);
            } else {
                redis_state.insert(key.clone(), None);
            }
        }
    }

    // Step 2: Build baseline (pending_state or redis_state)
    let has_pending = {
        let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
        ss.has_pending()
    };

    let mut before_for_diff: HashMap<String, String> = HashMap::new();
    let mut before_key_types: HashMap<String, String> = HashMap::new();
    let mut effective_values: HashMap<String, Option<String>> = HashMap::new();
    let mut effective_types: HashMap<String, String> = HashMap::new();

    {
        let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
        for key in &all_affected_keys {
            // ── Diff baseline: ALWAYS use Redis state ──
            // This ensures the diff always shows the change from the original
            // Redis state, regardless of how many previews have been run.
            if let Some(val) = redis_state.get(key).cloned().flatten() {
                before_for_diff.insert(key.clone(), val);
            }
            if let Some(kt) = redis_key_types.get(key) {
                before_key_types.insert(key.clone(), kt.clone());
            }

            // ── Simulation baseline: use pending state if available ──
            // This allows cumulative previews to build on each other correctly.
            if let Some(pending_val) = ss.pending_state.get(key) {
                effective_values.insert(key.clone(), pending_val.clone());
                if let Some(kt) = ss.pending_key_types.get(key) {
                    effective_types.insert(key.clone(), kt.clone());
                }
            } else {
                let rv = redis_state.get(key).cloned().flatten();
                effective_values.insert(key.clone(), rv.clone());
                if let Some(kt) = redis_key_types.get(key) {
                    effective_types.insert(key.clone(), kt.clone());
                }
            }
        }
    }

    // Step 3: Save original state on first pending preview
    if !has_pending {
        let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
        let mut ss = ss;
        for key in &all_affected_keys {
            if let Some(val) = redis_state.get(key).cloned().flatten() {
                ss.original_state.insert(key.clone(), val);
            }
            if let Some(kt) = redis_key_types.get(key) {
                ss.original_key_types.insert(key.clone(), kt.clone());
            }
        }
    }

    // Step 4: Simulate EACH write command sequentially
    for (cmd_idx, _parts) in write_cmds.iter().enumerate() {
        let refs = &per_cmd_refs[cmd_idx];
        let cmd_keys = &per_cmd_keys[cmd_idx];

        // Build per-command input from effective state (only affected keys)
        let mut cmd_values: HashMap<String, Option<String>> = HashMap::new();
        let mut cmd_types: HashMap<String, String> = HashMap::new();
        for key in cmd_keys {
            cmd_values.insert(key.clone(), effective_values.get(key).cloned().flatten());
            if let Some(kt) = effective_types.get(key) {
                cmd_types.insert(key.clone(), kt.clone());
            }
        }

        let simulated = simulate_write_command(
            refs[0],
            &refs[1..],
            &cmd_values,
            &cmd_types,
        )?;

        // Update effective state for next command
        for key in cmd_keys {
            if let Some(sim_val) = simulated.get(key) {
                effective_values.insert(key.clone(), sim_val.clone());
                let before_kt = effective_types.get(key).map(|s| s.as_str()).unwrap_or("none");
                let new_kt = simulated_key_type(refs[0], before_kt, sim_val);
                effective_types.insert(key.clone(), new_kt);
            }
        }
    }

    // Step 4b: Rollback commands are computed by the frontend using computeInverseCommands

    // Step 5: Create snapshot
    let snap_id = uuid::Uuid::new_v4().to_string();
    {
        let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
        let mut ss = ss;
        ss.create_snapshot(&snap_id, before_for_diff.clone());
    }

    // Step 6: Build after state & update pending
    let mut after_state: HashMap<String, String> = HashMap::new();
    let mut after_key_types: HashMap<String, String> = HashMap::new();
    for key in &all_affected_keys {
        if let Some(val) = effective_values.get(key).cloned().flatten() {
            after_state.insert(key.clone(), val);
        }
        if let Some(kt) = effective_types.get(key) {
            after_key_types.insert(key.clone(), kt.clone());
        }
    }

    {
        let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
        let mut ss = ss;

        for key in &all_affected_keys {
            if let Some(val) = effective_values.get(key) {
                ss.pending_state.insert(key.clone(), val.clone());
            }
            if let Some(kt) = effective_types.get(key) {
                ss.pending_key_types.insert(key.clone(), kt.clone());
            }
        }

        ss.set_after_state(&snap_id, after_state);
    }

    // Step 7: Compute final cumulative diff
    let diff_result = {
        let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
        ss.compute_diff(&snap_id)
    };

    // Convert DiffResult to Vec<DiffEntry>
    let mut diff = Vec::new();
    if let Some(dr) = diff_result {
        for (key, val) in dr.added {
            let kt = after_key_types.get(&key).cloned().unwrap_or_default();
            diff.push(DiffEntry {
                path: key.clone(),
                key_type: Some(kt.clone()),
                before: None,
                after: Some(format_for_display(&val, &kt)),
                before_raw: redis_state.get(&key).cloned().flatten(),
                after_raw: Some(val),
                change_type: "added".to_string(),
            });
        }
        for (key, before, after) in dr.modified {
            let kt = before_key_types.get(&key).cloned().unwrap_or_default();
            diff.push(DiffEntry {
                path: key.clone(),
                key_type: Some(kt.clone()),
                before: Some(format_for_display(&before, &kt)),
                after: Some(format_for_display(&after, &after_key_types.get(&key).cloned().unwrap_or(kt.clone()))),
                before_raw: redis_state.get(&key).cloned().flatten(),
                after_raw: Some(after),
                change_type: "modified".to_string(),
            });
        }
        for (key, val) in dr.deleted {
            let kt = before_key_types.get(&key).cloned().unwrap_or_default();
            diff.push(DiffEntry {
                path: key.clone(),
                key_type: Some(kt.clone()),
                before: Some(format_for_display(&val, &kt)),
                after: None,
                before_raw: redis_state.get(&key).cloned().flatten(),
                after_raw: None,
                change_type: "deleted".to_string(),
            });
        }
        for (key, val) in dr.unchanged {
            let kt = before_key_types.get(&key).cloned().unwrap_or_default();
            diff.push(DiffEntry {
                path: key.clone(),
                key_type: Some(kt.clone()),
                before: Some(format_for_display(&val, &kt)),
                after: Some(format_for_display(&val, &kt)),
                before_raw: redis_state.get(&key).cloned().flatten(),
                after_raw: Some(val),
                change_type: "unchanged".to_string(),
            });
        }
    }

    Ok(SandboxPreview {
        command,
        diff,
        command_result: None,
        snapshot_id: snap_id,
        key_types: before_key_types,
        rollback_commands: vec![],
    })
}

/// Apply the current sandbox command to Redis for real.
#[tauri::command]
pub async fn sandbox_apply(
    state: State<'_, AppState>,
    connection_id: String,
    command: String,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    // Parse single command
    let cmd = command.trim().to_string();
    if cmd.is_empty() {
        return Err("Empty command".to_string());
    }
    validate_command(&cmd)?;

    // Execute the command
    let parts: Vec<String> = parse_command_parts(&cmd);
    if parts.is_empty() {
        return Err("Empty command".to_string());
    }

    let mut redis_cmd = redis::cmd(&parts[0]);
    for arg in &parts[1..] {
        redis_cmd.arg(arg);
    }
    let _: redis::Value = redis_cmd
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("Apply error: {}", e))?;

    {
        let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
        let mut ss = ss;
        ss.clear_pending();
    }

    Ok(true)
}

/// Cancel all pending sandbox previews.
/// Redis was never modified during preview, so this only clears shadow state.
#[tauri::command]
pub async fn sandbox_cancel(
    state: State<'_, AppState>,
    _connection_id: String,
) -> Result<bool, String> {
    let ss = state.shadow_store.lock().map_err(|e| e.to_string())?;
    let mut ss = ss;
    ss.clear_pending();
    Ok(true)
}

/// Rollback a previously-applied sandbox history item by executing precise inverse commands.
#[tauri::command]
pub async fn sandbox_rollback(
    state: State<'_, AppState>,
    connection_id: String,
    commands: Vec<String>,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;
    for cmd in &commands {
        validate_command(cmd)?;
    }

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    for cmd_str in &commands {
        let parts: Vec<String> = parse_command_parts(cmd_str);
        if parts.is_empty() {
            continue;
        }
        let mut redis_cmd = redis::cmd(&parts[0]);
        for arg in &parts[1..] {
            redis_cmd.arg(arg);
        }
        let _: redis::Value = redis_cmd
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("Rollback cmd '{}' error: {}", cmd_str, e))?;
    }

    Ok(true)
}
