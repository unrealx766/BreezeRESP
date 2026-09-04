use serde::{Deserialize, Serialize};

use crate::core::cluster::per_master_values;
use crate::core::pool::AnyConn;

/// Maximum number of keys enriched per scan batch round-trip.
const MAX_BATCH_KEYS: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BigKeyEntry {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
    pub memory_bytes: u64,
    pub element_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BigKeyBatch {
    pub next_cursor: u64,
    pub entries: Vec<BigKeyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStatItem {
    pub name: String,
    pub value: i64,
}

/// Per-node MEMORY DOCTOR result for cluster-aware display.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDoctorEntry {
    /// Node address (e.g. "127.0.0.1:6379"), or empty for standalone.
    pub addr: String,
    /// Advice text. Empty when the node reports no problems.
    pub advice: String,
}

/// Element-count command for a given Redis type.
fn len_command(key_type: &str) -> Option<&'static str> {
    match key_type {
        "string" => Some("STRLEN"),
        "hash" => Some("HLEN"),
        "list" => Some("LLEN"),
        "set" => Some("SCARD"),
        "zset" => Some("ZCARD"),
        "stream" => Some("XLEN"),
        _ => None,
    }
}

/// Enrich a batch of keys with TYPE / PTTL / MEMORY USAGE / element count.
/// Standalone connections use pipelining (2 round-trips); cluster connections
/// query per key because cross-key pipelines hit CROSSSLOT errors.
async fn enrich_keys(conn: &mut AnyConn, keys: &[String]) -> Vec<BigKeyEntry> {
    if keys.is_empty() {
        return Vec::new();
    }
    if keys.len() > MAX_BATCH_KEYS {
        // Defensive: never pipeline an unbounded number of keys.
        return Vec::new();
    }

    // Cluster path: per-key queries (routed individually by redis-rs).
    if matches!(conn, AnyConn::Cluster(_)) {
        let mut entries = Vec::with_capacity(keys.len());
        for key in keys {
            let key_type: String = redis::cmd("TYPE")
                .arg(key)
                .query_async(conn)
                .await
                .unwrap_or_else(|_| "none".to_string());
            let ttl: i64 = redis::cmd("PTTL")
                .arg(key)
                .query_async(conn)
                .await
                .unwrap_or(-1);
            let memory: u64 = redis::cmd("MEMORY")
                .arg("USAGE")
                .arg(key)
                .query_async(conn)
                .await
                .unwrap_or(0);
            let count: u64 = match len_command(&key_type) {
                Some(cmd) => redis::cmd(cmd).arg(key).query_async(conn).await.unwrap_or(0),
                None => 0,
            };
            entries.push(BigKeyEntry {
                key: key.clone(),
                key_type,
                ttl,
                memory_bytes: memory,
                element_count: count,
            });
        }
        return entries;
    }

    // Standalone path: pipeline TYPE + PTTL + MEMORY USAGE in one round-trip.
    let n = keys.len();
    let mut pipe = redis::pipe();
    for key in keys {
        pipe.cmd("TYPE").arg(key);
    }
    for key in keys {
        pipe.cmd("PTTL").arg(key);
    }
    for key in keys {
        pipe.cmd("MEMORY").arg("USAGE").arg(key);
    }
    let values: Vec<redis::Value> = pipe.query_async(conn).await.unwrap_or_default();

    let mut types = vec!["none".to_string(); n];
    let mut ttls = vec![-1i64; n];
    // MEMORY USAGE may not exist on Redis < 4.0 — keep zeros on failure.
    let mut memories = vec![0u64; n];

    if values.len() == 3 * n {
        for (i, key_type) in types.iter_mut().enumerate() {
            *key_type =
                redis::from_redis_value::<String>(&values[i]).unwrap_or_else(|_| "none".into());
        }
        for (i, ttl) in ttls.iter_mut().enumerate() {
            *ttl = redis::from_redis_value::<i64>(&values[n + i]).unwrap_or(-1);
        }
        for (i, mem) in memories.iter_mut().enumerate() {
            *mem = redis::from_redis_value::<u64>(&values[2 * n + i]).unwrap_or(0);
        }
    }

    // Second round-trip: element counts per key based on its type.
    let mut count_pipe = redis::pipe();
    let mut has_cmd = vec![false; n];
    for (i, key) in keys.iter().enumerate() {
        match len_command(&types[i]) {
            Some(cmd) => {
                count_pipe.cmd(cmd).arg(key);
                has_cmd[i] = true;
            }
            None => {
                // Placeholder keeps pipeline positions aligned with keys.
                count_pipe.cmd("PING");
            }
        }
    }
    let count_vals: Vec<redis::Value> = count_pipe.query_async(conn).await.unwrap_or_default();
    let mut counts = vec![0u64; n];
    if count_vals.len() == n {
        for (i, count) in counts.iter_mut().enumerate() {
            if has_cmd[i] {
                *count = redis::from_redis_value::<u64>(&count_vals[i]).unwrap_or(0);
            }
        }
    }

    keys.iter()
        .enumerate()
        .map(|(i, key)| BigKeyEntry {
            key: key.clone(),
            key_type: types[i].clone(),
            ttl: ttls[i],
            memory_bytes: memories[i],
            element_count: counts[i],
        })
        .collect()
}

/// Analyses memory usage across the keyspace.
pub struct BigKeyAnalyzer;

impl BigKeyAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Enrich one SCAN batch of keys. The caller supplies the SCAN cursor
    /// protocol (standalone SCAN or cluster multi-master iteration).
    pub async fn enrich_batch(
        &self,
        conn: &mut AnyConn,
        next_cursor: u64,
        keys: Vec<String>,
    ) -> Result<BigKeyBatch, String> {
        let entries = enrich_keys(conn, &keys).await;
        Ok(BigKeyBatch { next_cursor, entries })
    }

    /// MEMORY STATS as a flat name/value list. Nested sub-sections
    /// (e.g. `db.0`) are flattened with dotted names.
    pub async fn memory_stats(&self, conn: &mut AnyConn) -> Result<Vec<MemoryStatItem>, String> {
        if let Some(cluster) = conn.as_cluster() {
            let values = per_master_values(
                cluster,
                redis::cmd("MEMORY").arg("STATS"),
            )
            .await?;
            // Aggregate numeric items across masters by summing.
            let mut acc: Vec<MemoryStatItem> = Vec::new();
            for (_addr, value) in values {
                let items = parse_memory_stats(&value);
                for item in items {
                    match acc.iter_mut().find(|a| a.name == item.name) {
                        Some(existing) => existing.value += item.value,
                        None => acc.push(item),
                    }
                }
            }
            if acc.is_empty() {
                return Err("MEMORY STATS not supported or no cluster nodes responded".to_string());
            }
            return Ok(acc);
        }

        let value: redis::Value = redis::cmd("MEMORY")
            .arg("STATS")
            .query_async(conn)
            .await
            .map_err(|e| format!("MEMORY STATS error: {}", e))?;
        let items = parse_memory_stats(&value);
        if items.is_empty() {
            return Err("MEMORY STATS not supported on this server".to_string());
        }
        Ok(items)
    }

    /// MEMORY DOCTOR per-node advice. Returns one entry per master in cluster
    /// mode, or a single entry with empty addr for standalone connections.
    pub async fn memory_doctor(&self, conn: &mut AnyConn) -> Result<Vec<MemoryDoctorEntry>, String> {
        if let Some(cluster) = conn.as_cluster() {
            let values = per_master_values(
                cluster,
                redis::cmd("MEMORY").arg("DOCTOR"),
            )
            .await?;
            let mut entries = Vec::new();
            for (addr, value) in values {
                if let Ok(text) = redis::from_redis_value::<String>(&value) {
                    let text = text.trim().to_string();
                    entries.push(MemoryDoctorEntry {
                        addr,
                        advice: text,
                    });
                }
            }
            return Ok(entries);
        }

        let result: Result<String, _> = redis::cmd("MEMORY").arg("DOCTOR").query_async(conn).await;
        match result {
            Ok(text) => Ok(vec![MemoryDoctorEntry {
                addr: String::new(),
                advice: text,
            }]),
            // Command absent on Redis < 4.0 — degrade silently.
            Err(_) => Ok(vec![]),
        }
    }
}

impl Default for BigKeyAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a MEMORY STATS response (flat `[name, value, ...]` array, where
/// values may themselves be nested pair arrays for db sections).
fn parse_memory_stats(value: &redis::Value) -> Vec<MemoryStatItem> {
    let mut items = Vec::new();
    flatten_stats_pairs(value, "", &mut items);
    items
}

fn flatten_stats_pairs(value: &redis::Value, prefix: &str, out: &mut Vec<MemoryStatItem>) {
    let entries = match value {
        redis::Value::Array(items) => items,
        _ => return,
    };
    let mut i = 0;
    while i + 1 < entries.len() {
        let name: String = match redis::from_redis_value(&entries[i]) {
            Ok(n) => n,
            Err(_) => {
                i += 1;
                continue;
            }
        };
        let full_name = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{}.{}", prefix, name)
        };
        match &entries[i + 1] {
            redis::Value::Int(v) => out.push(MemoryStatItem {
                name: full_name,
                value: *v,
            }),
            redis::Value::BulkString(bytes) => {
                if let Ok(s) = std::str::from_utf8(bytes) {
                    if let Ok(v) = s.parse::<i64>() {
                        out.push(MemoryStatItem {
                            name: full_name,
                            value: v,
                        });
                    }
                }
            }
            nested @ redis::Value::Array(_) => {
                flatten_stats_pairs(nested, &full_name, out);
            }
            _ => {}
        }
        i += 2;
    }
}
